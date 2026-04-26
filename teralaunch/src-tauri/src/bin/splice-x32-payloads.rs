// Offline tool: synthesize a deployable x64 GPK by splicing an x32 mod's
// export payloads onto a vanilla x64 reference's structure.
//
// Why: the launcher's install pipeline derives a patch manifest by diffing
// (modded, vanilla) and rejects diffs whose name/import tables differ in
// size from vanilla. Foglio's x32 mods inherit a 5+ year old vanilla x32
// structure (46 names, 4 imports) that doesn't match modern v100.02
// vanilla x64 (60 names, 32 imports). Mechanically converting x32 bytes
// to x64 layout (the previous approach) preserves the old structure and
// still fails derive_manifest.
//
// The fix: produce an x64 file whose name/import/export *count* matches
// vanilla x64 exactly, but whose export *payloads* come from the x32
// mod. The launcher then sees only ReplaceExportPayload diffs — exactly
// the shape Phase 1 applier supports.
//
// Usage:
//   splice-x32-payloads --vanilla-x64 <path> --modded-x32 <path> --output <path>

use std::env;
use std::fs;
use std::path::PathBuf;

#[allow(dead_code)]
#[path = "../services/mods/patch_manifest.rs"]
mod patch_manifest;

#[allow(dead_code)]
#[path = "../services/mods/gpk_package.rs"]
mod gpk_package;

#[allow(dead_code)]
#[path = "../services/mods/gpk_patch_applier.rs"]
mod gpk_patch_applier;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../services/mods/test_fixtures.rs"]
mod test_fixtures;

use patch_manifest::{
    CompatibilityPolicy, ExportPatch, ExportPatchOperation, PatchFamily, PatchManifest,
    ReferenceBaseline,
};

const USAGE: &str = "splice-x32-payloads --vanilla-x64 <path> --modded-x32 <path> --output <path> [--mod-id <id>] [--rename A=B ...] [--only-class <ClassName> ...]";

struct CliArgs {
    vanilla_x64: PathBuf,
    modded_x32: PathBuf,
    output: PathBuf,
    mod_id: String,
    /// Apply these renames to modded export object_paths before matching
    /// against the vanilla. Lets a mod authored against the standalone
    /// package (e.g. "ProgressBar") splice into a composite-stored dup
    /// container that names the same export "ProgressBar_dup".
    renames: Vec<(String, String)>,
    /// If non-empty, only splice exports whose vanilla class_name appears
    /// in this list. Used to skip x32→x64 incompatible payloads such as
    /// Texture2D (whose property block differs between archs); for UI
    /// removers only the GFxMovieInfo SWF needs to change.
    only_classes: Vec<String>,
    /// For GFxMovieInfo exports, instead of swapping the entire payload
    /// (which would replace the x64 UE3 property wrapper with foglio's
    /// x32 wrapper and confuse the engine's parser), keep vanilla's
    /// wrapper bytes and only swap the embedded "GFX" Scaleform asset
    /// section. The GFX format is arch-independent.
    gfx_swap: bool,
}

fn parse_args() -> Result<CliArgs, String> {
    let mut vanilla_x64: Option<PathBuf> = None;
    let mut modded_x32: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut mod_id = String::from("splice-poc");
    let mut renames: Vec<(String, String)> = Vec::new();
    let mut only_classes: Vec<String> = Vec::new();
    let mut gfx_swap = false;

    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--vanilla-x64" => {
                vanilla_x64 = iter.next().map(PathBuf::from);
            }
            "--modded-x32" => {
                modded_x32 = iter.next().map(PathBuf::from);
            }
            "--output" => {
                output = iter.next().map(PathBuf::from);
            }
            "--mod-id" => {
                if let Some(v) = iter.next() {
                    mod_id = v;
                }
            }
            "--rename" => {
                let v = iter.next().ok_or("--rename needs A=B value")?;
                let (a, b) = v.split_once('=').ok_or_else(|| format!("--rename '{v}' must be A=B"))?;
                renames.push((a.to_string(), b.to_string()));
            }
            "--only-class" => {
                let v = iter.next().ok_or("--only-class needs a class name")?;
                only_classes.push(v);
            }
            "--gfx-swap" => {
                gfx_swap = true;
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("Unknown arg '{other}'")),
        }
    }

    Ok(CliArgs {
        vanilla_x64: vanilla_x64.ok_or("--vanilla-x64 is required")?,
        modded_x32: modded_x32.ok_or("--modded-x32 is required")?,
        output: output.ok_or("--output is required")?,
        mod_id,
        renames,
        only_classes,
        gfx_swap,
    })
}

fn find_gfx_offset(bytes: &[u8]) -> Option<usize> {
    // Scaleform GFx files start with "GFX" (47 46 58) + 1 version byte.
    bytes.windows(4).position(|w| w[0] == b'G' && w[1] == b'F' && w[2] == b'X' && (w[3] >= 0x07 && w[3] <= 0x0C))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            std::process::exit(1);
        }
    };

    let vanilla_bytes = fs::read(&args.vanilla_x64)
        .unwrap_or_else(|e| {
            eprintln!("read vanilla failed: {e}");
            std::process::exit(1);
        });
    let modded_bytes = fs::read(&args.modded_x32)
        .unwrap_or_else(|e| {
            eprintln!("read modded failed: {e}");
            std::process::exit(1);
        });

    let vanilla = gpk_package::parse_package(&vanilla_bytes).unwrap_or_else(|e| {
        eprintln!("vanilla parse failed: {e}");
        std::process::exit(1);
    });
    let modded = gpk_package::parse_package(&modded_bytes).unwrap_or_else(|e| {
        eprintln!("modded parse failed: {e}");
        std::process::exit(1);
    });

    println!("vanilla: names={} imports={} exports={}", vanilla.names.len(), vanilla.imports.len(), vanilla.exports.len());
    println!("modded:  names={} imports={} exports={}", modded.names.len(), modded.imports.len(), modded.exports.len());

    println!("\n--- vanilla exports ---");
    for (i, e) in vanilla.exports.iter().enumerate() {
        println!("  [{i:2}] {} class={:?} payload={} bytes", e.object_path, e.class_name, e.payload.len());
    }
    println!("\n--- modded exports ---");
    for (i, e) in modded.exports.iter().enumerate() {
        println!("  [{i:2}] {} class={:?} payload={} bytes", e.object_path, e.class_name, e.payload.len());
    }

    // Match by object_path, applying user-supplied renames first so a mod
    // authored against e.g. "ProgressBar" can splice into a dup container
    // that names the same export "ProgressBar_dup".
    let rename_map: std::collections::HashMap<&str, &str> = args
        .renames
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let mut matched = 0usize;
    let mut unmatched_modded: Vec<String> = Vec::new();
    let mut unchanged = 0usize;
    let mut patches: Vec<ExportPatch> = Vec::new();
    for me in modded.exports.iter() {
        let resolved_path: String = rename_map
            .get(me.object_path.as_str())
            .map(|s| (*s).to_string())
            .unwrap_or_else(|| me.object_path.clone());
        let Some(ve) = vanilla.exports.iter().find(|v| v.object_path == resolved_path) else {
            unmatched_modded.push(resolved_path.clone());
            continue;
        };
        // --only-class filter: skip exports whose vanilla class isn't on the
        // allowlist. Used to skip x32→x64 incompatible payloads (Texture2D
        // properties differ between archs and would crash the engine).
        if !args.only_classes.is_empty() {
            let class_str = ve.class_name.as_deref().unwrap_or("");
            if !args.only_classes.iter().any(|c| class_str == c || class_str.ends_with(c)) {
                println!("  skip '{resolved_path}' class={class_str:?} (not in --only-class list)");
                continue;
            }
        }
        if ve.payload == me.payload {
            unchanged += 1;
            continue;
        }
        if ve.class_name != me.class_name {
            eprintln!(
                "WARN: '{}' class differs (vanilla={:?}, modded={:?}); skipping (Phase 1 applier rejects class changes)",
                me.object_path, ve.class_name, me.class_name
            );
            continue;
        }
        // For GFxMovieInfo with --gfx-swap: keep vanilla's UE3 wrapper
        // bytes (x64-formatted properties) and splice in only the
        // "GFX" Scaleform asset section from the modded payload. Also
        // patch the ArrayProperty Size + Count fields in the wrapper
        // to match the new byte count, otherwise the engine reads
        // past the buffer end and crashes.
        let payload_bytes = if args.gfx_swap
            && ve.class_name.as_deref() == Some("Core.GFxUI.GFxMovieInfo")
        {
            let v_off = find_gfx_offset(&ve.payload)
                .ok_or_else(|| format!("vanilla '{resolved_path}' has no GFX magic"))
                .unwrap_or_else(|e| { eprintln!("FAIL: {e}"); std::process::exit(8); });
            let f_off = find_gfx_offset(&me.payload)
                .ok_or_else(|| format!("modded '{}' has no GFX magic", me.object_path))
                .unwrap_or_else(|e| { eprintln!("FAIL: {e}"); std::process::exit(8); });
            // UE3 ArrayProperty<u8> wrapper (right before GFX bytes):
            //   ... property header ending with Size (4) + ArrayIndex (4)
            //   Count (4)
            //   Raw bytes
            // → Count is the u32 at v_off - 4
            // → Size (= Count + 4) is the u32 at v_off - 12 (Count + ArrayIndex + Size)
            let foglio_gfx_size = me.payload.len() - f_off;
            let new_count = foglio_gfx_size as u32;
            let new_size = (foglio_gfx_size + 4) as u32; // includes the 4-byte count

            let mut new_payload = Vec::with_capacity(v_off + foglio_gfx_size);
            new_payload.extend_from_slice(&ve.payload[..v_off]);
            new_payload.extend_from_slice(&me.payload[f_off..]);

            // Patch wrapper size fields. Locate by walking back from v_off:
            //   v_off - 4  → array Count (u32)
            //   v_off - 12 → property Size (u32)
            if v_off >= 12 {
                new_payload[v_off - 4..v_off].copy_from_slice(&new_count.to_le_bytes());
                new_payload[v_off - 12..v_off - 8].copy_from_slice(&new_size.to_le_bytes());
                let old_count = u32::from_le_bytes(ve.payload[v_off-4..v_off].try_into().unwrap());
                let old_size = u32::from_le_bytes(ve.payload[v_off-12..v_off-8].try_into().unwrap());
                println!(
                    "  gfx-swap '{}': v_off={} f_off={} v_old_count={} new_count={} v_old_size={} new_size={}",
                    resolved_path, v_off, f_off, old_count, new_count, old_size, new_size
                );
            } else {
                eprintln!("  WARN: v_off={} too small to patch wrapper size fields", v_off);
            }
            new_payload
        } else {
            me.payload.clone()
        };

        patches.push(ExportPatch {
            object_path: resolved_path.clone(),
            class_name: ve.class_name.clone(),
            reference_export_fingerprint: ve.payload_fingerprint.clone(),
            target_export_fingerprint: Some(ve.payload_fingerprint.clone()),
            operation: ExportPatchOperation::ReplaceExportPayload,
            new_class_name: None,
            replacement_payload_hex: hex_lower(&payload_bytes),
        });
        matched += 1;
    }

    println!("\nmatched-and-changed exports: {matched}");
    println!("matched-but-identical:        {unchanged}");
    println!("unmatched modded exports:     {} {:?}", unmatched_modded.len(), unmatched_modded);
    if matched == 0 {
        eprintln!("\nFATAL: no matching exports with payload differences — splice would be a no-op.");
        std::process::exit(2);
    }

    let manifest = PatchManifest {
        schema_version: 2,
        mod_id: args.mod_id.clone(),
        title: args.mod_id.clone(),
        target_package: format!("{}.gpk", vanilla.summary.package_name),
        patch_family: PatchFamily::UiLayout,
        reference: ReferenceBaseline {
            source_patch_label: "splice-x32-payloads".into(),
            package_fingerprint: format!(
                "exports:{}|imports:{}|names:{}",
                vanilla.exports.len(), vanilla.imports.len(), vanilla.names.len()
            ),
            provenance: None,
        },
        compatibility: CompatibilityPolicy {
            require_exact_package_fingerprint: false,
            require_all_exports_present: false,
            forbid_name_or_import_expansion: false,
        },
        exports: patches,
        import_patches: Vec::new(),
        name_patches: Vec::new(),
        notes: vec!["Generated by splice-x32-payloads".into()],
    };

    let uncompressed_vanilla = gpk_package::extract_uncompressed_package_bytes(&vanilla_bytes)
        .unwrap_or_else(|e| {
            eprintln!("decompress vanilla failed: {e}");
            std::process::exit(3);
        });
    println!("uncompressed vanilla: {} bytes", uncompressed_vanilla.len());

    let patched = gpk_patch_applier::apply_manifest(&uncompressed_vanilla, &manifest)
        .unwrap_or_else(|e| {
            eprintln!("apply_manifest failed: {e}");
            std::process::exit(4);
        });

    fs::write(&args.output, &patched).unwrap_or_else(|e| {
        eprintln!("write output failed: {e}");
        std::process::exit(5);
    });
    println!("\nWrote {} bytes to {}", patched.len(), args.output.display());

    println!("\nself-verify: re-parse output + diff against vanilla...");
    let verify = gpk_package::parse_package(&patched).unwrap_or_else(|e| {
        eprintln!("FAIL: re-parse spliced output: {e}");
        std::process::exit(6);
    });
    println!(
        "  spliced: names={} imports={} exports={} compression_flags={}",
        verify.names.len(), verify.imports.len(), verify.exports.len(),
        verify.summary.compression_flags
    );
    let diff = gpk_package::compare_packages(&vanilla, &verify);
    println!("  diff vs vanilla: changed={} added={:?} removed={:?}",
        diff.changed_exports.len(), diff.added_exports, diff.removed_exports);
    println!("  diff: name {}→{}, imports {}→{}, exports {}→{}",
        diff.name_count_before, diff.name_count_after,
        diff.import_count_before, diff.import_count_after,
        diff.export_count_before, diff.export_count_after);
    if diff.added_exports.is_empty()
        && diff.import_count_before == diff.import_count_after
        && diff.name_count_before == diff.name_count_after
    {
        println!("  ✓ derive_manifest will accept this output");
    } else {
        println!("  ✗ derive_manifest will REJECT this output");
        std::process::exit(7);
    }
}
