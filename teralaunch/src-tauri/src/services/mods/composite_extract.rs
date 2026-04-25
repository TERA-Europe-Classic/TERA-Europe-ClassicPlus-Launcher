//! Extracts the vanilla bytes for a target package from the user's
//! composite container.
//!
//! TERA's `CompositePackageMapper.dat` maps `(filename, offset, size)` for
//! every composite-routed object. Patch-based deploy needs the *raw vanilla
//! package bytes* for a target like `S1UI_GageBoss.S1UI_GageBoss` so it can
//! diff or apply against them. The vanilla `.clean` backup of the mapper is
//! the source of truth: even if a previous mod has redirected the live
//! mapper, `.clean` still points at the unmodded composite container at the
//! unmodded offset.
//!
//! This module is intentionally small — one read path, one error per
//! failure mode — so the install path can compose it with
//! `patch_derivation::derive_manifest` without owning the mapper IO itself.

use std::fs;
use std::path::Path;

use super::gpk::{
    decrypt_mapper, get_entry_by_incomplete_object_path, get_entry_by_object_path, parse_mapper,
    BACKUP_FILE, COOKED_PC_DIR,
};

pub fn extract_vanilla_for_object_path(
    game_root: &Path,
    object_path: &str,
) -> Result<Vec<u8>, String> {
    let cooked_pc = game_root.join(COOKED_PC_DIR);
    let clean = cooked_pc.join(BACKUP_FILE);
    if !clean.exists() {
        return Err(format!(
            "CompositePackageMapper.clean missing at {} — can't resolve vanilla bytes for '{}'. Run 'verify game files', then retry.",
            clean.display(),
            object_path
        ));
    }
    let bytes = fs::read(&clean).map_err(|e| {
        format!(
            "Failed to read CompositePackageMapper.clean at {}: {e}",
            clean.display()
        )
    })?;
    let plain = String::from_utf8_lossy(&decrypt_mapper(&bytes)).to_string();
    let map = parse_mapper(&plain);

    let entry = get_entry_by_object_path(&map, object_path)
        .or_else(|| get_entry_by_incomplete_object_path(&map, object_path))
        .ok_or_else(|| {
            format!(
                "object_path '{object_path}' not found in vanilla CompositePackageMapper.clean — your game version may not match the mod"
            )
        })?;

    let container_path = cooked_pc.join(&entry.filename);
    let container = fs::read(&container_path).map_err(|e| {
        format!(
            "Failed to read composite container {}: {e}",
            container_path.display()
        )
    })?;
    let off = if entry.offset < 0 {
        return Err(format!(
            "Vanilla mapper has a negative offset for '{object_path}' — refusing to extract"
        ));
    } else {
        entry.offset as usize
    };
    let size = if entry.size < 0 {
        return Err(format!(
            "Vanilla mapper has a negative size for '{object_path}' — refusing to extract"
        ));
    } else {
        entry.size as usize
    };
    let end = off.checked_add(size).ok_or_else(|| {
        format!("Vanilla mapper offset+size overflow for '{object_path}'")
    })?;
    if end > container.len() {
        return Err(format!(
            "Vanilla offset+size ({off}+{size}={end}) exceeds container length {} in {}",
            container.len(),
            container_path.display()
        ));
    }
    Ok(container[off..end].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::mods::gpk::encrypt_mapper;
    use crate::services::mods::test_fixtures::build_boss_window_test_package;
    use tempfile::TempDir;

    fn write_mapper(path: &Path, plain_text: &str) {
        fs::write(path, encrypt_mapper(plain_text.as_bytes())).unwrap();
    }

    #[test]
    fn returns_slice_at_resolved_offset() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();

        // Synthetic container with two packages back-to-back.
        let pkg_a = build_boss_window_test_package([0xA0, 0xA1, 0xA2, 0xA3], false);
        let pkg_b = build_boss_window_test_package([0xB0, 0xB1, 0xB2, 0xB3], false);
        let mut container = Vec::new();
        container.extend_from_slice(&pkg_a);
        let off_b = container.len() as i64;
        let size_b = pkg_b.len() as i64;
        container.extend_from_slice(&pkg_b);
        fs::write(cooked_pc.join("S1UI_GageBoss.gpk"), &container).unwrap();

        let mapper_text = format!(
            "S1UI_GageBoss.gpk?GageBossModded.GageBoss,Comp,{off_b},{size_b},|!"
        );
        write_mapper(&cooked_pc.join(BACKUP_FILE), &mapper_text);

        let extracted =
            extract_vanilla_for_object_path(game_root, "GageBossModded.GageBoss").unwrap();
        assert_eq!(extracted, pkg_b);
    }

    #[test]
    fn errors_when_object_not_in_clean_mapper() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        write_mapper(
            &cooked_pc.join(BACKUP_FILE),
            "S1UI_Other.gpk?Foo.Bar,X,0,10,|!",
        );

        let err =
            extract_vanilla_for_object_path(game_root, "Missing.Path").unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn errors_when_clean_backup_missing() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();

        let err = extract_vanilla_for_object_path(game_root, "Anything.Foo").unwrap_err();
        assert!(
            err.contains("CompositePackageMapper.clean missing"),
            "got: {err}"
        );
    }

    #[test]
    fn errors_when_container_missing() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        write_mapper(
            &cooked_pc.join(BACKUP_FILE),
            "Missing_Container.gpk?Foo.Bar,X,0,10,|!",
        );

        let err = extract_vanilla_for_object_path(game_root, "Foo.Bar").unwrap_err();
        assert!(
            err.contains("Failed to read composite container"),
            "got: {err}"
        );
    }

    #[test]
    fn errors_when_offset_exceeds_container_length() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();

        fs::write(cooked_pc.join("Tiny.gpk"), b"abcd").unwrap();
        write_mapper(
            &cooked_pc.join(BACKUP_FILE),
            "Tiny.gpk?Foo.Bar,X,100,200,|!",
        );

        let err = extract_vanilla_for_object_path(game_root, "Foo.Bar").unwrap_err();
        assert!(err.contains("exceeds container length"), "got: {err}");
    }
}
