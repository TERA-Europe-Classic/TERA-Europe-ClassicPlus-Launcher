// Shared between the main launcher bin and several experimental tooling
// bins via `#[path = ...]` includes; each compilation context exercises
// a different subset, so any single bin sees the rest as "dead".
#![allow(dead_code)]

//! Curated GPK patch-manifest schema and artifact-layout helpers.
//!
//! This module defines the launcher-side contract for drift-safe GPK patch
//! artifacts: schema validation, bundle-path resolution, and manifest loading.
//! The current launcher only preflights these artifacts and fails closed when
//! one exists, but the same module is also the seam future patch-application
//! work will build on. Key invariants: manifests must validate before use,
//! artifact bundles resolve deterministically from a mod id, and the on-disk
//! layout stays stable enough for offline converters to target safely.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Curated patch artifact for one launcher-supported GPK mod.
///
/// The launcher should eventually install these manifests instead of blindly
/// dropping legacy whole-package GPK replacements into `CookedPC`. A manifest is
/// authored offline from a known vanilla GPK plus a modded GPK, then reviewed
/// by a maintainer before being shipped to users.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchManifest {
    /// Schema version for forward-compatible evolution of the patch format.
    pub schema_version: u32,
    /// Stable catalog id this patch belongs to (`foglio1024.ui-remover-flight-gauge`).
    pub mod_id: String,
    /// Human-readable patch title for diagnostics / audit output.
    pub title: String,
    /// Package file the patch targets inside the user's current client.
    pub target_package: String,
    /// The patch family drives how strictly we validate + apply it.
    pub patch_family: PatchFamily,
    /// What version/baseline the patch was authored against.
    pub reference: ReferenceBaseline,
    /// Hard safety gates. If any of these fail, the launcher must refuse to apply.
    pub compatibility: CompatibilityPolicy,
    /// The actual export/object replacements to apply.
    pub exports: Vec<ExportPatch>,
    /// Structural import-table patches (v2+). Empty for v1 manifests.
    pub import_patches: Vec<ImportPatch>,
    /// Structural name-table patches (v2+). Empty for v1 manifests.
    pub name_patches: Vec<NamePatch>,
    /// Freeform maintainer notes for audits / troubleshooting.
    pub notes: Vec<String>,
}

impl PatchManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version == 0 {
            return Err("Patch manifest schema_version must be > 0".into());
        }
        if self.mod_id.trim().is_empty() {
            return Err("Patch manifest mod_id must not be empty".into());
        }
        if self.title.trim().is_empty() {
            return Err("Patch manifest title must not be empty".into());
        }
        if self.target_package.trim().is_empty() {
            return Err("Patch manifest target_package must not be empty".into());
        }
        if self.reference.source_patch_label.trim().is_empty() {
            return Err("Patch manifest reference.source_patch_label must not be empty".into());
        }
        if self.reference.package_fingerprint.trim().is_empty() {
            return Err("Patch manifest reference.package_fingerprint must not be empty".into());
        }
        if self.exports.is_empty() {
            return Err("Patch manifest must contain at least one export patch".into());
        }
        for (idx, export) in self.exports.iter().enumerate() {
            if export.object_path.trim().is_empty() {
                return Err(format!("Export patch #{idx} has empty object_path"));
            }
            if export.reference_export_fingerprint.trim().is_empty() {
                return Err(format!(
                    "Export patch #{idx} has empty reference_export_fingerprint"
                ));
            }
            match export.operation {
                ExportPatchOperation::ReplaceExportPayload
                | ExportPatchOperation::ReplaceExportClassAndPayload => {
                    if export.replacement_payload_hex.trim().is_empty() {
                        return Err(format!(
                            "Export patch #{idx} has empty replacement_payload_hex"
                        ));
                    }
                    if !export
                        .replacement_payload_hex
                        .chars()
                        .all(|c| c.is_ascii_hexdigit())
                    {
                        return Err(format!(
                            "Export patch #{idx} replacement_payload_hex must contain only hex digits"
                        ));
                    }
                    if export.replacement_payload_hex.len() % 2 != 0 {
                        return Err(format!(
                            "Export patch #{idx} replacement_payload_hex must have even length"
                        ));
                    }
                }
                ExportPatchOperation::RemoveExport => {
                    // RemoveExport carries no payload — that's fine.
                }
                ExportPatchOperation::PatchProperties => {
                    // Reserved: not yet validated in depth.
                }
            }
        }
        for (idx, imp) in self.import_patches.iter().enumerate() {
            if imp.import_path.trim().is_empty() {
                return Err(format!("Import patch #{idx} has empty import_path"));
            }
            if matches!(imp.operation, ImportPatchOperation::AddImport)
                && imp
                    .class_name
                    .as_ref()
                    .is_none_or(|s| s.trim().is_empty())
                {
                    return Err(format!(
                        "Import patch #{idx} (add_import) must specify class_name"
                    ));
                }
        }
        for (idx, name) in self.name_patches.iter().enumerate() {
            if name.name.trim().is_empty() {
                return Err(format!("Name patch #{idx} has empty name"));
            }
        }
        Ok(())
    }
}

pub fn get_manifest_root() -> Option<PathBuf> {
    dirs_next::config_dir().map(|d| {
        d.join("Crazy-eSports-ClassicPlus")
            .join("mods")
            .join("patch-manifests")
    })
}

fn manifest_bundle_name_for_mod(mod_id: &str) -> String {
    mod_id.replace(['/', '\\'], "_")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchArtifactLayout {
    pub bundle_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub payload_dir: PathBuf,
}

pub fn artifact_layout_for_bundle_dir(bundle_dir: &Path) -> PatchArtifactLayout {
    PatchArtifactLayout {
        manifest_path: bundle_dir.join("manifest.json"),
        payload_dir: bundle_dir.join("payloads"),
        bundle_dir: bundle_dir.to_path_buf(),
    }
}

pub fn artifact_layout_for_mod_at_root(root: &Path, mod_id: &str) -> PatchArtifactLayout {
    let bundle_dir = root.join(manifest_bundle_name_for_mod(mod_id));
    artifact_layout_for_bundle_dir(&bundle_dir)
}

pub fn artifact_layout_for_mod(mod_id: &str) -> Option<PatchArtifactLayout> {
    get_manifest_root().map(|root| artifact_layout_for_mod_at_root(&root, mod_id))
}

pub fn validate_bundle_layout(layout: &PatchArtifactLayout, mod_id: &str) -> Result<(), String> {
    let expected = manifest_bundle_name_for_mod(mod_id);
    let actual = layout
        .bundle_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            format!(
                "Patch artifact bundle dir {} has no valid final path component",
                layout.bundle_dir.display()
            )
        })?;
    if actual != expected {
        return Err(format!(
            "Patch artifact bundle dir name '{actual}' does not match sanitized mod id '{expected}'"
        ));
    }
    if !layout.bundle_dir.exists() {
        return Err(format!(
            "Patch artifact bundle dir does not exist: {}",
            layout.bundle_dir.display()
        ));
    }
    if !layout.bundle_dir.is_dir() {
        return Err(format!(
            "Patch artifact bundle path is not a directory: {}",
            layout.bundle_dir.display()
        ));
    }
    if !layout.manifest_path.exists() {
        return Err(format!(
            "Patch artifact bundle is missing manifest.json at {}",
            layout.manifest_path.display()
        ));
    }
    if !layout.payload_dir.exists() {
        return Err(format!(
            "Patch artifact bundle is missing payloads dir at {}",
            layout.payload_dir.display()
        ));
    }
    if !layout.payload_dir.is_dir() {
        return Err(format!(
            "Patch artifact payload path is not a directory: {}",
            layout.payload_dir.display()
        ));
    }
    for entry in fs::read_dir(&layout.bundle_dir).map_err(|e| {
        format!(
            "Failed to read patch artifact bundle {}: {}",
            layout.bundle_dir.display(),
            e
        )
    })? {
        let entry = entry.map_err(|e| {
            format!(
                "Failed to enumerate patch artifact bundle {}: {}",
                layout.bundle_dir.display(),
                e
            )
        })?;
        let path = entry.path();
        if path == layout.manifest_path || path == layout.payload_dir {
            continue;
        }
        return Err(format!(
            "Patch artifact bundle contains unsupported top-level path outside manifest.json/payloads: {}",
            path.display()
        ));
    }
    Ok(())
}

pub fn load_manifest(path: &Path) -> Result<PatchManifest, String> {
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read patch manifest {}: {}", path.display(), e))?;
    let manifest: PatchManifest = serde_json::from_str(&raw)
        .map_err(|e| format!("Failed to parse patch manifest {}: {}", path.display(), e))?;
    manifest.validate()?;
    Ok(manifest)
}

pub fn load_manifest_for_mod(mod_id: &str) -> Result<Option<PatchManifest>, String> {
    let Some(layout) = artifact_layout_for_mod(mod_id) else {
        return Ok(None);
    };
    if !layout.manifest_path.exists() {
        return Ok(None);
    }
    validate_bundle_layout(&layout, mod_id)?;
    load_manifest(&layout.manifest_path).map(Some)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PatchFamily {
    /// Window/layout/HUD/UI asset patch.
    UiLayout,
    /// Texture/material recolor style patch.
    UiRecolor,
    /// Effect / particle / postprocess cleanup.
    EffectCleanup,
    /// Character/model/cosmetic patch — highest drift risk.
    CosmeticModel,
    /// Anything that doesn't fit the constrained families yet.
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReferenceBaseline {
    /// Human label such as `v100.02` or `p90` from the original mod source.
    pub source_patch_label: String,
    /// Exact package fingerprint from the vanilla package the patch was diffed from.
    pub package_fingerprint: String,
    /// Optional source URL / provenance for the reference baseline.
    pub provenance: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompatibilityPolicy {
    /// Launcher must require exact package fingerprint match before applying.
    pub require_exact_package_fingerprint: bool,
    /// If true, every referenced export must exist in the target package.
    pub require_all_exports_present: bool,
    /// If true, name/import table growth is forbidden for v1 safety.
    pub forbid_name_or_import_expansion: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportPatch {
    /// Stable object locator inside the package (`S1UI_GageBar.SomeMovieClip`).
    pub object_path: String,
    /// Optional class name to strengthen matching.
    pub class_name: Option<String>,
    /// Fingerprint of the export in the reference vanilla package.
    pub reference_export_fingerprint: String,
    /// Fingerprint we expect in the target package before patching.
    pub target_export_fingerprint: Option<String>,
    /// How this export should be modified.
    pub operation: ExportPatchOperation,
    /// New class name for `ReplaceExportClassAndPayload` (e.g. `Core.Texture2D`).
    pub new_class_name: Option<String>,
    /// Replacement payload/body bytes encoded as hex.
    ///
    /// Required for `ReplaceExportPayload` and `ReplaceExportClassAndPayload`.
    /// Empty/unused for `RemoveExport` and `PatchProperties`.
    pub replacement_payload_hex: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportPatchOperation {
    /// Replace the export payload wholesale (v1-compatible).
    ReplaceExportPayload,
    /// Reserved for future structured property patches.
    PatchProperties,
    /// Replace the export's class (e.g. `ObjectRedirector` → `Texture2D`) and payload.
    ReplaceExportClassAndPayload,
    /// Remove the export from the export table entirely.
    RemoveExport,
}

/// Structural patch for import-table mutations (v2).
///
/// Represents a single atomic change to the import table. The applier
/// processes these in order after all export patches are resolved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportPatch {
    /// What to do with this import entry.
    pub operation: ImportPatchOperation,
    /// The import entry to add or remove, identified by its object-name path.
    pub import_path: String,
    /// Optional class-package name for add operations.
    pub class_package: Option<String>,
    /// Optional class name for add operations.
    pub class_name: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportPatchOperation {
    /// Remove this import from the import table.
    RemoveImport,
    /// Add this import to the import table.
    AddImport,
}

/// Structural patch for name-table mutations (v2).
///
/// Name-table changes are derived automatically from export/import patches,
/// but explicit name patches allow the converter to pin specific name entries
/// (e.g. adding a new name needed by a class-change export).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamePatch {
    /// What to do with this name-table entry.
    pub operation: NamePatchOperation,
    /// The name string to add or remove.
    pub name: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NamePatchOperation {
    /// Ensure this name exists in the name table (idempotent).
    EnsureName,
    /// Remove this name from the name table (only safe if unreferenced).
    RemoveName,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> PatchManifest {
        PatchManifest {
            schema_version: 1,
            mod_id: "foglio1024.ui-remover-flight-gauge".into(),
            title: "UI Remover: Flight Gauge".into(),
            target_package: "S1UI_ProgressBar.gpk".into(),
            patch_family: PatchFamily::UiLayout,
            reference: ReferenceBaseline {
                source_patch_label: "v100.02".into(),
                package_fingerprint: "sha256:reference-package".into(),
                provenance: Some(
                    "https://raw.githubusercontent.com/foglio1024/UI-Remover/master/README.md"
                        .into(),
                ),
            },
            compatibility: CompatibilityPolicy {
                require_exact_package_fingerprint: true,
                require_all_exports_present: true,
                forbid_name_or_import_expansion: false,
            },
            exports: vec![ExportPatch {
                object_path: "S1UI_ProgressBar.FlightGauge".into(),
                class_name: Some("GFxMovieInfo".into()),
                reference_export_fingerprint: "sha256:reference-export".into(),
                target_export_fingerprint: Some("sha256:target-export".into()),
                operation: ExportPatchOperation::ReplaceExportPayload,
                new_class_name: None,
                replacement_payload_hex: "deadbeef".into(),
            }],
            import_patches: vec![],
            name_patches: vec![],
            notes: vec![
                "Derived from old vanilla + modded pair".into(),
                "Review required before shipping".into(),
            ],
        }
    }

    #[test]
    fn patch_manifest_round_trips_through_json() {
        let manifest = sample_manifest();
        let json = serde_json::to_string_pretty(&manifest).expect("serialize manifest");
        let decoded: PatchManifest = serde_json::from_str(&json).expect("deserialize manifest");
        assert_eq!(manifest, decoded);
    }

    #[test]
    fn patch_manifest_validate_rejects_empty_exports() {
        let mut manifest = sample_manifest();
        manifest.exports.clear();
        let err = manifest
            .validate()
            .expect_err("manifest must fail when exports are missing");
        assert!(err.contains("at least one export patch"));
    }

    #[test]
    fn patch_manifest_validate_rejects_invalid_hex_payload() {
        let mut manifest = sample_manifest();
        manifest.exports[0].replacement_payload_hex = "xyz".into();
        let err = manifest
            .validate()
            .expect_err("manifest must fail when payload is non-hex");
        assert!(err.contains("hex"));
    }

    #[test]
    fn manifest_bundle_layout_uses_bundle_dir_and_manifest_json() {
        let root = Path::new("C:/patch-manifests");
        let layout = artifact_layout_for_mod_at_root(root, "foglio1024/ui-remover-flight-gauge");

        assert_eq!(
            layout.bundle_dir,
            PathBuf::from("C:/patch-manifests/foglio1024_ui-remover-flight-gauge")
        );
        assert_eq!(
            layout.manifest_path,
            PathBuf::from("C:/patch-manifests/foglio1024_ui-remover-flight-gauge/manifest.json")
        );
        assert_eq!(
            layout.payload_dir,
            PathBuf::from("C:/patch-manifests/foglio1024_ui-remover-flight-gauge/payloads")
        );
    }

    #[test]
    fn manifest_bundle_layout_sanitizes_backslashes_too() {
        let layout = artifact_layout_for_mod("foglio1024\\ui-remover-flight-gauge")
            .expect("manifest root should resolve");
        let bundle = layout
            .bundle_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        assert_eq!(bundle, "foglio1024_ui-remover-flight-gauge");
        assert_eq!(
            layout
                .manifest_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default(),
            "manifest.json"
        );
    }

    #[test]
    fn manifest_bundle_layout_from_explicit_bundle_dir_uses_expected_paths() {
        let bundle_dir = Path::new("C:/patch-manifests/foglio1024_ui-remover-flight-gauge");
        let layout = artifact_layout_for_bundle_dir(bundle_dir);

        assert_eq!(layout.bundle_dir, bundle_dir);
        assert_eq!(layout.manifest_path, bundle_dir.join("manifest.json"));
        assert_eq!(layout.payload_dir, bundle_dir.join("payloads"));
    }

    #[test]
    fn validate_bundle_layout_accepts_manifest_and_payload_dir_only() {
        let temp = tempfile::tempdir().expect("tempdir");
        let layout =
            artifact_layout_for_mod_at_root(temp.path(), "foglio1024.ui-remover-bosswindow");
        fs::create_dir_all(&layout.payload_dir).expect("create payload dir");
        fs::write(
            &layout.manifest_path,
            serde_json::to_string(&sample_manifest()).unwrap(),
        )
        .expect("write manifest");

        validate_bundle_layout(&layout, "foglio1024.ui-remover-bosswindow")
            .expect("bundle layout should validate");
    }

    #[test]
    fn validate_bundle_layout_rejects_unexpected_top_level_paths() {
        let temp = tempfile::tempdir().expect("tempdir");
        let layout =
            artifact_layout_for_mod_at_root(temp.path(), "foglio1024.ui-remover-bosswindow");
        fs::create_dir_all(&layout.payload_dir).expect("create payload dir");
        fs::write(
            &layout.manifest_path,
            serde_json::to_string(&sample_manifest()).unwrap(),
        )
        .expect("write manifest");
        fs::write(layout.bundle_dir.join("notes.txt"), "oops")
            .expect("write unexpected top-level file");

        let err = validate_bundle_layout(&layout, "foglio1024.ui-remover-bosswindow")
            .expect_err("extra top-level paths must fail closed");
        assert!(err.contains("unsupported top-level path"));
    }

    #[test]
    fn patch_family_serializes_as_snake_case() {
        let json = serde_json::to_string(&PatchFamily::UiLayout).expect("serialize enum");
        assert_eq!(json, "\"ui_layout\"");
    }

    #[test]
    fn export_operation_serializes_as_snake_case() {
        let json = serde_json::to_string(&ExportPatchOperation::ReplaceExportPayload)
            .expect("serialize operation");
        assert_eq!(json, "\"replace_export_payload\"");
    }

    #[test]
    fn v2_structural_manifest_round_trips_and_validates() {
        let manifest = PatchManifest {
            schema_version: 2,
            mod_id: "foglio1024.ui-remover-boss-window".into(),
            title: "UI Remover: Boss Window".into(),
            target_package: "S1UI_GageBoss.gpk".into(),
            patch_family: PatchFamily::UiLayout,
            reference: ReferenceBaseline {
                source_patch_label: "v100.02".into(),
                package_fingerprint: "sha256:boss-vanilla-v100".into(),
                provenance: None,
            },
            compatibility: CompatibilityPolicy {
                require_exact_package_fingerprint: true,
                require_all_exports_present: false,
                forbid_name_or_import_expansion: false,
            },
            exports: vec![
                ExportPatch {
                    object_path: "S1UI_GageBoss.GageBoss".into(),
                    class_name: Some("GFxMovieInfo".into()),
                    reference_export_fingerprint: "sha256:gageboss-ref".into(),
                    target_export_fingerprint: None,
                    operation: ExportPatchOperation::ReplaceExportPayload,
                    new_class_name: None,
                    replacement_payload_hex: "cafe".into(),
                },
                ExportPatch {
                    object_path: "S1UI_GageBoss.GageBoss_I1C".into(),
                    class_name: Some("ObjectRedirector".into()),
                    reference_export_fingerprint: "sha256:redirector-ref".into(),
                    target_export_fingerprint: None,
                    operation: ExportPatchOperation::RemoveExport,
                    new_class_name: None,
                    replacement_payload_hex: String::new(),
                },
            ],
            import_patches: vec![ImportPatch {
                operation: ImportPatchOperation::RemoveImport,
                import_path: "Core.Texture2D".into(),
                class_package: None,
                class_name: None,
            }],
            name_patches: vec![NamePatch {
                operation: NamePatchOperation::EnsureName,
                name: "GageBoss_Hidden".into(),
            }],
            notes: vec!["Boss gauge redirector removed".into()],
        };

        manifest.validate().expect("v2 manifest should validate");

        let json = serde_json::to_string_pretty(&manifest).expect("serialize v2");
        let decoded: PatchManifest = serde_json::from_str(&json).expect("deserialize v2");
        assert_eq!(manifest, decoded);
    }

    #[test]
    fn v2_class_change_export_validates_with_new_class_name() {
        let mut manifest = base_v2_manifest();
        manifest.exports.push(ExportPatch {
            object_path: "S1UI_ProgressBar.ProgressBar_I14".into(),
            class_name: Some("ObjectRedirector".into()),
            reference_export_fingerprint: "sha256:redirector-14".into(),
            target_export_fingerprint: None,
            operation: ExportPatchOperation::ReplaceExportClassAndPayload,
            new_class_name: Some("Core.Texture2D".into()),
            replacement_payload_hex: "01020304".into(),
        });
        manifest.import_patches.push(ImportPatch {
            operation: ImportPatchOperation::RemoveImport,
            import_path: "Core.ObjectRedirector".into(),
            class_package: None,
            class_name: None,
        });
        manifest
            .validate()
            .expect("class-change export should validate");
    }

    #[test]
    fn remove_export_patch_validates_without_payload() {
        let mut manifest = base_v2_manifest();
        manifest.exports.push(ExportPatch {
            object_path: "S1UI_GageBoss.GageBoss_I1C".into(),
            class_name: Some("ObjectRedirector".into()),
            reference_export_fingerprint: "sha256:redirector".into(),
            target_export_fingerprint: None,
            operation: ExportPatchOperation::RemoveExport,
            new_class_name: None,
            replacement_payload_hex: String::new(),
        });
        manifest
            .validate()
            .expect("remove-export should validate without payload");
    }

    #[test]
    fn add_import_patch_requires_class_name() {
        let mut manifest = base_v2_manifest();
        manifest.import_patches.push(ImportPatch {
            operation: ImportPatchOperation::AddImport,
            import_path: "Core.Texture2D".into(),
            class_package: Some("Core".into()),
            class_name: None, // missing — should fail validation
        });
        let err = manifest
            .validate()
            .expect_err("add_import without class_name must fail");
        assert!(err.contains("class_name"));
    }

    fn base_v2_manifest() -> PatchManifest {
        PatchManifest {
            schema_version: 2,
            mod_id: "test.v2-check".into(),
            title: "V2 Validation Test".into(),
            target_package: "S1UI_Test.gpk".into(),
            patch_family: PatchFamily::Custom,
            reference: ReferenceBaseline {
                source_patch_label: "test".into(),
                package_fingerprint: "sha256:test".into(),
                provenance: None,
            },
            compatibility: CompatibilityPolicy {
                require_exact_package_fingerprint: false,
                require_all_exports_present: false,
                forbid_name_or_import_expansion: false,
            },
            exports: vec![ExportPatch {
                object_path: "S1UI_Test.Main".into(),
                class_name: None,
                reference_export_fingerprint: "sha256:main".into(),
                target_export_fingerprint: None,
                operation: ExportPatchOperation::ReplaceExportPayload,
                new_class_name: None,
                replacement_payload_hex: "ff".into(),
            }],
            import_patches: vec![],
            name_patches: vec![],
            notes: vec![],
        }
    }
}
