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
    decrypt_mapper, get_entry_by_incomplete_object_path, get_entry_by_object_path,
    is_safe_gpk_container_filename, parse_mapper_strict, MapperEntry, BACKUP_FILE, COOKED_PC_DIR,
    PKG_MAPPER_BACKUP_FILE,
};

/// Convenience entry point for callers that only know the inner *package*
/// name (e.g. `"S1UI_ProgressBar"`, derived from the catalog's download URL
/// or the modded GPK header). Searches the vanilla mapper for any entry
/// whose `object_path` matches `package_name` or ends in `.<package_name>`,
/// then extracts the bytes at that entry's `(filename, offset, size)`.
///
/// Multiple mapper entries can name the same composite package via
/// different sub-objects; in TERA's classic mapper they all resolve to the
/// same byte range, so picking the first match is correct.
pub fn extract_vanilla_for_package_name(
    game_root: &Path,
    package_name: &str,
) -> Result<Vec<u8>, String> {
    let cooked_pc = game_root.join(COOKED_PC_DIR);
    let clean = cooked_pc.join(BACKUP_FILE);
    if !clean.exists() {
        return Err(format!(
            "CompositePackageMapper.clean missing at {} — can't resolve vanilla bytes for package '{}'. Run 'verify game files', then retry.",
            clean.display(),
            package_name
        ));
    }
    let bytes = fs::read(&clean).map_err(|e| {
        format!(
            "Failed to read CompositePackageMapper.clean at {}: {e}",
            clean.display()
        )
    })?;
    let plain = String::from_utf8_lossy(&decrypt_mapper(&bytes)).to_string();
    let map = parse_mapper_strict(&plain)?;

    let suffix = format!(".{package_name}");
    let entry = map.values().find(|e| {
        e.object_path.eq_ignore_ascii_case(package_name)
            || e.object_path
                .to_ascii_lowercase()
                .ends_with(&suffix.to_ascii_lowercase())
    });
    let entry = match entry {
        Some(entry) => entry,
        None => resolve_entry_via_pkg_mapper(&cooked_pc, &map, package_name)?.ok_or_else(|| {
            format!(
                "Package '{package_name}' not present in vanilla CompositePackageMapper.clean — your game version may not match the mod"
            )
        })?,
    };

    extract_from_container_entry(&cooked_pc, entry)
}

fn resolve_entry_via_pkg_mapper<'a>(
    cooked_pc: &Path,
    composite_map: &'a std::collections::HashMap<String, MapperEntry>,
    package_name: &str,
) -> Result<Option<&'a MapperEntry>, String> {
    let clean = cooked_pc.join(PKG_MAPPER_BACKUP_FILE);
    if !clean.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&clean)
        .map_err(|e| format!("Failed to read PkgMapper.clean at {}: {e}", clean.display()))?;
    let plain = String::from_utf8_lossy(&decrypt_mapper(&bytes)).to_string();
    let package_prefix = format!("{}.", package_name.to_ascii_lowercase());
    let mut resolved: Option<&MapperEntry> = None;

    for cell in plain.split('|') {
        let Some((uid, composite_uid)) = cell.trim().split_once(',') else {
            continue;
        };
        if !uid.to_ascii_lowercase().starts_with(&package_prefix) {
            continue;
        }
        let composite_uid = composite_uid.trim();
        let Some(entry) = composite_map.get(composite_uid).or_else(|| {
            composite_map
                .values()
                .find(|entry| entry.object_path.eq_ignore_ascii_case(composite_uid))
        }) else {
            continue;
        };
        if let Some(existing) = resolved {
            if existing.filename != entry.filename
                || existing.offset != entry.offset
                || existing.size != entry.size
            {
                return Err(format!(
                    "Package '{package_name}' maps to multiple vanilla composite byte ranges — use --object-path for an exact target"
                ));
            }
        } else {
            resolved = Some(entry);
        }
    }

    Ok(resolved)
}

fn extract_from_container_entry(
    cooked_pc: &Path,
    entry: &super::gpk::MapperEntry,
) -> Result<Vec<u8>, String> {
    if !is_safe_gpk_container_filename(&entry.filename) {
        return Err(format!(
            "Refusing to read unsafe composite container filename '{}' from vanilla mapper",
            entry.filename
        ));
    }
    let mut container_path = cooked_pc.join(&entry.filename);
    if !container_path.exists() && container_path.extension().is_none() {
        container_path = cooked_pc.join(format!("{}.gpk", entry.filename));
    }
    let backup_path = container_path.with_extension("gpk.vanilla-bak");
    let source_path = if backup_path.exists() {
        &backup_path
    } else {
        &container_path
    };
    let container = fs::read(source_path).map_err(|e| {
        format!(
            "Failed to read composite container {}: {e}",
            source_path.display()
        )
    })?;
    if entry.offset < 0 {
        return Err(format!(
            "Vanilla mapper has a negative offset for '{}' — refusing to extract",
            entry.object_path
        ));
    }
    if entry.size < 0 {
        return Err(format!(
            "Vanilla mapper has a negative size for '{}' — refusing to extract",
            entry.object_path
        ));
    }
    let off = entry.offset as usize;
    let size = entry.size as usize;
    let end = off.checked_add(size).ok_or_else(|| {
        format!(
            "Vanilla mapper offset+size overflow for '{}'",
            entry.object_path
        )
    })?;
    if end > container.len() {
        return Err(format!(
            "Vanilla offset+size ({off}+{size}={end}) exceeds container length {} in {}",
            container.len(),
            source_path.display()
        ));
    }
    Ok(container[off..end].to_vec())
}

/// Resolve a fully-qualified *logical* path like `"S1UI_PaperDoll.PaperDoll"`
/// to the vanilla bytes of the composite slice it routes to.
///
/// This is the lookup the engine itself performs at runtime: PkgMapper.clean
/// translates the logical path to a composite_object_path
/// (e.g. `"c7a706fb_268926b3_1ddcb.PaperDoll_dup"`); then
/// CompositePackageMapper.clean maps the composite_uid to
/// `(filename, offset, size)`. We then slice that range out of the
/// container.
///
/// Used by `vanilla_resolver` when the catalog entry specifies a
/// `target_object_path` qualifier — single-export disambiguation for
/// multi-object widget packages where `extract_vanilla_for_package_name`
/// would error with "maps to multiple vanilla composite byte ranges".
pub fn extract_vanilla_for_logical_path(
    game_root: &Path,
    logical_path: &str,
) -> Result<Vec<u8>, String> {
    let cooked_pc = game_root.join(COOKED_PC_DIR);
    let pkg_clean = cooked_pc.join(PKG_MAPPER_BACKUP_FILE);
    let comp_clean = cooked_pc.join(BACKUP_FILE);
    if !pkg_clean.exists() {
        return Err(format!(
            "PkgMapper.clean missing at {} — can't resolve vanilla bytes for logical path '{}'. Run 'verify game files', then retry.",
            pkg_clean.display(),
            logical_path
        ));
    }
    if !comp_clean.exists() {
        return Err(format!(
            "CompositePackageMapper.clean missing at {} — can't resolve vanilla bytes for logical path '{}'. Run 'verify game files', then retry.",
            comp_clean.display(),
            logical_path
        ));
    }

    // Step 1: PkgMapper logical → composite_object_path
    let pkg_bytes = fs::read(&pkg_clean)
        .map_err(|e| format!("Failed to read PkgMapper.clean at {}: {e}", pkg_clean.display()))?;
    let pkg_plain = String::from_utf8_lossy(&decrypt_mapper(&pkg_bytes)).to_string();
    let logical_lower = logical_path.to_ascii_lowercase();
    let composite_object_path = pkg_plain
        .split('|')
        .filter_map(|cell| cell.trim().split_once(','))
        .find_map(|(uid, comp)| {
            if uid.to_ascii_lowercase() == logical_lower {
                Some(comp.trim().to_string())
            } else {
                None
            }
        })
        .ok_or_else(|| format!(
            "Logical path '{logical_path}' not found in PkgMapper.clean — your game version may not match the mod, or the catalog target_object_path is misspelled"
        ))?;

    // Step 2: composite_uid (everything before the first dot) → CompositePackageMapper entry
    let composite_uid = composite_object_path
        .split('.')
        .next()
        .ok_or_else(|| {
            format!(
                "PkgMapper row for '{logical_path}' has no composite_uid prefix (got '{composite_object_path}')"
            )
        })?;

    let comp_bytes = fs::read(&comp_clean).map_err(|e| {
        format!(
            "Failed to read CompositePackageMapper.clean at {}: {e}",
            comp_clean.display()
        )
    })?;
    let comp_plain = String::from_utf8_lossy(&decrypt_mapper(&comp_bytes)).to_string();
    let comp_map = parse_mapper_strict(&comp_plain)?;
    let entry = comp_map.get(composite_uid).ok_or_else(|| {
        format!(
            "composite_uid '{composite_uid}' (resolved from logical path '{logical_path}') not found in CompositePackageMapper.clean"
        )
    })?;

    extract_from_container_entry(&cooked_pc, entry)
}

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
    let map = parse_mapper_strict(&plain)?;

    let entry = get_entry_by_object_path(&map, object_path)
        .or_else(|| get_entry_by_incomplete_object_path(&map, object_path))
        .ok_or_else(|| {
            format!(
                "object_path '{object_path}' not found in vanilla CompositePackageMapper.clean — your game version may not match the mod"
            )
        })?;

    extract_from_container_entry(&cooked_pc, entry)
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

        let mapper_text =
            format!("S1UI_GageBoss.gpk?GageBossModded.GageBoss,Comp,{off_b},{size_b},|!");
        write_mapper(&cooked_pc.join(BACKUP_FILE), &mapper_text);

        let extracted =
            extract_vanilla_for_object_path(game_root, "GageBossModded.GageBoss").unwrap();
        assert_eq!(extracted, pkg_b);
    }

    #[test]
    fn reads_vanilla_backup_container_when_live_container_is_patched() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();

        let vanilla_pkg = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], false);
        let patched_pkg = build_boss_window_test_package([0xAA, 0xBB, 0xCC, 0xDD], false);
        fs::write(cooked_pc.join("S1UI_GageBoss.gpk"), &patched_pkg).unwrap();
        fs::write(
            cooked_pc.join("S1UI_GageBoss.gpk.vanilla-bak"),
            &vanilla_pkg,
        )
        .unwrap();
        write_mapper(
            &cooked_pc.join(BACKUP_FILE),
            &format!(
                "S1UI_GageBoss.gpk?Owner.S1UI_GageBoss,Comp,0,{},|!",
                vanilla_pkg.len()
            ),
        );

        let extracted = extract_vanilla_for_package_name(game_root, "S1UI_GageBoss").unwrap();

        assert_eq!(extracted, vanilla_pkg);
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

        let err = extract_vanilla_for_object_path(game_root, "Missing.Path").unwrap_err();
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
    fn rejects_unsafe_mapper_container_filename_before_read() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        write_mapper(
            &cooked_pc.join(BACKUP_FILE),
            "..\\outside.gpk?Foo.Bar,X,0,10,|!",
        );

        let err = extract_vanilla_for_object_path(game_root, "Foo.Bar").unwrap_err();

        assert!(
            err.contains("unsafe composite container filename"),
            "got: {err}"
        );
    }

    #[test]
    fn extracts_extensionless_mapper_filename_from_gpk_container() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();

        let pkg = build_boss_window_test_package([0xC0, 0xC1, 0xC2, 0xC3], false);
        fs::write(cooked_pc.join("c7a706fb_154.gpk"), &pkg).unwrap();
        write_mapper(
            &cooked_pc.join(BACKUP_FILE),
            &format!("c7a706fb_154?Foo.Bar,X,0,{},|!", pkg.len()),
        );

        let extracted = extract_vanilla_for_object_path(game_root, "Foo.Bar").unwrap();
        assert_eq!(extracted, pkg);
    }

    #[test]
    fn package_name_lookup_resolves_via_object_path_suffix() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();

        let pkg_a = build_boss_window_test_package([0xA0, 0xA1, 0xA2, 0xA3], false);
        let pkg_b = build_boss_window_test_package([0xB0, 0xB1, 0xB2, 0xB3], false);
        let mut container = Vec::new();
        container.extend_from_slice(&pkg_a);
        let off_b = container.len() as i64;
        let size_b = pkg_b.len() as i64;
        container.extend_from_slice(&pkg_b);
        fs::write(cooked_pc.join("S1UI_GageBoss.gpk"), &container).unwrap();

        // Two entries — one whose object_path *equals* the package name,
        // one whose object_path *ends with* `.<package_name>`. Both should
        // resolve to the same vanilla bytes.
        let mapper_text =
            format!("S1UI_GageBoss.gpk?S1UI_OtherOwner.S1UI_FlightBar,Comp,{off_b},{size_b},|!");
        write_mapper(&cooked_pc.join(BACKUP_FILE), &mapper_text);

        let extracted = extract_vanilla_for_package_name(game_root, "S1UI_FlightBar").unwrap();
        assert_eq!(extracted, pkg_b);
    }

    #[test]
    fn package_name_lookup_errors_when_not_present_in_clean_mapper() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        write_mapper(
            &cooked_pc.join(BACKUP_FILE),
            "S1UI_Other.gpk?Foo.Bar,X,0,10,|!",
        );

        let err = extract_vanilla_for_package_name(game_root, "S1UI_NotThere").unwrap_err();
        assert!(err.contains("not present"), "got: {err}");
    }

    #[test]
    fn package_name_lookup_resolves_via_pkg_mapper_uid() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path();
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();

        let pkg_a = build_boss_window_test_package([0xA0, 0xA1, 0xA2, 0xA3], false);
        let pkg_b = build_boss_window_test_package([0xB0, 0xB1, 0xB2, 0xB3], false);
        let mut container = Vec::new();
        container.extend_from_slice(&pkg_a);
        let off_b = container.len() as i64;
        let size_b = pkg_b.len() as i64;
        container.extend_from_slice(&pkg_b);
        fs::write(cooked_pc.join("17d87899_1.gpk"), &container).unwrap();

        write_mapper(
            &cooked_pc.join(BACKUP_FILE),
            &format!("17d87899_1.gpk?c_inventory.Inventory_dup,c_inventory,{off_b},{size_b},|!"),
        );
        write_mapper(
            &cooked_pc.join(PKG_MAPPER_BACKUP_FILE),
            "S1UI_InventoryWindow.Inventory,c_inventory.Inventory_dup|",
        );

        let extracted =
            extract_vanilla_for_package_name(game_root, "S1UI_InventoryWindow").unwrap();
        assert_eq!(extracted, pkg_b);
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
