// Shared between the main launcher bin and several experimental tooling
// bins via `#[path = ...]` includes; each compilation context exercises
// a different subset, so any single bin sees the rest as "dead".
#![allow(dead_code)]

//! Drop-in install path for Type-D GPK mods whose target package isn't
//! in v100 vanilla's PkgMapper. Writes the (already-x64) GPK directly to
//! S1Game/CookedPC/<target_filename>.gpk and removes it on uninstall.
//!
//! Type-D detection happens at the catalog level (deploy_strategy=Dropin);
//! this module trusts its caller and does not introspect the GPK itself
//! beyond a minimal sanity check.
//!
//! `install_dropin_with_mapper` extends the plain drop-in with PkgMapper +
//! CompositePackageMapper registration so the v100 engine actually loads the
//! file. `install_dropin` (no-mapper variant) remains available for callers
//! that don't need mapper integration (e.g. tests that write fake payloads
//! that won't parse as valid GPKs).

use std::fs;
use std::path::Path;

use super::gpk::{is_safe_gpk_container_filename, COOKED_PC_DIR};

/// Classes the engine looks up by logical path. Exports of any other class
/// (ObjectReferencer, Package, etc.) are bookkeeping objects and must not be
/// registered in PkgMapper.
const INTERESTING_CLASSES: &[&str] = &[
    "Texture2D",
    "StaticMesh",
    "SkeletalMesh",
    "GFxMovieInfo",
    "AnimSet",
    "AnimNodeBlendList",
    "Material",
    "MaterialInstanceConstant",
    "PhysicsAsset",
    "ParticleSystem",
    "SoundCue",
    "SoundNodeWave",
];

/// Install a payload as `S1Game/CookedPC/<target_filename>`.
///
/// Refuses to overwrite an existing file (Type-D mods by definition target
/// packages that don't exist in vanilla; an existing file means either a
/// data-quality issue in the catalog or a stale install).
pub fn install_dropin(
    game_root: &Path,
    mod_id: &str,
    target_filename: &str,
    payload: &[u8],
) -> Result<(), String> {
    if !is_safe_gpk_container_filename(target_filename) {
        return Err(format!(
            "drop-in install of '{mod_id}': unsafe target filename '{target_filename}'"
        ));
    }
    let cooked = game_root.join(COOKED_PC_DIR);
    fs::create_dir_all(&cooked)
        .map_err(|e| format!("create CookedPC dir {}: {e}", cooked.display()))?;
    let target = cooked.join(target_filename);
    if target.exists() {
        return Err(format!(
            "drop-in install of '{mod_id}': refusing to overwrite existing {} — Type-D mods must target a non-vanilla filename",
            target.display()
        ));
    }
    fs::write(&target, payload).map_err(|e| format!("write {}: {e}", target.display()))?;
    Ok(())
}

/// Install a dropin payload AND register its exports in PkgMapper +
/// CompositePackageMapper so the v100 engine actually loads the file.
///
/// The `composite_uid` is synthesised deterministically from `mod_id`
/// (`modres_<sanitized_mod_id>`) so re-installs produce the same rows and a
/// future uninstall pass can locate them by prefix.
///
/// `package_name` strategy: if the GPK header's package_name begins with
/// `MOD:` it is a composite UID, not a real engine-visible name. In that case
/// (and when package_name is otherwise empty) we fall back to
/// `target_filename` stripped of the `.gpk` suffix. This mirrors how TMM
/// names standalone mods and is what the engine expects for logical-path
/// resolution.
///
/// `target_object_path`: when `Some`, the first mapper addition uses it as
/// the logical_path and points at the matching primary export (matched by
/// trailing component with optional `_dup` suffix). This is required for mods
/// (e.g. artexlib-style) whose internal package_name is a composite UID that
/// the engine cannot resolve — the catalog knows the real logical path the
/// engine looks up. Remaining exports (if any) are still registered under the
/// synthesised `package_name`. When `None`, all exports are registered under
/// the synthesised `package_name` (existing behaviour for Type-D mods that
/// don't override a specific logical path).
///
/// Returns the logical paths registered in PkgMapper (for logging /
/// future uninstall registry use).
pub fn install_dropin_with_mapper(
    game_root: &Path,
    mod_id: &str,
    target_filename: &str,
    payload: &[u8],
    target_object_path: Option<&str>,
) -> Result<Vec<String>, String> {
    // Sanity: filename must be safe.
    if !is_safe_gpk_container_filename(target_filename) {
        return Err(format!(
            "drop-in install of '{mod_id}': unsafe target filename '{target_filename}'"
        ));
    }

    // Step 1: parse to learn the package name + exports.
    let pkg = super::gpk_package::parse_package(payload)
        .map_err(|e| format!("dropin parse {target_filename}: {e}"))?;

    // Determine the logical-path prefix. If the header's package_name is a
    // composite UID (`MOD:…`) or is empty, fall back to the filename stem.
    let package_name = if pkg.summary.package_name.starts_with("MOD:") || pkg.summary.package_name.is_empty() {
        target_filename
            .strip_suffix(".gpk")
            .unwrap_or(target_filename)
            .to_string()
    } else {
        pkg.summary.package_name.clone()
    };

    // Step 2: filter to engine-loadable exports.
    // `class_name` is the full import object_path (e.g. `Core.Texture2D`),
    // not just the bare class name. Match on the last path component so both
    // `Core.Texture2D` and `Engine.Texture2D` are recognised.
    let to_register: Vec<&super::gpk_package::GpkExportEntry> = pkg
        .exports
        .iter()
        .filter(|e| {
            e.class_name.as_deref().map(|c| {
                let base = c.rsplit('.').next().unwrap_or(c);
                INTERESTING_CLASSES.contains(&base)
            }).unwrap_or(false)
        })
        .collect();

    if to_register.is_empty() {
        return Err(format!(
            "dropin install of '{mod_id}': payload has no engine-loadable exports (only ObjectReferencer / Package)"
        ));
    }

    // Step 3: write the file to CookedPC.
    install_dropin(game_root, mod_id, target_filename, payload)?;

    // Step 4: synthesise MapperAdditions.
    let composite_filename = target_filename
        .strip_suffix(".gpk")
        .unwrap_or(target_filename)
        .to_string();
    // Deterministic uid: keeps the same composite group across re-installs.
    let composite_uid = format!(
        "modres_{}",
        mod_id
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>()
    );
    let payload_size = payload.len() as i64;

    let make_addition = |logical_path: String, export: &super::gpk_package::GpkExportEntry| {
        super::mapper_extend::MapperAddition {
            logical_path,
            composite_uid: composite_uid.clone(),
            composite_object_path: format!("{}.{}", composite_uid, export.object_path),
            composite_filename: composite_filename.clone(),
            composite_offset: 0,
            composite_size: payload_size,
        }
    };

    let mut additions: Vec<super::mapper_extend::MapperAddition> =
        Vec::with_capacity(to_register.len());

    if let Some(tap) = target_object_path {
        // Find the primary export whose object_name matches the trailing
        // component of target_object_path, with or without a `_dup` suffix.
        let want = tap.rsplit('.').next().unwrap_or(tap);
        let primary = to_register
            .iter()
            .find(|e| {
                e.object_name == want
                    || e.object_name == format!("{want}_dup")
                    || e.object_name.strip_suffix("_dup") == Some(want)
            })
            .ok_or_else(|| {
                format!(
                    "dropin install of '{mod_id}': target_object_path '{tap}' has no matching export \
                     (want object_name in {{{want}, {want}_dup}}; available: {:?})",
                    to_register
                        .iter()
                        .map(|e| &e.object_name)
                        .collect::<Vec<_>>()
                )
            })?;

        // Primary export uses target_object_path as its logical_path.
        additions.push(make_addition(tap.to_string(), primary));

        // Register any remaining interesting exports under the synthesised
        // package_name so cross-export references inside the mod still resolve.
        for export in &to_register {
            if export.object_name == primary.object_name {
                continue;
            }
            additions.push(make_addition(
                format!("{}.{}", package_name, export.object_name),
                export,
            ));
        }
    } else {
        // No target_object_path: register every interesting export under the
        // synthesised package_name (existing behaviour for Type-D mods).
        for export in &to_register {
            additions.push(make_addition(
                format!("{}.{}", package_name, export.object_name),
                export,
            ));
        }
    }

    super::mapper_extend::extend_mappers(game_root, &additions)
        .map_err(|e| format!("dropin install of '{mod_id}': mapper extend failed: {e}"))?;

    Ok(additions.iter().map(|a| a.logical_path.clone()).collect())
}

/// Remove a previously-dropin-installed file and clean up mapper-extend rows.
/// Idempotent: missing file and missing mapper rows are both OK.
pub fn uninstall_dropin(
    game_root: &Path,
    mod_id: &str,
    target_filename: &str,
) -> Result<(), String> {
    if !is_safe_gpk_container_filename(target_filename) {
        return Err(format!(
            "drop-in uninstall: unsafe target filename '{target_filename}'"
        ));
    }
    let target = game_root.join(COOKED_PC_DIR).join(target_filename);
    if target.exists() {
        fs::remove_file(&target)
            .map_err(|e| format!("remove {}: {e}", target.display()))?;
    }
    // Remove the PkgMapper + CompositePackageMapper rows that
    // install_dropin_with_mapper created for this mod.
    super::gpk::clean_prior_dropin_state(game_root, mod_id)?;
    Ok(())
}
