//! Patch-based GPK install / enable / disable. Two deploy shapes are
//! supported:
//!
//! - **Composite-routed (Type A)**: vanilla bytes live inside a composite
//!   container at a `(filename, offset, size)` recorded in
//!   `CompositePackageMapper.clean`. Enable replaces that byte range inside
//!   the original container and shifts later same-container mapper offsets.
//!   Disable restores the mapper from `.clean` and copies the container
//!   `.vanilla-bak` back over the patched container.
//! - **Standalone-file (Type B)**: vanilla `<package>.gpk` lives at a
//!   deep filesystem path (e.g. `Art_Data/Packages/S1UI/<name>.gpk`).
//!   Enable backs the vanilla file up to `<path>.vanilla-bak` (once),
//!   then writes the patched bytes in place to `<path>`. Disable copies
//!   `.vanilla-bak` over `<path>`.
//!
//! Resolution kind is chosen at install time by `vanilla_resolver::
//! resolve_vanilla_for_package_name` and persisted in a per-mod
//! `install_target.json` sidecar so enable / disable always dispatch via
//! the same path that derived the manifest.
//!
//! Diff shapes the Phase 1 applier doesn't support (added exports, name /
//! import drift, class changes, or unsupported compression formats) are
//! refused at install time inside `patch_derivation::derive_manifest` —
//! strictly better than the legacy "install + break the client" failure mode.

use std::fs;
use std::path::{Path, PathBuf};

use super::manifest_store::InstallTarget;
use super::vanilla_resolver::VanillaSource;
use super::{
    gpk, gpk_package, gpk_patch_applier, manifest_store, patch_derivation, vanilla_resolver,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchInstallOutcome {
    /// The CookedPC standalone filename that enable will write to (e.g.
    /// `"S1UI_ProgressBar.gpk"`). Empty until enable runs.
    pub target_filename: String,
    /// The package name resolved from the modded GPK / URL hint
    /// (e.g. `"S1UI_ProgressBar"`).
    pub target_package_name: String,
}

pub fn install_via_patch(
    game_root: &Path,
    app_root: &Path,
    mod_id: &str,
    source_gpk: &Path,
    target_package_name: &str,
) -> Result<PatchInstallOutcome, String> {
    install_via_patch_with_qualifier(
        game_root,
        app_root,
        mod_id,
        source_gpk,
        target_package_name,
        None,
    )
}

/// Like `install_via_patch` but accepts an optional `target_object_path`
/// qualifier. When provided, the vanilla side is resolved by full
/// `Package.Object` rather than by package name — required for
/// multi-object widget packages where the package-only resolver errors
/// with "maps to multiple vanilla composite byte ranges".
pub fn install_via_patch_with_qualifier(
    game_root: &Path,
    app_root: &Path,
    mod_id: &str,
    source_gpk: &Path,
    target_package_name: &str,
    target_object_path: Option<&str>,
) -> Result<PatchInstallOutcome, String> {
    if target_package_name.trim().is_empty() {
        return Err("install_via_patch: target_package_name must not be empty".into());
    }
    let raw_modded = fs::read(source_gpk)
        .map_err(|e| format!("Failed to read modded GPK at {}: {e}", source_gpk.display()))?;

    let resolution = match target_object_path {
        Some(logical) if !logical.trim().is_empty() => {
            vanilla_resolver::resolve_vanilla_for_logical_path(game_root, logical.trim())?
        }
        _ => vanilla_resolver::resolve_vanilla_for_package_name(game_root, target_package_name)?,
    };

    // Refuse cleanly on x32-vs-x64 arch mismatch BEFORE attempting derivation.
    // v100.02 vanilla files are FileVersion 897 (x64); old Classic mods are
    // FileVersion 610 (x32). The byte structures don't correspond and the
    // engine's loader rejects the wrong-arch file. The legacy install path
    // produced a confusing parser error or a crashing client; this surfaces
    // the real problem (and points at who needs to fix it).
    let mod_version = gpk_package::read_file_version(&raw_modded).ok_or_else(|| {
        format!(
            "Modded GPK at {} is malformed (no GPK magic / FileVersion)",
            source_gpk.display()
        )
    })?;
    let vanilla_version = gpk_package::read_file_version(&resolution.bytes)
        .ok_or_else(|| "Vanilla GPK is malformed (no GPK magic / FileVersion)".to_string())?;
    if gpk_package::is_x64_file_version(mod_version)
        != gpk_package::is_x64_file_version(vanilla_version)
    {
        let arch_label = |fv: u16| {
            if gpk_package::is_x64_file_version(fv) {
                "x64 (v100.02 / Modern)"
            } else {
                "x32 (Classic / pre-patch 97)"
            }
        };
        return Err(format!(
            "Mod is {} but client is {} — incompatible. The mod was authored for the {} client and cannot run in the {} engine. Ask the mod author for a {}-rebuild.",
            arch_label(mod_version),
            arch_label(vanilla_version),
            arch_label(mod_version),
            arch_label(vanilla_version),
            arch_label(vanilla_version),
        ));
    }

    let modded = gpk_package::extract_uncompressed_package_bytes(&raw_modded).map_err(|e| {
        format!(
            "Failed to decompress modded GPK at {}: {e}",
            source_gpk.display()
        )
    })?;

    let manifest = patch_derivation::derive_manifest(mod_id, &resolution.bytes, &modded)?;
    manifest_store::save_manifest_at_root(app_root, mod_id, &manifest)?;
    manifest_store::save_raw_mod_package_at_root(app_root, mod_id, &raw_modded)?;

    let install_target = match &resolution.source {
        VanillaSource::Composite => InstallTarget::Composite {
            package_name: target_package_name.to_string(),
            object_path: target_object_path
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        },
        VanillaSource::Standalone { path } => InstallTarget::Standalone {
            relative_path: relative_to_game_root(game_root, path)?,
        },
    };
    manifest_store::save_install_target_at_root(app_root, mod_id, &install_target)?;

    Ok(PatchInstallOutcome {
        target_filename: format!("{target_package_name}.gpk"),
        target_package_name: target_package_name.to_string(),
    })
}

pub fn enable_via_patch(game_root: &Path, app_root: &Path, mod_id: &str) -> Result<(), String> {
    let manifest = manifest_store::load_manifest_at_root(app_root, mod_id)?
        .ok_or_else(|| format!("No persisted manifest for mod '{mod_id}' — reinstall required"))?;
    let target = manifest_store::load_install_target_at_root(app_root, mod_id)?
        .ok_or_else(|| {
            format!(
                "No install_target sidecar for '{mod_id}' — reinstall required so the launcher knows where to apply the patch"
            )
        })?;

    let raw_mod_package = manifest_store::load_raw_mod_package_at_root(app_root, mod_id)?;

    match target {
        InstallTarget::Composite { package_name, object_path } => enable_composite(
            game_root,
            &manifest,
            &package_name,
            object_path.as_deref(),
            raw_mod_package.as_deref(),
        ),
        InstallTarget::Standalone { relative_path } => {
            enable_standalone(game_root, &manifest, &relative_path)
        }
    }
}

pub fn disable_via_patch(game_root: &Path, app_root: &Path, mod_id: &str) -> Result<(), String> {
    let target = match manifest_store::load_install_target_at_root(app_root, mod_id)? {
        Some(t) => t,
        None => {
            return Err(format!(
                "No install_target sidecar for '{mod_id}' — refusing mapper-only restore; reinstall or repair GPK state"
            ));
        }
    };

    match target {
        InstallTarget::Composite { package_name, object_path } => {
            disable_composite_and_restore_mapper(game_root, &package_name, object_path.as_deref())?;
        }
        InstallTarget::Standalone { relative_path } => {
            disable_standalone(game_root, &relative_path)?;
            gpk::restore_clean_mapper_state(game_root)?;
        }
    }
    Ok(())
}

fn enable_composite(
    game_root: &Path,
    manifest: &super::patch_manifest::PatchManifest,
    target_package_name: &str,
    target_object_path: Option<&str>,
    raw_mod_package: Option<&[u8]>,
) -> Result<(), String> {
    if let Some(raw_mod_package) = raw_mod_package {
        return patch_composite_container_slice(
            game_root,
            target_package_name,
            target_object_path,
            raw_mod_package,
        );
    }

    let clean_entry = match target_object_path.filter(|s| !s.trim().is_empty()) {
        Some(logical) => resolve_clean_composite_entry_for_logical_path(game_root, logical)?,
        None => resolve_clean_composite_entry(game_root, target_package_name)?,
    };
    let raw_vanilla = extract_clean_composite_slice(game_root, &clean_entry)?;
    // The carved slice from a composite container preserves the package's
    // original compression. apply_manifest only handles uncompressed input,
    // so normalize first.
    let vanilla = gpk_package::extract_uncompressed_package_bytes(&raw_vanilla).map_err(|e| {
        format!("Failed to decompress composite-resolved vanilla for '{target_package_name}': {e}")
    })?;
    let patched = gpk_patch_applier::apply_manifest(&vanilla, manifest)?;

    patch_composite_container_slice(
        game_root,
        target_package_name,
        target_object_path,
        &patched,
    )
}

fn extract_clean_composite_slice(
    game_root: &Path,
    entry: &gpk::MapperEntry,
) -> Result<Vec<u8>, String> {
    let container_path = resolve_composite_container_path(game_root, &entry.filename)?;
    let backup_path = vanilla_backup_path(&container_path);
    let source_path = if backup_path.exists() {
        backup_path
    } else {
        container_path
    };
    let container = fs::read(&source_path).map_err(|e| {
        format!(
            "Failed to read composite container baseline {}: {e}",
            source_path.display()
        )
    })?;
    let offset = usize_from_i64(entry.offset, "offset", &entry.object_path)?;
    let size = usize_from_i64(entry.size, "size", &entry.object_path)?;
    let end = offset.checked_add(size).ok_or_else(|| {
        format!(
            "Composite mapper offset+size overflow for '{}'",
            entry.object_path
        )
    })?;
    if end > container.len() {
        return Err(format!(
            "Composite mapper slice {}+{} exceeds container length {} in {}",
            offset,
            size,
            container.len(),
            source_path.display()
        ));
    }
    Ok(container[offset..end].to_vec())
}

fn disable_composite(game_root: &Path, target_package_name: &str) -> Result<(), String> {
    let entry = resolve_clean_composite_entry(game_root, target_package_name)?;
    let container_path = resolve_composite_container_path(game_root, &entry.filename)?;
    let backup_path = vanilla_backup_path(&container_path);
    if backup_path.exists() {
        gpk::copy_atomic(&backup_path, &container_path).map_err(|e| {
            format!(
                "Failed to restore composite container {} from {}: {e}",
                container_path.display(),
                backup_path.display()
            )
        })?;
    }

    let legacy_standalone = game_root
        .join(gpk::COOKED_PC_DIR)
        .join(format!("{target_package_name}.gpk"));
    if legacy_standalone.exists() {
        if !gpk::is_safe_gpk_container_filename(&format!("{target_package_name}.gpk")) {
            return Err(format!(
                "Refusing to delete unsafe legacy standalone target '{target_package_name}.gpk'"
            ));
        }
        fs::remove_file(&legacy_standalone).map_err(|e| {
            format!(
                "Failed to delete legacy patched standalone {}: {e}",
                legacy_standalone.display()
            )
        })?;
    }
    Ok(())
}

fn disable_composite_and_restore_mapper(
    game_root: &Path,
    target_package_name: &str,
    target_object_path: Option<&str>,
) -> Result<(), String> {
    let entry = match target_object_path.filter(|s| !s.trim().is_empty()) {
        Some(logical) => resolve_clean_composite_entry_for_logical_path(game_root, logical)?,
        None => resolve_clean_composite_entry(game_root, target_package_name)?,
    };
    let container_path = resolve_composite_container_path(game_root, &entry.filename)?;
    let live_container = fs::read(&container_path).map_err(|e| {
        format!(
            "Failed to read live composite container {} before disable: {e}",
            container_path.display()
        )
    })?;

    disable_composite(game_root, target_package_name)?;

    gpk::restore_clean_mapper_state(game_root).map_err(|err| {
        if let Err(restore_err) = gpk::write_atomic_file(&container_path, &live_container) {
            format!(
                "{err}; additionally failed to roll composite container {} back to pre-disable bytes: {restore_err}",
                container_path.display()
            )
        } else {
            err
        }
    })
}

fn patch_composite_container_slice(
    game_root: &Path,
    target_package_name: &str,
    target_object_path: Option<&str>,
    patched_package: &[u8],
) -> Result<(), String> {
    let clean_entry = match target_object_path.filter(|s| !s.trim().is_empty()) {
        Some(logical) => resolve_clean_composite_entry_for_logical_path(game_root, logical)?,
        None => resolve_clean_composite_entry(game_root, target_package_name)?,
    };
    let live_entry = resolve_live_composite_entry(game_root, &clean_entry)?;
    let container_path = resolve_composite_container_path(game_root, &clean_entry.filename)?;
    let backup_path = vanilla_backup_path(&container_path);
    if !backup_path.exists() {
        gpk::copy_atomic(&container_path, &backup_path).map_err(|e| {
            format!(
                "Failed to back up composite container {} to {}: {e}",
                container_path.display(),
                backup_path.display()
            )
        })?;
    }

    let live_container = fs::read(&container_path).map_err(|e| {
        format!(
            "Failed to read live composite container {}: {e}",
            container_path.display()
        )
    })?;
    let old_offset = usize_from_i64(live_entry.offset, "offset", &live_entry.object_path)?;
    let old_size = usize_from_i64(live_entry.size, "size", &live_entry.object_path)?;
    let old_end = old_offset.checked_add(old_size).ok_or_else(|| {
        format!(
            "Composite mapper offset+size overflow for '{}'",
            live_entry.object_path
        )
    })?;
    if old_end > live_container.len() {
        return Err(format!(
            "Composite mapper slice {}+{} exceeds container length {} in {}",
            old_offset,
            old_size,
            live_container.len(),
            container_path.display()
        ));
    }

    let mut rebuilt = Vec::with_capacity(live_container.len() - old_size + patched_package.len());
    rebuilt.extend_from_slice(&live_container[..old_offset]);
    rebuilt.extend_from_slice(patched_package);
    rebuilt.extend_from_slice(&live_container[old_end..]);
    gpk::write_atomic_file(&container_path, &rebuilt).map_err(|e| {
        format!(
            "Failed to write patched composite container {}: {e}",
            container_path.display()
        )
    })?;

    rewrite_mapper_for_patched_slice(game_root, &live_entry, patched_package.len() as i64)
        .map_err(|err| {
            if let Err(restore_err) = gpk::write_atomic_file(&container_path, &live_container) {
                format!(
                    "{err}; additionally failed to restore composite container {} after mapper failure: {restore_err}",
                    container_path.display()
                )
            } else {
                err
            }
        })
}

fn resolve_live_composite_entry(
    game_root: &Path,
    clean_entry: &gpk::MapperEntry,
) -> Result<gpk::MapperEntry, String> {
    let mapper_path = game_root.join(gpk::COOKED_PC_DIR).join(gpk::MAPPER_FILE);
    let bytes = fs::read(&mapper_path).map_err(|e| {
        format!(
            "Failed to read CompositePackageMapper.dat at {}: {e}",
            mapper_path.display()
        )
    })?;
    let plain = String::from_utf8_lossy(&gpk::decrypt_mapper(&bytes)).to_string();
    let map = gpk::parse_mapper_strict(&plain)?;
    let live_entry = map.get(&clean_entry.composite_name).ok_or_else(|| {
        format!(
            "Live CompositePackageMapper.dat no longer contains '{}' — restore mapper state and rebuild enabled GPK mods",
            clean_entry.composite_name
        )
    })?;
    if !live_entry
        .filename
        .eq_ignore_ascii_case(&clean_entry.filename)
    {
        return Err(format!(
            "Live composite entry '{}' points at '{}' but clean mapper points at '{}' — refusing to patch drifted mapper state",
            clean_entry.composite_name, live_entry.filename, clean_entry.filename
        ));
    }
    if !live_entry
        .object_path
        .eq_ignore_ascii_case(&clean_entry.object_path)
    {
        return Err(format!(
            "Live composite entry '{}' points at object '{}' but clean mapper points at '{}' — refusing to patch drifted mapper state",
            clean_entry.composite_name, live_entry.object_path, clean_entry.object_path
        ));
    }
    Ok(live_entry.clone())
}

fn rewrite_mapper_for_patched_slice(
    game_root: &Path,
    target: &gpk::MapperEntry,
    patched_size: i64,
) -> Result<(), String> {
    let cooked_pc = game_root.join(gpk::COOKED_PC_DIR);
    let mapper_path = cooked_pc.join(gpk::MAPPER_FILE);
    let mapper_bytes = fs::read(&mapper_path).map_err(|e| {
        format!(
            "Failed to read CompositePackageMapper.dat at {}: {e}",
            mapper_path.display()
        )
    })?;
    let mapper_plain = String::from_utf8_lossy(&gpk::decrypt_mapper(&mapper_bytes)).to_string();
    let mut map = gpk::parse_mapper_strict(&mapper_plain)?;
    let old_end = target.offset.checked_add(target.size).ok_or_else(|| {
        format!(
            "Composite mapper offset+size overflow for '{}'",
            target.object_path
        )
    })?;
    let delta = patched_size - target.size;

    for entry in map.values_mut() {
        if !entry.filename.eq_ignore_ascii_case(&target.filename) {
            continue;
        }
        if entry.offset == target.offset && entry.size == target.size {
            entry.size = patched_size;
            continue;
        }
        if entry.offset >= old_end {
            entry.offset = entry.offset.checked_add(delta).ok_or_else(|| {
                format!(
                    "Composite mapper offset shift overflow for '{}'",
                    entry.object_path
                )
            })?;
        }
    }

    let encrypted = gpk::encrypt_mapper(gpk::serialize_mapper(&map).as_bytes());
    gpk::write_atomic_file(&mapper_path, &encrypted)
        .map_err(|e| format!("Failed to write patched mapper: {e}"))?;
    Ok(())
}

/// Disambiguating sibling of `resolve_clean_composite_entry` — looks up the
/// composite mapper entry for an exact `Package.Object` logical path
/// instead of matching every row whose package side equals
/// `package_name`. Used by enable/disable when the catalog entry's
/// `target_object_path` qualifier is set.
fn resolve_clean_composite_entry_for_logical_path(
    game_root: &Path,
    logical_path: &str,
) -> Result<gpk::MapperEntry, String> {
    let cooked_pc = game_root.join(gpk::COOKED_PC_DIR);
    let pkg_clean = cooked_pc.join(gpk::PKG_MAPPER_BACKUP_FILE);
    let comp_clean = cooked_pc.join(gpk::BACKUP_FILE);
    if !pkg_clean.exists() {
        return Err(format!(
            "PkgMapper.clean missing at {} — can't resolve composite entry for logical path '{}'",
            pkg_clean.display(),
            logical_path
        ));
    }
    if !comp_clean.exists() {
        return Err(format!(
            "CompositePackageMapper.clean missing at {} — can't resolve composite entry for logical path '{}'",
            comp_clean.display(),
            logical_path
        ));
    }
    // PkgMapper logical → composite_object_path
    let pkg_bytes = fs::read(&pkg_clean).map_err(|e| {
        format!(
            "Failed to read PkgMapper.clean at {}: {e}",
            pkg_clean.display()
        )
    })?;
    let pkg_plain = String::from_utf8_lossy(&gpk::decrypt_mapper(&pkg_bytes)).to_string();
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
        .ok_or_else(|| {
            format!(
                "Logical path '{logical_path}' not found in PkgMapper.clean — mapper drift between install and enable, or target_object_path is misspelled"
            )
        })?;
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
    let comp_plain = String::from_utf8_lossy(&gpk::decrypt_mapper(&comp_bytes)).to_string();
    let comp_map = gpk::parse_mapper_strict(&comp_plain)?;
    comp_map.get(composite_uid).cloned().ok_or_else(|| {
        format!(
            "composite_uid '{composite_uid}' (resolved from logical path '{logical_path}') not found in CompositePackageMapper.clean"
        )
    })
}

fn resolve_clean_composite_entry(
    game_root: &Path,
    package_name: &str,
) -> Result<gpk::MapperEntry, String> {
    let cooked_pc = game_root.join(gpk::COOKED_PC_DIR);
    let clean = cooked_pc.join(gpk::BACKUP_FILE);
    let bytes = fs::read(&clean).map_err(|e| {
        format!(
            "Failed to read CompositePackageMapper.clean at {}: {e}",
            clean.display()
        )
    })?;
    let plain = String::from_utf8_lossy(&gpk::decrypt_mapper(&bytes)).to_string();
    let map = gpk::parse_mapper_strict(&plain)?;
    let suffix = format!(".{package_name}").to_ascii_lowercase();
    let mut matches: Vec<gpk::MapperEntry> = map
        .values()
        .filter(|entry| {
            entry.object_path.eq_ignore_ascii_case(package_name)
                || entry.object_path.to_ascii_lowercase().ends_with(&suffix)
        })
        .cloned()
        .collect();

    if matches.is_empty() {
        matches = resolve_clean_entries_via_pkg_mapper(&cooked_pc, &map, package_name)?;
    }

    if matches.is_empty() {
        return Err(format!(
            "Composite mapper has no clean entry for vanilla '{package_name}' — mapper drift between install and enable"
        ));
    }

    matches.sort_by(|a, b| a.composite_name.cmp(&b.composite_name));
    let first = matches[0].clone();
    if matches.iter().any(|entry| {
        !entry.filename.eq_ignore_ascii_case(&first.filename)
            || entry.offset != first.offset
            || entry.size != first.size
    }) {
        return Err(format!(
            "Package '{package_name}' matches multiple clean composite byte ranges — reinstall with an exact target"
        ));
    }
    Ok(first)
}

fn resolve_clean_entries_via_pkg_mapper(
    cooked_pc: &Path,
    composite_map: &std::collections::HashMap<String, gpk::MapperEntry>,
    package_name: &str,
) -> Result<Vec<gpk::MapperEntry>, String> {
    let clean = cooked_pc.join(gpk::PKG_MAPPER_BACKUP_FILE);
    if !clean.exists() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(&clean)
        .map_err(|e| format!("Failed to read PkgMapper.clean at {}: {e}", clean.display()))?;
    let plain = String::from_utf8_lossy(&gpk::decrypt_mapper(&bytes)).to_string();
    let package_prefix = format!("{}.", package_name.to_ascii_lowercase());
    let mut entries = Vec::new();

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
        entries.push(entry.clone());
    }

    Ok(entries)
}

fn resolve_composite_container_path(game_root: &Path, filename: &str) -> Result<PathBuf, String> {
    if !gpk::is_safe_gpk_container_filename(filename) {
        return Err(format!(
            "Refusing to patch unsafe composite container filename '{filename}'"
        ));
    }
    let cooked_pc = game_root.join(gpk::COOKED_PC_DIR);
    let mut path = cooked_pc.join(filename);
    if !path.exists() && path.extension().is_none() {
        path = cooked_pc.join(format!("{filename}.gpk"));
    }
    if !path.exists() {
        return Err(format!(
            "Composite container {} does not exist",
            path.display()
        ));
    }
    Ok(path)
}

fn usize_from_i64(value: i64, field: &str, object_path: &str) -> Result<usize, String> {
    if value < 0 {
        return Err(format!(
            "Composite mapper has a negative {field} for '{object_path}' — refusing to patch"
        ));
    }
    usize::try_from(value).map_err(|_| {
        format!("Composite mapper {field} for '{object_path}' does not fit this platform")
    })
}

fn enable_standalone(
    game_root: &Path,
    manifest: &super::patch_manifest::PatchManifest,
    relative_path: &str,
) -> Result<(), String> {
    let vanilla_path = resolve_relative_to_game_root(game_root, relative_path)?;
    if !vanilla_path.exists() {
        return Err(format!(
            "Recorded vanilla file {} no longer exists — verify game files and reinstall the mod",
            vanilla_path.display()
        ));
    }

    let backup_path = vanilla_backup_path(&vanilla_path);

    // First-run: snapshot the current bytes as the trusted vanilla baseline.
    // We assume a freshly-installed mod has not yet been applied, so the
    // bytes at `vanilla_path` are still vanilla. If `.vanilla-bak` already
    // exists, trust it as the canonical baseline (a previous enable already
    // ran or someone restored manually).
    if !backup_path.exists() {
        gpk::copy_atomic(&vanilla_path, &backup_path).map_err(|e| {
            format!(
                "Failed to back up vanilla {} to {}: {e}",
                vanilla_path.display(),
                backup_path.display()
            )
        })?;
    }

    let raw_baseline = fs::read(&backup_path).map_err(|e| {
        format!(
            "Failed to read vanilla baseline {}: {e}",
            backup_path.display()
        )
    })?;
    let baseline = gpk_package::extract_uncompressed_package_bytes(&raw_baseline).map_err(|e| {
        format!(
            "Failed to decompress standalone vanilla baseline {}: {e}",
            backup_path.display()
        )
    })?;
    let patched = gpk_patch_applier::apply_manifest(&baseline, manifest)?;
    gpk::write_atomic_file(&vanilla_path, &patched).map_err(|e| {
        format!(
            "Failed to write patched bytes to {}: {e}",
            vanilla_path.display()
        )
    })?;
    Ok(())
}

fn disable_standalone(game_root: &Path, relative_path: &str) -> Result<(), String> {
    let vanilla_path = resolve_relative_to_game_root(game_root, relative_path)?;
    let backup_path = vanilla_backup_path(&vanilla_path);
    if backup_path.exists() {
        gpk::copy_atomic(&backup_path, &vanilla_path).map_err(|e| {
            format!(
                "Failed to restore vanilla {} from {}: {e}",
                vanilla_path.display(),
                backup_path.display()
            )
        })?;
    }
    Ok(())
}

fn vanilla_backup_path(vanilla_path: &Path) -> PathBuf {
    let mut s = vanilla_path.as_os_str().to_owned();
    s.push(".vanilla-bak");
    PathBuf::from(s)
}

fn relative_to_game_root(game_root: &Path, path: &Path) -> Result<String, String> {
    let stripped = path.strip_prefix(game_root).map_err(|_| {
        format!(
            "Resolved vanilla path {} is not inside game_root {}",
            path.display(),
            game_root.display()
        )
    })?;
    Ok(stripped.to_string_lossy().replace('\\', "/").to_string())
}

fn resolve_relative_to_game_root(game_root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    if relative_path.contains("..")
        || relative_path.starts_with('/')
        || relative_path.starts_with('\\')
    {
        return Err(format!(
            "install_target relative_path '{relative_path}' rejects parent traversal / absolute paths"
        ));
    }
    if relative_path.contains(':') {
        return Err(format!(
            "install_target relative_path '{relative_path}' must not contain a drive letter"
        ));
    }
    // Path::join accepts forward slashes on Windows fine, so no manual
    // separator translation is needed here.
    Ok(game_root.join(relative_path))
}

pub fn uninstall_via_patch(game_root: &Path, app_root: &Path, mod_id: &str) -> Result<(), String> {
    disable_via_patch(game_root, app_root, mod_id)?;
    manifest_store::delete_manifest_at_root(app_root, mod_id)?;
    Ok(())
}

/// Migration target: a registry slot whose patched bytes were applied by
/// the legacy whole-file install path (`deployed_filename` is set) and
/// has no patch manifest persisted. The new flow can't enable/disable it
/// safely, so we clean up CookedPC and ask the user to reinstall.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyMigrationOutcome {
    pub mod_id: String,
    pub target_filename: Option<String>,
    pub error: Option<String>,
}

/// Runs the one-time migration for slots installed by older launcher
/// versions. For each (id, deployed_filename, manifest-missing) tuple:
///   - deletes the standalone .gpk + restores any `.vanilla-bak` via the
///     legacy uninstall path,
///   - hard-restores the mapper from `.clean` so any mapper redirects the
///     legacy install drove are gone.
///
/// Returns one outcome per migrated slot. The caller is expected to
/// flip the registry rows to a needs-reinstall state and surface the
/// result to the user.
pub fn migrate_legacy_install(
    game_root: &Path,
    app_root: &Path,
    mod_id: &str,
    deployed_filename: &str,
) -> LegacyMigrationOutcome {
    if let Ok(Some(_)) = manifest_store::load_manifest_at_root(app_root, mod_id) {
        // A manifest already exists — this slot is on the new flow.
        return LegacyMigrationOutcome {
            mod_id: mod_id.to_string(),
            target_filename: Some(deployed_filename.to_string()),
            error: None,
        };
    }

    let legacy_err = match super::gpk::uninstall_legacy_gpk(game_root, deployed_filename) {
        Ok(()) => None,
        Err(err) => Some(err),
    };
    let mapper_err = match gpk::restore_clean_mapper_state(game_root) {
        Ok(()) => None,
        Err(err) => Some(err),
    };

    LegacyMigrationOutcome {
        mod_id: mod_id.to_string(),
        target_filename: Some(deployed_filename.to_string()),
        error: match (legacy_err, mapper_err) {
            (None, None) => None,
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (Some(a), Some(b)) => Some(format!("{a}; {b}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::mods::gpk::{
        decrypt_mapper, encrypt_mapper, parse_mapper, BACKUP_FILE, COOKED_PC_DIR, MAPPER_FILE,
        PKG_MAPPER_BACKUP_FILE,
    };
    use crate::services::mods::gpk_package::parse_package;
    use crate::services::mods::patch_manifest::{
        CompatibilityPolicy, ExportPatch, ExportPatchOperation, PatchFamily, PatchManifest,
        ReferenceBaseline,
    };
    use crate::services::mods::test_fixtures::{
        build_boss_window_test_package, build_x64_boss_window_test_package,
    };
    use tempfile::TempDir;

    struct Setup {
        _tmp: TempDir,
        game_root: std::path::PathBuf,
        app_root: std::path::PathBuf,
        cooked_pc: std::path::PathBuf,
        vanilla_pkg: Vec<u8>,
        mod_src: std::path::PathBuf,
    }

    fn setup_boss_window_install() -> Setup {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        // Synthetic vanilla composite container "S1Common.gpk" holds one
        // boss-window package whose internal package_name is
        // "S1UI_GageBoss". Container filename is intentionally different
        // from the package name — real TERA composite containers bundle
        // multiple packages and live under names like "S1Common_*.gpk".
        let vanilla_pkg = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], false);
        fs::write(cooked_pc.join("S1Common.gpk"), &vanilla_pkg).unwrap();

        // Mapper entry for object Owner.S1UI_GageBoss points at the
        // composite container — extract / redirect resolve by the
        // package-name suffix `.S1UI_GageBoss`.
        let mapper_text = format!(
            "S1Common.gpk?Owner.S1UI_GageBoss,Comp,0,{},|!",
            vanilla_pkg.len()
        );
        let encrypted = encrypt_mapper(mapper_text.as_bytes());
        fs::write(cooked_pc.join(MAPPER_FILE), &encrypted).unwrap();
        fs::write(cooked_pc.join(BACKUP_FILE), &encrypted).unwrap();

        // Modded standalone with a different payload.
        let modded_pkg = build_boss_window_test_package([0xAA, 0xBB, 0xCC, 0xDD], false);
        let mod_src = tmp.path().join("mod-src.gpk");
        fs::write(&mod_src, &modded_pkg).unwrap();

        Setup {
            _tmp: tmp,
            game_root,
            app_root,
            cooked_pc,
            vanilla_pkg,
            mod_src,
        }
    }

    const PACKAGE_NAME: &str = "S1UI_GageBoss";
    const STANDALONE_FILE: &str = "S1UI_GageBoss.gpk";

    fn size_changing_manifest(mod_id: &str, vanilla_pkg: &[u8]) -> PatchManifest {
        manifest_with_payload(mod_id, vanilla_pkg, "aabbccddeeff")
    }

    fn manifest_with_payload(
        mod_id: &str,
        vanilla_pkg: &[u8],
        replacement_payload_hex: &str,
    ) -> PatchManifest {
        let parsed = parse_package(vanilla_pkg).unwrap();
        let main = parsed
            .exports
            .iter()
            .find(|e| e.object_path == "GageBoss")
            .expect("GageBoss export present");

        PatchManifest {
            schema_version: 2,
            mod_id: mod_id.to_string(),
            title: mod_id.to_string(),
            target_package: "S1UI_GageBoss.gpk".into(),
            patch_family: PatchFamily::UiLayout,
            reference: ReferenceBaseline {
                source_patch_label: "test".into(),
                package_fingerprint: "exports:1|imports:2|names:6".into(),
                provenance: None,
            },
            compatibility: CompatibilityPolicy {
                require_exact_package_fingerprint: true,
                require_all_exports_present: false,
                forbid_name_or_import_expansion: false,
            },
            exports: vec![ExportPatch {
                object_path: main.object_path.clone(),
                class_name: main.class_name.clone(),
                reference_export_fingerprint: main.payload_fingerprint.clone(),
                target_export_fingerprint: Some(main.payload_fingerprint.clone()),
                operation: ExportPatchOperation::ReplaceExportPayload,
                new_class_name: None,
                replacement_payload_hex: replacement_payload_hex.into(),
            }],
            import_patches: Vec::new(),
            name_patches: Vec::new(),
            notes: vec!["test manifest with size-changing payload".into()],
        }
    }

    #[test]
    fn composite_enable_patches_container_slice_and_shifts_later_offsets() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        let target_pkg = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], false);
        let sibling_pkg = build_boss_window_test_package([0x20, 0x21, 0x22, 0x23], false);
        let target_offset = 0i64;
        let sibling_offset = target_pkg.len() as i64;
        let mut container = Vec::new();
        container.extend_from_slice(&target_pkg);
        container.extend_from_slice(&sibling_pkg);
        fs::write(cooked_pc.join("ff54e3e4_04.gpk"), &container).unwrap();

        let mapper_text = format!(
            "ff54e3e4_04.gpk?Owner.S1UI_GageBoss,CompTarget,{target_offset},{},|Owner.PaperDoll_I147_dup,CompSibling,{sibling_offset},{},|!",
            target_pkg.len(),
            sibling_pkg.len()
        );
        let encrypted = encrypt_mapper(mapper_text.as_bytes());
        fs::write(cooked_pc.join(MAPPER_FILE), &encrypted).unwrap();
        fs::write(cooked_pc.join(BACKUP_FILE), &encrypted).unwrap();
        manifest_store::save_manifest_at_root(
            &app_root,
            "test.mod",
            &size_changing_manifest("test.mod", &target_pkg),
        )
        .unwrap();
        manifest_store::save_install_target_at_root(
            &app_root,
            "test.mod",
            &InstallTarget::Composite {
                package_name: PACKAGE_NAME.into(),
            },
        )
        .unwrap();

        enable_via_patch(&game_root, &app_root, "test.mod").unwrap();

        assert!(
            !cooked_pc.join(STANDALONE_FILE).exists(),
            "composite enable must not write a redirected standalone GPK"
        );

        let patched_container = fs::read(cooked_pc.join("ff54e3e4_04.gpk")).unwrap();
        assert_eq!(
            patched_container.len(),
            container.len() + 2,
            "container length must grow by the patch delta"
        );

        let mapper_now = fs::read(cooked_pc.join(MAPPER_FILE)).unwrap();
        let map = parse_mapper(&String::from_utf8_lossy(&decrypt_mapper(&mapper_now)));
        let target = map.get("CompTarget").expect("target entry present");
        assert_eq!(target.filename, "ff54e3e4_04.gpk");
        assert_eq!(target.object_path, "Owner.S1UI_GageBoss");
        assert_eq!(target.offset, 0);
        assert_eq!(target.size, target_pkg.len() as i64 + 2);
        let sibling = map.get("CompSibling").expect("sibling entry present");
        assert_eq!(sibling.filename, "ff54e3e4_04.gpk");
        assert_eq!(sibling.object_path, "Owner.PaperDoll_I147_dup");
        assert_eq!(sibling.offset, sibling_offset + 2);
        assert_eq!(sibling.size, sibling_pkg.len() as i64);

        let patched_target = &patched_container[..target.size as usize];
        let parsed_target = parse_package(patched_target).unwrap();
        let main = parsed_target
            .exports
            .iter()
            .find(|e| e.object_path == "GageBoss")
            .unwrap();
        assert_eq!(main.payload, vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        assert_eq!(
            &patched_container
                [sibling.offset as usize..sibling.offset as usize + sibling.size as usize],
            sibling_pkg.as_slice(),
            "sibling package bytes must survive at the shifted offset"
        );
        assert_eq!(
            fs::read(cooked_pc.join(BACKUP_FILE)).unwrap(),
            encrypted,
            "clean mapper backup must stay vanilla"
        );
    }

    #[test]
    fn composite_enable_stacks_sequential_mods_in_same_container() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        let first_pkg = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], false);
        let second_pkg = build_boss_window_test_package([0x20, 0x21, 0x22, 0x23], false);
        let first_offset = 0i64;
        let second_offset = first_pkg.len() as i64;
        let mut container = Vec::new();
        container.extend_from_slice(&first_pkg);
        container.extend_from_slice(&second_pkg);
        fs::write(cooked_pc.join("ff54e3e4_04.gpk"), &container).unwrap();

        let mapper_text = format!(
            "ff54e3e4_04.gpk?Owner.S1UI_GageBoss,CompFirst,{first_offset},{},|Owner.PaperDoll_I147_dup,CompSecond,{second_offset},{},|!",
            first_pkg.len(),
            second_pkg.len()
        );
        let encrypted = encrypt_mapper(mapper_text.as_bytes());
        fs::write(cooked_pc.join(MAPPER_FILE), &encrypted).unwrap();
        fs::write(cooked_pc.join(BACKUP_FILE), &encrypted).unwrap();

        manifest_store::save_manifest_at_root(
            &app_root,
            "first.mod",
            &manifest_with_payload("first.mod", &first_pkg, "aabbccddeeff"),
        )
        .unwrap();
        manifest_store::save_install_target_at_root(
            &app_root,
            "first.mod",
            &InstallTarget::Composite {
                package_name: "S1UI_GageBoss".into(),
            },
        )
        .unwrap();
        manifest_store::save_manifest_at_root(
            &app_root,
            "second.mod",
            &manifest_with_payload("second.mod", &second_pkg, "010203040506"),
        )
        .unwrap();
        manifest_store::save_install_target_at_root(
            &app_root,
            "second.mod",
            &InstallTarget::Composite {
                package_name: "PaperDoll_I147_dup".into(),
            },
        )
        .unwrap();

        enable_via_patch(&game_root, &app_root, "first.mod").unwrap();
        enable_via_patch(&game_root, &app_root, "second.mod").unwrap();

        let mapper_now = fs::read(cooked_pc.join(MAPPER_FILE)).unwrap();
        let map = parse_mapper(&String::from_utf8_lossy(&decrypt_mapper(&mapper_now)));
        let first = map.get("CompFirst").expect("first entry present");
        let second = map.get("CompSecond").expect("second entry present");
        assert_eq!(first.offset, 0);
        assert_eq!(first.size, first_pkg.len() as i64 + 2);
        assert_eq!(second.offset, second_offset + 2);
        assert_eq!(second.size, second_pkg.len() as i64 + 2);

        let patched_container = fs::read(cooked_pc.join("ff54e3e4_04.gpk")).unwrap();
        let parsed_first = parse_package(
            &patched_container[first.offset as usize..first.offset as usize + first.size as usize],
        )
        .unwrap();
        let first_export = parsed_first
            .exports
            .iter()
            .find(|e| e.object_path == "GageBoss")
            .unwrap();
        assert_eq!(
            first_export.payload,
            vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
            "enabling the second mod must not erase the first patch"
        );

        let parsed_second = parse_package(
            &patched_container
                [second.offset as usize..second.offset as usize + second.size as usize],
        )
        .unwrap();
        let second_export = parsed_second
            .exports
            .iter()
            .find(|e| e.object_path == "GageBoss")
            .unwrap();
        assert_eq!(second_export.payload, vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(
            fs::read(cooked_pc.join("ff54e3e4_04.gpk.vanilla-bak")).unwrap(),
            container,
            "vanilla backup must remain the clean rollback source"
        );
    }

    #[test]
    fn composite_enable_supports_pkg_mapper_clean_resolution() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        let vanilla_pkg = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], false);
        fs::write(cooked_pc.join("ff54e3e4_04.gpk"), &vanilla_pkg).unwrap();
        let mapper_text = format!(
            "ff54e3e4_04.gpk?OpaqueCompositeUid,CompOpaque,0,{},|!",
            vanilla_pkg.len()
        );
        fs::write(
            cooked_pc.join(MAPPER_FILE),
            encrypt_mapper(mapper_text.as_bytes()),
        )
        .unwrap();
        fs::write(
            cooked_pc.join(BACKUP_FILE),
            encrypt_mapper(mapper_text.as_bytes()),
        )
        .unwrap();
        fs::write(
            cooked_pc.join(PKG_MAPPER_BACKUP_FILE),
            encrypt_mapper(b"S1UI_GageBoss.GageBoss,CompOpaque|"),
        )
        .unwrap();

        manifest_store::save_manifest_at_root(
            &app_root,
            "test.mod",
            &manifest_with_payload("test.mod", &vanilla_pkg, "aabbccdd"),
        )
        .unwrap();
        manifest_store::save_install_target_at_root(
            &app_root,
            "test.mod",
            &InstallTarget::Composite {
                package_name: PACKAGE_NAME.into(),
            },
        )
        .unwrap();

        enable_via_patch(&game_root, &app_root, "test.mod").unwrap();

        let patched_container = fs::read(cooked_pc.join("ff54e3e4_04.gpk")).unwrap();
        let parsed = parse_package(&patched_container).unwrap();
        let main = parsed
            .exports
            .iter()
            .find(|e| e.object_path == "GageBoss")
            .unwrap();
        assert_eq!(main.payload, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn composite_enable_refuses_live_mapper_missing_target_without_mutating_container() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        let vanilla_pkg = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], false);
        fs::write(cooked_pc.join("S1Common.gpk"), &vanilla_pkg).unwrap();
        let clean_text = format!(
            "S1Common.gpk?Owner.S1UI_GageBoss,Comp,0,{},|!",
            vanilla_pkg.len()
        );
        fs::write(
            cooked_pc.join(BACKUP_FILE),
            encrypt_mapper(clean_text.as_bytes()),
        )
        .unwrap();
        fs::write(
            cooked_pc.join(MAPPER_FILE),
            encrypt_mapper(b"Other.gpk?Other.Package,OtherComp,0,4,|!"),
        )
        .unwrap();
        manifest_store::save_manifest_at_root(
            &app_root,
            "test.mod",
            &manifest_with_payload("test.mod", &vanilla_pkg, "aabbccdd"),
        )
        .unwrap();
        manifest_store::save_install_target_at_root(
            &app_root,
            "test.mod",
            &InstallTarget::Composite {
                package_name: PACKAGE_NAME.into(),
            },
        )
        .unwrap();

        let err = enable_via_patch(&game_root, &app_root, "test.mod").unwrap_err();

        assert!(
            err.contains("no longer contains 'Comp'"),
            "unexpected error: {err}"
        );
        assert_eq!(
            fs::read(cooked_pc.join("S1Common.gpk")).unwrap(),
            vanilla_pkg
        );
    }

    #[test]
    fn composite_enable_refuses_live_mapper_filename_drift_without_mutating_container() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        let vanilla_pkg = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], false);
        fs::write(cooked_pc.join("S1Common.gpk"), &vanilla_pkg).unwrap();
        fs::write(cooked_pc.join("Redirected.gpk"), &vanilla_pkg).unwrap();
        let clean_text = format!(
            "S1Common.gpk?Owner.S1UI_GageBoss,Comp,0,{},|!",
            vanilla_pkg.len()
        );
        let live_text = format!(
            "Redirected.gpk?Owner.S1UI_GageBoss,Comp,0,{},|!",
            vanilla_pkg.len()
        );
        fs::write(
            cooked_pc.join(BACKUP_FILE),
            encrypt_mapper(clean_text.as_bytes()),
        )
        .unwrap();
        fs::write(
            cooked_pc.join(MAPPER_FILE),
            encrypt_mapper(live_text.as_bytes()),
        )
        .unwrap();
        manifest_store::save_manifest_at_root(
            &app_root,
            "test.mod",
            &manifest_with_payload("test.mod", &vanilla_pkg, "aabbccdd"),
        )
        .unwrap();
        manifest_store::save_install_target_at_root(
            &app_root,
            "test.mod",
            &InstallTarget::Composite {
                package_name: PACKAGE_NAME.into(),
            },
        )
        .unwrap();

        let err = enable_via_patch(&game_root, &app_root, "test.mod").unwrap_err();

        assert!(
            err.contains("refusing to patch drifted mapper state"),
            "got: {err}"
        );
        assert_eq!(
            fs::read(cooked_pc.join("S1Common.gpk")).unwrap(),
            vanilla_pkg
        );
    }

    #[test]
    fn composite_enable_refuses_live_mapper_object_drift_without_mutating_container() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        let vanilla_pkg = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], false);
        fs::write(cooked_pc.join("S1Common.gpk"), &vanilla_pkg).unwrap();
        let clean_text = format!(
            "S1Common.gpk?Owner.S1UI_GageBoss,Comp,0,{},|!",
            vanilla_pkg.len()
        );
        let live_text = format!("S1Common.gpk?Other.Package,Comp,0,{},|!", vanilla_pkg.len());
        fs::write(
            cooked_pc.join(BACKUP_FILE),
            encrypt_mapper(clean_text.as_bytes()),
        )
        .unwrap();
        fs::write(
            cooked_pc.join(MAPPER_FILE),
            encrypt_mapper(live_text.as_bytes()),
        )
        .unwrap();
        manifest_store::save_manifest_at_root(
            &app_root,
            "test.mod",
            &manifest_with_payload("test.mod", &vanilla_pkg, "aabbccdd"),
        )
        .unwrap();
        manifest_store::save_install_target_at_root(
            &app_root,
            "test.mod",
            &InstallTarget::Composite {
                package_name: PACKAGE_NAME.into(),
            },
        )
        .unwrap();

        let err = enable_via_patch(&game_root, &app_root, "test.mod").unwrap_err();

        assert!(
            err.contains("refusing to patch drifted mapper state"),
            "got: {err}"
        );
        assert_eq!(
            fs::read(cooked_pc.join("S1Common.gpk")).unwrap(),
            vanilla_pkg
        );
    }

    #[test]
    fn composite_enable_updates_same_range_aliases() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        let vanilla_pkg = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], false);
        let sibling_pkg = build_boss_window_test_package([0x20, 0x21, 0x22, 0x23], false);
        let sibling_offset = vanilla_pkg.len() as i64;
        let mut container = Vec::new();
        container.extend_from_slice(&vanilla_pkg);
        container.extend_from_slice(&sibling_pkg);
        fs::write(cooked_pc.join("S1Common.gpk"), &container).unwrap();
        let mapper_text = format!(
            "S1Common.gpk?Owner.S1UI_GageBoss,Comp,0,{},|Alias.S1UI_GageBoss,CompAlias,0,{},|Sibling.Package,CompSibling,{sibling_offset},{},|!",
            vanilla_pkg.len(),
            vanilla_pkg.len(),
            sibling_pkg.len()
        );
        let encrypted = encrypt_mapper(mapper_text.as_bytes());
        fs::write(cooked_pc.join(BACKUP_FILE), &encrypted).unwrap();
        fs::write(cooked_pc.join(MAPPER_FILE), &encrypted).unwrap();
        manifest_store::save_manifest_at_root(
            &app_root,
            "test.mod",
            &size_changing_manifest("test.mod", &vanilla_pkg),
        )
        .unwrap();
        manifest_store::save_install_target_at_root(
            &app_root,
            "test.mod",
            &InstallTarget::Composite {
                package_name: PACKAGE_NAME.into(),
            },
        )
        .unwrap();

        enable_via_patch(&game_root, &app_root, "test.mod").unwrap();

        let mapper_now = fs::read(cooked_pc.join(MAPPER_FILE)).unwrap();
        let map = parse_mapper(&String::from_utf8_lossy(&decrypt_mapper(&mapper_now)));
        assert_eq!(map.get("Comp").unwrap().size, vanilla_pkg.len() as i64 + 2);
        assert_eq!(
            map.get("CompAlias").unwrap().size,
            vanilla_pkg.len() as i64 + 2
        );
        assert_eq!(map.get("CompSibling").unwrap().offset, sibling_offset + 2);
    }

    #[test]
    fn composite_disable_refuses_unsafe_legacy_standalone_delete() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        let vanilla_pkg = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], false);
        fs::write(cooked_pc.join("S1Common.gpk"), &vanilla_pkg).unwrap();
        let mapper_text = format!("S1Common.gpk?..\\evil,Comp,0,{},|!", vanilla_pkg.len());
        fs::write(
            cooked_pc.join(BACKUP_FILE),
            encrypt_mapper(mapper_text.as_bytes()),
        )
        .unwrap();
        fs::write(
            cooked_pc.join(MAPPER_FILE),
            encrypt_mapper(mapper_text.as_bytes()),
        )
        .unwrap();
        let outside = cooked_pc.join("..").join("evil.gpk");
        fs::write(&outside, b"do-not-delete").unwrap();
        manifest_store::save_install_target_at_root(
            &app_root,
            "evil.mod",
            &InstallTarget::Composite {
                package_name: "..\\evil".into(),
            },
        )
        .unwrap();

        let err = disable_via_patch(&game_root, &app_root, "evil.mod").unwrap_err();

        assert!(err.contains("Refusing to delete unsafe"), "got: {err}");
        assert_eq!(fs::read(outside).unwrap(), b"do-not-delete");
    }

    #[test]
    fn disable_missing_install_target_fails_closed_without_mapper_only_restore() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        let clean_mapper = encrypt_mapper(b"S1Common.gpk?Owner.S1UI_GageBoss,Comp,0,4,|!");
        let dirty_mapper = encrypt_mapper(b"S1Common.gpk?Owner.S1UI_GageBoss,Comp,0,6,|!");
        fs::write(cooked_pc.join(BACKUP_FILE), &clean_mapper).unwrap();
        fs::write(cooked_pc.join(MAPPER_FILE), &dirty_mapper).unwrap();

        let err = disable_via_patch(&game_root, &app_root, "missing-target.mod").unwrap_err();

        assert!(
            err.contains("No install_target sidecar"),
            "missing sidecar must fail closed instead of partially restoring mapper: {err}"
        );
        assert_eq!(
            fs::read(cooked_pc.join(MAPPER_FILE)).unwrap(),
            dirty_mapper,
            "disable without an install target must not restore the mapper alone"
        );
    }

    #[test]
    fn composite_backup_and_restore_do_not_use_direct_fs_copy() {
        let source = include_str!("gpk_patch_deploy.rs");
        let backup_start = source
            .find("fn patch_composite_container_slice")
            .expect("patch_composite_container_slice present");
        let backup_body = &source[backup_start
            ..source[backup_start..]
                .find("fn resolve_live_composite_entry")
                .map(|idx| backup_start + idx)
                .expect("resolve_live_composite_entry follows")];
        let restore_start = source
            .find("fn disable_composite")
            .expect("disable_composite present");
        let restore_body = &source[restore_start
            ..source[restore_start..]
                .find("fn disable_composite_and_restore_mapper")
                .map(|idx| restore_start + idx)
                .expect("disable_composite_and_restore_mapper follows")];

        assert!(
            !backup_body.contains("fs::copy"),
            "first-run .vanilla-bak creation must use atomic copy, not direct fs::copy"
        );
        assert!(
            !restore_body.contains("fs::copy"),
            "composite restore must use atomic copy, not direct fs::copy"
        );
        assert!(
            backup_body.contains("gpk::copy_atomic") && restore_body.contains("gpk::copy_atomic"),
            "backup and restore should share the audited atomic file-copy helper"
        );
    }

    #[test]
    fn standalone_backup_write_and_restore_use_atomic_helpers() {
        let source = include_str!("gpk_patch_deploy.rs");
        let enable_start = source
            .find("fn enable_standalone")
            .expect("enable_standalone present");
        let enable_body = &source[enable_start
            ..source[enable_start..]
                .find("fn disable_standalone")
                .map(|idx| enable_start + idx)
                .expect("disable_standalone follows")];
        let disable_start = source
            .find("fn disable_standalone")
            .expect("disable_standalone present");
        let disable_body = &source[disable_start
            ..source[disable_start..]
                .find("fn vanilla_backup_path")
                .map(|idx| disable_start + idx)
                .expect("vanilla_backup_path follows")];

        assert!(
            !enable_body.contains("fs::copy") && !enable_body.contains("fs::write"),
            "standalone enable must use atomic helpers for backup and patched writes"
        );
        assert!(
            !disable_body.contains("fs::copy") && !disable_body.contains("fs::write"),
            "standalone disable must use atomic helpers for restore"
        );
        assert!(
            enable_body.contains("gpk::copy_atomic")
                && enable_body.contains("gpk::write_atomic_file")
                && disable_body.contains("gpk::copy_atomic"),
            "standalone paths should share audited atomic helpers"
        );
    }

    #[test]
    fn install_persists_manifest_and_does_not_touch_cooked_pc() {
        let s = setup_boss_window_install();
        let mapper_before = fs::read(s.cooked_pc.join(MAPPER_FILE)).unwrap();

        let outcome = install_via_patch(
            &s.game_root,
            &s.app_root,
            "test.mod",
            &s.mod_src,
            PACKAGE_NAME,
        )
        .unwrap();

        assert_eq!(outcome.target_filename, STANDALONE_FILE);
        assert_eq!(outcome.target_package_name, PACKAGE_NAME);

        // Manifest persisted
        let loaded = manifest_store::load_manifest_at_root(&s.app_root, "test.mod")
            .unwrap()
            .expect("manifest persisted");
        assert_eq!(loaded.exports.len(), 1);
        assert_eq!(loaded.exports[0].replacement_payload_hex, "aabbccdd");

        // CookedPC mapper untouched until enable
        let mapper_after = fs::read(s.cooked_pc.join(MAPPER_FILE)).unwrap();
        assert_eq!(mapper_after, mapper_before);
        assert!(!s.cooked_pc.join(STANDALONE_FILE).exists());
    }

    #[test]
    fn enable_writes_patched_bytes_into_composite_container() {
        let s = setup_boss_window_install();
        let container_before = fs::read(s.cooked_pc.join("S1Common.gpk")).unwrap();
        install_via_patch(
            &s.game_root,
            &s.app_root,
            "test.mod",
            &s.mod_src,
            PACKAGE_NAME,
        )
        .unwrap();

        enable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();

        assert!(
            !s.cooked_pc.join(STANDALONE_FILE).exists(),
            "composite enable must not write a standalone redirect file"
        );
        let patched_container = fs::read(s.cooked_pc.join("S1Common.gpk")).unwrap();
        assert_ne!(patched_container, container_before);
        let parsed = parse_package(&patched_container).unwrap();
        let main = parsed
            .exports
            .iter()
            .find(|e| e.object_path == "GageBoss")
            .expect("GageBoss export present");
        assert_eq!(main.payload, vec![0xAA, 0xBB, 0xCC, 0xDD]);

        // The .clean backup is unchanged.
        let clean_now = fs::read(s.cooked_pc.join(BACKUP_FILE)).unwrap();
        let original_clean = encrypt_mapper(
            format!(
                "S1Common.gpk?Owner.S1UI_GageBoss,Comp,0,{},|!",
                s.vanilla_pkg.len()
            )
            .as_bytes(),
        );
        assert_eq!(clean_now, original_clean, "clean backup must not change");
        assert_eq!(
            fs::read(s.cooked_pc.join("S1Common.gpk.vanilla-bak")).unwrap(),
            s.vanilla_pkg,
            "container backup must preserve vanilla bytes"
        );
    }

    #[test]
    fn disable_restores_clean_mapper_and_composite_container() {
        let s = setup_boss_window_install();
        let container_before = fs::read(s.cooked_pc.join("S1Common.gpk")).unwrap();
        install_via_patch(
            &s.game_root,
            &s.app_root,
            "test.mod",
            &s.mod_src,
            PACKAGE_NAME,
        )
        .unwrap();
        enable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();
        assert_ne!(
            fs::read(s.cooked_pc.join("S1Common.gpk")).unwrap(),
            container_before
        );

        disable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();

        assert!(!s.cooked_pc.join(STANDALONE_FILE).exists());
        assert_eq!(
            fs::read(s.cooked_pc.join("S1Common.gpk")).unwrap(),
            container_before,
            "container bytes must restore from vanilla backup"
        );
        let mapper_now = fs::read(s.cooked_pc.join(MAPPER_FILE)).unwrap();
        let clean_now = fs::read(s.cooked_pc.join(BACKUP_FILE)).unwrap();
        assert_eq!(
            mapper_now, clean_now,
            "mapper must equal clean after disable"
        );
    }

    #[test]
    fn uninstall_deletes_manifest_and_reverts_state() {
        let s = setup_boss_window_install();
        install_via_patch(
            &s.game_root,
            &s.app_root,
            "test.mod",
            &s.mod_src,
            PACKAGE_NAME,
        )
        .unwrap();
        enable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();

        uninstall_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();

        assert!(!s.cooked_pc.join(STANDALONE_FILE).exists());
        assert!(
            manifest_store::load_manifest_at_root(&s.app_root, "test.mod")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn enable_after_disable_reapplies_cleanly() {
        let s = setup_boss_window_install();
        install_via_patch(
            &s.game_root,
            &s.app_root,
            "test.mod",
            &s.mod_src,
            PACKAGE_NAME,
        )
        .unwrap();
        enable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();
        disable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();

        // Re-enable
        enable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();

        let container = fs::read(s.cooked_pc.join("S1Common.gpk")).unwrap();
        let parsed = parse_package(&container).unwrap();
        let main = parsed
            .exports
            .iter()
            .find(|e| e.object_path == "GageBoss")
            .unwrap();
        assert_eq!(main.payload, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn install_refuses_unsupported_diff_shape() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        // Vanilla = 1-export pkg; modded = 2-export pkg → added export.
        let vanilla_pkg = build_boss_window_test_package([0x10; 4], false);
        let modded_pkg = build_boss_window_test_package([0x10; 4], true);
        fs::write(cooked_pc.join("S1Common.gpk"), &vanilla_pkg).unwrap();
        let mapper_text = format!(
            "S1Common.gpk?Owner.S1UI_GageBoss,Comp,0,{},|!",
            vanilla_pkg.len()
        );
        fs::write(
            cooked_pc.join(BACKUP_FILE),
            encrypt_mapper(mapper_text.as_bytes()),
        )
        .unwrap();
        fs::write(
            cooked_pc.join(MAPPER_FILE),
            encrypt_mapper(mapper_text.as_bytes()),
        )
        .unwrap();
        let mod_src = tmp.path().join("mod-src.gpk");
        fs::write(&mod_src, &modded_pkg).unwrap();

        let err = install_via_patch(&game_root, &app_root, "test.mod", &mod_src, "S1UI_GageBoss")
            .unwrap_err();
        assert!(
            err.contains("added exports"),
            "expected unsupported-diff error, got: {err}"
        );
        // No manifest persisted on refusal
        assert!(manifest_store::load_manifest_at_root(&app_root, "test.mod")
            .unwrap()
            .is_none());
    }

    #[test]
    fn enable_errors_when_no_manifest_persisted() {
        let s = setup_boss_window_install();
        let err = enable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap_err();
        assert!(err.contains("No persisted manifest"), "got: {err}");
    }

    #[test]
    fn migrate_legacy_install_cleans_cooked_pc_when_no_manifest_exists() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        // Pretend a legacy install dropped a "modded" S1UI_FlightBar.gpk
        // into CookedPC. There is no manifest persisted for the mod.
        let modded_blob = b"FAKE-MODDED-PAYLOAD".to_vec();
        let standalone = cooked_pc.join("S1UI_FlightBar.gpk");
        fs::write(&standalone, &modded_blob).unwrap();
        let clean_text = b"S1Common.gpk?Foo.S1UI_FlightBar,Comp,0,42,|!";
        fs::write(cooked_pc.join(BACKUP_FILE), encrypt_mapper(clean_text)).unwrap();
        fs::write(cooked_pc.join(MAPPER_FILE), encrypt_mapper(clean_text)).unwrap();

        let outcome =
            migrate_legacy_install(&game_root, &app_root, "test.mod", "S1UI_FlightBar.gpk");

        assert_eq!(outcome.mod_id, "test.mod");
        assert_eq!(
            outcome.target_filename.as_deref(),
            Some("S1UI_FlightBar.gpk")
        );
        assert!(outcome.error.is_none(), "error: {:?}", outcome.error);
        assert!(!standalone.exists(), "legacy standalone must be removed");
    }

    #[test]
    fn migrate_legacy_install_is_noop_when_manifest_exists() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        let standalone = cooked_pc.join("S1UI_FlightBar.gpk");
        fs::write(&standalone, b"PATCHED-PAYLOAD").unwrap();
        let clean_text = b"S1Common.gpk?Foo.S1UI_FlightBar,Comp,0,42,|!";
        fs::write(cooked_pc.join(BACKUP_FILE), encrypt_mapper(clean_text)).unwrap();
        fs::write(cooked_pc.join(MAPPER_FILE), encrypt_mapper(clean_text)).unwrap();

        // Pre-existing manifest → already on the new flow.
        let mod_src = tmp.path().join("mod-src.gpk");
        let modded_pkg = build_boss_window_test_package([0xAA; 4], false);
        fs::write(&mod_src, &modded_pkg).unwrap();
        let vanilla_pkg = build_boss_window_test_package([0x10; 4], false);
        fs::write(cooked_pc.join("S1Common.gpk"), &vanilla_pkg).unwrap();
        let mapper = format!(
            "S1Common.gpk?Owner.S1UI_GageBoss,Comp,0,{},|!",
            vanilla_pkg.len()
        );
        fs::write(
            cooked_pc.join(BACKUP_FILE),
            encrypt_mapper(mapper.as_bytes()),
        )
        .unwrap();
        fs::write(
            cooked_pc.join(MAPPER_FILE),
            encrypt_mapper(mapper.as_bytes()),
        )
        .unwrap();
        install_via_patch(&game_root, &app_root, "test.mod", &mod_src, "S1UI_GageBoss").unwrap();

        let standalone_before = fs::read(&standalone).unwrap();
        let outcome =
            migrate_legacy_install(&game_root, &app_root, "test.mod", "S1UI_FlightBar.gpk");
        // Manifest exists → migration is a no-op; the standalone is left alone.
        assert!(outcome.error.is_none(), "error: {:?}", outcome.error);
        assert_eq!(
            fs::read(&standalone).unwrap(),
            standalone_before,
            "standalone file must be untouched when a manifest already exists"
        );
    }

    #[test]
    fn enable_refuses_ambiguous_clean_suffix_matches_without_writing_container() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        let vanilla_pkg = build_boss_window_test_package([0x10; 4], false);
        let other_pkg = build_boss_window_test_package([0x20; 4], false);
        let mut container = Vec::new();
        container.extend_from_slice(&vanilla_pkg);
        let other_offset = container.len();
        container.extend_from_slice(&other_pkg);
        fs::write(cooked_pc.join("S1Common.gpk"), &container).unwrap();
        let mapper_text = format!(
            "S1Common.gpk?Owner.S1UI_GageBoss,CompA,0,{},|Other.S1UI_GageBoss,CompB,{other_offset},{},|!",
            vanilla_pkg.len(),
            other_pkg.len()
        );
        fs::write(
            cooked_pc.join(BACKUP_FILE),
            encrypt_mapper(mapper_text.as_bytes()),
        )
        .unwrap();
        fs::write(
            cooked_pc.join(MAPPER_FILE),
            encrypt_mapper(mapper_text.as_bytes()),
        )
        .unwrap();

        let modded_pkg = build_boss_window_test_package([0xAA; 4], false);
        let mod_src = tmp.path().join("mod-src.gpk");
        fs::write(&mod_src, &modded_pkg).unwrap();

        manifest_store::save_manifest_at_root(
            &app_root,
            "test.mod",
            &size_changing_manifest("test.mod", &vanilla_pkg),
        )
        .unwrap();
        manifest_store::save_install_target_at_root(
            &app_root,
            "test.mod",
            &InstallTarget::Composite {
                package_name: PACKAGE_NAME.into(),
            },
        )
        .unwrap();

        let err = enable_via_patch(&game_root, &app_root, "test.mod").unwrap_err();
        assert!(
            err.contains("multiple clean composite byte ranges"),
            "got: {err}"
        );
        assert_eq!(
            fs::read(cooked_pc.join("S1Common.gpk")).unwrap(),
            container,
            "ambiguous enable must not mutate the container"
        );
    }

    // --- Type B (standalone in-place) tests ----------------------------

    struct StandaloneSetup {
        _tmp: TempDir,
        game_root: std::path::PathBuf,
        app_root: std::path::PathBuf,
        vanilla_path: std::path::PathBuf,
        backup_path: std::path::PathBuf,
        mod_src: std::path::PathBuf,
    }

    fn setup_standalone_install() -> StandaloneSetup {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        let s1ui_dir = cooked_pc.join("Art_Data").join("Packages").join("S1UI");
        fs::create_dir_all(&s1ui_dir).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        // Empty composite mapper backup so resolver falls through to
        // filesystem walk.
        fs::write(cooked_pc.join(BACKUP_FILE), encrypt_mapper(b"")).unwrap();
        fs::write(cooked_pc.join(MAPPER_FILE), encrypt_mapper(b"")).unwrap();

        let vanilla_pkg = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], false);
        let vanilla_path = s1ui_dir.join("S1UI_GageBoss.gpk");
        fs::write(&vanilla_path, &vanilla_pkg).unwrap();
        let backup_path = {
            let mut s = vanilla_path.as_os_str().to_owned();
            s.push(".vanilla-bak");
            std::path::PathBuf::from(s)
        };

        let modded_pkg = build_boss_window_test_package([0xAA, 0xBB, 0xCC, 0xDD], false);
        let mod_src = tmp.path().join("mod-src.gpk");
        fs::write(&mod_src, &modded_pkg).unwrap();

        StandaloneSetup {
            _tmp: tmp,
            game_root,
            app_root,
            vanilla_path,
            backup_path,
            mod_src,
        }
    }

    #[test]
    fn standalone_install_persists_manifest_and_install_target_sidecar() {
        let s = setup_standalone_install();
        let outcome = install_via_patch(
            &s.game_root,
            &s.app_root,
            "test.mod",
            &s.mod_src,
            "S1UI_GageBoss",
        )
        .unwrap();

        assert_eq!(outcome.target_filename, "S1UI_GageBoss.gpk");

        // Manifest persisted.
        assert!(
            manifest_store::load_manifest_at_root(&s.app_root, "test.mod")
                .unwrap()
                .is_some()
        );
        // Install_target sidecar persisted with Standalone kind.
        let target = manifest_store::load_install_target_at_root(&s.app_root, "test.mod")
            .unwrap()
            .expect("sidecar persisted");
        match target {
            InstallTarget::Standalone { relative_path } => {
                assert!(
                    relative_path.contains("S1UI_GageBoss.gpk"),
                    "got: {relative_path}"
                );
                // Forward-slash on disk so cross-platform path is portable.
                assert!(!relative_path.contains('\\'), "got: {relative_path}");
            }
            other => panic!("expected Standalone, got {other:?}"),
        }

        // Install must NOT touch the vanilla file or write a backup yet —
        // those happen at enable time.
        assert!(
            !s.backup_path.exists(),
            "backup must not exist after install alone"
        );
    }

    #[test]
    fn standalone_enable_writes_patched_bytes_in_place_and_creates_backup() {
        let s = setup_standalone_install();
        let vanilla_before = fs::read(&s.vanilla_path).unwrap();

        install_via_patch(
            &s.game_root,
            &s.app_root,
            "test.mod",
            &s.mod_src,
            "S1UI_GageBoss",
        )
        .unwrap();
        enable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();

        // Backup created with the original vanilla bytes.
        assert!(s.backup_path.exists(), "backup must exist after enable");
        assert_eq!(fs::read(&s.backup_path).unwrap(), vanilla_before);

        // Vanilla path now contains patched bytes.
        let patched = fs::read(&s.vanilla_path).unwrap();
        assert_ne!(
            patched, vanilla_before,
            "patched bytes must differ from vanilla"
        );
        let parsed = parse_package(&patched).unwrap();
        let main = parsed
            .exports
            .iter()
            .find(|e| e.object_path == "GageBoss")
            .unwrap();
        assert_eq!(main.payload, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn standalone_disable_restores_vanilla_from_backup() {
        let s = setup_standalone_install();
        let vanilla_before = fs::read(&s.vanilla_path).unwrap();

        install_via_patch(
            &s.game_root,
            &s.app_root,
            "test.mod",
            &s.mod_src,
            "S1UI_GageBoss",
        )
        .unwrap();
        enable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();
        disable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();

        assert_eq!(
            fs::read(&s.vanilla_path).unwrap(),
            vanilla_before,
            "vanilla file must be restored after disable"
        );
    }

    #[test]
    fn standalone_enable_after_disable_reapplies_cleanly() {
        let s = setup_standalone_install();
        install_via_patch(
            &s.game_root,
            &s.app_root,
            "test.mod",
            &s.mod_src,
            "S1UI_GageBoss",
        )
        .unwrap();
        enable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();
        disable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();

        enable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();

        let patched = fs::read(&s.vanilla_path).unwrap();
        let parsed = parse_package(&patched).unwrap();
        let main = parsed
            .exports
            .iter()
            .find(|e| e.object_path == "GageBoss")
            .unwrap();
        assert_eq!(main.payload, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn standalone_uninstall_restores_vanilla_and_deletes_manifest() {
        let s = setup_standalone_install();
        let vanilla_before = fs::read(&s.vanilla_path).unwrap();

        install_via_patch(
            &s.game_root,
            &s.app_root,
            "test.mod",
            &s.mod_src,
            "S1UI_GageBoss",
        )
        .unwrap();
        enable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();

        uninstall_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();

        assert_eq!(
            fs::read(&s.vanilla_path).unwrap(),
            vanilla_before,
            "vanilla restored after uninstall"
        );
        assert!(
            manifest_store::load_manifest_at_root(&s.app_root, "test.mod")
                .unwrap()
                .is_none(),
            "manifest deleted after uninstall"
        );
        assert!(
            manifest_store::load_install_target_at_root(&s.app_root, "test.mod")
                .unwrap()
                .is_none(),
            "install_target sidecar deleted after uninstall (bundle dir removed)"
        );
    }

    #[test]
    fn install_refuses_x32_mod_when_vanilla_is_x64() {
        // Realistic Classic+ scenario: vanilla file on disk is v100.02 (x64,
        // FileVersion 897) but the catalog mod was authored for old Classic
        // (x32, FileVersion 610). The two binary layouts don't correspond
        // and the engine can't load an x32 package — the install must
        // refuse cleanly with a message that names the arch mismatch.
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        let s1ui_dir = cooked_pc.join("Art_Data").join("Packages").join("S1UI");
        fs::create_dir_all(&s1ui_dir).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        // Empty mapper backups → resolver falls through to filesystem walk.
        fs::write(cooked_pc.join(BACKUP_FILE), encrypt_mapper(b"")).unwrap();
        fs::write(cooked_pc.join(MAPPER_FILE), encrypt_mapper(b"")).unwrap();

        // x64 vanilla on disk.
        let vanilla_pkg = build_x64_boss_window_test_package([0x10; 4], false);
        fs::write(s1ui_dir.join("S1UI_GageBoss.gpk"), &vanilla_pkg).unwrap();

        // x32 mod (FileVersion 610) — the real-world foglio1024 case.
        let modded_pkg = build_boss_window_test_package([0xAA; 4], false);
        let mod_src = tmp.path().join("mod-src.gpk");
        fs::write(&mod_src, &modded_pkg).unwrap();

        let err = install_via_patch(&game_root, &app_root, "test.mod", &mod_src, "S1UI_GageBoss")
            .unwrap_err();

        assert!(err.contains("incompatible"), "got: {err}");
        assert!(err.contains("x32"), "got: {err}");
        assert!(err.contains("x64"), "got: {err}");

        // No artefacts persisted on refusal.
        assert!(
            manifest_store::load_manifest_at_root(&app_root, "test.mod")
                .unwrap()
                .is_none(),
            "manifest must not be persisted on arch refusal"
        );
        assert!(
            manifest_store::load_install_target_at_root(&app_root, "test.mod")
                .unwrap()
                .is_none(),
            "install_target sidecar must not be persisted on arch refusal"
        );
    }

    #[test]
    fn install_accepts_x64_mod_when_vanilla_is_x64() {
        // Sanity check the inverse: a v100.02-authored mod against v100.02
        // vanilla must NOT be refused for arch reasons. (SaltyMonkey
        // S1UI_Message is real-world example.)
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        let s1ui_dir = cooked_pc.join("Art_Data").join("Packages").join("S1UI");
        fs::create_dir_all(&s1ui_dir).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        fs::write(cooked_pc.join(BACKUP_FILE), encrypt_mapper(b"")).unwrap();
        fs::write(cooked_pc.join(MAPPER_FILE), encrypt_mapper(b"")).unwrap();

        let vanilla_pkg = build_x64_boss_window_test_package([0x10; 4], false);
        fs::write(s1ui_dir.join("S1UI_GageBoss.gpk"), &vanilla_pkg).unwrap();

        let modded_pkg = build_x64_boss_window_test_package([0xAA; 4], false);
        let mod_src = tmp.path().join("mod-src.gpk");
        fs::write(&mod_src, &modded_pkg).unwrap();

        install_via_patch(&game_root, &app_root, "test.mod", &mod_src, "S1UI_GageBoss")
            .expect("x64-vs-x64 install must succeed");

        assert!(
            manifest_store::load_manifest_at_root(&app_root, "test.mod")
                .unwrap()
                .is_some(),
            "x64 install must persist a manifest"
        );
    }

    #[test]
    fn install_accepts_lzo_compressed_x64_mod_when_vanilla_is_x64() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        let s1ui_dir = cooked_pc.join("Art_Data").join("Packages").join("S1UI");
        fs::create_dir_all(&s1ui_dir).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        fs::write(cooked_pc.join(BACKUP_FILE), encrypt_mapper(b"")).unwrap();
        fs::write(cooked_pc.join(MAPPER_FILE), encrypt_mapper(b"")).unwrap();

        let vanilla_pkg = build_x64_boss_window_test_package([0x10; 4], false);
        fs::write(s1ui_dir.join("S1UI_GageBoss.gpk"), &vanilla_pkg).unwrap();

        let modded_pkg = build_x64_boss_window_test_package([0xAA; 4], false);
        let compressed_modded = lzo_compress_fixture(&modded_pkg);
        let mod_src = tmp.path().join("mod-src.gpk");
        fs::write(&mod_src, &compressed_modded).unwrap();

        install_via_patch(&game_root, &app_root, "test.mod", &mod_src, "S1UI_GageBoss")
            .expect("compressed x64-vs-x64 install must be normalized and accepted");

        let manifest = manifest_store::load_manifest_at_root(&app_root, "test.mod")
            .unwrap()
            .expect("compressed x64 install must persist a manifest");
        assert_eq!(manifest.exports.len(), 1);
        assert_eq!(manifest.exports[0].replacement_payload_hex, "aaaaaaaa");
    }

    #[test]
    fn composite_enable_preserves_raw_compressed_mod_package_bytes() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        let vanilla_pkg = build_x64_boss_window_test_package([0x10; 4], false);
        let compressed_vanilla = lzo_compress_fixture(&vanilla_pkg);
        let sibling_pkg = build_x64_boss_window_test_package([0x20; 4], false);
        let sibling_offset = compressed_vanilla.len() as i64;
        let mut container = Vec::new();
        container.extend_from_slice(&compressed_vanilla);
        container.extend_from_slice(&sibling_pkg);
        fs::write(cooked_pc.join("ff54e3e4_04.gpk"), &container).unwrap();

        let mapper_text = format!(
            "ff54e3e4_04.gpk?Owner.S1UI_GageBoss,CompTarget,0,{},|Owner.Sibling,CompSibling,{sibling_offset},{},|!",
            compressed_vanilla.len(),
            sibling_pkg.len()
        );
        let encrypted = encrypt_mapper(mapper_text.as_bytes());
        fs::write(cooked_pc.join(MAPPER_FILE), &encrypted).unwrap();
        fs::write(cooked_pc.join(BACKUP_FILE), &encrypted).unwrap();

        let modded_pkg = build_x64_boss_window_test_package([0xAA; 4], false);
        let compressed_modded = lzo_compress_fixture(&modded_pkg);
        let mod_src = tmp.path().join("mod-src.gpk");
        fs::write(&mod_src, &compressed_modded).unwrap();

        install_via_patch(&game_root, &app_root, "test.mod", &mod_src, PACKAGE_NAME).unwrap();
        enable_via_patch(&game_root, &app_root, "test.mod").unwrap();

        let patched_container = fs::read(cooked_pc.join("ff54e3e4_04.gpk")).unwrap();
        assert_eq!(
            &patched_container[..compressed_modded.len()],
            compressed_modded.as_slice(),
            "composite deploy must preserve the raw modded package bytes the game loader will parse"
        );

        let mapper_now = fs::read(cooked_pc.join(MAPPER_FILE)).unwrap();
        let map = parse_mapper(&String::from_utf8_lossy(&decrypt_mapper(&mapper_now)));
        let target = map.get("CompTarget").expect("target entry present");
        assert_eq!(target.size, compressed_modded.len() as i64);
        let sibling = map.get("CompSibling").expect("sibling entry present");
        assert_eq!(sibling.offset, compressed_modded.len() as i64);
    }

    fn lzo_compress_fixture(bytes: &[u8]) -> Vec<u8> {
        const PACKAGE_MAGIC: u32 = 0x9E2A83C1;
        const BLOCK_SIZE: usize = 131_072;

        fn read_u16(bytes: &[u8], offset: usize) -> u16 {
            u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
        }

        fn read_u32(bytes: &[u8], offset: usize) -> u32 {
            u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
        }

        fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }

        fn read_fstring_end(bytes: &[u8], offset: usize) -> usize {
            let len = i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            if len >= 0 {
                offset + 4 + len as usize
            } else {
                offset + 4 + (-len as usize) * 2
            }
        }

        assert_eq!(read_u32(bytes, 0), PACKAGE_MAGIC);
        let file_version = read_u16(bytes, 4);
        let mut cursor = read_fstring_end(bytes, 12);
        let package_flags_pos = cursor;
        cursor += 8;
        let name_offset = read_u32(bytes, cursor) as usize;
        cursor += 4;
        cursor += 20;
        if gpk_package::is_x64_file_version(file_version) {
            cursor += 16;
        }
        cursor += 16;
        let generation_count = read_u32(bytes, cursor) as usize;
        cursor += 4 + generation_count * 12 + 8;
        let compression_flags_pos = cursor;
        let chunk_count_pos = cursor + 4;

        let body = &bytes[name_offset..];
        let mut compressed_blocks = Vec::new();
        let mut block_table = Vec::new();
        for chunk in body.chunks(BLOCK_SIZE) {
            let compressed = lzokay::compress::compress(chunk).expect("compress fixture block");
            block_table.push((compressed.len() as u32, chunk.len() as u32));
            compressed_blocks.push(compressed);
        }
        let compressed_payload_len: usize = compressed_blocks.iter().map(Vec::len).sum();
        let chunk_on_disk_size = 16 + block_table.len() * 8 + compressed_payload_len;
        let chunk_offset = name_offset + 16;

        let mut chunk_blob = Vec::with_capacity(chunk_on_disk_size);
        chunk_blob.extend_from_slice(&PACKAGE_MAGIC.to_le_bytes());
        chunk_blob.extend_from_slice(&(BLOCK_SIZE as u32).to_le_bytes());
        chunk_blob.extend_from_slice(&(compressed_payload_len as u32).to_le_bytes());
        chunk_blob.extend_from_slice(&(body.len() as u32).to_le_bytes());
        for (compressed_size, uncompressed_size) in &block_table {
            chunk_blob.extend_from_slice(&compressed_size.to_le_bytes());
            chunk_blob.extend_from_slice(&uncompressed_size.to_le_bytes());
        }
        for compressed in compressed_blocks {
            chunk_blob.extend_from_slice(&compressed);
        }

        let mut out = Vec::with_capacity(chunk_offset + chunk_on_disk_size);
        out.extend_from_slice(&bytes[..name_offset]);
        for filler in [7u32, 0, 1, 5] {
            out.extend_from_slice(&filler.to_le_bytes());
        }
        let package_flags = read_u32(&out, package_flags_pos) | 0x0200_0000;
        write_u32(&mut out, package_flags_pos, package_flags);
        write_u32(&mut out, compression_flags_pos, 2);
        write_u32(&mut out, chunk_count_pos, 1);
        let chunk_header = chunk_count_pos + 4;
        write_u32(&mut out, chunk_header, name_offset as u32);
        write_u32(&mut out, chunk_header + 4, body.len() as u32);
        write_u32(&mut out, chunk_header + 8, chunk_offset as u32);
        write_u32(&mut out, chunk_header + 12, chunk_on_disk_size as u32);
        out.extend_from_slice(&chunk_blob);
        out
    }

    #[test]
    fn standalone_install_target_relative_path_rejects_traversal_at_apply_time() {
        let s = setup_standalone_install();
        // Hand-craft a malicious sidecar to verify enable defends against it.
        manifest_store::save_install_target_at_root(
            &s.app_root,
            "evil.mod",
            &InstallTarget::Standalone {
                relative_path: "../../Windows/System32/foo.gpk".into(),
            },
        )
        .unwrap();
        // Also write a manifest so enable doesn't bail out earlier.
        let manifest = patch_derivation::derive_manifest(
            "evil.mod",
            &fs::read(&s.vanilla_path).unwrap(),
            &fs::read(&s.mod_src).unwrap(),
        )
        .unwrap();
        manifest_store::save_manifest_at_root(&s.app_root, "evil.mod", &manifest).unwrap();

        let err = enable_via_patch(&s.game_root, &s.app_root, "evil.mod").unwrap_err();
        assert!(err.contains("rejects parent traversal"), "got: {err}");
    }
}
