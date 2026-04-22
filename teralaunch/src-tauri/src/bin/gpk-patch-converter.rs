#![deny(clippy::all, clippy::pedantic)]

use std::env;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

#[allow(dead_code)]
#[path = "../services/mods/patch_manifest.rs"]
mod patch_manifest;

#[allow(dead_code)]
#[path = "../services/mods/gpk_package.rs"]
mod gpk_package;

use patch_manifest::artifact_layout_for_bundle_dir;

const USAGE: &str = concat!(
    "gpk-patch-converter\n",
    "\n",
    "Usage:\n",
    "  gpk-patch-converter --reference-vanilla <path> --modded-gpk <path> --mod-id <id> --output-bundle-dir <dir>\n",
    "\n",
    "Flags:\n",
    "  --reference-vanilla   Path to the vanilla reference GPK\n",
    "  --modded-gpk          Path to the modded GPK\n",
    "  --mod-id              Stable mod identifier\n",
    "  --output-bundle-dir    Explicit bundle directory for manifest + payloads\n",
    "  -h, --help            Show this help and exit\n"
);

#[derive(Debug)]
struct CliArgs {
    reference_vanilla: PathBuf,
    modded_gpk: PathBuf,
    mod_id: String,
    output_bundle_dir: PathBuf,
}

fn main() {
    match parse_args(env::args_os().skip(1).collect()) {
        Ok(ParseOutcome::Help) => {
            println!("{USAGE}");
            std::process::exit(0);
        }
        Ok(ParseOutcome::Args(args)) => match validate_args(args) {
            Ok(args) => {
                let layout = artifact_layout_for_bundle_dir(&args.output_bundle_dir);
                println!("reference-vanilla: {}", args.reference_vanilla.display());
                println!("modded-gpk: {}", args.modded_gpk.display());
                println!("mod-id: {}", args.mod_id);
                println!("bundle-dir: {}", layout.bundle_dir.display());
                println!("manifest-path: {}", layout.manifest_path.display());
                println!("payload-dir: {}", layout.payload_dir.display());
                if let Ok(reference_bytes) = std::fs::read(&args.reference_vanilla) {
                    if let Ok(reference) = gpk_package::parse_package(&reference_bytes) {
                        println!(
                            "reference-summary: names={} imports={} exports={}",
                            reference.names.len(),
                            reference.imports.len(),
                            reference.exports.len()
                        );
                        if let Ok(modded_bytes) = std::fs::read(&args.modded_gpk) {
                            if let Ok(modded) = gpk_package::parse_package(&modded_bytes) {
                                let diff = gpk_package::compare_packages(&reference, &modded);
                                for line in render_diff_summary(&diff) {
                                    println!("{line}");
                                }
                                if let Ok(manifest) = build_manifest_candidate(
                                    &args.mod_id,
                                    &reference,
                                    &modded,
                                    &diff,
                                ) {
                                    for line in render_manifest_candidate_summary(&manifest) {
                                        println!("{line}");
                                    }
                                }
                            }
                        }
                    }
                }
                if let Ok(modded_bytes) = std::fs::read(&args.modded_gpk) {
                    if let Ok(modded) = gpk_package::parse_package(&modded_bytes) {
                        println!(
                            "modded-summary: names={} imports={} exports={}",
                            modded.names.len(),
                            modded.imports.len(),
                            modded.exports.len()
                        );
                    }
                }
                eprintln!("gpk-patch-converter: not implemented yet");
                std::process::exit(2);
            }
            Err(message) => {
                eprintln!("{message}\n\n{USAGE}");
                std::process::exit(1);
            }
        },
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            std::process::exit(1);
        }
    }
}

enum ParseOutcome {
    Help,
    Args(CliArgs),
}

fn parse_args(args: Vec<OsString>) -> Result<ParseOutcome, String> {
    if args
        .iter()
        .any(|arg| matches!(arg.to_str(), Some("-h") | Some("--help")))
    {
        return Ok(ParseOutcome::Help);
    }

    let mut reference_vanilla = None;
    let mut modded_gpk = None;
    let mut mod_id = None;
    let mut output_bundle_dir = None;

    let mut iter = args.into_iter().peekable();
    while let Some(arg) = iter.next() {
        match arg.to_string_lossy().as_ref() {
            "--reference-vanilla" => {
                reference_vanilla = Some(parse_path_value("--reference-vanilla", &mut iter)?);
            }
            "--modded-gpk" => {
                modded_gpk = Some(parse_path_value("--modded-gpk", &mut iter)?);
            }
            "--mod-id" => {
                mod_id = Some(parse_text_value("--mod-id", &mut iter)?);
            }
            "--output-bundle-dir" => {
                output_bundle_dir = Some(parse_path_value("--output-bundle-dir", &mut iter)?);
            }
            value if value.starts_with('-') => {
                return Err(format!("Unknown flag: {value}"));
            }
            value => {
                return Err(format!("Unexpected positional argument: {value}"));
            }
        }
    }

    let reference_vanilla = reference_vanilla
        .ok_or_else(|| "Missing required flag: --reference-vanilla".to_string())?;
    let modded_gpk = modded_gpk.ok_or_else(|| "Missing required flag: --modded-gpk".to_string())?;
    let mod_id = mod_id.ok_or_else(|| "Missing required flag: --mod-id".to_string())?;
    let output_bundle_dir = output_bundle_dir
        .ok_or_else(|| "Missing required flag: --output-bundle-dir".to_string())?;

    Ok(ParseOutcome::Args(CliArgs {
        reference_vanilla,
        modded_gpk,
        mod_id,
        output_bundle_dir,
    }))
}

fn validate_args(args: CliArgs) -> Result<CliArgs, String> {
    validate_existing_file("--reference-vanilla", &args.reference_vanilla)?;
    validate_existing_file("--modded-gpk", &args.modded_gpk)?;
    validate_output_bundle_dir(&args.output_bundle_dir)?;
    Ok(args)
}

fn parse_path_value(
    flag: &str,
    iter: &mut std::iter::Peekable<std::vec::IntoIter<OsString>>,
) -> Result<PathBuf, String> {
    let value = iter
        .next()
        .ok_or_else(|| format!("Missing value for {flag}"))?;
    if is_flag_token(&value) {
        return Err(format!("Missing value for {flag}"));
    }
    if value.is_empty() {
        return Err(format!("Empty value for {flag}"));
    }
    Ok(PathBuf::from(value))
}

fn parse_text_value(
    flag: &str,
    iter: &mut std::iter::Peekable<std::vec::IntoIter<OsString>>,
) -> Result<String, String> {
    let value = iter
        .next()
        .ok_or_else(|| format!("Missing value for {flag}"))?;
    if is_flag_token(&value) {
        return Err(format!("Missing value for {flag}"));
    }
    let value = value.to_string_lossy().trim().to_string();
    if value.is_empty() {
        return Err(format!("Empty value for {flag}"));
    }
    Ok(value)
}

fn is_flag_token(value: &OsStr) -> bool {
    matches!(value.to_str(), Some(text) if text.starts_with('-'))
}

fn validate_existing_file(flag: &str, path: &PathBuf) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("{flag} does not exist: {}", path.display()));
    }
    if !path.is_file() {
        return Err(format!("{flag} must be a file: {}", path.display()));
    }
    Ok(())
}

fn validate_output_bundle_dir(path: &PathBuf) -> Result<(), String> {
    if path.exists() && !path.is_dir() {
        return Err(format!(
            "--output-bundle-dir must be a directory when it already exists: {}",
            path.display()
        ));
    }
    Ok(())
}

fn render_diff_summary(diff: &gpk_package::GpkPackageDiff) -> Vec<String> {
    let mut lines = vec![format!(
        "diff-summary: names {} -> {}, imports {} -> {}, exports {} -> {}",
        diff.name_count_before,
        diff.name_count_after,
        diff.import_count_before,
        diff.import_count_after,
        diff.export_count_before,
        diff.export_count_after
    )];

    for changed in &diff.changed_exports {
        lines.push(format!(
            "changed-export: {} | class {:?} -> {:?} | payload {} -> {}",
            changed.object_path,
            changed.class_before,
            changed.class_after,
            changed.payload_fingerprint_before,
            changed.payload_fingerprint_after
        ));
    }
    for removed in &diff.removed_exports {
        lines.push(format!("removed-export: {removed}"));
    }
    for added in &diff.added_exports {
        lines.push(format!("added-export: {added}"));
    }
    lines
}

fn render_manifest_candidate_summary(manifest: &patch_manifest::PatchManifest) -> Vec<String> {
    let mut lines = vec![format!(
        "manifest-candidate: schema={} mod_id={} target_package={} exports={}",
        manifest.schema_version,
        manifest.mod_id,
        manifest.target_package,
        manifest.exports.len()
    )];
    for export in &manifest.exports {
        lines.push(format!(
            "manifest-export: {} | op={:?} | class={:?} | payload-bytes={}",
            export.object_path,
            export.operation,
            export.class_name,
            export.replacement_payload_hex.len() / 2
        ));
    }
    lines
}

fn build_manifest_candidate(
    mod_id: &str,
    reference: &gpk_package::GpkPackage,
    modded: &gpk_package::GpkPackage,
    diff: &gpk_package::GpkPackageDiff,
) -> Result<patch_manifest::PatchManifest, String> {
    if !diff.added_exports.is_empty() {
        return Err("Manifest candidate emission does not support added exports yet".into());
    }
    if diff.import_count_before != diff.import_count_after {
        return Err("Manifest candidate emission does not support import-count drift yet".into());
    }
    if diff.name_count_before != diff.name_count_after {
        return Err("Manifest candidate emission does not support name-count drift yet".into());
    }

    let mut exports = Vec::new();
    for changed in &diff.changed_exports {
        let reference_export = reference
            .exports
            .iter()
            .find(|export| export.object_path == changed.object_path)
            .ok_or_else(|| format!("Reference export '{}' missing", changed.object_path))?;
        let modded_export = modded
            .exports
            .iter()
            .find(|export| export.object_path == changed.object_path)
            .ok_or_else(|| format!("Modded export '{}' missing", changed.object_path))?;

        let operation = if reference_export.class_name == modded_export.class_name {
            patch_manifest::ExportPatchOperation::ReplaceExportPayload
        } else {
            patch_manifest::ExportPatchOperation::ReplaceExportClassAndPayload
        };

        exports.push(patch_manifest::ExportPatch {
            object_path: changed.object_path.clone(),
            class_name: reference_export.class_name.clone(),
            reference_export_fingerprint: reference_export.payload_fingerprint.clone(),
            target_export_fingerprint: Some(reference_export.payload_fingerprint.clone()),
            operation,
            new_class_name: if matches!(
                operation,
                patch_manifest::ExportPatchOperation::ReplaceExportClassAndPayload
            ) {
                modded_export.class_name.clone()
            } else {
                None
            },
            replacement_payload_hex: hex_lower(&modded_export.payload),
        });
    }

    for removed in &diff.removed_exports {
        let reference_export = reference
            .exports
            .iter()
            .find(|export| export.object_path == *removed)
            .ok_or_else(|| format!("Reference export '{}' missing", removed))?;
        exports.push(patch_manifest::ExportPatch {
            object_path: removed.clone(),
            class_name: reference_export.class_name.clone(),
            reference_export_fingerprint: reference_export.payload_fingerprint.clone(),
            target_export_fingerprint: Some(reference_export.payload_fingerprint.clone()),
            operation: patch_manifest::ExportPatchOperation::RemoveExport,
            new_class_name: None,
            replacement_payload_hex: String::new(),
        });
    }

    let manifest = patch_manifest::PatchManifest {
        schema_version: 2,
        mod_id: mod_id.to_string(),
        title: mod_id.to_string(),
        target_package: format!("{}.gpk", reference.summary.package_name),
        patch_family: patch_manifest::PatchFamily::UiLayout,
        reference: patch_manifest::ReferenceBaseline {
            source_patch_label: "converter-candidate".into(),
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
        notes: vec![
            "Auto-generated manifest candidate from parsed package diff".into(),
            "Current converter supports only no-addition/no-import-drift candidates".into(),
        ],
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn render_diff_summary_reports_structural_changes() {
        let diff = gpk_package::GpkPackageDiff {
            name_count_before: 6,
            name_count_after: 6,
            import_count_before: 2,
            import_count_after: 2,
            export_count_before: 2,
            export_count_after: 1,
            changed_exports: vec![gpk_package::ChangedExport {
                object_path: "GageBoss".into(),
                class_before: Some("Core.GFxMovieInfo".into()),
                class_after: Some("Core.GFxMovieInfo".into()),
                payload_fingerprint_before: "sha256:before".into(),
                payload_fingerprint_after: "sha256:after".into(),
            }],
            removed_exports: vec!["GageBoss.GageBoss_I1C".into()],
            added_exports: vec![],
        };

        let lines = render_diff_summary(&diff);

        assert!(lines.iter().any(|line| line.contains("diff-summary:")));
        assert!(lines.iter().any(|line| line.contains("exports 2 -> 1")));
        assert!(lines
            .iter()
            .any(|line| line.contains("changed-export: GageBoss")));
        assert!(lines
            .iter()
            .any(|line| line.contains("removed-export: GageBoss.GageBoss_I1C")));
    }

    #[test]
    fn build_manifest_candidate_handles_boss_window_shape() {
        let reference = gpk_package::GpkPackage {
            summary: gpk_package::GpkPackageSummary {
                file_version: 610,
                license_version: 0,
                package_name: "S1UI_GageBoss".into(),
                package_flags: 0,
                name_count: 0,
                name_offset: 0,
                export_count: 2,
                export_offset: 0,
                import_count: 2,
                import_offset: 0,
                depends_offset: 0,
                compression_flags: 0,
            },
            names: vec![],
            imports: vec![
                gpk_package::GpkImportEntry {
                    class_package_name: "Core".into(),
                    class_name: "GFxMovieInfo".into(),
                    owner_index: 0,
                    object_name: "GFxMovieInfo".into(),
                    object_path: "Core.GFxMovieInfo".into(),
                },
                gpk_package::GpkImportEntry {
                    class_package_name: "Core".into(),
                    class_name: "ObjectRedirector".into(),
                    owner_index: 0,
                    object_name: "ObjectRedirector".into(),
                    object_path: "Core.ObjectRedirector".into(),
                },
            ],
            exports: vec![
                gpk_package::GpkExportEntry {
                    class_index: -1,
                    super_index: 0,
                    package_index: 0,
                    object_name: "GageBoss".into(),
                    object_path: "GageBoss".into(),
                    class_name: Some("Core.GFxMovieInfo".into()),
                    serial_size: 4,
                    serial_offset: Some(0),
                    export_flags: 0,
                    payload: vec![0x10, 0x11, 0x12, 0x13],
                    payload_fingerprint: "sha256:before-main".into(),
                },
                gpk_package::GpkExportEntry {
                    class_index: -2,
                    super_index: 0,
                    package_index: 1,
                    object_name: "GageBoss_I1C".into(),
                    object_path: "GageBoss.GageBoss_I1C".into(),
                    class_name: Some("Core.ObjectRedirector".into()),
                    serial_size: 4,
                    serial_offset: Some(4),
                    export_flags: 0,
                    payload: vec![0x20, 0x21, 0x22, 0x23],
                    payload_fingerprint: "sha256:before-redirector".into(),
                },
            ],
        };
        let modded = gpk_package::GpkPackage {
            summary: gpk_package::GpkPackageSummary {
                export_count: 1,
                ..reference.summary.clone()
            },
            names: vec![],
            imports: reference.imports.clone(),
            exports: vec![gpk_package::GpkExportEntry {
                class_index: -1,
                super_index: 0,
                package_index: 0,
                object_name: "GageBoss".into(),
                object_path: "GageBoss".into(),
                class_name: Some("Core.GFxMovieInfo".into()),
                serial_size: 4,
                serial_offset: Some(0),
                export_flags: 0,
                payload: vec![0x90, 0x91, 0x92, 0x93],
                payload_fingerprint: "sha256:after-main".into(),
            }],
        };

        let diff = gpk_package::compare_packages(&reference, &modded);
        let manifest = build_manifest_candidate(
            "foglio1024.ui-remover-boss-window",
            &reference,
            &modded,
            &diff,
        )
        .expect("build candidate manifest");

        assert_eq!(manifest.schema_version, 2);
        assert_eq!(manifest.target_package, "S1UI_GageBoss.gpk");
        assert_eq!(manifest.exports.len(), 2);
        assert!(manifest.exports.iter().any(|export| {
            export.object_path == "GageBoss"
                && matches!(
                    export.operation,
                    patch_manifest::ExportPatchOperation::ReplaceExportPayload
                )
        }));
        assert!(manifest.exports.iter().any(|export| {
            export.object_path == "GageBoss.GageBoss_I1C"
                && matches!(
                    export.operation,
                    patch_manifest::ExportPatchOperation::RemoveExport
                )
        }));
    }

    #[test]
    fn write_manifest_candidate_persists_manifest_json_and_payload_dir() {
        let temp = tempdir().expect("tempdir");
        let layout = patch_manifest::artifact_layout_for_bundle_dir(temp.path());
        let manifest = patch_manifest::PatchManifest {
            schema_version: 2,
            mod_id: "foglio1024.ui-remover-boss-window".into(),
            title: "UI Remover: Boss Window".into(),
            target_package: "S1UI_GageBoss.gpk".into(),
            patch_family: patch_manifest::PatchFamily::UiLayout,
            reference: patch_manifest::ReferenceBaseline {
                source_patch_label: "converter-candidate".into(),
                package_fingerprint: "exports:2|imports:2|names:6".into(),
                provenance: None,
            },
            compatibility: patch_manifest::CompatibilityPolicy {
                require_exact_package_fingerprint: true,
                require_all_exports_present: false,
                forbid_name_or_import_expansion: false,
            },
            exports: vec![patch_manifest::ExportPatch {
                object_path: "GageBoss.GageBoss_I1C".into(),
                class_name: Some("Core.ObjectRedirector".into()),
                reference_export_fingerprint: "sha256:before".into(),
                target_export_fingerprint: Some("sha256:before".into()),
                operation: patch_manifest::ExportPatchOperation::RemoveExport,
                new_class_name: None,
                replacement_payload_hex: String::new(),
            }],
            import_patches: vec![],
            name_patches: vec![],
            notes: vec!["candidate".into()],
        };

        write_manifest_candidate(&layout, &manifest).expect("write candidate manifest");

        assert!(layout.manifest_path.exists(), "manifest.json should be created");
        assert!(layout.payload_dir.exists(), "payloads dir should be created");
        let loaded = patch_manifest::load_manifest(&layout.manifest_path).expect("reload manifest");
        assert_eq!(loaded.mod_id, manifest.mod_id);
        assert_eq!(loaded.exports.len(), 1);
    }
}
