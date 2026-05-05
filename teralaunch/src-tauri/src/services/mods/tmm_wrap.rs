// Shared between the main launcher bin and several experimental tooling
// bins via `#[path = ...]` includes; each compilation context exercises
// a different subset, so any single bin sees the rest as "dead".
#![allow(dead_code)]

//! TMM mod-file footer writer.
//!
//! Inverse of `gpk::parse_mod_file`: takes one or more inner GPK byte
//! buffers + metadata (container filename, mod_name, mod_author, per-package
//! object_paths) and emits a single TMM-format mod file with a v1 footer.
//!
//! On-disk layout produced by `wrap_as_tmm`:
//!
//! ```text
//! [composite_package_0_bytes]      ← inner GPK 0 (already begins with package magic)
//! [composite_package_1_bytes]      ← inner GPK 1 (if multi-package)
//! ...
//! --- footer begins here ---
//! [author_string  : i32 len + ANSI bytes (no null)]
//! [name_string    : i32 len + ANSI bytes (no null)]
//! [container_str  : i32 len + ANSI bytes (no null)]
//! [composite_offsets[N] : i32 each]   ← absolute offsets of each inner GPK
//! [version=MAGIC : u32]                ← v1 sentinel
//! [region_lock   : i32 = 0]
//! [author_offset : i32]
//! [name_offset   : i32]
//! [container_offset : i32]
//! [offsets_offset : i32]
//! [composite_count : i32 = N]
//! [meta_size : i32 = footer_size]
//! [MAGIC : u32]                        ← terminator parse_mod_file looks for
//! ```
//!
//! Reverse-engineered from `services/mods/gpk.rs::parse_mod_file` (lines
//! 515-606) and `parse_composite_package` (lines 609-627). The composite
//! package header itself doesn't need to be rewritten — TMM reads
//! `file_version`/`licensee_version` directly from the embedded GPK header
//! at `offset+4..offset+8`, and the `MOD:<object_path>` folder name from
//! the embedded GPK's package-name FString at `offset+12`. So as long as
//! the inner GPK is valid and has its package name set to `"MOD:<path>"`
//! (composite_author already does this), the footer just needs the
//! per-package offsets table.

use super::gpk::PACKAGE_MAGIC;

#[derive(Debug, Clone)]
pub struct TmmComposite {
    /// Bytes of one embedded GPK. Must already have its package name set
    /// to `"MOD:<object_path>"` (matches what `parse_composite_package`
    /// expects at `offset+12`).
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct TmmModSpec {
    /// Filename TMM places into `CookedPC/`. Must satisfy
    /// `is_safe_gpk_container_filename`.
    pub container: String,
    pub mod_name: String,
    pub mod_author: String,
    pub composites: Vec<TmmComposite>,
}

/// Build a complete TMM mod file. Concatenates each composite's bytes, then
/// appends the v1 footer with strings, the offsets array, and the trailing
/// header that `parse_mod_file` walks backwards from.
pub fn wrap_as_tmm(spec: &TmmModSpec) -> Result<Vec<u8>, String> {
    if spec.composites.is_empty() {
        return Err("wrap_as_tmm: at least one composite required".into());
    }
    if spec.container.is_empty() {
        return Err("wrap_as_tmm: container filename required".into());
    }

    let mut out: Vec<u8> = Vec::new();
    let mut composite_offsets: Vec<i32> = Vec::with_capacity(spec.composites.len());
    for c in &spec.composites {
        let off = out.len() as i32;
        composite_offsets.push(off);
        out.extend_from_slice(&c.bytes);
    }

    // String section: author, name, container — each as ANSI FString
    // without trailing null. (parse_prefixed_string reads exactly `len`
    // bytes and constructs the string from them; including a null would
    // leave a trailing \0 in `modfile.container`, which install_gpk then
    // tries to use as a filename.)
    let author_offset = out.len() as i32;
    write_prefixed_ansi_no_null(&mut out, &spec.mod_author);

    let name_offset = out.len() as i32;
    write_prefixed_ansi_no_null(&mut out, &spec.mod_name);

    let container_offset = out.len() as i32;
    write_prefixed_ansi_no_null(&mut out, &spec.container);

    // Offsets array: one i32 per composite package, in declaration order.
    let offsets_offset = out.len() as i32;
    for off in &composite_offsets {
        out.extend_from_slice(&off.to_le_bytes());
    }

    // The trailing header is fixed-size for v1 mode: 8 i32 slots + final
    // u32 magic = 36 bytes. meta_size is the byte distance from the start
    // of the footer (immediately after the last composite package) to EOF.
    let footer_start = composite_offsets.last().copied().unwrap_or(0) as usize
        + spec.composites.last().map(|c| c.bytes.len()).unwrap_or(0);
    let trailing_block_size = 8 * 4 + 4; // 8 i32s + magic
    let final_size = out.len() + trailing_block_size;
    let meta_size = (final_size - footer_start) as i32;

    let composite_count = spec.composites.len() as i32;
    let region_lock: i32 = 0;
    let version_slot: i32 = PACKAGE_MAGIC as i32; // v1: this slot equals MAGIC

    // Forward-write order (parse walks backwards from MAGIC):
    out.extend_from_slice(&version_slot.to_le_bytes());
    out.extend_from_slice(&region_lock.to_le_bytes());
    out.extend_from_slice(&author_offset.to_le_bytes());
    out.extend_from_slice(&name_offset.to_le_bytes());
    out.extend_from_slice(&container_offset.to_le_bytes());
    out.extend_from_slice(&offsets_offset.to_le_bytes());
    out.extend_from_slice(&composite_count.to_le_bytes());
    out.extend_from_slice(&meta_size.to_le_bytes());
    out.extend_from_slice(&PACKAGE_MAGIC.to_le_bytes());

    Ok(out)
}

fn write_prefixed_ansi_no_null(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len() as i32;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

#[cfg(all(test, feature = "lib-tests"))]
mod tests {
    use super::*;
    use crate::services::mods::gpk::parse_mod_file;

    /// Build a minimal valid embedded composite-package buffer that
    /// `parse_composite_package` will accept. Layout from the parser:
    ///   off+0..4   : package magic 0x9E2A83C1 (read but unused here)
    ///   off+4..6   : file_version u16
    ///   off+6..8   : licensee_version u16
    ///   off+8..12  : padding / header fields (parser doesn't read 8..12)
    ///   off+12..   : prefixed string "MOD:<object_path>"
    fn synthesize_composite_with_modfolder(object_path: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&PACKAGE_MAGIC.to_le_bytes());
        buf.extend_from_slice(&897u16.to_le_bytes()); // file_version
        buf.extend_from_slice(&14u16.to_le_bytes()); // licensee_version
        buf.extend_from_slice(&0u32.to_le_bytes()); // padding for off+8..12
        // Then the FString at off+12:
        let folder = format!("MOD:{object_path}");
        let folder_bytes = folder.as_bytes();
        // composite_author writes WITH null terminator and parser
        // trim_end_matches('\0') strips it. Match that convention here so
        // the parsed object_path comes back clean.
        let len = folder_bytes.len() as i32 + 1;
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(folder_bytes);
        buf.push(0);
        // Pad to a non-empty body so `last.size = end_i64 - last.offset` > 0.
        buf.extend_from_slice(&[0u8; 32]);
        buf
    }

    #[test]
    fn round_trips_single_composite() {
        let inner = synthesize_composite_with_modfolder("S1UI_Test.TestObject_dup");
        let spec = TmmModSpec {
            container: "S1UI_Test_mod.gpk".to_string(),
            mod_name: "Test Mod".to_string(),
            mod_author: "Tester".to_string(),
            composites: vec![TmmComposite { bytes: inner.clone() }],
        };
        let wrapped = wrap_as_tmm(&spec).expect("wrap should succeed");
        let parsed = parse_mod_file(&wrapped).expect("parse should succeed");

        assert_eq!(parsed.container, "S1UI_Test_mod.gpk");
        assert_eq!(parsed.mod_name, "Test Mod");
        assert_eq!(parsed.mod_author, "Tester");
        assert_eq!(parsed.packages.len(), 1);
        assert_eq!(parsed.packages[0].object_path, "S1UI_Test.TestObject_dup");
        assert_eq!(parsed.packages[0].offset, 0);
        assert!(parsed.packages[0].size > 0);
    }

    #[test]
    fn round_trips_multiple_composites() {
        let inner_a = synthesize_composite_with_modfolder("S1UI_A.A_dup");
        let inner_b = synthesize_composite_with_modfolder("S1UI_B.B_dup");
        let spec = TmmModSpec {
            container: "S1UI_Multi_mod.gpk".to_string(),
            mod_name: "Multi".to_string(),
            mod_author: "Tester".to_string(),
            composites: vec![
                TmmComposite { bytes: inner_a.clone() },
                TmmComposite { bytes: inner_b.clone() },
            ],
        };
        let wrapped = wrap_as_tmm(&spec).expect("wrap should succeed");
        let parsed = parse_mod_file(&wrapped).expect("parse should succeed");

        assert_eq!(parsed.packages.len(), 2);
        assert_eq!(parsed.packages[0].object_path, "S1UI_A.A_dup");
        assert_eq!(parsed.packages[0].offset, 0);
        assert_eq!(parsed.packages[1].object_path, "S1UI_B.B_dup");
        assert_eq!(parsed.packages[1].offset, inner_a.len() as i64);
    }

    #[test]
    fn rejects_empty_composites() {
        let spec = TmmModSpec {
            container: "x.gpk".into(),
            mod_name: "x".into(),
            mod_author: "x".into(),
            composites: vec![],
        };
        assert!(wrap_as_tmm(&spec).is_err());
    }

    #[test]
    fn rejects_empty_container() {
        let inner = synthesize_composite_with_modfolder("S1UI_X.X_dup");
        let spec = TmmModSpec {
            container: "".into(),
            mod_name: "x".into(),
            mod_author: "x".into(),
            composites: vec![TmmComposite { bytes: inner }],
        };
        assert!(wrap_as_tmm(&spec).is_err());
    }
}
