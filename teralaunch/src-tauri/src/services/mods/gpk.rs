//! TMM-compatible GPK mod deployer.
//!
//! Port of VenoMKO/TMM's `CompositeMapper.cpp` and `Mod.cpp` to Rust:
//!   - Decrypts and re-encrypts `CompositePackageMapper.dat` using the same
//!     three-pass XOR + byte-swap + 16-byte shuffle scheme TMM uses.
//!   - Parses the decrypted mapper into a map of composite-name → entry.
//!   - Reads `.gpk` mod files that carry TMM metadata (magic `0x9E2A83C1`
//!     at EOF) and applies their composite-package override to the mapper.
//!   - Writes a `CompositePackageMapper.clean` backup on first touch so
//!     uninstall can restore vanilla entries.
//!   - Maintains a `ModList.tmm` registry alongside the backup so
//!     re-opens of the launcher know which mods are on disk.
//!
//! The file format itself is not documented anywhere outside of TMM's
//! source, so every non-obvious byte decision is annotated with the line
//! in TMM that it mirrors.

use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

/// Magic footer at the end of a `.gpk` mod file that carries metadata
/// TMM writes. Without it the file is treated as raw game content and
/// can't be installed.
pub(crate) const PACKAGE_MAGIC: u32 = 0x9E2A83C1;

/// Folder-name prefix TMM writes into a mod's CompositePackage folder
/// name. The real object path begins immediately after `MOD:`.
const MOD_PREFIX: &str = "MOD:";

/// Sentinel key TMM drops into the mapper so it can tell whether a file
/// has ever been touched. We write the same one so a user can still open
/// the file in TMM and have it recognised.
const TMM_MARKER: &str = "tmm_marker";

// --- Encryption primitives --------------------------------------------------
//
// Mirror of `const char Key1[]` and `const char Key2[]` in
// TMM/Model/CompositeMapper.cpp lines 11-12.

const KEY1: [usize; 16] = [12, 6, 9, 4, 3, 14, 1, 10, 13, 2, 7, 15, 0, 8, 5, 11];
const KEY2: &[u8] = b"GeneratePackageMapper";

/// Inverse of `EncryptMapper` (TMM/Model/CompositeMapper.cpp:15).
/// Order: undo the 16-byte shuffle, undo the pair-swap, XOR with Key2.
pub fn decrypt_mapper(encrypted: &[u8]) -> Vec<u8> {
    let size = encrypted.len();
    let mut out = vec![0u8; size];

    // Pass A (inverse of pass C in encrypt): 16-byte block un-shuffle.
    let mut offset = 0;
    while offset + KEY1.len() <= size {
        for idx in 0..KEY1.len() {
            out[offset + idx] = encrypted[offset + KEY1[idx]];
        }
        offset += KEY1.len();
    }
    // Tail that doesn't fit a full 16-byte block is copied verbatim.
    out[offset..size].copy_from_slice(&encrypted[offset..size]);

    // Pass B (self-inverse): swap pairs from the middle outward.
    // `a` starts at 1, `b` at size-1, both move in +2 / -2 steps, for
    // `(size/2 + 1)/2` iterations. Same count as encrypt, same effect
    // (swap of a swap is identity).
    let mut a = 1usize;
    let mut b = size.saturating_sub(1);
    let iters = (size / 2).div_ceil(2);
    for _ in 0..iters {
        if a < size && b < size && a != b {
            out.swap(a, b);
        }
        a = a.saturating_add(2);
        b = b.saturating_sub(2);
    }

    // Pass C (self-inverse): XOR with the repeating 21-byte key.
    for i in 0..size {
        out[i] ^= KEY2[i % KEY2.len()];
    }

    out
}

/// Inverse of `DecryptMapper` (TMM/Model/CompositeMapper.cpp:49).
/// Order: XOR with Key2, swap pairs, 16-byte shuffle.
pub fn encrypt_mapper(decrypted: &[u8]) -> Vec<u8> {
    let size = decrypted.len();
    let mut out = vec![0u8; size];

    // XOR first.
    for i in 0..size {
        out[i] = decrypted[i] ^ KEY2[i % KEY2.len()];
    }

    // Pair swap (same primitive as decrypt — it's self-inverse).
    let mut a = 1usize;
    let mut b = size.saturating_sub(1);
    let iters = (size / 2).div_ceil(2);
    for _ in 0..iters {
        if a < size && b < size && a != b {
            out.swap(a, b);
        }
        a = a.saturating_add(2);
        b = b.saturating_sub(2);
    }

    // 16-byte shuffle (forward this time): tmp = block; block[KEY1[i]] = tmp[i]
    let mut offset = 0;
    while offset + KEY1.len() <= size {
        let tmp: [u8; 16] = out[offset..offset + KEY1.len()].try_into().unwrap();
        for idx in 0..KEY1.len() {
            out[offset + idx] = tmp[KEY1[idx]];
        }
        offset += KEY1.len();
    }
    out
}

// --- Mapper text format -----------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MapperEntry {
    pub filename: String,
    pub object_path: String,
    pub composite_name: String,
    pub offset: i64,
    pub size: i64,
}

/// Parses the decrypted mapper text into a composite-name-keyed map.
///
/// Format (mirror of SerializeCompositeMapFromString, TMM/Model/CompositeMapper.cpp:218):
/// ```text
/// <filename>?<objectPath>,<compositeName>,<offset>,<size>,|<objectPath>,...|!...
/// ```
/// Each `?...!` block is one composite filename. Each `|`-separated cell
/// inside is one (objectPath, compositeName, offset, size) entry.
pub fn parse_mapper(decrypted: &str) -> HashMap<String, MapperEntry> {
    parse_mapper_impl(decrypted, false).unwrap_or_default()
}

pub fn parse_mapper_strict(decrypted: &str) -> Result<HashMap<String, MapperEntry>, String> {
    parse_mapper_impl(decrypted, true)
}

fn parse_mapper_impl(
    decrypted: &str,
    strict_numbers: bool,
) -> Result<HashMap<String, MapperEntry>, String> {
    let mut out = HashMap::new();
    let bytes = decrypted.as_bytes();
    let mut pos_end = 0usize;

    loop {
        let Some(q) = find_from(bytes, b'?', pos_end) else {
            break;
        };
        let filename = decrypted[pos_end..q].to_string();
        let Some(bang) = find_from(bytes, b'!', q) else {
            break;
        };
        let mut pos = q + 1;
        while pos < bang {
            // object_path
            let Some(c1) = find_from(bytes, b',', pos) else {
                break;
            };
            let object_path = &decrypted[pos..c1];
            pos = c1 + 1;
            // composite_name
            let Some(c2) = find_from(bytes, b',', pos) else {
                break;
            };
            let composite_name = &decrypted[pos..c2];
            pos = c2 + 1;
            // offset
            let Some(c3) = find_from(bytes, b',', pos) else {
                break;
            };
            let offset_s = &decrypted[pos..c3];
            pos = c3 + 1;
            // size
            let Some(c4) = find_from(bytes, b',', pos) else {
                break;
            };
            let size_s = &decrypted[pos..c4];
            pos = c4 + 1;

            let offset = parse_mapper_i64(offset_s, "offset", composite_name, strict_numbers)?;
            let size = parse_mapper_i64(size_s, "size", composite_name, strict_numbers)?;

            let entry = MapperEntry {
                filename: filename.clone(),
                object_path: object_path.to_string(),
                composite_name: composite_name.to_string(),
                offset,
                size,
            };
            out.insert(entry.composite_name.clone(), entry);

            // Next entry (TMM uses `decrypted.find('|', pos) < posEnd - 1` as
            // the loop condition; we simply check for '|' before the bang).
            if pos >= bang {
                break;
            }
            if bytes[pos] == b'|' {
                pos += 1;
            }
            if pos >= bang
                || find_from(bytes, b'|', pos)
                    .map(|p| p < bang)
                    .unwrap_or(false)
            {
                // continue loop — there's another entry before the '!'
            } else {
                break;
            }
        }
        pos_end = bang + 1;
    }
    Ok(out)
}

fn parse_mapper_i64(
    value: &str,
    field: &str,
    composite_name: &str,
    strict: bool,
) -> Result<i64, String> {
    match value.parse::<i64>() {
        Ok(parsed) if !strict || parsed >= 0 => Ok(parsed),
        Ok(parsed) => Err(format!(
            "invalid {field} '{parsed}' for composite '{composite_name}'"
        )),
        Err(_) if strict => Err(format!(
            "invalid {field} '{value}' for composite '{composite_name}'"
        )),
        Err(_) => Ok(0),
    }
}

fn find_from(hay: &[u8], needle: u8, from: usize) -> Option<usize> {
    hay.get(from..)?
        .iter()
        .position(|&b| b == needle)
        .map(|p| p + from)
}

/// Inverse of `parse_mapper`. Groups entries by Filename (as TMM does via
/// the reverse map in SerializeCompositeMapToString, line 259).
pub fn serialize_mapper(map: &HashMap<String, MapperEntry>) -> String {
    let mut grouped: HashMap<String, Vec<&MapperEntry>> = HashMap::new();
    for e in map.values() {
        grouped.entry(e.filename.clone()).or_default().push(e);
    }
    // Sort filenames so output is deterministic (TMM uses std::map which is
    // ordered; we mimic that so diffs stay reviewable).
    let mut names: Vec<&String> = grouped.keys().collect();
    names.sort();

    let mut out = String::new();
    for name in names {
        let entries = grouped.get(name).unwrap();
        if entries.is_empty() {
            continue;
        }
        out.push_str(name);
        out.push('?');
        // TMM's inner order is by composite_name because the source map is
        // std::map<composite_name, entry>; sort the same way.
        let mut sorted: Vec<&&MapperEntry> = entries.iter().collect();
        sorted.sort_by(|a, b| a.composite_name.cmp(&b.composite_name));
        for e in sorted {
            out.push_str(&e.object_path);
            out.push(',');
            out.push_str(&e.composite_name);
            out.push(',');
            out.push_str(&e.offset.to_string());
            out.push(',');
            out.push_str(&e.size.to_string());
            out.push(',');
            out.push('|');
        }
        out.push('!');
    }
    out
}

// --- Incomplete-path match (TMM/Utils.cpp:9) --------------------------------
//
// "S1UI_PartyWindow.S1UI_PartyWindow" strips to (composite="S1UI_PartyWindow",
// path="S1UI_PartyWindow") by taking everything after the first `.` as the
// object-sub-path and everything before the last `_` before `.` as the
// composite prefix. Two paths match if both halves match.

fn split_incomplete(path: &str) -> Option<(&str, &str)> {
    let dot = path.find('.')?;
    let obj = &path[dot + 1..];
    let before_dot = &path[..dot];
    let underscore = before_dot.rfind('_')?;
    let composite = &path[..underscore];
    Some((composite, obj))
}

pub fn incomplete_paths_equal(a: &str, b: &str) -> bool {
    match (split_incomplete(a), split_incomplete(b)) {
        (Some((ca, oa)), Some((cb, ob))) => {
            ca.eq_ignore_ascii_case(cb) && oa.eq_ignore_ascii_case(ob)
        }
        _ => false,
    }
}

pub fn get_entry_by_object_path<'a>(
    map: &'a HashMap<String, MapperEntry>,
    path: &str,
) -> Option<&'a MapperEntry> {
    map.values()
        .find(|e| e.object_path.eq_ignore_ascii_case(path))
}

pub fn get_entry_by_incomplete_object_path<'a>(
    map: &'a HashMap<String, MapperEntry>,
    path: &str,
) -> Option<&'a MapperEntry> {
    map.values()
        .find(|e| incomplete_paths_equal(&e.object_path, path))
}

// --- Mapper patch application (PRD 3.3.2) ----------------------------------

/// Applies `incoming`'s packages to `map` in-place. For each package:
/// look up the matching entry by object_path (region_lock picks exact vs.
/// incomplete match), repoint `(filename, offset, size)` to the incoming
/// mod's container, and keep `(composite_name, object_path)` so the game
/// still resolves via the same lookup keys.
///
/// PRD 3.3.2: per-object merge. Two mods patching different composites
/// (or different objects sharing a composite via incomplete-path matching)
/// both apply — the prior entries for objects the incoming mod doesn't
/// touch stay intact. Two mods patching the *same* (composite, object)
/// is a last-install-wins overwrite, surfaced separately by
/// `detect_conflicts` so the user can approve before this runs.
///
/// Err if any requested object_path has no match in the current mapper —
/// typically a game-version skew; the install aborts before writing.
pub fn apply_mod_patches(
    map: &mut HashMap<String, MapperEntry>,
    incoming: &ModFile,
) -> Result<(), String> {
    for pkg in &incoming.packages {
        let existing = if incoming.region_lock {
            get_entry_by_object_path(map, &pkg.object_path)
        } else {
            get_entry_by_incomplete_object_path(map, &pkg.object_path)
        };
        let mut entry = match existing {
            Some(e) => e.clone(),
            None => {
                return Err(format!(
                    "Composite entry for '{}' not found in mapper. Your game version may not match the mod.",
                    pkg.object_path
                ));
            }
        };
        entry.filename = incoming.container.clone();
        entry.offset = pkg.offset;
        entry.size = pkg.size;
        map.insert(entry.composite_name.clone(), entry);
    }
    Ok(())
}

// --- Install-time conflict detection (PRD 3.3.3) ---------------------------

/// One `(composite, object)` slot that's already been patched by a different
/// mod than the one about to install. The user should confirm before
/// overwriting — last-install-wins is our semantic, but opaque.
///
/// Wired via `commands::mods::preview_mod_install_conflicts`
/// (fix.conflict-modal-wiring landed iter 76). Serializable so Tauri
/// can hand it straight to the frontend modal.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModConflict {
    pub composite_name: String,
    pub object_path: String,
    /// The container filename the existing mapper entry points at. Neither
    /// the vanilla file nor the incoming mod's container.
    pub previous_filename: String,
}

/// Inspects the current mapper vs the vanilla mapper (from `.clean`) and the
/// incoming `modfile`, and returns one `ModConflict` per slot whose current
/// filename is neither the vanilla nor the incoming mod's container.
///
/// Three cases:
/// - `current == vanilla` → no mod patched this yet → no conflict.
/// - `current == incoming.container` → re-install of the same mod → no conflict.
/// - otherwise → a *different* mod owns this slot → conflict.
///
/// Matches the lookup semantic `install_gpk` uses (region_lock gates
/// exact-path vs. incomplete-path equality).
pub fn detect_conflicts(
    vanilla_map: &HashMap<String, MapperEntry>,
    current_map: &HashMap<String, MapperEntry>,
    incoming: &ModFile,
) -> Vec<ModConflict> {
    let mut conflicts = Vec::new();

    for pkg in &incoming.packages {
        let current = if incoming.region_lock {
            get_entry_by_object_path(current_map, &pkg.object_path)
        } else {
            get_entry_by_incomplete_object_path(current_map, &pkg.object_path)
        };
        let current = match current {
            Some(e) => e,
            None => continue, // slot doesn't exist — install_gpk will raise
        };

        let vanilla = if incoming.region_lock {
            get_entry_by_object_path(vanilla_map, &pkg.object_path)
        } else {
            get_entry_by_incomplete_object_path(vanilla_map, &pkg.object_path)
        };

        let is_vanilla_unchanged = vanilla
            .map(|v| v.filename.eq_ignore_ascii_case(&current.filename))
            .unwrap_or(false);
        let is_self_reinstall = current.filename.eq_ignore_ascii_case(&incoming.container);

        if !is_vanilla_unchanged && !is_self_reinstall {
            conflicts.push(ModConflict {
                composite_name: current.composite_name.clone(),
                object_path: current.object_path.clone(),
                previous_filename: current.filename.clone(),
            });
        }
    }

    conflicts
}

/// Call-site bundle for `detect_conflicts`. Reads + decrypts + parses the
/// vanilla `.clean` backup and the current `CompositePackageMapper.dat`,
/// parses the incoming mod file from its on-disk bytes, and runs
/// `detect_conflicts`. The Tauri command layer (`commands::mods::
/// preview_mod_install_conflicts`) is a thin wrapper around this — all
/// the fs + decrypt + parse error paths live here so the command body
/// stays one unit under `#[cfg(not(tarpaulin_include))]`.
///
/// If `.clean` is missing we return an empty vec rather than erroring —
/// without a vanilla baseline we can't prove a slot is dirty, and a
/// silent no-op is the safer default for a preview. Install paths still
/// refuse outright (see `ensure_backup` + `install_gpk`).
pub fn preview_conflicts_from_bytes(
    game_root: &Path,
    source_gpk_bytes: &[u8],
) -> Result<Vec<ModConflict>, String> {
    let backup = backup_path(game_root);
    if !backup.exists() {
        return Ok(Vec::new());
    }
    let vanilla_bytes = fs::read(&backup).map_err(|e| format!("Failed to read backup: {}", e))?;
    let vanilla_dec = decrypt_mapper(&vanilla_bytes);
    let vanilla_map = parse_mapper(&String::from_utf8_lossy(&vanilla_dec));

    let mapper = mapper_path(game_root);
    if !mapper.exists() {
        return Err(format!(
            "CompositePackageMapper.dat not found at {}",
            mapper.display()
        ));
    }
    let current_bytes = fs::read(&mapper).map_err(|e| format!("Failed to read mapper: {}", e))?;
    let current_dec = decrypt_mapper(&current_bytes);
    let current_map = parse_mapper(&String::from_utf8_lossy(&current_dec));

    let modfile = parse_mod_file(source_gpk_bytes)?;
    Ok(detect_conflicts(&vanilla_map, &current_map, &modfile))
}

// --- Mod file (.gpk with TMM metadata) reader -------------------------------

#[derive(Debug, Clone, Default)]
pub struct ModPackage {
    pub object_path: String,
    pub offset: i64,
    pub size: i64,
    // Parsed from the TMM footer for format parity; not consumed by
    // the current deployer but preserved to keep round-trip fidelity.
    #[allow(dead_code)]
    pub file_version: u16,
    #[allow(dead_code)]
    pub licensee_version: u16,
}

#[derive(Debug, Clone, Default)]
pub struct ModFile {
    pub mod_name: String,
    pub mod_author: String,
    pub container: String,
    pub region_lock: bool,
    pub mod_file_version: i32,
    pub packages: Vec<ModPackage>,
}

/// Mirrors `operator>>` for `ModFile` in TMM/Model/Mod.cpp:55.
///
/// Reads the metadata footer from the END of the file, hop-skipping
/// backwards through int32 slots. If the magic is missing we return a
/// one-package ModFile with just the raw container size — TMM treats
/// those as legacy mods too.
pub fn parse_mod_file(bytes: &[u8]) -> Result<ModFile, String> {
    let end = bytes.len();
    if end < 4 {
        return Err("Mod file is too small to contain metadata".into());
    }

    let mut m = ModFile {
        mod_file_version: 1,
        ..Default::default()
    };

    let magic_off = end - 4;
    let magic = read_u32_le(bytes, magic_off);
    if magic != PACKAGE_MAGIC {
        // No metadata — treat the whole file as a single unknown package.
        // Upstream leaves ObjectPath empty; without it the installer won't
        // know which composite to override.
        m.packages.push(ModPackage {
            size: end as i64,
            ..Default::default()
        });
        return Ok(m);
    }

    // Step backwards through the footer reading i32 slots, starting just
    // before the magic.
    let mut pos = end - 4;
    let read_back_i32 = |p: &mut usize| -> Result<i32, String> {
        if *p < 4 {
            return Err("Unexpected EOF while reading mod footer".into());
        }
        *p -= 4;
        Ok(read_i32_le(bytes, *p))
    };

    let meta_size = read_back_i32(&mut pos)?;
    let composite_count = read_back_i32(&mut pos)?;
    let offsets_offset = read_back_i32(&mut pos)?;
    let container_offset = read_back_i32(&mut pos)?;
    let name_offset = read_back_i32(&mut pos)?;
    let author_offset = read_back_i32(&mut pos)?;
    let region_lock = read_back_i32(&mut pos)?;
    let version = read_back_i32(&mut pos)?;
    m.region_lock = region_lock != 0;

    let mut composite_end: i32 = 0;
    if (version as u32) != PACKAGE_MAGIC {
        // v2+ format: extra fields for TFC offsets. We don't install TFCs
        // yet, but we still consume them to seek past them.
        m.mod_file_version = version;
        composite_end = read_back_i32(&mut pos)?;
        let _tfc_offsets_count = read_back_i32(&mut pos)?;
        let _tfc_offsets_offset = read_back_i32(&mut pos)?;
        let _tfc_end = read_back_i32(&mut pos)?;
    }

    // Read strings at their absolute offsets.
    m.mod_author = read_prefixed_string(bytes, author_offset as usize)?;
    m.mod_name = read_prefixed_string(bytes, name_offset as usize)?;
    m.container = read_prefixed_string(bytes, container_offset as usize)?;

    // Read the table of composite-package offsets, then each package.
    let oo = offsets_offset as usize;
    if composite_count < 0 || oo + (composite_count as usize) * 4 > end {
        return Err("Mod footer references offsets past EOF".into());
    }
    let mut offsets = Vec::with_capacity(composite_count as usize);
    for i in 0..composite_count as usize {
        offsets.push(read_i32_le(bytes, oo + i * 4) as usize);
    }

    m.packages
        .resize(composite_count as usize, ModPackage::default());
    for (i, &pkg_off) in offsets.iter().enumerate() {
        let p = parse_composite_package(bytes, pkg_off)?;
        m.packages[i] = p;
        if i > 0 {
            // Size of the PREVIOUS package is (this package's offset) minus its own offset.
            m.packages[i - 1].size = (pkg_off as i64) - m.packages[i - 1].offset;
        }
    }
    // Last package's size ends at composite_end if set, otherwise the
    // start of the metadata footer.
    if let Some(last) = m.packages.last_mut() {
        let end_i64 = if composite_end != 0 {
            composite_end as i64
        } else {
            end as i64 - meta_size as i64
        };
        last.size = end_i64 - last.offset;
    }
    Ok(m)
}

fn parse_composite_package(bytes: &[u8], off: usize) -> Result<ModPackage, String> {
    if off + 12 > bytes.len() {
        return Err("Composite package offset past EOF".into());
    }
    // TMM/Model/Mod.cpp:212 — p.Offset = tellg() at entry (the file offset of
    // the package itself, used later to rewrite the mapper entry).
    let mut p = ModPackage {
        offset: off as i64,
        file_version: read_u16_le(bytes, off + 4),
        licensee_version: read_u16_le(bytes, off + 6),
        ..Default::default()
    };
    let folder = read_prefixed_string(bytes, off + 12)?;
    let folder = folder.trim_end_matches('\0');
    if let Some(stripped) = folder.strip_prefix(MOD_PREFIX) {
        p.object_path = stripped.to_string();
    }
    Ok(p)
}

/// TMM's length-prefixed string (Mod.cpp:9). Positive length = ANSI,
/// negative = UTF-16LE with |length| code units.
fn read_prefixed_string(bytes: &[u8], off: usize) -> Result<String, String> {
    if off + 4 > bytes.len() {
        return Err("string header past EOF".into());
    }
    let size = read_i32_le(bytes, off);
    const MAX: i32 = 1024;
    if size == 0 {
        return Ok(String::new());
    }
    if size.abs() > MAX {
        return Ok(String::new()); // matches TMM's bail-out
    }
    if size > 0 {
        let s = size as usize;
        if off + 4 + s > bytes.len() {
            return Err("ANSI string past EOF".into());
        }
        Ok(String::from_utf8_lossy(&bytes[off + 4..off + 4 + s]).to_string())
    } else {
        let count = (-size) as usize;
        let byte_len = count * 2;
        if off + 4 + byte_len > bytes.len() {
            return Err("UTF-16 string past EOF".into());
        }
        let mut buf = Vec::with_capacity(count);
        for i in 0..count {
            buf.push(read_u16_le(bytes, off + 4 + i * 2));
        }
        Ok(String::from_utf16_lossy(&buf))
    }
}

// Little-endian read helpers.
fn read_i32_le(b: &[u8], off: usize) -> i32 {
    i32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
fn read_u32_le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
fn read_u16_le(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

// --- Install / uninstall ----------------------------------------------------

/// Relative path under the game root where TERA's CookedPC folder lives.
pub const COOKED_PC_DIR: &str = "S1Game/CookedPC";
pub const MAPPER_FILE: &str = "CompositePackageMapper.dat";
pub const BACKUP_FILE: &str = "CompositePackageMapper.clean";
pub const PKG_MAPPER_FILE: &str = "PkgMapper.dat";
pub const PKG_MAPPER_BACKUP_FILE: &str = "PkgMapper.clean";

fn mapper_path(game_root: &Path) -> PathBuf {
    game_root.join(COOKED_PC_DIR).join(MAPPER_FILE)
}

fn backup_path(game_root: &Path) -> PathBuf {
    game_root.join(COOKED_PC_DIR).join(BACKUP_FILE)
}

fn pkg_mapper_path(game_root: &Path) -> PathBuf {
    game_root.join(COOKED_PC_DIR).join(PKG_MAPPER_FILE)
}

fn pkg_mapper_backup_path(game_root: &Path) -> PathBuf {
    game_root.join(COOKED_PC_DIR).join(PKG_MAPPER_BACKUP_FILE)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PkgMapperEntry {
    uid: String,
    composite_uid: String,
}

#[derive(Debug, Clone)]
pub struct RebuildItem {
    pub source_gpk: PathBuf,
    pub enabled: bool,
    pub deployed_filename: Option<String>,
}

fn parse_pkg_mapper(decrypted: &str) -> Vec<PkgMapperEntry> {
    decrypted
        .split('|')
        .filter_map(|entry| {
            let trimmed = entry.trim();
            if trimmed.is_empty() {
                return None;
            }
            let mut parts = trimmed.splitn(2, ',');
            let uid = parts.next()?.trim();
            let composite_uid = parts.next()?.trim();
            if uid.is_empty() || composite_uid.is_empty() {
                return None;
            }
            Some(PkgMapperEntry {
                uid: uid.to_string(),
                composite_uid: composite_uid.to_string(),
            })
        })
        .collect()
}

fn serialize_pkg_mapper(entries: &[PkgMapperEntry]) -> String {
    let mut out = String::new();
    for entry in entries {
        out.push_str(&entry.uid);
        out.push(',');
        out.push_str(&entry.composite_uid);
        out.push('|');
    }
    out
}

fn ensure_pkg_mapper_backup(game_root: &Path) -> Result<(), String> {
    let src = pkg_mapper_path(game_root);
    let dst = pkg_mapper_backup_path(game_root);
    if !src.exists() {
        return Ok(());
    }
    if dst.exists() {
        return Ok(());
    }
    fs::copy(&src, &dst).map_err(|e| format!("Failed to back up PkgMapper.dat: {}", e))?;
    Ok(())
}

pub fn restore_clean_mapper_state(game_root: &Path) -> Result<(), String> {
    let clean = backup_path(game_root);
    let current = mapper_path(game_root);
    if clean.exists() {
        let bytes = fs::read(&clean).map_err(|e| {
            format!(
                "Failed to read clean CompositePackageMapper.dat backup {}: {}",
                clean.display(),
                e
            )
        })?;
        write_atomic_file(&current, &bytes).map_err(|e| {
            format!(
                "Failed to restore CompositePackageMapper.dat from clean backup: {}",
                e
            )
        })?;
    } else if current.exists() {
        ensure_backup(game_root)?;
    }
    Ok(())
}

fn restore_clean_pkg_mapper_state(game_root: &Path) -> Result<(), String> {
    let clean = pkg_mapper_backup_path(game_root);
    let current = pkg_mapper_path(game_root);
    if clean.exists() {
        copy_atomic(&clean, &current)
            .map_err(|e| format!("Failed to restore PkgMapper.dat from clean backup: {}", e))?;
    } else if current.exists() {
        ensure_pkg_mapper_backup(game_root)?;
    }
    Ok(())
}

pub fn restore_clean_gpk_state(game_root: &Path) -> Result<(), String> {
    restore_vanilla_gpk_backups(game_root)?;
    restore_clean_mapper_state(game_root)?;
    restore_clean_pkg_mapper_state(game_root)?;
    Ok(())
}

fn restore_vanilla_gpk_backups(game_root: &Path) -> Result<usize, String> {
    let cooked_pc = game_root.join(COOKED_PC_DIR);
    if !cooked_pc.exists() {
        return Ok(0);
    }

    restore_vanilla_gpk_backups_in_dir(&cooked_pc)
}

fn restore_vanilla_gpk_backups_in_dir(dir: &Path) -> Result<usize, String> {
    let mut restored = 0usize;
    let entries =
        fs::read_dir(dir).map_err(|e| format!("Failed to list GPK dir {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read GPK dir entry: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            restored += restore_vanilla_gpk_backups_in_dir(&path)?;
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(target_name) = file_name.strip_suffix(".vanilla-bak") else {
            continue;
        };
        if !target_name.ends_with(".gpk") || !is_safe_gpk_container_filename(target_name) {
            continue;
        }
        let target_path = path
            .parent()
            .ok_or_else(|| format!("Backup path has no parent: {}", path.display()))?
            .join(target_name);
        copy_atomic(&path, &target_path).map_err(|e| {
            format!(
                "Failed to restore vanilla GPK {} from {}: {e}",
                target_path.display(),
                path.display()
            )
        })?;
        restored += 1;
    }
    Ok(restored)
}

pub(crate) fn copy_atomic(src: &Path, dst: &Path) -> Result<(), String> {
    let bytes = fs::read(src).map_err(|e| format!("Failed to read {}: {e}", src.display()))?;
    write_atomic_file(dst, &bytes)
}

pub(crate) fn write_atomic_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| format!("Failed to write tmp {}: {e}", tmp.display()))?;
    replace_file(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!(
            "Failed to commit tmp {} to {}: {e}",
            tmp.display(),
            path.display()
        )
    })
}

#[cfg(windows)]
fn replace_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::winbase::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};

    let src_wide: Vec<u16> = src.as_os_str().encode_wide().chain(Some(0)).collect();
    let dst_wide: Vec<u16> = dst.as_os_str().encode_wide().chain(Some(0)).collect();
    let ok = unsafe {
        MoveFileExW(
            src_wide.as_ptr(),
            dst_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::rename(src, dst)
}

pub fn rebuild_gpk_state(game_root: &Path, items: &[RebuildItem]) -> Result<(), String> {
    restore_clean_gpk_state(game_root)?;

    // First clean up every launcher-managed legacy drop-in so reapplication
    // starts from a vanilla baseline. Composite mods are safe to leave on disk
    // until their mapper rebuild repoints away from them, but remove them too
    // so stale containers don't accumulate.
    for item in items {
        if let Some(target_filename) = item.deployed_filename.as_deref() {
            let _ = uninstall_legacy_gpk(game_root, target_filename);
        }

        if item.source_gpk.exists() {
            if let Ok(bytes) = fs::read(&item.source_gpk) {
                if let Ok(modfile) = parse_mod_file(&bytes) {
                    if !modfile.container.trim().is_empty() {
                        let dest = game_root.join(COOKED_PC_DIR).join(&modfile.container);
                        if dest.exists() {
                            let _ = fs::remove_file(dest);
                        }
                    }
                }
            }
        }
    }

    // Reapply only active mods from a clean baseline.
    for item in items.iter().filter(|item| item.enabled) {
        if !item.source_gpk.exists() {
            return Err(format!(
                "Enabled GPK source file is missing: {}",
                item.source_gpk.display()
            ));
        }

        let bytes = fs::read(&item.source_gpk)
            .map_err(|e| format!("Failed to read {}: {}", item.source_gpk.display(), e))?;
        let parsed = parse_mod_file(&bytes);

        match parsed {
            Ok(modfile) if !modfile.container.trim().is_empty() && !modfile.packages.is_empty() => {
                install_gpk(game_root, &item.source_gpk).map_err(|e| {
                    format!(
                        "Failed to rebuild composite GPK state for {}: {}",
                        item.source_gpk.display(),
                        e
                    )
                })?;
            }
            _ => {
                install_legacy_gpk(
                    game_root,
                    &item.source_gpk,
                    item.deployed_filename.as_deref(),
                )
                .map_err(|e| {
                    format!(
                        "Failed to rebuild legacy GPK state for {}: {}",
                        item.source_gpk.display(),
                        e
                    )
                })?;
            }
        }
    }

    Ok(())
}

fn patch_pkg_mapper_for_standalone_gpk(
    game_root: &Path,
    folder_name: &str,
) -> Result<usize, String> {
    let path = pkg_mapper_path(game_root);
    if !path.exists() {
        return Ok(0);
    }

    ensure_pkg_mapper_backup(game_root)?;

    let encrypted = fs::read(&path).map_err(|e| format!("Failed to read PkgMapper.dat: {}", e))?;
    let decrypted = decrypt_mapper(&encrypted);
    let text = String::from_utf8_lossy(&decrypted).to_string();
    let mut entries = parse_pkg_mapper(&text);
    let before = entries.len();
    let prefix = format!("{folder_name}.");
    entries.retain(|entry| !entry.uid.starts_with(&prefix));
    let removed = before.saturating_sub(entries.len());

    if removed > 0 {
        let serialized = serialize_pkg_mapper(&entries);
        let reencrypted = encrypt_mapper(serialized.as_bytes());
        fs::write(&path, &reencrypted)
            .map_err(|e| format!("Failed to write patched PkgMapper.dat: {}", e))?;
    }

    Ok(removed)
}

fn restore_pkg_mapper_for_standalone_gpk(
    game_root: &Path,
    folder_name: &str,
) -> Result<usize, String> {
    let current_path = pkg_mapper_path(game_root);
    let backup_path = pkg_mapper_backup_path(game_root);
    if !current_path.exists() || !backup_path.exists() {
        return Ok(0);
    }

    let current_encrypted =
        fs::read(&current_path).map_err(|e| format!("Failed to read PkgMapper.dat: {}", e))?;
    let backup_encrypted =
        fs::read(&backup_path).map_err(|e| format!("Failed to read PkgMapper.clean: {}", e))?;
    let current_text = String::from_utf8_lossy(&decrypt_mapper(&current_encrypted)).to_string();
    let backup_text = String::from_utf8_lossy(&decrypt_mapper(&backup_encrypted)).to_string();
    let mut current_entries = parse_pkg_mapper(&current_text);
    let backup_entries = parse_pkg_mapper(&backup_text);
    let prefix = format!("{folder_name}.");

    let mut restored = 0usize;
    for entry in backup_entries
        .into_iter()
        .filter(|entry| entry.uid.starts_with(&prefix))
    {
        if current_entries
            .iter()
            .any(|existing| existing.uid.eq_ignore_ascii_case(&entry.uid))
        {
            continue;
        }
        current_entries.push(entry);
        restored += 1;
    }

    if restored > 0 {
        let serialized = serialize_pkg_mapper(&current_entries);
        let reencrypted = encrypt_mapper(serialized.as_bytes());
        fs::write(&current_path, &reencrypted)
            .map_err(|e| format!("Failed to restore PkgMapper.dat: {}", e))?;
    }

    Ok(restored)
}

/// Recovers a missing `.clean` backup. Intended as a user-triggered one-shot
/// for the rare case where `.clean` was deleted while no mods were installed.
///
/// PRD 3.2.9.clean-recovery-logic. Three branches:
///   - `.clean` already exists → no-op (safe to call speculatively).
///   - `.clean` missing, current mapper has no `TMM_MARKER` entry →
///     treat current as vanilla and copy it to `.clean`. Relies on the TMM
///     convention that any mapper touched by a TMM-style installer carries
///     the marker; absent → mods weren't installed via TMM.
///   - `.clean` missing, current mapper has `TMM_MARKER` → refuse. We'd be
///     capturing already-modded bytes as the "vanilla" baseline, which
///     silently breaks uninstall forever. Err message tells the user to
///     run Steam / their launcher's "verify game files" to restore
///     `CompositePackageMapper.dat` first.
pub fn recover_missing_clean(game_root: &Path) -> Result<(), String> {
    let src = mapper_path(game_root);
    let dst = backup_path(game_root);

    if dst.exists() {
        return Ok(());
    }
    if !src.exists() {
        return Err(format!(
            "CompositePackageMapper.dat not found at {}. Verify game files.",
            src.display()
        ));
    }

    let current_bytes = fs::read(&src).map_err(|e| format!("Failed to read mapper: {}", e))?;
    let decrypted = decrypt_mapper(&current_bytes);
    let decrypted_str = String::from_utf8_lossy(&decrypted).to_string();
    let map = parse_mapper(&decrypted_str);

    if map.contains_key(TMM_MARKER) {
        return Err(
            "Cannot recover .clean: the current CompositePackageMapper.dat \
             has mod entries (patcher marker present). Run Steam / the launcher's \
             \"verify game files\" to restore the vanilla mapper, then retry."
                .into(),
        );
    }

    fs::copy(&src, &dst).map_err(|e| format!("Failed to back up mapper: {}", e))?;
    Ok(())
}

/// Copies the vanilla mapper to `.clean` on first touch. Safe to call on
/// every install — it's a no-op once the backup exists.
pub fn ensure_backup(game_root: &Path) -> Result<(), String> {
    let src = mapper_path(game_root);
    let dst = backup_path(game_root);
    if !src.exists() {
        return Err(format!(
            "CompositePackageMapper.dat not found at {}",
            src.display()
        ));
    }
    if dst.exists() {
        return Ok(());
    }
    fs::copy(&src, &dst).map_err(|e| format!("Failed to back up mapper: {}", e))?;
    Ok(())
}

/// Validates that a TMM container filename from an untrusted `.gpk` is a
/// plain leaf name (no separators, no parent traversal, no drive letters,
/// no null bytes, not dot-only). Real TMM containers are flat filenames
/// like `S1Data_2.gpk`; anything else is either malformed or hostile.
pub fn is_safe_gpk_container_filename(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if name.contains('/') || name.contains('\\') {
        return false;
    }
    if name.contains('\0') {
        return false;
    }
    if name == "." || name == ".." {
        return false;
    }
    // Reject any literal `..` component (even embedded, e.g. "foo..bar")
    // just in case some platform normalises it.
    if name.contains("..") {
        return false;
    }
    // Windows drive-letter prefix (e.g. "C:foo").
    let bytes = name.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        return false;
    }
    true
}

/// Reads the UE3 package FolderName (a.k.a. PackageName) from a .gpk
/// header. Used as the drop-in filename for legacy mods that lack a
/// TMM footer. Returns None if the header is malformed or non-UE3.
///
/// Layout per TeraCoreLib FStructs.cpp `operator<<(FStream&, FPackageSummary&)`:
/// - offset 0: u32 magic = 0x9E2A83C1
/// - offset 4: u32 FileVersion
/// - offset 8: i32 HeaderSize
/// - offset 12: FString FolderName (i32 len + bytes; +ASCII / -UTF16, incl null)
pub fn extract_package_folder_name(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 16 {
        return None;
    }
    let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if magic != PACKAGE_MAGIC {
        return None;
    }
    let len = i32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    if len == 0 {
        return None;
    }
    if len > 0 {
        // ASCII: `len` chars including trailing null. Sanity-cap to 256.
        let n = len as usize;
        if n > 256 || 16 + n > bytes.len() {
            return None;
        }
        let slice = &bytes[16..16 + n];
        // Drop trailing null(s).
        let trimmed: &[u8] = if slice.last() == Some(&0) {
            &slice[..slice.len() - 1]
        } else {
            slice
        };
        let s = std::str::from_utf8(trimmed).ok()?.to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    } else {
        // UTF-16 LE: (-len) chars including trailing null. Sanity-cap to 256.
        let n = (-len) as usize;
        if n > 256 || 16 + n * 2 > bytes.len() {
            return None;
        }
        let mut u16s = Vec::with_capacity(n);
        for i in 0..n {
            u16s.push(u16::from_le_bytes([
                bytes[16 + i * 2],
                bytes[16 + i * 2 + 1],
            ]));
        }
        if u16s.last() == Some(&0) {
            u16s.pop();
        }
        let s = String::from_utf16(&u16s).ok()?;
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
}

fn is_placeholder_package_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none")
}

/// Resolves the game-facing filename for a legacy `.gpk` that lacks embedded
/// override metadata. Resolution priority:
///
///   1. UE3 package header `FolderName` (when present and not a placeholder).
///   2. `url_filename_hint` — the last path segment of the download URL
///      (e.g. `S1UI_ProgressBar.gpk` from `.../remove_FlightGauge/S1UI_ProgressBar.gpk`).
///      Catalog-installed mods are stored as `<catalog-id>.gpk`, so the
///      file stem is the opaque catalog ID, not the real game-package name.
///   3. Source filename stem (last resort for manually imported files).
pub fn resolve_legacy_target_filename(
    bytes: &[u8],
    source_gpk: &Path,
    url_filename_hint: Option<&str>,
) -> Option<String> {
    // 1. UE3 header FolderName
    if let Some(folder_name) = extract_package_folder_name(bytes) {
        if !is_placeholder_package_name(&folder_name) {
            return Some(format!("{folder_name}.gpk"));
        }
    }

    // 2. URL-derived filename — the download URL's last path segment is the
    //    real game-package name (e.g. S1UI_ProgressBar.gpk). This is more
    //    reliable than the file stem for catalog-installed mods, where the
    //    on-disk name is an opaque catalog ID like "foglio1024.ui-remover-flight-gauge".
    if let Some(hint) = url_filename_hint {
        let trimmed = hint.trim();
        if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("none") && trimmed.ends_with(".gpk")
        {
            return Some(trimmed.to_string());
        }
        // Also accept a bare folder name without the .gpk extension.
        if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("none") {
            return Some(format!("{trimmed}.gpk"));
        }
    }

    // 3. Source filename stem (matches toolbox-style removable mods where
    //    users manually named the file to match the game package).
    let stem = source_gpk.file_stem()?.to_str()?.trim();
    if stem.is_empty() || stem.eq_ignore_ascii_case("none") {
        return None;
    }
    Some(format!("{stem}.gpk"))
}

/// Drop-in install for legacy (non-TMM) .gpk mods.
///
/// TERA Classic+'s game engine loads GPKs through
/// `CompositePackageMapper.dat`, which maps composite UIDs to
/// `(filename, offset, length)` in the packaged container file.
/// Simply copying a .gpk into CookedPC/ is a no-op — the game still
/// loads the vanilla bytes because the mapper still points at the
/// vanilla file. To make a legacy (non-TMM) mod actually override
/// its target composite we must:
///   1. Read the PackageName from the mod's UE3 header.
///   2. Copy the file into CookedPC/ with that name + `.gpk`.
///   3. Patch every mapper entry whose ObjectPath ends in
///      `.<PackageName>` so it points at our new file at offset 0,
///      length = file size.
///
/// This mirrors what tera-toolbox's `installer.gpk(path)` does (see
/// TCC's `tcc-launcher.js::tryInstallRemover`). Every TMM-stamped
/// mod encodes this information in its footer; plain community mods
/// don't — we derive it from the UE3 header instead.
pub fn install_legacy_gpk(
    game_root: &Path,
    source_gpk: &Path,
    url_filename_hint: Option<&str>,
) -> Result<String, String> {
    let bytes = fs::read(source_gpk).map_err(|e| format!("Failed to read mod file: {}", e))?;
    let target_filename =
        resolve_legacy_target_filename(&bytes, source_gpk, url_filename_hint).ok_or_else(|| {
            "Mod file has no usable package name in its UE3 header or filename — can't map it to a game file."
                .to_string()
        })?;
    let folder_name = target_filename
        .strip_suffix(".gpk")
        .unwrap_or(&target_filename);

    // Sanity-gate the filename so a malformed header can't escape CookedPC.
    if !is_safe_gpk_container_filename(&target_filename) {
        return Err(format!(
            "Package name '{folder_name}' would produce an unsafe CookedPC filename — refusing to deploy."
        ));
    }

    let cooked_pc = game_root.join(COOKED_PC_DIR);
    fs::create_dir_all(&cooked_pc).map_err(|e| format!("Failed to create CookedPC dir: {}", e))?;
    let dest = cooked_pc.join(&target_filename);

    // Back up existing vanilla .gpk if present and no backup exists yet.
    let backup = cooked_pc.join(format!("{target_filename}.vanilla-bak"));
    if dest.exists() && !backup.exists() {
        fs::copy(&dest, &backup).map_err(|e| format!("Failed to back up vanilla .gpk: {}", e))?;
    }

    fs::copy(source_gpk, &dest)
        .map_err(|e| format!("Failed to install .gpk into CookedPC: {}", e))?;

    // CompositePackageMapper patch: redirect every composite whose ObjectPath
    // ends in `.<folder_name>` at our new file. This is what makes drop-in
    // composite overrides work in clients that still route the object through
    // CompositePackageMapper.dat.
    ensure_backup(game_root)?;
    let mapper_bytes = fs::read(mapper_path(game_root))
        .map_err(|e| format!("Failed to read mapper after install: {e}"))?;
    let decrypted = decrypt_mapper(&mapper_bytes);
    let decrypted_str = String::from_utf8_lossy(&decrypted).to_string();
    let mut map = parse_mapper(&decrypted_str);

    let file_size = fs::metadata(&dest)
        .map_err(|e| format!("Failed to stat installed .gpk: {e}"))?
        .len() as i64;
    let suffix = format!(".{folder_name}");

    let mut rewritten = 0usize;
    for entry in map.values_mut() {
        if entry.object_path.ends_with(&suffix) || entry.object_path == folder_name {
            entry.filename = target_filename.clone();
            entry.offset = 0;
            entry.size = file_size;
            rewritten += 1;
        }
    }

    if rewritten == 0 {
        // Classic standalone overrides are often controlled by PkgMapper.dat,
        // not CompositePackageMapper.dat. Removing `S1UI_ProgressBar.*` style
        // redirect entries lets the game fall back to the standalone GPK we
        // just copied into CookedPC.
        let removed_pkg_entries = patch_pkg_mapper_for_standalone_gpk(game_root, folder_name)?;
        if removed_pkg_entries > 0 {
            log::info!(
                "Installed {target_filename} as standalone and removed {removed_pkg_entries} PkgMapper entries for `{folder_name}`"
            );
        } else {
            log::info!("Installed {target_filename} as standalone (no composite or PkgMapper entry matched `{folder_name}`)");
        }
        return Ok(target_filename);
    }

    // Serialize + re-encrypt + write mapper back.
    let new_plain = serialize_mapper(&map);
    let new_encrypted = encrypt_mapper(new_plain.as_bytes());
    fs::write(mapper_path(game_root), &new_encrypted)
        .map_err(|e| format!("Failed to write patched mapper: {e}"))?;

    Ok(target_filename)
}

/// Restore mapper state to remove any leftover rows from prior
/// dropin+mapper_extend installs of `mod_id`. Idempotent — no-op if
/// no such rows exist.
///
/// PkgMapper rows whose right-hand side starts with `modres_<sanitized_mod_id>.`
/// are restored to their .clean baseline (or removed if the logical path
/// doesn't exist in .clean). CompositePackageMapper rows whose composite_name
/// equals `modres_<sanitized_mod_id>` are removed entirely.
pub fn clean_prior_dropin_state(game_root: &Path, mod_id: &str) -> Result<(), String> {
    let cooked = game_root.join(COOKED_PC_DIR);
    let sanitized: String = mod_id
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let composite_uid = format!("modres_{sanitized}");

    // Step 1: clean PkgMapper.dat — restore or remove rows pointing at our composite uid.
    let pm_path = cooked.join(PKG_MAPPER_FILE);
    let pm_clean_path = cooked.join(PKG_MAPPER_BACKUP_FILE);
    if pm_path.exists() && pm_clean_path.exists() {
        let pm_live_bytes =
            fs::read(&pm_path).map_err(|e| format!("read PkgMapper: {e}"))?;
        let pm_clean_bytes =
            fs::read(&pm_clean_path).map_err(|e| format!("read PkgMapper.clean: {e}"))?;
        let pm_live_str =
            String::from_utf8_lossy(&decrypt_mapper(&pm_live_bytes)).to_string();
        let pm_clean_str =
            String::from_utf8_lossy(&decrypt_mapper(&pm_clean_bytes)).to_string();

        let needle = format!(",{composite_uid}.");
        let mut rows: Vec<String> = pm_live_str
            .split('|')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        let mut pm_dirty = false;
        let mut new_rows: Vec<String> = Vec::with_capacity(rows.len());
        for row in rows.drain(..) {
            if !row.contains(&needle) {
                new_rows.push(row);
                continue;
            }
            // Row points at our modres_ composite uid. Restore from .clean.
            let logical = row.split(',').next().unwrap_or("").to_string();
            let key_prefix = format!("{logical},");
            let vanilla = pm_clean_str
                .split('|')
                .find(|r| r.starts_with(&key_prefix))
                .map(|s| s.to_string());
            if let Some(v) = vanilla {
                new_rows.push(v);
            }
            // else: logical path has no clean baseline — drop it entirely.
            pm_dirty = true;
        }

        if pm_dirty {
            let mut new_text = String::with_capacity(pm_live_str.len());
            for r in &new_rows {
                new_text.push_str(r);
                new_text.push('|');
            }
            let new_enc = encrypt_mapper(new_text.as_bytes());
            write_atomic_file(&pm_path, &new_enc)?;
        }
    }

    // Step 2: clean CompositePackageMapper.dat — remove rows with our composite_uid.
    let cm_path = cooked.join(MAPPER_FILE);
    let cm_clean_path = cooked.join(BACKUP_FILE);
    if cm_path.exists() && cm_clean_path.exists() {
        let cm_live_bytes =
            fs::read(&cm_path).map_err(|e| format!("read CompositePackageMapper: {e}"))?;
        let cm_str =
            String::from_utf8_lossy(&decrypt_mapper(&cm_live_bytes)).to_string();
        let mut cm_map = parse_mapper(&cm_str);
        let to_remove: Vec<String> = cm_map
            .iter()
            .filter_map(|(k, v)| {
                if v.composite_name == composite_uid {
                    Some(k.clone())
                } else {
                    None
                }
            })
            .collect();
        let cm_dirty = !to_remove.is_empty();
        for k in to_remove {
            cm_map.remove(&k);
        }
        if cm_dirty {
            let new_plain = serialize_mapper(&cm_map);
            let new_enc = encrypt_mapper(new_plain.as_bytes());
            write_atomic_file(&cm_path, &new_enc)?;
        }
    }

    Ok(())
}

/// Install a mod that targets a v100 vanilla composite slice.
///
/// Deploys the file to CookedPC under a name derived from `target_object_path`'s
/// tail — `<tail>_dup.gpk` (the TMM convention for vanilla composite object
/// filenames). Then rewrites every `CompositePackageMapper.dat` entry whose
/// `object_path` ends with `.<tail>_dup` to redirect to our file at offset 0,
/// size = file size. Both lookup paths (PkgMapper logical → composite UID →
/// file, and direct composite-path → file) converge to the modded content.
///
/// The filename is derived from `target_object_path` directly, bypassing the
/// GPK's own header package-name — which may differ from the target (e.g. an
/// artexlib mod shipping as `LancerGigaChadBlock.gpk` but targeting
/// `S1UI_Message.Message_I1CF`).
///
/// `mod_id` is used only for the pre-flight cleanup of any prior
/// dropin+mapper_extend state; it does not affect the deployed filename.
pub fn install_composite_redirect(
    game_root: &Path,
    source_gpk: &Path,
    target_object_path: &str,
    mod_id: &str,
) -> Result<String, String> {
    // Pre-flight: remove any stale rows from a prior dropin+mapper_extend install.
    clean_prior_dropin_state(game_root, mod_id)?;
    if !target_object_path.contains('.') {
        return Err(format!(
            "target_object_path '{target_object_path}' has no tail (expected 'Package.Object' format)"
        ));
    }
    let tail = target_object_path
        .rsplit('.')
        .next()
        .ok_or_else(|| format!("target_object_path '{target_object_path}' has no tail"))?;
    let folder_name = if tail.ends_with("_dup") {
        tail.to_string()
    } else {
        format!("{tail}_dup")
    };
    let target_filename = format!("{folder_name}.gpk");

    if !is_safe_gpk_container_filename(&target_filename) {
        return Err(format!(
            "composite_redirect: derived filename '{target_filename}' is not safe"
        ));
    }

    let cooked_pc = game_root.join(COOKED_PC_DIR);
    fs::create_dir_all(&cooked_pc)
        .map_err(|e| format!("Failed to create CookedPC dir: {e}"))?;
    let dest = cooked_pc.join(&target_filename);

    // Back up existing vanilla .gpk if present and no backup exists yet.
    let backup = cooked_pc.join(format!("{target_filename}.vanilla-bak"));
    if dest.exists() && !backup.exists() {
        fs::copy(&dest, &backup)
            .map_err(|e| format!("Failed to back up vanilla .gpk: {e}"))?;
    }

    fs::copy(source_gpk, &dest)
        .map_err(|e| format!("Failed to install .gpk into CookedPC: {e}"))?;

    // Redirect every composite mapper entry whose ObjectPath ends in
    // `.<folder_name>` so the engine's direct composite-path lookup also
    // resolves to our modded file.
    ensure_backup(game_root)?;
    let mapper_bytes = fs::read(mapper_path(game_root))
        .map_err(|e| format!("Failed to read mapper after install: {e}"))?;
    let decrypted = decrypt_mapper(&mapper_bytes);
    let decrypted_str = String::from_utf8_lossy(&decrypted).to_string();
    let mut map = parse_mapper(&decrypted_str);

    let file_size = fs::metadata(&dest)
        .map_err(|e| format!("Failed to stat installed .gpk: {e}"))?
        .len() as i64;
    let suffix = format!(".{folder_name}");

    let mut rewritten = 0usize;
    for entry in map.values_mut() {
        if entry.object_path.ends_with(&suffix) || entry.object_path == folder_name {
            entry.filename = target_filename.clone();
            entry.offset = 0;
            entry.size = file_size;
            rewritten += 1;
        }
    }

    if rewritten > 0 {
        let new_plain = serialize_mapper(&map);
        let new_encrypted = encrypt_mapper(new_plain.as_bytes());
        fs::write(mapper_path(game_root), &new_encrypted)
            .map_err(|e| format!("Failed to write patched mapper: {e}"))?;
        log::info!(
            "composite_redirect: installed {target_filename}, redirected {rewritten} mapper entries for `{folder_name}`"
        );
    } else {
        log::info!(
            "composite_redirect: installed {target_filename} (no CompositePackageMapper entry matched `{folder_name}`)"
        );
    }

    Ok(target_filename)
}

/// Restores the vanilla .gpk for a legacy drop-in install. Removes
/// the modded .gpk and copies the .vanilla-bak back over if present.
/// If no backup exists (meaning the vanilla slot was empty before the
/// install), the modded .gpk is simply removed.
pub fn uninstall_legacy_gpk(game_root: &Path, target_filename: &str) -> Result<(), String> {
    if !is_safe_gpk_container_filename(target_filename) {
        return Err(format!(
            "Refusing to uninstall - '{target_filename}' is not a safe filename."
        ));
    }
    let cooked_pc = game_root.join(COOKED_PC_DIR);
    let dest = cooked_pc.join(target_filename);
    let backup = cooked_pc.join(format!("{target_filename}.vanilla-bak"));

    if backup.exists() {
        fs::copy(&backup, &dest).map_err(|e| format!("Failed to restore vanilla .gpk: {}", e))?;
        fs::remove_file(&backup)
            .map_err(|e| format!("Failed to remove backup after restore: {}", e))?;
    } else if dest.exists() {
        fs::remove_file(&dest).map_err(|e| format!("Failed to remove modded .gpk: {}", e))?;
    }

    // Restore composite mapper entries from .clean if possible.
    let folder_name = target_filename
        .strip_suffix(".gpk")
        .unwrap_or(target_filename);
    let suffix = format!(".{folder_name}");
    let clean_mapper = backup_path(game_root);
    let current_mapper = mapper_path(game_root);

    if clean_mapper.exists() && current_mapper.exists() {
        if let (Ok(clean_bytes), Ok(current_bytes)) =
            (fs::read(&clean_mapper), fs::read(&current_mapper))
        {
            let clean_map = parse_mapper(&String::from_utf8_lossy(&decrypt_mapper(&clean_bytes)));
            let mut current_map =
                parse_mapper(&String::from_utf8_lossy(&decrypt_mapper(&current_bytes)));
            let mut rewritten = 0;

            for (composite_name, current_entry) in current_map.iter_mut() {
                if current_entry.object_path.ends_with(&suffix)
                    || current_entry.object_path == folder_name
                {
                    if let Some(clean_entry) = clean_map.get(composite_name) {
                        current_entry.filename = clean_entry.filename.clone();
                        current_entry.offset = clean_entry.offset;
                        current_entry.size = clean_entry.size;
                        rewritten += 1;
                    }
                }
            }

            if rewritten > 0 {
                let new_plain = serialize_mapper(&current_map);
                let new_encrypted = encrypt_mapper(new_plain.as_bytes());
                let _ = fs::write(&current_mapper, &new_encrypted);
                log::info!(
                    "Restored {} mapper entries for legacy mod {}",
                    rewritten,
                    target_filename
                );
            }
        }
    }

    let _ = restore_pkg_mapper_for_standalone_gpk(game_root, folder_name);

    Ok(())
}

/// Installs a mod by:
///   1. Reading the vanilla mapper from `.clean` (or the current mapper
///      as a fallback — first install will hit this path).
///   2. Parsing the `.gpk`'s TMM metadata.
///   3. Copying the `.gpk` into CookedPC so the game can load it.
///   4. Rewriting mapper entries to point each composite package at the
///      new file + offset.
///   5. Writing the encrypted mapper back to disk.
pub fn install_gpk(game_root: &Path, source_gpk: &Path) -> Result<ModFile, String> {
    // Parse the mod file BEFORE touching any filesystem state (including the
    // backup). This way a rejected install leaves `.clean` untouched (PRD
    // 3.1.4.gpk-deploy-sandbox).
    let gpk_bytes = fs::read(source_gpk).map_err(|e| format!("Failed to read mod file: {}", e))?;
    let modfile = parse_mod_file(&gpk_bytes)?;
    if modfile.container.is_empty() {
        return Err(
            "Mod file has no embedded container metadata — falling back to filename-based install."
                .into(),
        );
    }
    if !is_safe_gpk_container_filename(&modfile.container) {
        return Err(format!(
            "Mod container filename '{}' is unsafe — refusing to deploy (would escape CookedPC).",
            modfile.container
        ));
    }
    if modfile.packages.is_empty() {
        return Err("Mod file declares no composite packages to override.".into());
    }
    if modfile.packages.iter().any(|p| p.object_path.is_empty()) {
        return Err(
            "Mod file has a composite package with no object path — can't be installed.".into(),
        );
    }

    ensure_backup(game_root)?;

    // Copy the mod file into CookedPC with the exact container filename TMM
    // expects. The game loads whatever the mapper points at, so the filename
    // must match the mapper entries we're about to write.
    let dest_gpk = game_root.join(COOKED_PC_DIR).join(&modfile.container);
    fs::create_dir_all(dest_gpk.parent().unwrap_or_else(|| Path::new(".")))
        .map_err(|e| format!("Failed to create CookedPC dir: {}", e))?;
    fs::copy(source_gpk, &dest_gpk)
        .map_err(|e| format!("Failed to copy mod into CookedPC: {}", e))?;

    // Load and decrypt current mapper.
    let mapper_bytes =
        fs::read(mapper_path(game_root)).map_err(|e| format!("Failed to read mapper: {}", e))?;
    let decrypted = decrypt_mapper(&mapper_bytes);
    let decrypted_str = String::from_utf8_lossy(&decrypted).to_string();
    let mut map = parse_mapper(&decrypted_str);

    apply_mod_patches(&mut map, &modfile)?;

    // Drop the TMM marker so the file is recognizable by TMM too.
    map.insert(
        TMM_MARKER.into(),
        MapperEntry {
            filename: TMM_MARKER.into(),
            object_path: TMM_MARKER.into(),
            composite_name: TMM_MARKER.into(),
            offset: 0,
            size: 0,
        },
    );

    let serialized = serialize_mapper(&map);
    let encrypted = encrypt_mapper(serialized.as_bytes());
    fs::write(mapper_path(game_root), &encrypted)
        .map_err(|e| format!("Failed to write mapper: {}", e))?;
    Ok(modfile)
}

/// Reverses `install_gpk`:
///   1. Reads the `.clean` backup to look up vanilla entries.
///   2. Reads the current mapper, writes back vanilla entries for each
///      of this mod's object paths.
///   3. Deletes the `.gpk` from CookedPC.
pub fn uninstall_gpk(
    game_root: &Path,
    container: &str,
    object_paths: &[String],
) -> Result<(), String> {
    if !is_safe_gpk_container_filename(container) {
        return Err(format!(
            "Refusing to uninstall: container filename '{}' is unsafe — would escape CookedPC.",
            container
        ));
    }
    let backup = backup_path(game_root);
    if !backup.exists() {
        return Err(
            "No CompositePackageMapper.clean backup on disk — can't restore vanilla entries. Verify game files.".into(),
        );
    }
    let backup_bytes = fs::read(&backup).map_err(|e| format!("Failed to read backup: {}", e))?;
    let backup_decrypted = decrypt_mapper(&backup_bytes);
    let backup_str = String::from_utf8_lossy(&backup_decrypted).to_string();
    let backup_map = parse_mapper(&backup_str);

    let current_bytes =
        fs::read(mapper_path(game_root)).map_err(|e| format!("Failed to read mapper: {}", e))?;
    let current_decrypted = decrypt_mapper(&current_bytes);
    let current_str = String::from_utf8_lossy(&current_decrypted).to_string();
    let mut map = parse_mapper(&current_str);

    for path in object_paths {
        let vanilla = get_entry_by_incomplete_object_path(&backup_map, path)
            .or_else(|| get_entry_by_object_path(&backup_map, path));
        let Some(vanilla) = vanilla else {
            // Not fatal — just skip; the mod may never have patched this path.
            continue;
        };
        map.insert(vanilla.composite_name.clone(), vanilla.clone());
    }

    // PRD 3.2.4.uninstall-all-restores-vanilla: if removing this mod leaves
    // every remaining entry at its vanilla (filename, offset, size), this
    // was the last mod — strip the TMM_MARKER so the on-disk mapper is
    // byte-identical to `.clean`. Keeping the marker otherwise so downstream
    // TMM tools still recognise mixed installs.
    let non_marker_all_vanilla = map.iter().all(|(k, e)| {
        if k == TMM_MARKER {
            return true;
        }
        match backup_map.get(k) {
            Some(v) => {
                v.filename.eq_ignore_ascii_case(&e.filename)
                    && v.offset == e.offset
                    && v.size == e.size
                    && v.object_path.eq_ignore_ascii_case(&e.object_path)
            }
            None => false,
        }
    });
    if non_marker_all_vanilla {
        map.remove(TMM_MARKER);
    }

    let serialized = serialize_mapper(&map);
    let encrypted = encrypt_mapper(serialized.as_bytes());
    fs::write(mapper_path(game_root), &encrypted)
        .map_err(|e| format!("Failed to write mapper: {}", e))?;

    // Remove the container .gpk we copied in.
    let dest_gpk = game_root.join(COOKED_PC_DIR).join(container);
    if dest_gpk.exists() {
        fs::remove_file(&dest_gpk).map_err(|e| format!("Failed to remove mod gpk: {}", e))?;
    }
    Ok(())
}

#[allow(dead_code)]
fn _unused_cursor_suppress(_: Cursor<Vec<u8>>, _: Box<dyn Read>) {}

// --- Tests ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn encrypt_then_decrypt_is_identity() {
        let original: Vec<u8> = (0..=255u8).cycle().take(333).collect();
        let enc = encrypt_mapper(&original);
        let dec = decrypt_mapper(&enc);
        assert_eq!(original, dec, "round-trip mismatch");
    }

    #[test]
    fn parse_then_serialize_roundtrips_a_simple_map() {
        let text = "A.gpk?Obj1,Comp1,100,200,|Obj2,Comp2,300,400,|!B.gpk?Obj3,Comp3,500,600,|!";
        let map = parse_mapper(text);
        assert_eq!(map.len(), 3);
        let comp1 = map.get("Comp1").expect("Comp1 must parse");
        assert_eq!(comp1.object_path, "Obj1");
        assert_eq!(comp1.offset, 100);
        assert_eq!(comp1.size, 200);

        let s = serialize_mapper(&map);
        let map2 = parse_mapper(&s);
        assert_eq!(map, map2);
    }

    #[test]
    fn strict_mapper_parse_rejects_invalid_numeric_fields() {
        let bad_offset = "A.gpk?Obj1,Comp1,not-a-number,200,|!";
        let bad_size = "A.gpk?Obj1,Comp1,100,not-a-number,|!";

        assert!(
            parse_mapper_strict(bad_offset)
                .unwrap_err()
                .contains("invalid offset"),
            "strict parser must not silently coerce an invalid offset to zero"
        );
        assert!(
            parse_mapper_strict(bad_size)
                .unwrap_err()
                .contains("invalid size"),
            "strict parser must not silently coerce an invalid size to zero"
        );
    }

    #[test]
    fn incomplete_paths_equal_matches_by_prefix_and_suffix() {
        // TMM's matching trims off everything from the LAST '_' before
        // the '.' onward in the composite name. So
        // "S1UI_PartyWindow.Foo" → composite = "S1UI", path = "Foo".
        // Two paths match when both halves match case-insensitively.

        // Same pre-_, same post-. → match
        assert!(incomplete_paths_equal(
            "S1UI_PartyWindow.Foo",
            "S1UI_Other.Foo"
        ));

        // Different pre-_ → no match
        assert!(!incomplete_paths_equal("S1UI_Foo.Bar", "Other_Foo.Bar"));

        // Same composite prefix, different post-. → no match
        assert!(!incomplete_paths_equal("S1UI_Foo.Bar", "S1UI_Foo.Baz"));
    }

    /// adv.bogus-gpk-footer — PRD §5.3 adversarial corpus.
    ///
    /// An attacker-supplied `.gpk` can carry any bytes in the tail region
    /// where TMM expects its magic footer + size/offset fields. Three
    /// invariants must hold for every byte pattern:
    ///
    ///   (a) `parse_mod_file` never panics (no bounds-check blow-up,
    ///       no integer overflow, no unchecked indexing).
    ///   (b) Bytes that clearly cannot be TMM (too small, or missing
    ///       magic) either return `Err(msg)` or return `Ok(m)` with
    ///       `m.container.is_empty()` — both shapes are caught downstream
    ///       by `install_gpk`'s "no TMM container" Err branch.
    ///   (c) Bytes that have the right magic but a truncated/corrupt
    ///       footer return `Err(msg)`, never silently deploy a partial
    ///       ModFile.
    ///
    /// The corpus below covers the main bogus-footer shapes a malicious
    /// catalog entry or man-in-the-middle could plant. The test is the
    /// behavioural pin; `tests/bogus_gpk_footer.rs` is the wiring guard
    /// that checks this corpus stays in the source across refactors.
    #[test]
    fn parse_mod_file_rejects_non_tmm_gpks() {
        // (1) The long-standing baseline: 64 bytes of junk. Magic check
        //     fails → fallback single-package ModFile with empty container.
        let m = parse_mod_file(&[0x42u8; 64]).unwrap();
        assert!(m.container.is_empty(), "64 bytes junk: container empty");
        assert_eq!(m.packages.len(), 1);

        // (2) Empty buffer → Err("too small"). Must not panic.
        assert!(parse_mod_file(&[]).is_err(), "empty buffer must err");

        // (3) 3 bytes — one below the 4-byte magic threshold. Err.
        assert!(parse_mod_file(&[0, 0, 0]).is_err(), "3 bytes must err");

        // (4) 4 bytes of zeros. Magic check fails → fallback Ok.
        let m = parse_mod_file(&[0, 0, 0, 0]).unwrap();
        assert!(m.container.is_empty(), "4 zero bytes: container empty");

        // (5) 1024 bytes of 0xff. Magic check fails → fallback Ok.
        let m = parse_mod_file(&[0xFFu8; 1024]).unwrap();
        assert!(m.container.is_empty(), "1024 bytes 0xff: container empty");

        // (6) PACKAGE_MAGIC bytes alone (4 bytes). Magic check passes but
        //     read_back_i32 has no room for the next slot → Err.
        let magic_only = [0xC1, 0x83, 0x2A, 0x9E]; // little-endian 0x9E2A83C1
        assert!(
            parse_mod_file(&magic_only).is_err(),
            "magic-only 4 bytes must err (no footer slots)"
        );

        // (7) Magic at wrong offset — PACKAGE_MAGIC in the middle of a
        //     long buffer, trailing bytes != magic. Magic check at end-4
        //     fails → fallback Ok with empty container.
        let mut buf = vec![0u8; 64];
        buf[30..34].copy_from_slice(&magic_only);
        let m = parse_mod_file(&buf).unwrap();
        assert!(m.container.is_empty(), "misplaced magic: container empty");

        // (8) Magic correct at the tail but footer slots point past EOF
        //     via a huge composite_count. Must err, not panic.
        //     Layout (from read_back order starting at end-4): magic,
        //     meta_size, composite_count (HUGE), offsets_offset, ...
        let mut trap = vec![0u8; 64];
        // place PACKAGE_MAGIC at the very end
        let n = trap.len();
        trap[n - 4..n].copy_from_slice(&magic_only);
        // composite_count slot (2nd from the end): huge positive i32
        // (read_back_i32 reads meta_size first, then composite_count).
        // So composite_count lives at bytes [n-12..n-8].
        trap[n - 12..n - 8].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(
            parse_mod_file(&trap).is_err(),
            "huge composite_count must err, not panic"
        );

        // (9) Buffer of exactly 4 bytes non-magic. Magic check fails →
        //     fallback Ok. This is the smallest legal fallback input.
        let m = parse_mod_file(&[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();
        assert!(
            m.container.is_empty(),
            "4-byte non-magic: container empty → install_gpk rejects"
        );
    }

    /// PRD 3.1.4.gpk-deploy-sandbox. The TMM container filename in a `.gpk`
    /// mod footer is attacker-controlled — a hostile mod can set it to
    /// `../../Windows/foo.gpk` and install_gpk's `game_root.join(CookedPC)
    /// .join(&modfile.container)` would resolve through Path's parent
    /// traversal, escaping CookedPC. `is_safe_gpk_container_filename` is
    /// the first gate; this test pins it over ≥5 `..`-based vectors plus
    /// absolute-path, drive-letter, separator, null, and dot-only variants.
    #[test]
    fn deploy_path_clamped_inside_game_root() {
        // Negative: every one of these must be rejected.
        let hostile = [
            // ..-based (5+, per PRD bar)
            "..",
            "../evil.gpk",
            "../../evil.gpk",
            "..\\evil.gpk",
            "..\\..\\evil.gpk",
            "foo..bar.gpk", // embedded .. too (prevents creative normalisation bypasses)
            // Absolute POSIX
            "/etc/passwd",
            // Subdir with forward or back slash
            "sub/evil.gpk",
            "sub\\evil.gpk",
            // Windows drive-letter
            "C:evil.gpk",
            "D:/evil.gpk",
            // Null byte / empty / dot-only
            "\0evil.gpk",
            "evil\0.gpk",
            "",
            ".",
        ];
        for name in hostile {
            assert!(
                !is_safe_gpk_container_filename(name),
                "vector {name:?} must be rejected"
            );
        }

        // Positive control: realistic TMM container names are plain leafs.
        let safe = [
            "S1Data_2.gpk",
            "ModFile.gpk",
            "a.gpk",
            "file_with_underscore.bin",
            "no-extension",
        ];
        for name in safe {
            assert!(
                is_safe_gpk_container_filename(name),
                "vector {name:?} should have been accepted"
            );
        }
    }

    // --- PRD 3.2.4.uninstall-all-restores-vanilla --------------------------

    #[test]
    fn uninstall_all_restores_vanilla_bytes() {
        use sha2::{Digest, Sha256};

        // Vanilla mapper state.
        let vanilla: HashMap<String, MapperEntry> = mapper_with(&[
            ("S1UI_Party", "S1UI_Party.Foo", "S1Data.gpk"),
            ("S1UI_Inv", "S1UI_Inv.Bar", "S1Data.gpk"),
            ("S1UI_Chat", "S1UI_Chat.Baz", "S1Data.gpk"),
        ]);

        // Capture vanilla bytes (what .clean would hold on disk).
        let vanilla_serialised = serialize_mapper(&vanilla);
        let vanilla_encrypted = encrypt_mapper(vanilla_serialised.as_bytes());
        let vanilla_sha = Sha256::digest(&vanilla_encrypted);

        // Install: apply mod A's patches to a clone of the vanilla map, add
        // the TMM_MARKER exactly like install_gpk does.
        let mut post_install = vanilla.clone();
        let mod_a = ModFile {
            container: "modA.gpk".into(),
            region_lock: true,
            packages: vec![
                ModPackage {
                    object_path: "S1UI_Party.Foo".into(),
                    offset: 100,
                    size: 200,
                    ..Default::default()
                },
                ModPackage {
                    object_path: "S1UI_Inv.Bar".into(),
                    offset: 300,
                    size: 400,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        apply_mod_patches(&mut post_install, &mod_a).unwrap();
        post_install.insert(
            TMM_MARKER.into(),
            MapperEntry {
                filename: TMM_MARKER.into(),
                object_path: TMM_MARKER.into(),
                composite_name: TMM_MARKER.into(),
                offset: 0,
                size: 0,
            },
        );

        // Sanity: post-install state differs from vanilla.
        let post_install_sha =
            Sha256::digest(encrypt_mapper(serialize_mapper(&post_install).as_bytes()));
        assert_ne!(
            vanilla_sha, post_install_sha,
            "post-install must differ from vanilla"
        );

        // Uninstall: restore each object_path from the backup (simulates the
        // loop in uninstall_gpk) and drop the TMM_MARKER now that no mods
        // remain.
        let backup_map = vanilla.clone();
        let mut post_uninstall = post_install;
        for path in ["S1UI_Party.Foo", "S1UI_Inv.Bar"] {
            let v = get_entry_by_object_path(&backup_map, path).unwrap();
            post_uninstall.insert(v.composite_name.clone(), v.clone());
        }
        // "No mods remaining" check (same predicate uninstall_gpk uses post-
        // iter-42).
        let all_vanilla = post_uninstall.iter().all(|(k, e)| {
            if k == TMM_MARKER {
                return true;
            }
            backup_map
                .get(k)
                .map(|v| v.filename == e.filename && v.offset == e.offset && v.size == e.size)
                .unwrap_or(false)
        });
        assert!(
            all_vanilla,
            "test scenario must exercise the all-vanilla branch"
        );
        post_uninstall.remove(TMM_MARKER);

        let post_uninstall_sha =
            Sha256::digest(encrypt_mapper(serialize_mapper(&post_uninstall).as_bytes()));
        assert_eq!(
            vanilla_sha, post_uninstall_sha,
            "install + uninstall-all must leave the mapper bytes identical to vanilla"
        );
    }

    // --- PRD 3.2.3.clean-backup-not-overwritten ----------------------------

    #[test]
    fn clean_backup_not_overwritten_on_second_install() {
        // ensure_backup copies <game>/S1Game/CookedPC/CompositePackageMapper.dat
        // to .clean on first touch, then must be a no-op if the .clean is
        // already present. If a subsequent install overwrites .clean with a
        // MOD-POLLUTED current mapper, uninstall can't restore vanilla.
        let tmp = TempDir::new().unwrap();
        let cooked = tmp.path().join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked).unwrap();

        let mapper = cooked.join(MAPPER_FILE);
        let vanilla_bytes: &[u8] = b"VANILLA-MAPPER-BYTES";
        fs::write(&mapper, vanilla_bytes).unwrap();

        // First ensure_backup: .clean doesn't exist yet -> copies vanilla.
        ensure_backup(tmp.path()).unwrap();
        let backup = cooked.join(BACKUP_FILE);
        assert!(
            backup.exists(),
            ".clean must exist after first ensure_backup"
        );
        assert_eq!(
            fs::read(&backup).unwrap(),
            vanilla_bytes,
            ".clean must contain the vanilla mapper bytes"
        );

        // Simulate an install that modified the current mapper.
        let polluted_bytes: &[u8] = b"MOD-POLLUTED-MAPPER-BYTES-DIFFERENT-LEN";
        fs::write(&mapper, polluted_bytes).unwrap();

        // Second ensure_backup: .clean already exists -> must be a no-op.
        // Most importantly, must NOT re-copy the (now polluted) current
        // mapper over the .clean — that would permanently destroy the
        // vanilla baseline uninstall relies on.
        ensure_backup(tmp.path()).unwrap();
        assert_eq!(
            fs::read(&backup).unwrap(),
            vanilla_bytes,
            ".clean must still contain vanilla after second ensure_backup"
        );
        assert_ne!(
            fs::read(&backup).unwrap(),
            polluted_bytes,
            ".clean must not have been overwritten with the polluted current"
        );
    }

    #[test]
    fn ensure_backup_errors_when_mapper_missing() {
        let tmp = TempDir::new().unwrap();
        let err = ensure_backup(tmp.path()).unwrap_err();
        assert!(err.contains("not found"), "got {err}");
    }

    // --- PRD 3.2.9.clean-recovery-logic ------------------------------------

    /// Writes a realistic-looking encrypted mapper into `<game_root>/CookedPC/`
    /// with the given map.
    fn write_mapper_at(game_root: &Path, map: &HashMap<String, MapperEntry>) {
        let cooked = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked).unwrap();
        let serialised = serialize_mapper(map);
        let encrypted = encrypt_mapper(serialised.as_bytes());
        fs::write(cooked.join(MAPPER_FILE), encrypted).unwrap();
    }

    #[test]
    fn clean_recovery_logic_nop_when_backup_exists() {
        let tmp = TempDir::new().unwrap();
        let map = mapper_with(&[("S1UI", "S1UI_Foo.Bar", "S1Data.gpk")]);
        write_mapper_at(tmp.path(), &map);
        // Pre-existing backup with different bytes.
        fs::write(
            tmp.path().join(COOKED_PC_DIR).join(BACKUP_FILE),
            b"PRE-EXISTING-BACKUP",
        )
        .unwrap();

        recover_missing_clean(tmp.path()).unwrap();
        // Must not have touched the backup (no-op).
        assert_eq!(
            fs::read(tmp.path().join(COOKED_PC_DIR).join(BACKUP_FILE)).unwrap(),
            b"PRE-EXISTING-BACKUP"
        );
    }

    #[test]
    fn clean_recovery_logic_creates_backup_from_vanilla_current() {
        let tmp = TempDir::new().unwrap();
        let vanilla = mapper_with(&[
            ("S1UI_Party", "S1UI_Party.Foo", "S1Data.gpk"),
            ("S1UI_Inv", "S1UI_Inv.Bar", "S1Data.gpk"),
        ]);
        write_mapper_at(tmp.path(), &vanilla);

        recover_missing_clean(tmp.path()).unwrap();
        // .clean now exists and contains the current mapper bytes.
        let mapper_bytes = fs::read(tmp.path().join(COOKED_PC_DIR).join(MAPPER_FILE)).unwrap();
        let backup_bytes = fs::read(tmp.path().join(COOKED_PC_DIR).join(BACKUP_FILE)).unwrap();
        assert_eq!(mapper_bytes, backup_bytes);
    }

    #[test]
    fn clean_recovery_logic_refuses_when_current_is_modded() {
        // Current mapper carries the TMM_MARKER — means mods were installed.
        // We don't know what "vanilla" looked like, so we can't recover.
        let tmp = TempDir::new().unwrap();
        let mut current = mapper_with(&[("S1UI", "S1UI_Foo.Bar", "modA.gpk")]);
        current.insert(
            TMM_MARKER.into(),
            MapperEntry {
                filename: TMM_MARKER.into(),
                object_path: TMM_MARKER.into(),
                composite_name: TMM_MARKER.into(),
                offset: 0,
                size: 0,
            },
        );
        write_mapper_at(tmp.path(), &current);

        let err = recover_missing_clean(tmp.path()).unwrap_err();
        assert!(
            err.contains("verify game files") || err.contains("mod entries"),
            "error must tell user how to recover: {err}"
        );
        // .clean must NOT have been written — we don't want to poison the
        // baseline.
        assert!(!tmp.path().join(COOKED_PC_DIR).join(BACKUP_FILE).exists());
    }

    #[test]
    fn clean_recovery_logic_errors_when_mapper_missing() {
        let tmp = TempDir::new().unwrap();
        let err = recover_missing_clean(tmp.path()).unwrap_err();
        assert!(err.contains("not found"), "got {err}");
    }

    // --- PRD 3.3.2.per-object-gpk-merge ------------------------------------

    #[test]
    fn per_object_merge_both_apply() {
        // Two mods touching different composites (and therefore different
        // object slots) both apply — neither clobbers the other.
        let mut map = mapper_with(&[
            ("S1UI_Party", "S1UI_Party.Foo", "S1Data.gpk"),
            ("S1UI_Inv", "S1UI_Inv.Bar", "S1Data.gpk"),
        ]);

        let mod_a = ModFile {
            container: "modA.gpk".into(),
            region_lock: true,
            packages: vec![ModPackage {
                object_path: "S1UI_Party.Foo".into(),
                offset: 100,
                size: 200,
                ..Default::default()
            }],
            ..Default::default()
        };
        apply_mod_patches(&mut map, &mod_a).unwrap();

        let mod_b = ModFile {
            container: "modB.gpk".into(),
            region_lock: true,
            packages: vec![ModPackage {
                object_path: "S1UI_Inv.Bar".into(),
                offset: 300,
                size: 400,
                ..Default::default()
            }],
            ..Default::default()
        };
        apply_mod_patches(&mut map, &mod_b).unwrap();

        let party = map
            .values()
            .find(|e| e.object_path == "S1UI_Party.Foo")
            .expect("Party entry must survive second install");
        assert_eq!(party.filename, "modA.gpk");
        assert_eq!(party.offset, 100);
        assert_eq!(party.size, 200);

        let inv = map
            .values()
            .find(|e| e.object_path == "S1UI_Inv.Bar")
            .expect("Inv entry must be patched by modB");
        assert_eq!(inv.filename, "modB.gpk");
        assert_eq!(inv.offset, 300);
        assert_eq!(inv.size, 400);
    }

    #[test]
    fn patch_apply_aborts_on_unknown_object_path() {
        // Incoming mod references a slot that doesn't exist in the current
        // mapper — happens when the game version doesn't match the mod.
        // Err before writing anything.
        let mut map = mapper_with(&[("S1UI", "S1UI_Foo.Bar", "S1Data.gpk")]);
        let incoming = ModFile {
            container: "mod.gpk".into(),
            region_lock: true,
            packages: vec![ModPackage {
                object_path: "NoSuchComp.NoSuchObj".into(),
                offset: 0,
                size: 0,
                ..Default::default()
            }],
            ..Default::default()
        };
        let err = apply_mod_patches(&mut map, &incoming).unwrap_err();
        assert!(err.contains("not found in mapper"), "got {err}");
        // Verify no partial mutation.
        let orig = map.values().next().unwrap();
        assert_eq!(orig.filename, "S1Data.gpk");
    }

    #[test]
    fn patch_apply_is_idempotent_on_reinstall() {
        // Re-applying the same modfile to a map that already reflects it
        // produces the same final state (no extra entries, no drift).
        let mut map = mapper_with(&[("S1UI", "S1UI_Foo.Bar", "S1Data.gpk")]);
        let incoming = ModFile {
            container: "mod.gpk".into(),
            region_lock: true,
            packages: vec![ModPackage {
                object_path: "S1UI_Foo.Bar".into(),
                offset: 50,
                size: 75,
                ..Default::default()
            }],
            ..Default::default()
        };

        apply_mod_patches(&mut map, &incoming).unwrap();
        let snapshot: HashMap<_, _> = map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

        apply_mod_patches(&mut map, &incoming).unwrap();
        assert_eq!(map, snapshot, "re-apply must be idempotent");
    }

    // --- PRD 3.3.3.conflict-warning-ui -------------------------------------

    fn mapper_with(entries: &[(&str, &str, &str)]) -> HashMap<String, MapperEntry> {
        // entries: (composite_name, object_path, filename). Keyed by
        // composite_name to match production `parse_mapper`, which assumes
        // composite_name is unique per entry (each UPackage has one name).
        // Tests that want multiple entries must use distinct composite_names.
        entries
            .iter()
            .map(|(c, o, f)| {
                (
                    (*c).to_string(),
                    MapperEntry {
                        composite_name: (*c).to_string(),
                        object_path: (*o).to_string(),
                        filename: (*f).to_string(),
                        offset: 0,
                        size: 0,
                    },
                )
            })
            .collect()
    }

    fn modfile_with(container: &str, object_paths: &[&str]) -> ModFile {
        let mut m = ModFile {
            container: container.to_string(),
            region_lock: true,
            ..Default::default()
        };
        for p in object_paths {
            m.packages.push(ModPackage {
                object_path: (*p).to_string(),
                ..Default::default()
            });
        }
        m
    }

    fn pkg_mapper_text(entries: &[(&str, &str)]) -> String {
        let mut out = String::new();
        for (uid, composite_uid) in entries {
            out.push_str(uid);
            out.push(',');
            out.push_str(composite_uid);
            out.push('|');
        }
        out
    }

    fn write_pkg_mapper_at(game_root: &Path, text: &str) {
        let cooked = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked).unwrap();
        let encrypted = encrypt_mapper(text.as_bytes());
        fs::write(cooked.join(PKG_MAPPER_FILE), encrypted).unwrap();
    }

    #[test]
    fn detect_conflicts_returns_empty_on_vanilla_current() {
        // current mapper is vanilla → no mod has patched this slot yet.
        let vanilla = mapper_with(&[("S1UI", "S1UI_Party.Foo", "S1Data.gpk")]);
        let current = vanilla.clone();
        let incoming = modfile_with("mymod.gpk", &["S1UI_Party.Foo"]);
        assert!(detect_conflicts(&vanilla, &current, &incoming).is_empty());
    }

    #[test]
    fn detect_conflicts_returns_empty_on_self_reinstall() {
        // current already points at the incoming mod's container — re-install.
        let vanilla = mapper_with(&[("S1UI", "S1UI_Party.Foo", "S1Data.gpk")]);
        let current = mapper_with(&[("S1UI", "S1UI_Party.Foo", "mymod.gpk")]);
        let incoming = modfile_with("mymod.gpk", &["S1UI_Party.Foo"]);
        assert!(detect_conflicts(&vanilla, &current, &incoming).is_empty());
    }

    #[test]
    fn detect_conflicts_flags_other_mod_owning_slot() {
        let vanilla = mapper_with(&[("S1UI", "S1UI_Party.Foo", "S1Data.gpk")]);
        let current = mapper_with(&[("S1UI", "S1UI_Party.Foo", "othermod.gpk")]);
        let incoming = modfile_with("mymod.gpk", &["S1UI_Party.Foo"]);

        let conflicts = detect_conflicts(&vanilla, &current, &incoming);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].composite_name, "S1UI");
        assert_eq!(conflicts[0].object_path, "S1UI_Party.Foo");
        assert_eq!(conflicts[0].previous_filename, "othermod.gpk");
    }

    #[test]
    fn detect_conflicts_reports_multiple_slots() {
        // Incoming mod touches 2 slots (distinct composites, distinct
        // objects), both owned by a different mod.
        let vanilla = mapper_with(&[
            ("S1UI_Party", "S1UI_Party.Foo", "S1Data.gpk"),
            ("S1UI_Inv", "S1UI_Inv.Bar", "S1Data.gpk"),
        ]);
        let current = mapper_with(&[
            ("S1UI_Party", "S1UI_Party.Foo", "othermod.gpk"),
            ("S1UI_Inv", "S1UI_Inv.Bar", "othermod.gpk"),
        ]);
        let incoming = modfile_with("mymod.gpk", &["S1UI_Party.Foo", "S1UI_Inv.Bar"]);

        let conflicts = detect_conflicts(&vanilla, &current, &incoming);
        assert_eq!(conflicts.len(), 2);
    }

    #[test]
    fn detect_conflicts_mixed_slots_partial_report() {
        // Incoming touches 2 slots; one vanilla (ok), one owned by other mod.
        let vanilla = mapper_with(&[
            ("S1UI_Party", "S1UI_Party.Foo", "S1Data.gpk"),
            ("S1UI_Inv", "S1UI_Inv.Bar", "S1Data.gpk"),
        ]);
        let current = mapper_with(&[
            ("S1UI_Party", "S1UI_Party.Foo", "S1Data.gpk"), // vanilla
            ("S1UI_Inv", "S1UI_Inv.Bar", "othermod.gpk"),   // conflict
        ]);
        let incoming = modfile_with("mymod.gpk", &["S1UI_Party.Foo", "S1UI_Inv.Bar"]);

        let conflicts = detect_conflicts(&vanilla, &current, &incoming);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].object_path, "S1UI_Inv.Bar");
    }

    #[test]
    fn detect_conflicts_missing_slot_is_not_a_conflict() {
        // Slot doesn't exist in the current mapper — install_gpk will raise
        // a different error; this fn shouldn't double-report.
        let vanilla: HashMap<String, MapperEntry> = HashMap::new();
        let current: HashMap<String, MapperEntry> = HashMap::new();
        let incoming = modfile_with("mymod.gpk", &["S1UI_Party.Foo"]);
        assert!(detect_conflicts(&vanilla, &current, &incoming).is_empty());
    }

    #[test]
    fn uninstall_gpk_rejects_hostile_container_before_any_fs_write() {
        // Lightweight higher-level proof that the sandbox is wired up on
        // uninstall (mirror of install's entry-point check). No game root
        // fs state is needed: we expect the guard to err out immediately.
        let tmp = TempDir::new().unwrap();
        let err = uninstall_gpk(tmp.path(), "../escape.gpk", &[])
            .expect_err("uninstall must reject hostile container");
        assert!(
            err.contains("unsafe") || err.contains("escape"),
            "unexpected error: {err}"
        );
        // tmp should have no CookedPC dir or any other artifact created.
        let entries: Vec<_> = fs::read_dir(tmp.path()).unwrap().flatten().collect();
        assert!(
            entries.is_empty(),
            "uninstall created filesystem state despite rejection: {entries:?}"
        );
    }

    // --- pin.tmm.parser (iter 89) -------------------------------------------
    //
    // Golden-file pin for `parse_mod_file`. The TMM footer format is an
    // implicit contract with the upstream `TMM/Model/Mod.cpp` reader. A
    // silent refactor that reordered fields, changed endian-ness, or
    // reshuffled the string-prefix reader would break compatibility with
    // every existing TMM-packaged mod — and the failure mode would be a
    // cryptic runtime error at install time.
    //
    // Build a hand-packed v1 fixture (1 composite package, ASCII strings,
    // no TFC extras) and assert every field of the resulting ModFile +
    // first ModPackage byte-for-byte. Companion to the iter 79
    // adversarial corpus (`parse_mod_file_rejects_non_tmm_gpks`): the
    // happy path is pinned here, the negative path there. Together
    // they cover the full parser contract.

    /// Pack a length-prefixed ANSI string at `bytes`. Matches
    /// `read_prefixed_string`'s positive-length branch: i32 length
    /// followed by raw bytes.
    fn pack_ansi(bytes: &mut Vec<u8>, s: &str) {
        let len = s.len() as i32;
        bytes.extend_from_slice(&len.to_le_bytes());
        bytes.extend_from_slice(s.as_bytes());
    }

    /// Build a minimal v1 TMM fixture. Layout (decimal byte offsets, LE):
    ///
    ///   0..16    filler (0xAA) — packages reference real data offsets,
    ///            and the first package starts at offset 16. Filler
    ///            keeps the numerology obvious.
    ///   16..28   package header: 4 unused + u16 file_version (3) +
    ///            u16 licensee_version (4) + 4 unused
    ///   28..46   length-prefixed folder "MOD:SomeObject" (14 chars)
    ///   46..48   padding (0xBB) so author lands at 48
    ///   48..57   length-prefixed author "Alice" (5 chars)
    ///   57..64   padding (0xCC) so name lands at 64
    ///   64..75   length-prefixed name "TestMod" (7 chars)
    ///   75..80   padding (0xDD) so container lands at 80
    ///   80..95   length-prefixed container "TestMod.gpk" (11 chars)
    ///   95..96   padding (0xEE) so offsets-array lands at 96
    ///   96..100  offsets array: [16 as i32 LE]
    ///   100..104 footer: version slot = PACKAGE_MAGIC (signals v1 format
    ///            — no TFC extras; counter-intuitive but matches the
    ///            upstream reader: a magic value here means legacy)
    ///   104..108 footer: region_lock = 0
    ///   108..112 footer: author_offset = 48
    ///   112..116 footer: name_offset = 64
    ///   116..120 footer: container_offset = 80
    ///   120..124 footer: offsets_offset = 96
    ///   124..128 footer: composite_count = 1
    ///   128..132 footer: meta_size = 90 (end(136) - package-end(46),
    ///            so last.size = end - meta_size - offset = 30 via
    ///            the v1 fallback branch)
    ///   132..136 PACKAGE_MAGIC = 0x9E2A83C1
    ///
    /// Total: 136 bytes.
    fn v1_fixture() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&[0xAAu8; 16]);
        out.extend_from_slice(&[0x00u8; 4]);
        out.extend_from_slice(&3u16.to_le_bytes()); // file_version
        out.extend_from_slice(&4u16.to_le_bytes()); // licensee_version
        out.extend_from_slice(&[0x00u8; 4]);
        assert_eq!(out.len(), 28);
        pack_ansi(&mut out, "MOD:SomeObject");
        assert_eq!(out.len(), 46);
        out.extend_from_slice(&[0xBBu8; 2]);
        assert_eq!(out.len(), 48);
        pack_ansi(&mut out, "Alice");
        assert_eq!(out.len(), 57);
        out.extend_from_slice(&[0xCCu8; 7]);
        assert_eq!(out.len(), 64);
        pack_ansi(&mut out, "TestMod");
        assert_eq!(out.len(), 75);
        out.extend_from_slice(&[0xDDu8; 5]);
        assert_eq!(out.len(), 80);
        pack_ansi(&mut out, "TestMod.gpk");
        assert_eq!(out.len(), 95);
        out.extend_from_slice(&[0xEEu8; 1]);
        assert_eq!(out.len(), 96);
        out.extend_from_slice(&16i32.to_le_bytes());
        assert_eq!(out.len(), 100);
        out.extend_from_slice(&0x9E2A83C1u32.to_le_bytes()); // version slot = MAGIC → v1
        out.extend_from_slice(&0i32.to_le_bytes()); // region_lock
        out.extend_from_slice(&48i32.to_le_bytes()); // author_offset
        out.extend_from_slice(&64i32.to_le_bytes()); // name_offset
        out.extend_from_slice(&80i32.to_le_bytes()); // container_offset
        out.extend_from_slice(&96i32.to_le_bytes()); // offsets_offset
        out.extend_from_slice(&1i32.to_le_bytes()); // composite_count
        out.extend_from_slice(&90i32.to_le_bytes()); // meta_size
        out.extend_from_slice(&0x9E2A83C1u32.to_le_bytes());
        assert_eq!(out.len(), 136);
        out
    }

    #[test]
    fn golden_v1_fixture_parses_to_expected_modfile() {
        let bytes = v1_fixture();
        let m = parse_mod_file(&bytes).expect("v1 fixture must parse");

        assert_eq!(m.mod_name, "TestMod", "mod_name round-trip");
        assert_eq!(m.mod_author, "Alice", "mod_author round-trip");
        assert_eq!(m.container, "TestMod.gpk", "container round-trip");
        assert!(!m.region_lock, "region_lock round-trip");
        assert_eq!(m.mod_file_version, 1, "v1 preserves version=1");

        assert_eq!(m.packages.len(), 1, "one composite package");
        let p = &m.packages[0];
        assert_eq!(p.offset, 16, "package offset round-trip");
        assert_eq!(p.file_version, 3, "file_version round-trip");
        assert_eq!(p.licensee_version, 4, "licensee_version round-trip");
        assert_eq!(
            p.object_path, "SomeObject",
            "MOD: prefix must be stripped from folder name"
        );
        assert_eq!(
            p.size, 30,
            "v1 package size = end - meta_size - offset (136-90-16=30)"
        );
    }

    #[test]
    fn composite_package_folder_trims_gpk_null_terminator() {
        let mut bytes = v1_fixture();
        let replacement = "MOD:SomeObject\0";
        let folder_offset = 28usize;
        bytes[folder_offset..folder_offset + 4]
            .copy_from_slice(&(replacement.len() as i32).to_le_bytes());
        bytes[folder_offset + 4..folder_offset + 4 + replacement.len()]
            .copy_from_slice(replacement.as_bytes());

        let parsed = parse_composite_package(&bytes, 16).expect("parse package");

        assert_eq!(parsed.object_path, "SomeObject");
    }

    /// Regression guard: if `v1_fixture()` itself drifts, this test
    /// would silently change what we're pinning. Cross-check the
    /// fixture length + a few interior bytes.
    #[test]
    fn golden_fixture_shape_is_stable() {
        let bytes = v1_fixture();
        assert_eq!(bytes.len(), 136, "fixture length is fixed");
        assert_eq!(
            &bytes[132..136],
            &[0xC1, 0x83, 0x2A, 0x9E],
            "last 4 bytes must be PACKAGE_MAGIC (0x9E2A83C1) LE"
        );
        assert_eq!(&bytes[0..2], &[0xAA, 0xAA], "filler byte @0 = 0xAA");
        assert_eq!(&bytes[20..22], &3u16.to_le_bytes(), "file_version @20 = 3");
        assert_eq!(
            &bytes[124..128],
            &1i32.to_le_bytes(),
            "composite_count slot @124 = 1"
        );
    }

    /// Sanity: re-parsing the fixture twice yields identical structs.
    /// If `parse_mod_file` ever picked up hidden state (global counter,
    /// RNG-seeded order, thread-local cache), this test would catch it.
    #[test]
    fn golden_parse_is_deterministic() {
        let bytes = v1_fixture();
        let a = parse_mod_file(&bytes).unwrap();
        let b = parse_mod_file(&bytes).unwrap();
        assert_eq!(a.mod_name, b.mod_name);
        assert_eq!(a.mod_author, b.mod_author);
        assert_eq!(a.container, b.container);
        assert_eq!(a.region_lock, b.region_lock);
        assert_eq!(a.mod_file_version, b.mod_file_version);
        assert_eq!(a.packages.len(), b.packages.len());
        assert_eq!(a.packages[0].offset, b.packages[0].offset);
        assert_eq!(a.packages[0].object_path, b.packages[0].object_path);
        assert_eq!(a.packages[0].size, b.packages[0].size);
    }

    // --- pin.tmm.cipher (iter 92) -------------------------------------------
    //
    // Golden-file pin for the 3-pass CompositePackageMapper cipher
    // (`encrypt_mapper` + `decrypt_mapper` at the top of this file). The
    // upstream algorithm lives in TMM/Model/CompositeMapper.cpp:15 / :49
    // and is a wire-format contract: any silent drift here would make
    // our launcher produce mapper files incompatible with TMM's own
    // reader, silently corrupting installs for users who round-trip
    // mods between this launcher and the reference TMM tool.
    //
    // Three passes in decrypt order: (a) 16-byte block un-shuffle under
    // KEY1 permutation, (b) middle-outward pair-swap (self-inverse), (c)
    // XOR against the repeating 21-byte KEY2 = "GeneratePackageMapper".
    // Encrypt is the exact reverse.
    //
    // This section pins: (1) a specific 16-byte input produces a specific
    // 16-byte output under `encrypt_mapper` (byte-for-byte), (2) the two
    // functions round-trip both ways on short + long + tail-unaligned
    // inputs, (3) the KEY1 permutation is a permutation of 0..16 (not a
    // wrong table), (4) KEY2 is the literal ASCII
    // "GeneratePackageMapper".

    /// The golden expected output for `encrypt_mapper(&[0; 16])`.
    ///
    /// Derivation (hand-traced for the reader; the test asserts the value
    /// this function actually returns):
    ///   Step 1 — XOR zeros with KEY2[0..16] = b"GeneratePackageM"
    ///     → [71,101,110,101,114,97,116,101,80,97,99,107,97,103,101,77]
    ///   Step 2 — pair-swap (1↔15, 3↔13, 5↔11, 7↔9):
    ///     → [71,77,110,103,114,107,116,97,80,101,99,97,97,101,101,101]
    ///   Step 3 — KEY1-forward shuffle (out[KEY1[i]] = tmp[i]):
    ///     → [97,116,101,114,103,101,77,99,101,110,97,101,71,80,107,97]
    ///       = ASCII "atergeMcenaeGPka"
    const GOLDEN_ENCRYPT_OF_ZEROS_16: [u8; 16] = [
        97, 116, 101, 114, 103, 101, 77, 99, 101, 110, 97, 101, 71, 80, 107, 97,
    ];

    /// Byte-for-byte pin of `encrypt_mapper` output on a fixed 16-byte
    /// plaintext. A drift in KEY1, KEY2, the pair-swap loop bounds, or
    /// the shuffle direction would fail here with a diffable mismatch.
    #[test]
    fn golden_cipher_encrypt_zeros_16() {
        let plaintext = [0u8; 16];
        let cipher = encrypt_mapper(&plaintext);
        assert_eq!(
            cipher.as_slice(),
            &GOLDEN_ENCRYPT_OF_ZEROS_16,
            "encrypt_mapper(&[0; 16]) changed — check KEY1 / KEY2 / \
             pair-swap bounds / shuffle direction against \
             TMM/Model/CompositeMapper.cpp"
        );
    }

    /// Encrypt + decrypt is the identity. Covers the "inverse" contract
    /// between the two functions; if someone edits one without updating
    /// the other, this test fires before any user ships a corrupted
    /// mapper file.
    #[test]
    fn golden_cipher_round_trip_identity() {
        // A variety of inputs: zeros, ones, an ASCII line, a 48-byte
        // multi-block buffer, and a tail-unaligned buffer (19 bytes;
        // one full 16-byte block + 3 bytes of tail that the un-shuffle
        // must copy verbatim).
        let fixtures: Vec<Vec<u8>> = vec![
            vec![0u8; 16],
            vec![0xFFu8; 16],
            b"compName/objPath.gpk".to_vec(), // 20 bytes: 16 + 4 tail
            (0u8..48).collect(),              // 48 bytes = 3 blocks
            b"abcdefghijklmnopqrs".to_vec(),  // 19 bytes: 16 + 3 tail
        ];
        for (i, plain) in fixtures.iter().enumerate() {
            let back = decrypt_mapper(&encrypt_mapper(plain));
            assert_eq!(
                &back,
                plain,
                "encrypt→decrypt must be identity (fixture #{i}, len={})",
                plain.len()
            );
            // And the other direction: decrypt treated as inverse of
            // encrypt (which also means encrypt treated as inverse of
            // decrypt — same math).
            let forward = encrypt_mapper(&decrypt_mapper(plain));
            assert_eq!(
                &forward,
                plain,
                "decrypt→encrypt must be identity (fixture #{i}, len={})",
                plain.len()
            );
        }
    }

    /// Structural pin: KEY1 must be a permutation of 0..16, i.e. each
    /// value from 0 to 15 appears exactly once. A typo that made KEY1
    /// non-bijective (e.g. two values equal, or a value ≥ 16) would
    /// either mangle output silently or panic at the `KEY1[idx]` index.
    /// Catch it directly.
    #[test]
    fn golden_cipher_key1_is_permutation() {
        let mut seen = [false; 16];
        for &k in KEY1.iter() {
            assert!(k < 16, "KEY1 value {k} out of range 0..16");
            assert!(!seen[k], "KEY1 value {k} appears twice");
            seen[k] = true;
        }
        assert!(seen.iter().all(|&x| x), "KEY1 missing some value in 0..16");
    }

    /// Structural pin: KEY2 is exactly b"GeneratePackageMapper" (21
    /// bytes). Upstream TMM hard-codes this string; losing any byte or
    /// typing one wrong would make ciphertext incompatible with every
    /// existing TMM-packaged mod.
    #[test]
    fn golden_cipher_key2_is_exact_constant() {
        assert_eq!(KEY2, b"GeneratePackageMapper");
        assert_eq!(KEY2.len(), 21, "KEY2 length is 21");
    }

    // --- pin.tmm.merger (iter 93) -------------------------------------------
    //
    // Completes the pin.tmm trio (parser @ iter 89, cipher @ iter 92).
    //
    // The "merger" in our tmm.rs is `apply_mod_patches`: each mod's
    // packages patch the running `HashMap<composite_name, MapperEntry>`
    // state. The merge contract has two halves and both matter:
    //
    //   1. DISJOINT mods (different slots) commute — apply(A, B) ==
    //      apply(B, A). Loss of commutativity would mean the install
    //      order of independent mods changes the final mapper, which
    //      would confuse users and break TMM-compat golden output for
    //      every user whose mod list is non-alphabetical.
    //
    //   2. OVERLAPPING mods (same slot) are last-install-wins. PRD
    //      3.3.3 explicitly ships this as the behaviour (with
    //      detect_conflicts surfacing the overwrite for user
    //      confirmation). Pinning this stops a refactor from silently
    //      switching to first-wins or merge-both semantics.
    //
    // The following inline tests pin both halves plus identity-on-empty.

    /// Helper: cheap deep-equality on two mapper states. HashMap !=
    /// preserves nothing about iteration order; we compare by
    /// sorted (composite_name, MapperEntry) pairs.
    fn sorted_entries(map: &HashMap<String, MapperEntry>) -> Vec<(String, MapperEntry)> {
        let mut v: Vec<(String, MapperEntry)> =
            map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    /// Disjoint-slot commutativity: two mods patching different
    /// composites must produce identical final maps regardless of
    /// install order.
    #[test]
    fn golden_merger_commutes_on_disjoint_slots() {
        let base = mapper_with(&[
            ("S1UI", "S1UI_Party.Foo", "S1Data.gpk"),
            ("S1UI2", "S1UI_Inv.Bar", "S1Data.gpk"),
        ]);

        let mod_a = modfile_with("modA.gpk", &["S1UI_Party.Foo"]);
        let mod_b = modfile_with("modB.gpk", &["S1UI_Inv.Bar"]);

        let mut map_ab = base.clone();
        apply_mod_patches(&mut map_ab, &mod_a).unwrap();
        apply_mod_patches(&mut map_ab, &mod_b).unwrap();

        let mut map_ba = base.clone();
        apply_mod_patches(&mut map_ba, &mod_b).unwrap();
        apply_mod_patches(&mut map_ba, &mod_a).unwrap();

        assert_eq!(
            sorted_entries(&map_ab),
            sorted_entries(&map_ba),
            "disjoint-slot merges must commute — final map must be \
             identical regardless of A-then-B vs B-then-A"
        );
    }

    /// Three-mod disjoint case: every permutation of install order
    /// yields the same final map. If commutativity holds for 2 mods
    /// but not 3, something path-dependent slipped into the merger
    /// (e.g. a HashMap-iteration-order leak into the final state).
    #[test]
    fn golden_merger_three_disjoint_mods_all_orders_agree() {
        let base = mapper_with(&[
            ("A", "PkgA.Obj", "S1Data.gpk"),
            ("B", "PkgB.Obj", "S1Data.gpk"),
            ("C", "PkgC.Obj", "S1Data.gpk"),
        ]);
        let mods = [
            modfile_with("modA.gpk", &["PkgA.Obj"]),
            modfile_with("modB.gpk", &["PkgB.Obj"]),
            modfile_with("modC.gpk", &["PkgC.Obj"]),
        ];

        // Pin the 3! = 6 permutations to a single reference result.
        let reference = {
            let mut m = base.clone();
            for mf in &mods {
                apply_mod_patches(&mut m, mf).unwrap();
            }
            sorted_entries(&m)
        };

        let permutations: [[usize; 3]; 6] = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        for perm in permutations.iter() {
            let mut m = base.clone();
            for &i in perm.iter() {
                apply_mod_patches(&mut m, &mods[i]).unwrap();
            }
            assert_eq!(
                sorted_entries(&m),
                reference,
                "permutation {perm:?} diverged from reference order"
            );
        }
    }

    /// Overlapping slot (same composite_name): last install wins.
    /// The two orders must NOT commute — the final filename is the
    /// container of whichever mod was applied last. Pinning this
    /// stops a refactor from silently switching to first-wins or
    /// merge-both semantics.
    #[test]
    fn golden_merger_last_install_wins_on_overlap() {
        let base = mapper_with(&[("S1UI", "S1UI_Party.Foo", "S1Data.gpk")]);
        let mod_a = modfile_with("modA.gpk", &["S1UI_Party.Foo"]);
        let mod_b = modfile_with("modB.gpk", &["S1UI_Party.Foo"]);

        let mut map_ab = base.clone();
        apply_mod_patches(&mut map_ab, &mod_a).unwrap();
        apply_mod_patches(&mut map_ab, &mod_b).unwrap();
        assert_eq!(
            map_ab.values().next().unwrap().filename,
            "modB.gpk",
            "A-then-B order: B must win the slot (last-install-wins)"
        );

        let mut map_ba = base.clone();
        apply_mod_patches(&mut map_ba, &mod_b).unwrap();
        apply_mod_patches(&mut map_ba, &mod_a).unwrap();
        assert_eq!(
            map_ba.values().next().unwrap().filename,
            "modA.gpk",
            "B-then-A order: A must win (last-install-wins)"
        );

        // And the two maps must NOT agree — overlap is the case where
        // order DOES matter (PRD 3.3.3).
        assert_ne!(
            sorted_entries(&map_ab),
            sorted_entries(&map_ba),
            "overlapping-slot installs must diverge by order — \
             last-install-wins is the PRD 3.3.3 contract"
        );
    }

    /// Identity: applying a ModFile with zero packages must not
    /// mutate the map. A refactor that accidentally inserted a
    /// placeholder entry would surface as this test failing.
    #[test]
    fn golden_merger_identity_on_empty_modfile() {
        let base = mapper_with(&[("S1UI", "S1UI_Party.Foo", "S1Data.gpk")]);
        let before = sorted_entries(&base);

        let mut map = base;
        let empty_mod = ModFile {
            container: "empty.gpk".into(),
            region_lock: true,
            packages: vec![],
            ..Default::default()
        };
        apply_mod_patches(&mut map, &empty_mod).unwrap();

        let after = sorted_entries(&map);
        assert_eq!(
            before, after,
            "apply_mod_patches on ModFile{{packages: []}} must be a no-op"
        );
    }

    // --------------------------------------------------------------------
    // extract_package_folder_name — UE3 header FolderName parser.
    // Covers the legacy drop-in GPK install path (fix.gpk-install v1.13):
    // determines the CookedPC filename the mod must be copied to so the
    // CompositePackageMapper patch can re-route the composite at the new
    // file. A wrong parse yields the wrong drop-in filename → mapper
    // patch lands on the wrong composite → mod silently does nothing.
    // --------------------------------------------------------------------

    /// Build a fake UE3 header: magic + version + headerSize + FString.
    /// Caller supplies the FString bytes (length prefix + string bytes).
    fn fake_header(fstring: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&PACKAGE_MAGIC.to_le_bytes()); // 4: magic
        v.extend_from_slice(&897u32.to_le_bytes()); // 4: version (tera-modern)
        v.extend_from_slice(&0i32.to_le_bytes()); // 4: headerSize placeholder
        v.extend_from_slice(fstring); // FString at offset 12
        v
    }

    #[test]
    fn extract_folder_name_reads_ascii() {
        // FString: len=+5 (4 ASCII chars + null), bytes "S1UI\0"
        let mut fstr = Vec::new();
        fstr.extend_from_slice(&5i32.to_le_bytes());
        fstr.extend_from_slice(b"S1UI\0");
        let bytes = fake_header(&fstr);
        assert_eq!(
            extract_package_folder_name(&bytes).as_deref(),
            Some("S1UI"),
            "ASCII FString must decode to the non-null-terminated name"
        );
    }

    #[test]
    fn extract_folder_name_reads_utf16() {
        // FString: len=-5 (4 UTF-16 chars + null), bytes "S1UI\0" as u16 LE
        let mut fstr = Vec::new();
        fstr.extend_from_slice(&(-5i32).to_le_bytes());
        for c in "S1UI".chars() {
            fstr.extend_from_slice(&(c as u16).to_le_bytes());
        }
        fstr.extend_from_slice(&0u16.to_le_bytes()); // trailing null
        let bytes = fake_header(&fstr);
        assert_eq!(
            extract_package_folder_name(&bytes).as_deref(),
            Some("S1UI"),
            "UTF-16 FString must decode to the non-null-terminated name"
        );
    }

    #[test]
    fn extract_folder_name_rejects_bad_magic() {
        let mut bytes = vec![0xDE, 0xAD, 0xBE, 0xEF]; // wrong magic
        bytes.extend_from_slice(&[0u8; 16]);
        assert!(
            extract_package_folder_name(&bytes).is_none(),
            "non-UE3 header must yield None; otherwise legacy install \
             would copy arbitrary files into CookedPC/"
        );
    }

    #[test]
    fn extract_folder_name_rejects_truncated_header() {
        let bytes = vec![0u8; 15]; // <16 bytes total
        assert!(
            extract_package_folder_name(&bytes).is_none(),
            "<16-byte input must yield None rather than panic on OOB read"
        );
    }

    #[test]
    fn extract_folder_name_rejects_zero_length_fstring() {
        // FString len=0 — not a legal UE3 FString; treat as malformed.
        let mut fstr = Vec::new();
        fstr.extend_from_slice(&0i32.to_le_bytes());
        let bytes = fake_header(&fstr);
        assert!(
            extract_package_folder_name(&bytes).is_none(),
            "zero-length FString must yield None (would produce empty \
             filename `.gpk` — invalid CookedPC drop)"
        );
    }

    #[test]
    fn extract_folder_name_rejects_oversized_length() {
        // Length > 256 cap — malicious or corrupt header trying to
        // trick the parser into reading huge slices.
        let mut fstr = Vec::new();
        fstr.extend_from_slice(&257i32.to_le_bytes());
        fstr.extend_from_slice(&[b'X'; 32]);
        let bytes = fake_header(&fstr);
        assert!(
            extract_package_folder_name(&bytes).is_none(),
            "length > 256 must yield None (sanity cap — longest known \
             TERA package name is ~40 chars)"
        );
    }

    #[test]
    fn extract_folder_name_rejects_length_beyond_buffer() {
        // Length that would read past the end of bytes.
        let mut fstr = Vec::new();
        fstr.extend_from_slice(&100i32.to_le_bytes());
        fstr.extend_from_slice(&[b'A'; 10]); // only 10 bytes, not 100
        let bytes = fake_header(&fstr);
        assert!(
            extract_package_folder_name(&bytes).is_none(),
            "length beyond buffer must yield None rather than OOB panic"
        );
    }

    #[test]
    fn extract_folder_name_trims_trailing_null_only() {
        // Three-char name "Abc" stored as ASCII with null ⇒ len=+4.
        // Decoder must strip exactly the trailing null, not the `c`.
        let mut fstr = Vec::new();
        fstr.extend_from_slice(&4i32.to_le_bytes());
        fstr.extend_from_slice(b"Abc\0");
        let bytes = fake_header(&fstr);
        assert_eq!(
            extract_package_folder_name(&bytes).as_deref(),
            Some("Abc"),
            "decoder must strip ONLY the single trailing null"
        );
    }

    #[test]
    fn resolve_legacy_target_filename_prefers_url_hint_over_opaque_source_stem() {
        let mut fstr = Vec::new();
        fstr.extend_from_slice(&5i32.to_le_bytes());
        fstr.extend_from_slice(b"None\0");
        let bytes = fake_header(&fstr);
        let source_gpk = Path::new("foglio1024.ui-remover-flight-gauge.gpk");

        let resolved =
            resolve_legacy_target_filename(&bytes, source_gpk, Some("S1UI_ProgressBar.gpk"));

        assert_eq!(resolved.as_deref(), Some("S1UI_ProgressBar.gpk"));
    }

    // --------------------------------------------------------------------
    // install_legacy_gpk / uninstall_legacy_gpk integration tests.
    // Covers the full path: parse UE3 header → copy into CookedPC →
    // patch mapper → (uninstall: restore from backup). These functions
    // shipped in iter-228 to fix the user-reported flight-gauge bug —
    // drop-in copy alone left the mapper pointing at the vanilla bytes,
    // so mods silently did nothing. Without integration coverage, a
    // refactor that broke the mapper-rewrite step would re-introduce
    // that bug.
    // --------------------------------------------------------------------

    /// Writes a fake .gpk (UE3 header with folder_name) plus some extra
    /// bytes so the file has realistic size. Returns path to the file.
    fn write_fake_gpk(dir: &Path, filename: &str, folder_name: &str) -> std::path::PathBuf {
        let mut fstr = Vec::new();
        let len = (folder_name.len() + 1) as i32; // +null
        fstr.extend_from_slice(&len.to_le_bytes());
        fstr.extend_from_slice(folder_name.as_bytes());
        fstr.push(0);
        let mut bytes = fake_header(&fstr);
        // Pad to 1 KiB so the file has a non-trivial size.
        bytes.extend_from_slice(&vec![0xCDu8; 1024 - bytes.len()]);
        let p = dir.join(filename);
        fs::write(&p, &bytes).unwrap();
        p
    }

    #[test]
    fn install_legacy_gpk_rewrites_matching_mapper_entries() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();

        // Seed the game with a mapper that has:
        //   entry1 — object_path = "SomePackage.S1UI_Gauge" (matches suffix .S1UI_Gauge)
        //   entry2 — object_path = "Other.Foo"              (does NOT match)
        let vanilla_map = mapper_with(&[
            ("S1UI_Gauge", "SomePackage.S1UI_Gauge", "S1Data1.gpk"),
            ("Other", "Other.Foo", "S1Data2.gpk"),
        ]);
        write_mapper_at(game_root, &vanilla_map);

        // Write the mod file outside CookedPC/ so the install path copies
        // it in explicitly (mirrors production add-from-file flow).
        let source_gpk = write_fake_gpk(game_root, "mod-flight-gauge.gpk", "S1UI_Gauge");

        let target_name = install_legacy_gpk(game_root, &source_gpk, None)
            .expect("install must succeed on matching mapper entry");
        assert_eq!(target_name, "S1UI_Gauge.gpk");

        // Mod file is in CookedPC under the target name.
        let cooked = game_root.join(COOKED_PC_DIR);
        assert!(cooked.join("S1UI_Gauge.gpk").exists());

        // Mapper must now point the matching entry at the new file;
        // the non-matching entry must be untouched.
        let patched_enc = fs::read(cooked.join(MAPPER_FILE)).unwrap();
        let patched_plain = decrypt_mapper(&patched_enc);
        let patched_map = parse_mapper(&String::from_utf8_lossy(&patched_plain));

        let matched = patched_map
            .get("S1UI_Gauge")
            .expect("matched entry must still exist by composite key");
        assert_eq!(
            matched.filename, "S1UI_Gauge.gpk",
            "filename must be rewritten"
        );
        assert_eq!(matched.offset, 0, "offset must be zeroed");
        assert!(matched.size > 0, "size must match the installed file");

        let unmatched = patched_map
            .get("Other")
            .expect("non-matching entry must remain");
        assert_eq!(
            unmatched.filename, "S1Data2.gpk",
            "non-matching entry must be untouched"
        );
    }

    #[test]
    fn install_legacy_gpk_succeeds_as_standalone_when_no_matching_mapper_entry() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();

        // Mapper has no entry ending in `.UnknownPkg`.
        // Standalone files are natively overridden just by dropping them in CookedPC.
        let map = mapper_with(&[("Other", "Other.Foo", "S1Data.gpk")]);
        write_mapper_at(game_root, &map);

        let source_gpk = write_fake_gpk(game_root, "mod-orphan.gpk", "UnknownPkg");
        let target_name = install_legacy_gpk(game_root, &source_gpk, None)
            .expect("install must succeed as standalone file");

        assert_eq!(target_name, "UnknownPkg.gpk");
        assert!(game_root
            .join(COOKED_PC_DIR)
            .join("UnknownPkg.gpk")
            .exists());
    }

    #[test]
    fn install_legacy_gpk_falls_back_to_filename_when_header_name_is_none() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();

        let vanilla_map = mapper_with(&[(
            "S1UI_ProgressBar",
            "SomePackage.S1UI_ProgressBar",
            "S1Data1.gpk",
        )]);
        write_mapper_at(game_root, &vanilla_map);

        let source_gpk = write_fake_gpk(game_root, "S1UI_ProgressBar.gpk", "None");

        let target_name = install_legacy_gpk(game_root, &source_gpk, None)
            .expect("filename fallback must deploy remover-style gpks");
        assert_eq!(target_name, "S1UI_ProgressBar.gpk");

        let patched_enc = fs::read(game_root.join(COOKED_PC_DIR).join(MAPPER_FILE)).unwrap();
        let patched_plain = decrypt_mapper(&patched_enc);
        let patched_map = parse_mapper(&String::from_utf8_lossy(&patched_plain));
        let matched = patched_map.get("S1UI_ProgressBar").unwrap();
        assert_eq!(matched.filename, "S1UI_ProgressBar.gpk");
        assert_eq!(matched.offset, 0);
        assert!(matched.size > 0);
    }

    #[test]
    fn install_legacy_gpk_removes_pkg_mapper_entries_for_progress_bar() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();

        let vanilla_map = mapper_with(&[("Other", "Other.Foo", "S1Data1.gpk")]);
        write_mapper_at(game_root, &vanilla_map);
        write_pkg_mapper_at(
            game_root,
            &pkg_mapper_text(&[
                ("S1UI_ProgressBar.ProgressBar_IF", "uid_if"),
                ("S1UI_ProgressBar.ProgressBar_IC", "uid_ic"),
                ("S1UI_Chat2.Chat2", "uid_chat"),
            ]),
        );

        let source_gpk =
            write_fake_gpk(game_root, "foglio1024.ui-remover-flight-gauge.gpk", "None");
        let target_name = install_legacy_gpk(game_root, &source_gpk, Some("S1UI_ProgressBar.gpk"))
            .expect("standalone install must succeed");

        assert_eq!(target_name, "S1UI_ProgressBar.gpk");
        let encrypted = fs::read(game_root.join(COOKED_PC_DIR).join(PKG_MAPPER_FILE)).unwrap();
        let text = String::from_utf8_lossy(&decrypt_mapper(&encrypted)).to_string();
        assert!(!text.contains("S1UI_ProgressBar.ProgressBar_IF"));
        assert!(!text.contains("S1UI_ProgressBar.ProgressBar_IC"));
        assert!(text.contains("S1UI_Chat2.Chat2"));
        assert!(game_root
            .join(COOKED_PC_DIR)
            .join(PKG_MAPPER_BACKUP_FILE)
            .exists());
    }

    #[test]
    fn uninstall_legacy_gpk_restores_vanilla_from_backup() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked).unwrap();

        // Seed: vanilla .gpk in CookedPC/, plus a .vanilla-bak that
        // install_legacy_gpk would have created before overwriting.
        let vanilla_bytes = b"VANILLA-GPK-BYTES".to_vec();
        let mod_bytes = b"MOD-GPK-BYTES-DIFFERENT".to_vec();
        fs::write(cooked.join("S1UI_Party.gpk"), &mod_bytes).unwrap();
        fs::write(cooked.join("S1UI_Party.gpk.vanilla-bak"), &vanilla_bytes).unwrap();

        uninstall_legacy_gpk(game_root, "S1UI_Party.gpk")
            .expect("uninstall must succeed when backup exists");

        let restored = fs::read(cooked.join("S1UI_Party.gpk")).unwrap();
        assert_eq!(
            restored, vanilla_bytes,
            "uninstall must restore the vanilla bytes from .vanilla-bak"
        );
        assert!(
            !cooked.join("S1UI_Party.gpk.vanilla-bak").exists(),
            "backup must be removed after a successful restore"
        );
    }

    #[test]
    fn uninstall_legacy_gpk_removes_modded_file_when_no_backup() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked).unwrap();

        // Case: the vanilla slot was EMPTY before install (no .vanilla-bak
        // created). Uninstall must still clean up by removing the mod .gpk.
        fs::write(cooked.join("S1UI_Chat.gpk"), b"MOD-GPK").unwrap();
        assert!(!cooked.join("S1UI_Chat.gpk.vanilla-bak").exists());

        uninstall_legacy_gpk(game_root, "S1UI_Chat.gpk")
            .expect("uninstall must succeed when no backup exists");
        assert!(
            !cooked.join("S1UI_Chat.gpk").exists(),
            "modded file must be removed when no backup to restore"
        );
    }

    #[test]
    fn restore_clean_gpk_state_restores_mappers_and_backed_up_containers() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked).unwrap();

        let clean_mapper = mapper_with(&[("Comp", "Obj.Package", "S1Common.gpk")]);
        let dirty_mapper = mapper_with(&[("Comp", "Obj.Package", "Modded.gpk")]);
        fs::write(
            cooked.join(BACKUP_FILE),
            encrypt_mapper(serialize_mapper(&clean_mapper).as_bytes()),
        )
        .unwrap();
        fs::write(
            cooked.join(MAPPER_FILE),
            encrypt_mapper(serialize_mapper(&dirty_mapper).as_bytes()),
        )
        .unwrap();

        let clean_pkg_mapper = pkg_mapper_text(&[("Obj.Package", "Comp.Package")]);
        let dirty_pkg_mapper = pkg_mapper_text(&[("Obj.Package", "Dirty.Package")]);
        fs::write(
            cooked.join(PKG_MAPPER_BACKUP_FILE),
            encrypt_mapper(clean_pkg_mapper.as_bytes()),
        )
        .unwrap();
        fs::write(
            cooked.join(PKG_MAPPER_FILE),
            encrypt_mapper(dirty_pkg_mapper.as_bytes()),
        )
        .unwrap();

        fs::write(cooked.join("S1Common.gpk"), b"TRUNCATED-MODDED").unwrap();
        fs::write(cooked.join("S1Common.gpk.vanilla-bak"), b"FULL-VANILLA").unwrap();
        let nested = cooked.join("Art_Data").join("Packages").join("S1UI");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("S1UI_PaperDoll.gpk"), b"NESTED-MODDED").unwrap();
        fs::write(
            nested.join("S1UI_PaperDoll.gpk.vanilla-bak"),
            b"NESTED-VANILLA",
        )
        .unwrap();

        restore_clean_gpk_state(game_root).expect("clean GPK restore must succeed");

        assert_eq!(
            fs::read(cooked.join("S1Common.gpk")).unwrap(),
            b"FULL-VANILLA",
            "container bytes must be restored before clean mapper offsets are trusted"
        );
        assert_eq!(
            fs::read(nested.join("S1UI_PaperDoll.gpk")).unwrap(),
            b"NESTED-VANILLA",
            "nested standalone GPK backups under CookedPC must also be restored"
        );
        assert_eq!(
            fs::read(cooked.join(MAPPER_FILE)).unwrap(),
            fs::read(cooked.join(BACKUP_FILE)).unwrap(),
            "composite mapper must match clean backup"
        );
        assert_eq!(
            fs::read(cooked.join(PKG_MAPPER_FILE)).unwrap(),
            fs::read(cooked.join(PKG_MAPPER_BACKUP_FILE)).unwrap(),
            "package mapper must match clean backup"
        );
    }

    #[test]
    fn uninstall_legacy_gpk_restores_pkg_mapper_entries_from_backup() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked).unwrap();

        let current_pkg = pkg_mapper_text(&[("S1UI_Chat2.Chat2", "uid_chat")]);
        let backup_pkg = pkg_mapper_text(&[
            ("S1UI_ProgressBar.ProgressBar_IF", "uid_if"),
            ("S1UI_ProgressBar.ProgressBar_IC", "uid_ic"),
            ("S1UI_Chat2.Chat2", "uid_chat"),
        ]);
        fs::write(
            cooked.join(PKG_MAPPER_FILE),
            encrypt_mapper(current_pkg.as_bytes()),
        )
        .unwrap();
        fs::write(
            cooked.join(PKG_MAPPER_BACKUP_FILE),
            encrypt_mapper(backup_pkg.as_bytes()),
        )
        .unwrap();
        fs::write(cooked.join("S1UI_ProgressBar.gpk"), b"MODDED").unwrap();

        uninstall_legacy_gpk(game_root, "S1UI_ProgressBar.gpk")
            .expect("uninstall must restore pkg mapper entries");

        let encrypted = fs::read(cooked.join(PKG_MAPPER_FILE)).unwrap();
        let text = String::from_utf8_lossy(&decrypt_mapper(&encrypted)).to_string();
        assert!(text.contains("S1UI_ProgressBar.ProgressBar_IF"));
        assert!(text.contains("S1UI_ProgressBar.ProgressBar_IC"));
        assert!(text.contains("S1UI_Chat2.Chat2"));
    }

    #[test]
    fn uninstall_legacy_gpk_rejects_hostile_filename() {
        let tmp = TempDir::new().unwrap();
        let err = uninstall_legacy_gpk(tmp.path(), "../../../Windows/System32/foo.gpk")
            .expect_err("path-traversal attempt must be rejected");
        assert!(
            err.contains("not a safe filename"),
            "error must flag filename as unsafe; got: {err}"
        );
    }
}
