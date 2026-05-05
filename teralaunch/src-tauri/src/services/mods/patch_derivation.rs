//! Runtime derivation of a curated `PatchManifest` from `(reference, modded)`
//! GPK byte pairs.
//!
//! The launcher's install path uses this to convert a downloaded community
//! mod into a manifest the existing narrow applier
//! (`gpk_patch_applier::apply_manifest`) can apply against the user's vanilla
//! bytes. Diff shapes the applier cannot handle yet — added exports, import
//! or name table size changes, compressed packages, class changes — are
//! refused at derivation time so the user gets a clear "unsupported diff"
//! error instead of a silently-broken client.
//!
//! This module is the runtime sibling of `bin/gpk-patch-converter.rs`'s
//! offline `build_manifest_candidate` helper. Both paths emit schema v2
//! manifests with `runtime-derived` / `converter-candidate` source labels so
//! they're distinguishable in audit output.

use super::{gpk_package, patch_manifest};

pub fn derive_manifest(
    mod_id: &str,
    reference_bytes: &[u8],
    modded_bytes: &[u8],
) -> Result<patch_manifest::PatchManifest, String> {
    let reference = gpk_package::parse_package(reference_bytes)
        .map_err(|e| format!("Failed to parse vanilla reference package: {e}"))?;
    let modded = gpk_package::parse_package(modded_bytes)
        .map_err(|e| format!("Failed to parse modded package: {e}"))?;

    if reference.summary.compression_flags != 0 || modded.summary.compression_flags != 0 {
        return Err(
            "Compressed packages are not supported by the Phase 1 patch applier yet".to_string(),
        );
    }

    let diff = gpk_package::compare_packages(&reference, &modded);

    if !diff.added_exports.is_empty() {
        return Err(format!(
            "Mod adds exports ({:?}); the Phase 1 patch applier does not support added exports yet",
            diff.added_exports
        ));
    }
    if diff.import_count_before != diff.import_count_after {
        return Err(format!(
            "Mod changes import-table size ({} -> {}); the Phase 1 patch applier does not support import patches yet",
            diff.import_count_before, diff.import_count_after
        ));
    }
    if diff.name_count_before != diff.name_count_after {
        return Err(format!(
            "Mod changes name-table size ({} -> {}); the Phase 1 patch applier does not support name patches yet",
            diff.name_count_before, diff.name_count_after
        ));
    }

    let mut exports = Vec::new();
    for changed in &diff.changed_exports {
        let r = reference
            .exports
            .iter()
            .find(|e| e.object_path == changed.object_path)
            .ok_or_else(|| format!("Reference export '{}' missing", changed.object_path))?;
        let m = modded
            .exports
            .iter()
            .find(|e| e.object_path == changed.object_path)
            .ok_or_else(|| format!("Modded export '{}' missing", changed.object_path))?;
        if r.class_name != m.class_name {
            return Err(format!(
                "Export '{}' changes class ({:?} -> {:?}); the Phase 1 patch applier does not support class changes yet",
                changed.object_path, r.class_name, m.class_name
            ));
        }
        exports.push(patch_manifest::ExportPatch {
            object_path: changed.object_path.clone(),
            class_name: r.class_name.clone(),
            reference_export_fingerprint: r.payload_fingerprint.clone(),
            target_export_fingerprint: Some(r.payload_fingerprint.clone()),
            operation: patch_manifest::ExportPatchOperation::ReplaceExportPayload,
            new_class_name: None,
            replacement_payload_hex: hex_lower(&m.payload),
        });
    }
    for removed in &diff.removed_exports {
        let r = reference
            .exports
            .iter()
            .find(|e| e.object_path == *removed)
            .ok_or_else(|| format!("Reference export '{}' missing", removed))?;
        exports.push(patch_manifest::ExportPatch {
            object_path: removed.clone(),
            class_name: r.class_name.clone(),
            reference_export_fingerprint: r.payload_fingerprint.clone(),
            target_export_fingerprint: Some(r.payload_fingerprint.clone()),
            operation: patch_manifest::ExportPatchOperation::RemoveExport,
            new_class_name: None,
            replacement_payload_hex: String::new(),
        });
    }

    if exports.is_empty() {
        return Err("Modded package is byte-equivalent to vanilla — nothing to patch".to_string());
    }

    let manifest = patch_manifest::PatchManifest {
        schema_version: 2,
        mod_id: mod_id.to_string(),
        title: mod_id.to_string(),
        target_package: format!("{}.gpk", reference.summary.package_name),
        patch_family: patch_manifest::PatchFamily::UiLayout,
        reference: patch_manifest::ReferenceBaseline {
            source_patch_label: "runtime-derived".into(),
            package_fingerprint: format!(
                "exports:{}|imports:{}|names:{}",
                reference.exports.len(),
                reference.imports.len(),
                reference.names.len()
            ),
            provenance: None,
        },
        compatibility: patch_manifest::CompatibilityPolicy {
            require_exact_package_fingerprint: true,
            require_all_exports_present: false,
            forbid_name_or_import_expansion: false,
        },
        exports,
        import_patches: Vec::new(),
        name_patches: Vec::new(),
        notes: vec!["Derived at install time from vanilla composite + modded GPK".into()],
    };
    manifest.validate()?;
    Ok(manifest)
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(all(test, feature = "lib-tests"))]
mod tests {
    use super::super::test_fixtures::{
        build_boss_window_test_package, build_x64_boss_window_test_package,
    };
    use super::super::{gpk_package, gpk_patch_applier, patch_manifest};
    use super::*;

    #[test]
    fn emits_replace_export_payload_for_changed_export() {
        let reference = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], false);
        let modded = build_boss_window_test_package([0xAA, 0xBB, 0xCC, 0xDD], false);

        let manifest = derive_manifest("test.mod", &reference, &modded).expect("derive ok");

        assert_eq!(manifest.mod_id, "test.mod");
        assert_eq!(manifest.exports.len(), 1);
        let patch = &manifest.exports[0];
        assert_eq!(patch.object_path, "GageBoss");
        assert_eq!(
            patch.operation,
            patch_manifest::ExportPatchOperation::ReplaceExportPayload
        );
        assert_eq!(patch.replacement_payload_hex, "aabbccdd");
        assert!(patch.target_export_fingerprint.is_some());
    }

    #[test]
    fn round_trips_through_applier_against_reference_bytes() {
        let reference = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], false);
        let modded = build_boss_window_test_package([0x90, 0x91, 0x92, 0x93], false);

        let manifest = derive_manifest("test.mod", &reference, &modded).expect("derive ok");
        let patched = gpk_patch_applier::apply_manifest(&reference, &manifest)
            .expect("apply_manifest must accept the derived manifest");

        let parsed = gpk_package::parse_package(&patched).expect("parse patched");
        let main = parsed
            .exports
            .iter()
            .find(|e| e.object_path == "GageBoss")
            .expect("GageBoss export present");
        assert_eq!(main.payload, vec![0x90, 0x91, 0x92, 0x93]);
    }

    #[test]
    fn refuses_added_exports_at_derivation_time() {
        let reference = build_boss_window_test_package([0x10; 4], false);
        let modded = build_boss_window_test_package([0x10; 4], true);

        let err = derive_manifest("test.mod", &reference, &modded)
            .expect_err("added exports must be refused at derivation time");
        assert!(
            err.contains("added exports"),
            "expected message to call out added exports, got: {err}"
        );
    }

    #[test]
    fn refuses_when_modded_equals_reference() {
        let reference = build_boss_window_test_package([0x10; 4], false);
        let modded = build_boss_window_test_package([0x10; 4], false);

        let err = derive_manifest("test.mod", &reference, &modded)
            .expect_err("byte-equal packages must be refused");
        assert!(err.contains("nothing to patch"), "got: {err}");
    }

    #[test]
    fn emits_remove_export_for_dropped_redirector() {
        // Reference has 2 exports (GageBoss + GageBoss.GageBoss_I1C).
        // Modded keeps only GageBoss → diff has one removed_export.
        let reference = build_boss_window_test_package([0x10; 4], true);
        let modded = build_boss_window_test_package([0x10; 4], false);

        let manifest = derive_manifest("test.mod", &reference, &modded).expect("derive ok");
        assert_eq!(manifest.exports.len(), 1);
        let patch = &manifest.exports[0];
        assert_eq!(patch.object_path, "GageBoss.GageBoss_I1C");
        assert_eq!(
            patch.operation,
            patch_manifest::ExportPatchOperation::RemoveExport
        );
        assert!(patch.replacement_payload_hex.is_empty());
    }

    #[test]
    fn surfaces_parse_failure_with_clear_prefix() {
        let bogus = vec![0u8; 16]; // too small + bad magic
        let err = derive_manifest("test.mod", &bogus, &bogus).expect_err("must fail to parse");
        assert!(
            err.contains("Failed to parse"),
            "expected clear parse-failure prefix, got: {err}"
        );
    }

    #[test]
    fn derives_and_applies_against_x64_modern_packages() {
        // v100.02 vanilla + v100.02 modded. Both x64 (FileVersion 897).
        let reference = build_x64_boss_window_test_package([0x10, 0x11, 0x12, 0x13], false);
        let modded = build_x64_boss_window_test_package([0xAA, 0xBB, 0xCC, 0xDD], false);

        let manifest = derive_manifest("test.mod", &reference, &modded).expect("derive ok");
        assert_eq!(manifest.exports.len(), 1);
        assert_eq!(manifest.exports[0].replacement_payload_hex, "aabbccdd");

        let patched =
            gpk_patch_applier::apply_manifest(&reference, &manifest).expect("apply x64 manifest");
        let parsed = gpk_package::parse_package(&patched).expect("parse patched x64");
        assert_eq!(parsed.summary.file_version, 897);
        let main = parsed
            .exports
            .iter()
            .find(|e| e.object_path == "GageBoss")
            .expect("GageBoss export present");
        assert_eq!(main.payload, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    }
}
