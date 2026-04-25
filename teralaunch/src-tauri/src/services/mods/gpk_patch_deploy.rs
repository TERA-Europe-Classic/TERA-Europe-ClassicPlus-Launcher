//! Patch-based GPK install / enable / disable.
//!
//! Replaces the legacy "copy modded GPK whole-cloth into CookedPC" path
//! with a flow that:
//!   1. At install: derives a `PatchManifest` from the modded GPK against
//!      vanilla bytes extracted from the user's composite container,
//!      persists the manifest under `<app_root>/patch-manifests/<id>/`.
//!   2. At enable: re-extracts the vanilla bytes, applies the manifest via
//!      `gpk_patch_applier::apply_manifest`, writes the patched bytes as a
//!      standalone `<game>/CookedPC/<target>.gpk`, redirects the composite
//!      mapper at it.
//!   3. At disable: hard-restores the mapper from `.clean` and deletes the
//!      standalone. The vanilla bytes still live in the composite container
//!      so no per-package baseline needs to be persisted.
//!
//! Diff shapes the Phase 1 applier doesn't support (added exports, name /
//! import drift, compressed packages, class changes) are refused at
//! install time inside `patch_derivation::derive_manifest` — strictly
//! better than the legacy "install + break the client" failure mode.

use std::fs;
use std::path::Path;

use super::{
    composite_extract, gpk, gpk_patch_applier, manifest_store, patch_derivation,
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
    let vanilla =
        composite_extract::extract_vanilla_for_package_name(game_root, target_package_name)?;
    let manifest = patch_derivation::derive_manifest(mod_id, &vanilla, &modded)?;
    manifest_store::save_manifest_at_root(app_root, mod_id, &manifest)?;
    Ok(PatchInstallOutcome {
        target_filename: format!("{target_package_name}.gpk"),
        target_package_name: target_package_name.to_string(),
    })
}

pub fn enable_via_patch(game_root: &Path, app_root: &Path, mod_id: &str) -> Result<(), String> {
    let manifest = manifest_store::load_manifest_at_root(app_root, mod_id)?
        .ok_or_else(|| format!("No persisted manifest for mod '{mod_id}' — reinstall required"))?;

    let target_package_name = manifest
        .target_package
        .strip_suffix(".gpk")
        .unwrap_or(&manifest.target_package)
        .to_string();
    if target_package_name.trim().is_empty() {
        return Err(format!(
            "Manifest for '{mod_id}' has empty target_package — refusing to enable"
        ));
    }

    let vanilla =
        composite_extract::extract_vanilla_for_package_name(game_root, &target_package_name)?;
    let patched = gpk_patch_applier::apply_manifest(&vanilla, &manifest)?;

    let target_filename = format!("{target_package_name}.gpk");
    let cooked_pc = game_root.join(gpk::COOKED_PC_DIR);
    fs::create_dir_all(&cooked_pc)
        .map_err(|e| format!("Failed to create CookedPC dir: {e}"))?;
    let dest = cooked_pc.join(&target_filename);
    fs::write(&dest, &patched)
        .map_err(|e| format!("Failed to write patched GPK to {}: {e}", dest.display()))?;

    let file_size = patched.len() as i64;
    let rewritten =
        gpk::redirect_mapper_to_standalone(game_root, &target_package_name, file_size)?;
    if rewritten == 0 {
        // Vanilla mapper had no composite entry for this package. Roll the
        // CookedPC write back so we don't leave an orphan file the engine
        // never references.
        let _ = fs::remove_file(&dest);
        return Err(format!(
            "Composite mapper has no entry pointing at vanilla '{target_package_name}' — \
             this mod targets a package shape the launcher's patch deploy can't route yet"
        ));
    }
    Ok(())
}

pub fn disable_via_patch(game_root: &Path, app_root: &Path, mod_id: &str) -> Result<(), String> {
    // Mapper restore is independent of whether the manifest exists — even
    // if the manifest bundle was deleted out from under us, we still want
    // to give the user a clean mapper.
    gpk::restore_clean_mapper_state(game_root)?;

    if let Some(manifest) = manifest_store::load_manifest_at_root(app_root, mod_id)? {
        let cooked_pc = game_root.join(gpk::COOKED_PC_DIR);
        let dest = cooked_pc.join(&manifest.target_package);
        if dest.exists() {
            fs::remove_file(&dest).map_err(|e| {
                format!(
                    "Failed to delete patched standalone {}: {e}",
                    dest.display()
                )
            })?;
        }
    }
    Ok(())
}

pub fn uninstall_via_patch(game_root: &Path, app_root: &Path, mod_id: &str) -> Result<(), String> {
    disable_via_patch(game_root, app_root, mod_id)?;
    manifest_store::delete_manifest_at_root(app_root, mod_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::mods::gpk::{encrypt_mapper, BACKUP_FILE, COOKED_PC_DIR, MAPPER_FILE};
    use crate::services::mods::gpk_package::parse_package;
    use crate::services::mods::test_fixtures::build_boss_window_test_package;
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
        assert_eq!(mapper_now, clean_now, "mapper must equal clean after disable");
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

        let err = install_via_patch(
            &game_root,
            &app_root,
            "test.mod",
            &mod_src,
            "S1UI_GageBoss",
        )
        .unwrap_err();
        assert!(
            err.contains("added exports"),
            "expected unsupported-diff error, got: {err}"
        );
        // No manifest persisted on refusal
        assert!(
            manifest_store::load_manifest_at_root(&app_root, "test.mod")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn enable_errors_when_no_manifest_persisted() {
        let s = setup_boss_window_install();
        let err = enable_via_patch(&s.game_root, &s.app_root, "test.mod").unwrap_err();
        assert!(err.contains("No persisted manifest"), "got: {err}");
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

        install_via_patch(
            &game_root,
            &app_root,
            "test.mod",
            &mod_src,
            "S1UI_GageBoss",
        )
        .unwrap();

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
}
