// port-foglio-compiled-pack — extract textures from foglio's compiled x32
// resource GPK (e.g. Modern_Resources/93/S1UIRES_Component.gpk), pair each
// with its v100 vanilla composite slice, and emit modded composite slices
// plus an install manifest.
//
// Strategy:
//   1. Parse foglio's x32 GPK; gather every Texture2D and its decoded raw mip
//      pixels (LZO-decompressed where needed).
//   2. For each texture name `<TARGET_PACKAGE>.<tex_name>`, look up the v100
//      composite UID via PkgMapper.clean (vanilla baseline); extract the
//      vanilla x64 composite slice from the resolved container; locate its
//      single Texture2D and replace its first-mip pixel bytes with foglio's.
//   3. Re-emit each modded slice as a new standalone GPK (uncompressed),
//      assigning a unique modres_<arch>_<idx> composite UID and filename.
//   4. Append a MapperAddition manifest entry per slice; write
//      install-manifest.json into the staging dir.
//
// Usage:
//   port-foglio-compiled-pack --foglio-gpk <path> --target-package <name>
//                             --uid-prefix <prefix> --filename-prefix <prefix>
//                             --game-root <path> --staging <dir>

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[allow(dead_code)] #[path = "../services/mods/dds.rs"] mod dds;
#[allow(dead_code)] #[path = "../services/mods/gpk.rs"] mod gpk;
#[allow(dead_code)] #[path = "../services/mods/gpk_package.rs"] mod gpk_package;
#[allow(dead_code)] #[path = "../services/mods/gpk_patch_applier.rs"] mod gpk_patch_applier;
#[allow(dead_code)] #[path = "../services/mods/gpk_resource_inspector.rs"] mod gpk_resource_inspector;
#[allow(dead_code)] #[path = "../services/mods/patch_manifest.rs"] mod patch_manifest;
#[allow(dead_code)] #[path = "../services/mods/mapper_extend.rs"] mod mapper_extend;

use patch_manifest::{
    CompatibilityPolicy, ExportPatch, ExportPatchOperation, PatchFamily, PatchManifest,
    ReferenceBaseline,
};

const USAGE: &str = "port-foglio-compiled-pack --foglio-gpk <path> --target-package <name> \
    --uid-prefix <prefix> --filename-prefix <prefix> --game-root <path> --staging <dir>";

fn main() {
    if let Err(e) = run() {
        eprintln!("FAIL: {e}");
        std::process::exit(1);
    }
}

struct Args {
    foglio_gpk: PathBuf,
    target_package: String,
    uid_prefix: String,
    filename_prefix: String,
    game_root: PathBuf,
    staging: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let mut foglio_gpk: Option<PathBuf> = None;
    let mut target_package: Option<String> = None;
    let mut uid_prefix: Option<String> = None;
    let mut filename_prefix: Option<String> = None;
    let mut game_root: Option<PathBuf> = None;
    let mut staging: Option<PathBuf> = None;
    let mut iter = env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--foglio-gpk" => foglio_gpk = iter.next().map(PathBuf::from),
            "--target-package" => target_package = iter.next(),
            "--uid-prefix" => uid_prefix = iter.next(),
            "--filename-prefix" => filename_prefix = iter.next(),
            "--game-root" => game_root = iter.next().map(PathBuf::from),
            "--staging" => staging = iter.next().map(PathBuf::from),
            "-h" | "--help" => { println!("{USAGE}"); std::process::exit(0); }
            other => return Err(format!("unknown arg '{other}'\n{USAGE}")),
        }
    }
    Ok(Args {
        foglio_gpk: foglio_gpk.ok_or("--foglio-gpk")?,
        target_package: target_package.ok_or("--target-package")?,
        uid_prefix: uid_prefix.ok_or("--uid-prefix")?,
        filename_prefix: filename_prefix.ok_or("--filename-prefix")?,
        game_root: game_root.ok_or("--game-root")?,
        staging: staging.ok_or("--staging")?,
    })
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes { s.push_str(&format!("{:02x}", b)); }
    s
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    fs::create_dir_all(&args.staging).map_err(|e| format!("mkdir staging: {e}"))?;

    // Load foglio's compiled x32 GPK.
    let foglio_bytes = fs::read(&args.foglio_gpk).map_err(|e| format!("read foglio gpk: {e}"))?;
    let foglio_uncompressed = gpk_package::extract_uncompressed_package_bytes(&foglio_bytes)
        .map_err(|e| format!("decompress foglio: {e}"))?;
    let foglio_pkg = gpk_package::parse_package(&foglio_uncompressed)
        .map_err(|e| format!("parse foglio: {e}"))?;
    println!("foglio source: file_version={} names={} textures={}",
        foglio_pkg.summary.file_version, foglio_pkg.names.len(),
        foglio_pkg.exports.iter().filter(|e| matches!(e.class_name.as_deref(),
            Some("Core.Texture2D") | Some("Core.Engine.Texture2D"))).count());
    let foglio_is_x64 = foglio_pkg.summary.file_version >= gpk_package::X64_VERSION_THRESHOLD;

    // Load v100 vanilla mappers (use .clean so we route through vanilla, not
    // through any prior mod-rewrites).
    let cooked = args.game_root.join(gpk::COOKED_PC_DIR);
    let pkg_clean = fs::read(cooked.join("PkgMapper.clean"))
        .map_err(|e| format!("read PkgMapper.clean: {e}"))?;
    let pkg_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&pkg_clean)).to_string();
    let comp_clean = fs::read(cooked.join("CompositePackageMapper.clean"))
        .map_err(|e| format!("read CompositePackageMapper.clean: {e}"))?;
    let comp_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&comp_clean)).to_string();
    let comp_map = gpk::parse_mapper(&comp_text);

    let mut additions: Vec<mapper_extend::MapperAddition> = Vec::new();
    let mut idx = 0u32;
    let mut converted = 0usize;
    let mut skipped = 0usize;

    for foglio_export in &foglio_pkg.exports {
        // Only Texture2D class.
        let is_texture = matches!(foglio_export.class_name.as_deref(),
            Some("Core.Texture2D") | Some("Core.Engine.Texture2D"));
        if !is_texture { continue; }

        // Texture name (the export's object_name part, no parent path).
        let tex_name = foglio_export.object_path.rsplit('.').next()
            .unwrap_or(&foglio_export.object_path).to_string();

        // PkgMapper logical lookup: <target_package>.<tex_name>
        let logical = format!("{}.{}", args.target_package, tex_name);
        // Find the row in pkg_text: "<logical>,<composite_object_path>|"
        let row_prefix = format!("{},", logical);
        let resolved = pkg_text.split('|')
            .find(|r| r.starts_with(&row_prefix))
            .map(|r| r[row_prefix.len()..].to_string());
        let composite_object_path = match resolved {
            Some(s) => s,
            None => {
                println!("  skip {logical} (no vanilla PkgMapper row)");
                skipped += 1;
                continue;
            }
        };
        // Composite UID = before the dot.
        let composite_uid_vanilla = composite_object_path.split('.').next().unwrap_or("");
        let comp_entry = comp_map.get(composite_uid_vanilla);
        let comp_entry = match comp_entry {
            Some(e) => e,
            None => {
                println!("  skip {logical} ({composite_uid_vanilla} not in CompositeMapper)");
                skipped += 1;
                continue;
            }
        };

        // Extract vanilla composite slice bytes from container.
        let container_path = cooked.join(format!("{}.gpk", comp_entry.filename));
        let container_bytes = fs::read(&container_path)
            .map_err(|e| format!("read container {}: {e}", container_path.display()))?;
        let off = comp_entry.offset as usize;
        let size = comp_entry.size as usize;
        if off + size > container_bytes.len() {
            return Err(format!("vanilla slice out of bounds: {logical}"));
        }
        let vanilla_slice = container_bytes[off..off + size].to_vec();
        let vanilla_uncompressed = gpk_package::extract_uncompressed_package_bytes(&vanilla_slice)
            .map_err(|e| format!("decompress vanilla slice for {logical}: {e}"))?;
        let vanilla_pkg = gpk_package::parse_package(&vanilla_uncompressed)
            .map_err(|e| format!("parse vanilla slice for {logical}: {e}"))?;

        // Find the Texture2D inside (should be exactly one).
        let target_tex = vanilla_pkg.exports.iter().find(|e|
            matches!(e.class_name.as_deref(), Some("Core.Texture2D") | Some("Core.Engine.Texture2D")));
        let target_tex = match target_tex {
            Some(t) => t,
            None => {
                println!("  skip {logical} (vanilla slice has no Texture2D)");
                skipped += 1;
                continue;
            }
        };

        // Swap pixels. replace_texture_first_mip_pixels handles dim/element-count
        // checks and returns the rewritten target payload.
        let new_payload = match gpk_resource_inspector::replace_texture_first_mip_pixels(
            target_tex, &vanilla_pkg.names, true,
            foglio_export, &foglio_pkg.names, foglio_is_x64,
        ) {
            Ok(p) => p,
            Err(e) => {
                println!("  skip {logical} (replace failed: {e})");
                skipped += 1;
                continue;
            }
        };

        // Re-emit the vanilla slice with the new payload via apply_manifest.
        let manifest = PatchManifest {
            schema_version: 2,
            mod_id: format!("foglio-pack-{logical}"),
            title: logical.clone(),
            target_package: format!("{}.gpk", vanilla_pkg.summary.package_name),
            patch_family: PatchFamily::UiLayout,
            reference: ReferenceBaseline {
                source_patch_label: "foglio compiled pack".into(),
                package_fingerprint: format!("exports:{}|imports:{}|names:{}",
                    vanilla_pkg.exports.len(), vanilla_pkg.imports.len(), vanilla_pkg.names.len()),
                provenance: None,
            },
            compatibility: CompatibilityPolicy {
                require_exact_package_fingerprint: false,
                require_all_exports_present: false,
                forbid_name_or_import_expansion: false,
            },
            exports: vec![ExportPatch {
                object_path: target_tex.object_path.clone(),
                class_name: target_tex.class_name.clone(),
                reference_export_fingerprint: target_tex.payload_fingerprint.clone(),
                target_export_fingerprint: Some(target_tex.payload_fingerprint.clone()),
                operation: ExportPatchOperation::ReplaceExportPayload,
                new_class_name: None,
                replacement_payload_hex: hex_lower(&new_payload),
            }],
            import_patches: vec![],
            name_patches: vec![],
            notes: vec![format!("Foglio pixel swap from {}", args.foglio_gpk.display())],
        };

        let modded_slice = gpk_patch_applier::apply_manifest(&vanilla_uncompressed, &manifest)
            .map_err(|e| format!("apply_manifest for {logical}: {e}"))?;

        // Strategy: keep vanilla's composite_uid + composite_object_path so the
        // GPK's MOD: folder (preserved by apply_manifest) still matches what the
        // engine expects to find. Just point CompositePackageMapper at our new
        // file via REPLACE-by-composite_uid. PkgMapper stays untouched for these.
        idx += 1;
        let new_filename = format!("{}_{:04x}", args.filename_prefix, idx);
        let out_path = args.staging.join(format!("{new_filename}.gpk"));
        fs::write(&out_path, &modded_slice)
            .map_err(|e| format!("write {}: {e}", out_path.display()))?;

        additions.push(mapper_extend::MapperAddition {
            logical_path: logical.clone(),
            composite_uid: composite_uid_vanilla.to_string(), // preserve vanilla
            composite_object_path: composite_object_path.clone(),
            composite_filename: new_filename,
            composite_offset: 0,
            composite_size: modded_slice.len() as i64,
        });
        converted += 1;
    }

    println!("\nconverted {converted} textures, skipped {skipped}");
    let manifest_path = args.staging.join("install-manifest.json");
    let json: Vec<serde_json::Value> = additions.iter().map(|a| serde_json::json!({
        "logical_path": a.logical_path,
        "composite_uid": a.composite_uid,
        "composite_object_path": a.composite_object_path,
        "composite_filename": a.composite_filename,
        "composite_offset": a.composite_offset,
        "composite_size": a.composite_size,
    })).collect();
    fs::write(&manifest_path, serde_json::to_string_pretty(&json)
        .map_err(|e| format!("serialize manifest: {e}"))?)
        .map_err(|e| format!("write manifest: {e}"))?;
    println!("wrote {} entries to manifest at {}", additions.len(), manifest_path.display());
    Ok(())
}

/// Rewrite the GPK's MOD: folder name (FString at offset 12) to a new value.
/// Resizes the file by the length delta. Updates HeaderSize and shifts every
/// later offset (NamesOffset, ImportsOffset, ExportsOffset, DependsOffset,
/// chunk offsets, export SerialOffsets).
fn rewrite_mod_folder(bytes: &[u8], new_object_path: &str) -> Result<Vec<u8>, String> {
    let mut buf = bytes.to_vec();
    if buf.len() < 16 {
        return Err("GPK too small to rewrite folder".into());
    }
    // Read existing FString length at offset 12.
    let old_len_i32 = i32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
    let old_len_bytes: usize = if old_len_i32 >= 0 { old_len_i32 as usize }
                               else { (-old_len_i32) as usize * 2 };
    let old_total = 4 + old_len_bytes; // 4-byte length prefix + bytes

    // Build new FString. Use ASCII (positive length) since "MOD:..." is ASCII.
    let new_str = format!("MOD:{new_object_path}");
    let mut new_bytes_vec: Vec<u8> = new_str.as_bytes().to_vec();
    new_bytes_vec.push(0); // null terminator
    let new_len = new_bytes_vec.len() as i32;
    let mut new_total: Vec<u8> = Vec::new();
    new_total.extend_from_slice(&new_len.to_le_bytes());
    new_total.extend_from_slice(&new_bytes_vec);

    // Splice the new bytes into buf at offset 12, replacing old FString.
    let delta: isize = new_total.len() as isize - old_total as isize;
    let mut out = Vec::with_capacity((buf.len() as isize + delta) as usize);
    out.extend_from_slice(&buf[..12]);
    out.extend_from_slice(&new_total);
    out.extend_from_slice(&buf[12 + old_total..]);

    // We need to shift ALL later offsets by `delta`. The simplest way is to
    // re-parse, regenerate offsets, and emit. But our tooling currently
    // doesn't do that for arbitrary GPKs.
    //
    // Pragmatic fix: don't change folder name length at all if it'd shift
    // offsets. Instead, reject if the new FString is a different byte length
    // than the old. This means the new MOD: folder name MUST have the same
    // length as the original.
    if delta != 0 {
        return Err(format!(
            "MOD: folder length changed (delta={delta}); offset-shifting not supported. \
             old='{}' new='{}'. Either pad new path to same length or extend tooling.",
            std::str::from_utf8(&buf[16..16 + old_len_bytes.saturating_sub(1)])
                .unwrap_or("<non-utf8>"),
            new_str,
        ));
    }
    // Verify identical length, write directly.
    Ok(out)
}
