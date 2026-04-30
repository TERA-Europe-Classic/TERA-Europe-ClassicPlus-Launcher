//! Patch-based GPK install / enable / disable. Two deploy shapes are
//! supported:
//!
//! - **Composite-routed (Type A)**: vanilla bytes live inside a composite
//!   container at a `(filename, offset, size)` recorded in
//!   `CompositePackageMapper.clean`. Enable writes the patched bytes as a
//!   standalone in CookedPC root and redirects the composite mapper at
//!   it. Disable restores the mapper from `.clean` and deletes the
//!   standalone.
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
//! import drift, compressed packages, class changes) are refused at
//! install time inside `patch_derivation::derive_manifest` — strictly
//! better than the legacy "install + break the client" failure mode.

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
    if target_package_name.trim().is_empty() {
        return Err("install_via_patch: target_package_name must not be empty".into());
    }
    let modded = fs::read(source_gpk)
        .map_err(|e| format!("Failed to read modded GPK at {}: {e}", source_gpk.display()))?;

    let resolution =
        vanilla_resolver::resolve_vanilla_for_package_name(game_root, target_package_name)?;

    // Refuse cleanly on x32-vs-x64 arch mismatch BEFORE attempting derivation.
    // v100.02 vanilla files are FileVersion 897 (x64); old Classic mods are
    // FileVersion 610 (x32). The byte structures don't correspond and the
    // engine's loader rejects the wrong-arch file. The legacy install path
    // produced a confusing parser error or a crashing client; this surfaces
    // the real problem (and points at who needs to fix it).
    let mod_version = gpk_package::read_file_version(&modded).ok_or_else(|| {
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

    let manifest = patch_derivation::derive_manifest(mod_id, &resolution.bytes, &modded)?;
    manifest_store::save_manifest_at_root(app_root, mod_id, &manifest)?;

    let install_target = match &resolution.source {
        VanillaSource::Composite => InstallTarget::Composite {
            package_name: target_package_name.to_string(),
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

    match target {
        InstallTarget::Composite { package_name } => {
            enable_composite(game_root, &manifest, &package_name)
        }
        InstallTarget::Standalone { relative_path } => {
            enable_standalone(game_root, &manifest, &relative_path)
        }
    }
}

pub fn disable_via_patch(game_root: &Path, app_root: &Path, mod_id: &str) -> Result<(), String> {
    // We always restore the mapper from `.clean` regardless of resolution
    // kind — a Type A enable redirected the mapper, and we want it back to
    // vanilla even if the install_target sidecar is missing. For Type B
    // this is a cheap no-op since the mapper was never modified.
    gpk::restore_clean_mapper_state(game_root)?;

    let target = match manifest_store::load_install_target_at_root(app_root, mod_id)? {
        Some(t) => t,
        None => {
            // No sidecar — nothing else we can safely clean up. Mapper
            // restore above is the conservative fallback.
            return Ok(());
        }
    };

    match target {
        InstallTarget::Composite { package_name } => {
            let dest = game_root
                .join(gpk::COOKED_PC_DIR)
                .join(format!("{package_name}.gpk"));
            if dest.exists() {
                fs::remove_file(&dest).map_err(|e| {
                    format!(
                        "Failed to delete patched standalone {}: {e}",
                        dest.display()
                    )
                })?;
            }
        }
        InstallTarget::Standalone { relative_path } => {
            disable_standalone(game_root, &relative_path)?;
        }
    }
    Ok(())
}

fn enable_composite(
    game_root: &Path,
    manifest: &super::patch_manifest::PatchManifest,
    target_package_name: &str,
) -> Result<(), String> {
    let raw_vanilla =
        super::composite_extract::extract_vanilla_for_package_name(game_root, target_package_name)?;
    // The carved slice from a composite container preserves the package's
    // original compression. apply_manifest only handles uncompressed input,
    // so normalize first.
    let vanilla = gpk_package::extract_uncompressed_package_bytes(&raw_vanilla).map_err(|e| {
        format!("Failed to decompress composite-resolved vanilla for '{target_package_name}': {e}")
    })?;
    let patched = gpk_patch_applier::apply_manifest(&vanilla, manifest)?;

    let target_filename = format!("{target_package_name}.gpk");
    let cooked_pc = game_root.join(gpk::COOKED_PC_DIR);
    fs::create_dir_all(&cooked_pc).map_err(|e| format!("Failed to create CookedPC dir: {e}"))?;
    let dest = cooked_pc.join(&target_filename);
    fs::write(&dest, &patched)
        .map_err(|e| format!("Failed to write patched GPK to {}: {e}", dest.display()))?;

    let file_size = patched.len() as i64;
    let rewritten = gpk::redirect_mapper_to_standalone(game_root, target_package_name, file_size)?;
    if rewritten == 0 {
        let _ = fs::remove_file(&dest);
        return Err(format!(
            "Composite mapper has no entry pointing at vanilla '{target_package_name}' — mapper drift between install and enable"
        ));
    }
    Ok(())
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
        fs::copy(&vanilla_path, &backup_path).map_err(|e| {
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
    fs::write(&vanilla_path, &patched).map_err(|e| {
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
        fs::copy(&backup_path, &vanilla_path).map_err(|e| {
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
    use crate::services::mods::gpk::{encrypt_mapper, BACKUP_FILE, COOKED_PC_DIR, MAPPER_FILE};
    use crate::services::mods::gpk_package::parse_package;
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
        modded_pkg: Vec<u8>,
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
            modded_pkg,
            mod_src,
        }
    }

    const PACKAGE_NAME: &str = "S1UI_GageBoss";
    const STANDALONE_FILE: &str = "S1UI_GageBoss.gpk";

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
    fn enable_writes_patched_bytes_and_redirects_mapper() {
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

        // Patched standalone exists and parses as a GPK with the modded payload.
        let standalone = fs::read(s.cooked_pc.join(STANDALONE_FILE)).unwrap();
        let parsed = parse_package(&standalone).unwrap();
        let main = parsed
            .exports
            .iter()
            .find(|e| e.object_path == "GageBoss")
            .expect("GageBoss export present");
        assert_eq!(main.payload, vec![0xAA, 0xBB, 0xCC, 0xDD]);

        // Mapper redirects: the entry now points at the standalone
        // file at offset 0 with size = patched bytes len.
        let mapper_now = fs::read(s.cooked_pc.join(MAPPER_FILE)).unwrap();
        assert_ne!(
            mapper_now,
            fs::read(s.cooked_pc.join(BACKUP_FILE)).unwrap(),
            "mapper must change after enable"
        );
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
    }

    #[test]
    fn disable_restores_clean_mapper_and_deletes_standalone() {
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
        assert!(s.cooked_pc.join(STANDALONE_FILE).exists());

        disable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap();

        assert!(!s.cooked_pc.join(STANDALONE_FILE).exists());
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

        let standalone = fs::read(s.cooked_pc.join(STANDALONE_FILE)).unwrap();
        let parsed = parse_package(&standalone).unwrap();
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
    fn enable_rolls_back_standalone_when_mapper_has_no_target_entry() {
        let tmp = TempDir::new().unwrap();
        let game_root = tmp.path().join("game");
        let app_root = tmp.path().join("app");
        let cooked_pc = game_root.join(COOKED_PC_DIR);
        fs::create_dir_all(&cooked_pc).unwrap();
        fs::create_dir_all(&app_root).unwrap();

        let vanilla_pkg = build_boss_window_test_package([0x10; 4], false);
        fs::write(cooked_pc.join("S1Common.gpk"), &vanilla_pkg).unwrap();
        // .clean has the entry the mod targets so install_via_patch can
        // extract vanilla and derive a manifest.
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

        let modded_pkg = build_boss_window_test_package([0xAA; 4], false);
        let mod_src = tmp.path().join("mod-src.gpk");
        fs::write(&mod_src, &modded_pkg).unwrap();

        install_via_patch(&game_root, &app_root, "test.mod", &mod_src, "S1UI_GageBoss").unwrap();

        // …but then we corrupt the LIVE mapper to remove the entry, simulating
        // a state where the mapper has drifted between install and enable.
        fs::write(
            cooked_pc.join(MAPPER_FILE),
            encrypt_mapper(b"S1UI_Other.gpk?Foo.Other,X,0,10,|!"),
        )
        .unwrap();

        let err = enable_via_patch(&game_root, &app_root, "test.mod").unwrap_err();
        assert!(err.contains("no entry pointing at vanilla"), "got: {err}");
        assert!(
            !cooked_pc.join("S1UI_GageBoss.gpk").exists(),
            "standalone must be rolled back when mapper redirect produces no rewrite"
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
