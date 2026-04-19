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
const PACKAGE_MAGIC: u32 = 0x9E2A83C1;

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
    let mut out = HashMap::new();
    let bytes = decrypted.as_bytes();
    let mut pos_end = 0usize;

    loop {
        let Some(q) = find_from(bytes, b'?', pos_end) else { break };
        let filename = decrypted[pos_end..q].to_string();
        let Some(bang) = find_from(bytes, b'!', q) else { break };
        let mut pos = q + 1;
        while pos < bang {
            // object_path
            let Some(c1) = find_from(bytes, b',', pos) else { break };
            let object_path = &decrypted[pos..c1];
            pos = c1 + 1;
            // composite_name
            let Some(c2) = find_from(bytes, b',', pos) else { break };
            let composite_name = &decrypted[pos..c2];
            pos = c2 + 1;
            // offset
            let Some(c3) = find_from(bytes, b',', pos) else { break };
            let offset_s = &decrypted[pos..c3];
            pos = c3 + 1;
            // size
            let Some(c4) = find_from(bytes, b',', pos) else { break };
            let size_s = &decrypted[pos..c4];
            pos = c4 + 1;

            let entry = MapperEntry {
                filename: filename.clone(),
                object_path: object_path.to_string(),
                composite_name: composite_name.to_string(),
                offset: offset_s.parse::<i64>().unwrap_or(0),
                size: size_s.parse::<i64>().unwrap_or(0),
            };
            out.insert(entry.composite_name.clone(), entry);

            // Next entry (TMM uses `decrypted.find('|', pos) < posEnd - 1` as
            // the loop condition; we simply check for '|' before the bang).
            if pos >= bang { break; }
            if bytes[pos] == b'|' { pos += 1; }
            if pos >= bang || find_from(bytes, b'|', pos).map(|p| p < bang).unwrap_or(false) {
                // continue loop — there's another entry before the '!'
            } else {
                break;
            }
        }
        pos_end = bang + 1;
    }
    out
}

fn find_from(hay: &[u8], needle: u8, from: usize) -> Option<usize> {
    hay.get(from..)?.iter().position(|&b| b == needle).map(|p| p + from)
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
        if entries.is_empty() { continue; }
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
        (Some((ca, oa)), Some((cb, ob))) => ca.eq_ignore_ascii_case(cb) && oa.eq_ignore_ascii_case(ob),
        _ => false,
    }
}

pub fn get_entry_by_object_path<'a>(
    map: &'a HashMap<String, MapperEntry>,
    path: &str,
) -> Option<&'a MapperEntry> {
    map.values().find(|e| e.object_path.eq_ignore_ascii_case(path))
}

pub fn get_entry_by_incomplete_object_path<'a>(
    map: &'a HashMap<String, MapperEntry>,
    path: &str,
) -> Option<&'a MapperEntry> {
    map.values().find(|e| incomplete_paths_equal(&e.object_path, path))
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
        if *p < 4 { return Err("Unexpected EOF while reading mod footer".into()); }
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

    m.packages.resize(composite_count as usize, ModPackage::default());
    for (i, &pkg_off) in offsets.iter().enumerate() {
        let p = parse_composite_package(bytes, pkg_off)?;
        m.packages[i] = p;
        if i > 0 {
            // Size of the PREVIOUS package is (this package's offset) minus its own offset.
            m.packages[i - 1].size =
                (pkg_off as i64) - m.packages[i - 1].offset;
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

fn mapper_path(game_root: &Path) -> PathBuf {
    game_root.join(COOKED_PC_DIR).join(MAPPER_FILE)
}

fn backup_path(game_root: &Path) -> PathBuf {
    game_root.join(COOKED_PC_DIR).join(BACKUP_FILE)
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
pub(crate) fn is_safe_gpk_container_filename(name: &str) -> bool {
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
    let gpk_bytes =
        fs::read(source_gpk).map_err(|e| format!("Failed to read mod file: {}", e))?;
    let modfile = parse_mod_file(&gpk_bytes)?;
    if modfile.container.is_empty() {
        return Err(
            "Mod file has no TMM container name — this .gpk is not TMM-compatible."
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
            "Mod file has a composite package with no object path — can't be installed."
                .into(),
        );
    }

    ensure_backup(game_root)?;

    // Copy the mod file into CookedPC with the exact container filename TMM
    // expects. The game loads whatever the mapper points at, so the filename
    // must match the mapper entries we're about to write.
    let dest_gpk = game_root
        .join(COOKED_PC_DIR)
        .join(&modfile.container);
    fs::create_dir_all(dest_gpk.parent().unwrap_or_else(|| Path::new(".")))
        .map_err(|e| format!("Failed to create CookedPC dir: {}", e))?;
    fs::copy(source_gpk, &dest_gpk)
        .map_err(|e| format!("Failed to copy mod into CookedPC: {}", e))?;

    // Load and decrypt current mapper.
    let mapper_bytes = fs::read(mapper_path(game_root))
        .map_err(|e| format!("Failed to read mapper: {}", e))?;
    let decrypted = decrypt_mapper(&mapper_bytes);
    let decrypted_str = String::from_utf8_lossy(&decrypted).to_string();
    let mut map = parse_mapper(&decrypted_str);

    // Patch entries for each of the mod's packages. We keep the existing
    // (composite_name, object_path) so the game still looks them up the
    // same way; we only repoint (filename, offset, size).
    for pkg in &modfile.packages {
        let existing = if modfile.region_lock {
            get_entry_by_object_path(&map, &pkg.object_path)
        } else {
            get_entry_by_incomplete_object_path(&map, &pkg.object_path)
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
        entry.filename = modfile.container.clone();
        entry.offset = pkg.offset;
        entry.size = pkg.size;
        map.insert(entry.composite_name.clone(), entry);
    }

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
pub fn uninstall_gpk(game_root: &Path, container: &str, object_paths: &[String]) -> Result<(), String> {
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

    let current_bytes = fs::read(mapper_path(game_root))
        .map_err(|e| format!("Failed to read mapper: {}", e))?;
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

    let serialized = serialize_mapper(&map);
    let encrypted = encrypt_mapper(serialized.as_bytes());
    fs::write(mapper_path(game_root), &encrypted)
        .map_err(|e| format!("Failed to write mapper: {}", e))?;

    // Remove the container .gpk we copied in.
    let dest_gpk = game_root.join(COOKED_PC_DIR).join(container);
    if dest_gpk.exists() {
        fs::remove_file(&dest_gpk)
            .map_err(|e| format!("Failed to remove mod gpk: {}", e))?;
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
        assert!(!incomplete_paths_equal(
            "S1UI_Foo.Bar",
            "Other_Foo.Bar"
        ));

        // Same composite prefix, different post-. → no match
        assert!(!incomplete_paths_equal(
            "S1UI_Foo.Bar",
            "S1UI_Foo.Baz"
        ));
    }

    #[test]
    fn parse_mod_file_rejects_non_tmm_gpks() {
        // A .gpk with no TMM magic footer should return a default ModFile
        // with one zero-length package and no container.
        let bytes = vec![0x42u8; 64];
        let m = parse_mod_file(&bytes).unwrap();
        assert!(m.container.is_empty());
        assert_eq!(m.packages.len(), 1);
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
            "foo..bar.gpk",  // embedded .. too (prevents creative normalisation bypasses)
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
}
