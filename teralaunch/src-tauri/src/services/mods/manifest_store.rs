//! Save/load/delete persisted patch manifests under
//! `<app_data>/mods/patch-manifests/<mod_id>/manifest.json`.
//!
//! This is the persistence companion to `patch_derivation::derive_manifest`.
//! Production code uses the `*_for_mod` helpers, which read the manifest
//! root from `dirs_next::config_dir()`. Tests parameterize on a `root`
//! path so they don't touch the user's real config dir.
//!
//! Manifest validation is enforced on load so a corrupted on-disk artifact
//! never gets handed to the applier.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::patch_manifest::{self, artifact_layout_for_mod_at_root, PatchManifest};

/// Per-mod sidecar persisted next to the manifest. Records *how* the
/// vanilla baseline was resolved at install time so enable/disable can
/// dispatch to the same code path that derived the manifest. Without this
/// the runtime classifier could disagree between install and apply (e.g.
/// the user's mapper changed between the two calls), causing a Type A
/// install to be re-applied as Type B.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstallTarget {
    /// Composite-routed: vanilla bytes lived in a composite container.
    /// On enable: write patched standalone in CookedPC root, redirect
    /// composite mapper. On disable: restore mapper from `.clean`,
    /// delete standalone.
    Composite {
        /// The package name (e.g. `"S1UI_GageBoss"`). Used to resolve
        /// the composite entry again at apply time.
        package_name: String,
    },
    /// Standalone-file: vanilla `.gpk` lived at a deep path under
    /// CookedPC. On enable: backup vanilla to `<path>.vanilla-bak` if
    /// not present, apply manifest, write patched bytes to `<path>`.
    /// On disable: copy `.vanilla-bak` over `<path>`.
    Standalone {
        /// Relative path under `game_root` (e.g.
        /// `"S1Game/CookedPC/Art_Data/Packages/S1UI/S1UI_ProgressBar.gpk"`).
        /// Stored relative so the launcher works across users with
        /// different game install drives.
        relative_path: String,
    },
}

const INSTALL_TARGET_FILENAME: &str = "install_target.json";

pub fn save_install_target_at_root(
    root: &Path,
    mod_id: &str,
    target: &InstallTarget,
) -> Result<(), String> {
    let layout = artifact_layout_for_mod_at_root(root, mod_id);
    fs::create_dir_all(&layout.bundle_dir).map_err(|e| {
        format!(
            "Failed to create bundle dir {} for install_target: {e}",
            layout.bundle_dir.display()
        )
    })?;
    let target_path = layout.bundle_dir.join(INSTALL_TARGET_FILENAME);
    let json = serde_json::to_string_pretty(target)
        .map_err(|e| format!("Failed to serialize install_target for '{mod_id}': {e}"))?;
    let tmp = target_path.with_extension("json.tmp");
    fs::write(&tmp, json)
        .map_err(|e| format!("Failed to write install_target tmp {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &target_path).map_err(|e| {
        format!(
            "Failed to commit install_target {}: {e}",
            target_path.display()
        )
    })
}

pub fn load_install_target_at_root(
    root: &Path,
    mod_id: &str,
) -> Result<Option<InstallTarget>, String> {
    let layout = artifact_layout_for_mod_at_root(root, mod_id);
    let target_path = layout.bundle_dir.join(INSTALL_TARGET_FILENAME);
    if !target_path.exists() {
        return Ok(None);
    }
    let body = fs::read_to_string(&target_path).map_err(|e| {
        format!(
            "Failed to read install_target {}: {e}",
            target_path.display()
        )
    })?;
    let target: InstallTarget = serde_json::from_str(&body).map_err(|e| {
        format!(
            "Failed to parse install_target {}: {e}",
            target_path.display()
        )
    })?;
    Ok(Some(target))
}

pub fn save_manifest_at_root(
    root: &Path,
    mod_id: &str,
    manifest: &PatchManifest,
) -> Result<(), String> {
    manifest.validate()?;
    let layout = artifact_layout_for_mod_at_root(root, mod_id);
    fs::create_dir_all(&layout.bundle_dir).map_err(|e| {
        format!(
            "Failed to create patch-manifest bundle dir {}: {e}",
            layout.bundle_dir.display()
        )
    })?;
    fs::create_dir_all(&layout.payload_dir).map_err(|e| {
        format!(
            "Failed to create patch-manifest payload dir {}: {e}",
            layout.payload_dir.display()
        )
    })?;
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|e| format!("Failed to serialize manifest for '{mod_id}': {e}"))?;
    let tmp = layout.manifest_path.with_extension("json.tmp");
    fs::write(&tmp, json)
        .map_err(|e| format!("Failed to write manifest tmp {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &layout.manifest_path).map_err(|e| {
        format!(
            "Failed to commit manifest {}: {e}",
            layout.manifest_path.display()
        )
    })
}

pub fn load_manifest_at_root(root: &Path, mod_id: &str) -> Result<Option<PatchManifest>, String> {
    let layout = artifact_layout_for_mod_at_root(root, mod_id);
    if !layout.manifest_path.exists() {
        return Ok(None);
    }
    let body = fs::read_to_string(&layout.manifest_path).map_err(|e| {
        format!(
            "Failed to read manifest {}: {e}",
            layout.manifest_path.display()
        )
    })?;
    let manifest: PatchManifest = serde_json::from_str(&body).map_err(|e| {
        format!(
            "Failed to parse manifest {}: {e}",
            layout.manifest_path.display()
        )
    })?;
    manifest.validate()?;
    Ok(Some(manifest))
}

pub fn delete_manifest_at_root(root: &Path, mod_id: &str) -> Result<(), String> {
    let layout = artifact_layout_for_mod_at_root(root, mod_id);
    if layout.bundle_dir.exists() {
        fs::remove_dir_all(&layout.bundle_dir).map_err(|e| {
            format!(
                "Failed to delete patch-manifest bundle {}: {e}",
                layout.bundle_dir.display()
            )
        })?;
    }
    Ok(())
}

// --- Production helpers backed by dirs_next::config_dir() -----------------

pub fn save_manifest(mod_id: &str, manifest: &PatchManifest) -> Result<(), String> {
    let root = patch_manifest::get_manifest_root()
        .ok_or_else(|| "Could not resolve patch-manifest root directory".to_string())?;
    save_manifest_at_root(&root, mod_id, manifest)
}

pub fn load_manifest(mod_id: &str) -> Result<Option<PatchManifest>, String> {
    let root = patch_manifest::get_manifest_root()
        .ok_or_else(|| "Could not resolve patch-manifest root directory".to_string())?;
    load_manifest_at_root(&root, mod_id)
}

pub fn delete_manifest(mod_id: &str) -> Result<(), String> {
    let root = patch_manifest::get_manifest_root()
        .ok_or_else(|| "Could not resolve patch-manifest root directory".to_string())?;
    delete_manifest_at_root(&root, mod_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::mods::patch_manifest::{
        CompatibilityPolicy, ExportPatch, ExportPatchOperation, PatchFamily, PatchManifest,
        ReferenceBaseline,
    };
    use tempfile::TempDir;

    fn sample_manifest(mod_id: &str) -> PatchManifest {
        PatchManifest {
            schema_version: 2,
            mod_id: mod_id.to_string(),
            title: "sample".into(),
            target_package: "S1UI_Sample.gpk".into(),
            patch_family: PatchFamily::UiLayout,
            reference: ReferenceBaseline {
                source_patch_label: "test".into(),
                package_fingerprint: "exports:1|imports:0|names:0".into(),
                provenance: None,
            },
            compatibility: CompatibilityPolicy {
                require_exact_package_fingerprint: true,
                require_all_exports_present: false,
                forbid_name_or_import_expansion: false,
            },
            exports: vec![ExportPatch {
                object_path: "Foo.Bar".into(),
                class_name: Some("Engine.Texture2D".into()),
                reference_export_fingerprint: "abcd".into(),
                target_export_fingerprint: Some("abcd".into()),
                operation: ExportPatchOperation::ReplaceExportPayload,
                new_class_name: None,
                replacement_payload_hex: "deadbeef".into(),
            }],
            import_patches: vec![],
            name_patches: vec![],
            notes: vec![],
        }
    }

    #[test]
    fn save_then_load_round_trips_manifest() {
        let tmp = TempDir::new().unwrap();
        let manifest = sample_manifest("test.mod");
        save_manifest_at_root(tmp.path(), "test.mod", &manifest).unwrap();
        let loaded = load_manifest_at_root(tmp.path(), "test.mod").unwrap();
        assert_eq!(loaded.as_ref(), Some(&manifest));
    }

    #[test]
    fn load_returns_none_when_manifest_absent() {
        let tmp = TempDir::new().unwrap();
        let loaded = load_manifest_at_root(tmp.path(), "missing.mod").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn delete_removes_bundle_dir() {
        let tmp = TempDir::new().unwrap();
        let manifest = sample_manifest("test.mod");
        save_manifest_at_root(tmp.path(), "test.mod", &manifest).unwrap();
        assert!(load_manifest_at_root(tmp.path(), "test.mod")
            .unwrap()
            .is_some());

        delete_manifest_at_root(tmp.path(), "test.mod").unwrap();
        assert!(load_manifest_at_root(tmp.path(), "test.mod")
            .unwrap()
            .is_none());
    }

    #[test]
    fn delete_is_idempotent_when_bundle_missing() {
        let tmp = TempDir::new().unwrap();
        delete_manifest_at_root(tmp.path(), "never.existed").unwrap();
    }

    #[test]
    fn save_refuses_invalid_manifest() {
        let tmp = TempDir::new().unwrap();
        let mut bad = sample_manifest("test.mod");
        bad.exports.clear(); // validate() rejects empty exports list
        let err = save_manifest_at_root(tmp.path(), "test.mod", &bad).unwrap_err();
        assert!(err.contains("at least one export patch"), "got: {err}");
        // Nothing should have been written
        assert!(load_manifest_at_root(tmp.path(), "test.mod")
            .unwrap()
            .is_none());
    }

    #[test]
    fn load_rejects_corrupted_manifest_json() {
        let tmp = TempDir::new().unwrap();
        let layout = artifact_layout_for_mod_at_root(tmp.path(), "test.mod");
        fs::create_dir_all(&layout.bundle_dir).unwrap();
        fs::write(&layout.manifest_path, "{ not json").unwrap();
        let err = load_manifest_at_root(tmp.path(), "test.mod").unwrap_err();
        assert!(err.contains("parse manifest"), "got: {err}");
    }

    #[test]
    fn save_overwrites_existing_manifest() {
        let tmp = TempDir::new().unwrap();
        let m1 = sample_manifest("test.mod");
        save_manifest_at_root(tmp.path(), "test.mod", &m1).unwrap();

        let mut m2 = sample_manifest("test.mod");
        m2.exports[0].replacement_payload_hex = "01020304".into();
        save_manifest_at_root(tmp.path(), "test.mod", &m2).unwrap();

        let loaded = load_manifest_at_root(tmp.path(), "test.mod")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.exports[0].replacement_payload_hex, "01020304");
    }
}
