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

#[path = "../services/mods/patch_manifest.rs"]
mod patch_manifest;

#[path = "../services/mods/gpk_package.rs"]
mod gpk_package;

#[path = "../services/mods/gpk_patch_applier.rs"]
mod gpk_patch_applier;

#[cfg(test)]
#[path = "../services/mods/test_fixtures.rs"]
mod test_fixtures;

use patch_manifest::{
    CompatibilityPolicy, ExportPatch, ExportPatchOperation, PatchFamily, PatchManifest,
    ReferenceBaseline,
};

const USAGE: &str = "splice-x32-payloads --vanilla-x64 <path> --modded-x32 <path> --output <path> [--mod-id <id>] [--rename A=B ...] [--only-class <ClassName> ...] [--gfx-swap]";

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
                let (a, b) = v
                    .split_once('=')
                    .ok_or_else(|| format!("--rename '{v}' must be A=B"))?;
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
    // Range 0x07–0x10 covers the AS2 versions present across foglio's source
    // and v100 vanilla wrappers (observed: 0x09 in most mods, 0x0B in
    // targetinfo, 0x0F in newer Scaleform builds — bosswindow + equipment-
    // upgrade widget).
    bytes.windows(4).position(|w| {
        w[0] == b'G' && w[1] == b'F' && w[2] == b'X' && (w[3] >= 0x07 && w[3] <= 0x10)
    })
}

fn build_gfx_swap_payload(
    vanilla_payload: &[u8],
    modded_payload: &[u8],
) -> Result<Vec<u8>, String> {
    let v_off = find_gfx_offset(vanilla_payload).ok_or("vanilla payload has no GFX magic")?;
    let m_off = find_gfx_offset(modded_payload).ok_or("modded payload has no GFX magic")?;
    let vanilla_count = read_u32_before(vanilla_payload, v_off, 4)? as usize;
    let modded_count = read_u32_before(modded_payload, m_off, 4)? as usize;
    let vanilla_end = v_off
        .checked_add(vanilla_count)
        .ok_or("vanilla GFX byte count overflows")?;
    let modded_end = m_off
        .checked_add(modded_count)
        .ok_or("modded GFX byte count overflows")?;
    if vanilla_end > vanilla_payload.len() {
        return Err("vanilla GFX byte count extends past payload".into());
    }
    if modded_end > modded_payload.len() {
        return Err("modded GFX byte count extends past payload".into());
    }

    let new_count = modded_count as u32;
    let new_size = (modded_count + 4) as u32;
    let mut new_payload =
        Vec::with_capacity(v_off + modded_count + vanilla_payload.len() - vanilla_end);
    new_payload.extend_from_slice(&vanilla_payload[..v_off]);
    new_payload.extend_from_slice(&modded_payload[m_off..modded_end]);
    new_payload.extend_from_slice(&vanilla_payload[vanilla_end..]);
    patch_u32_before(&mut new_payload, v_off, 4, new_count)?;
    patch_u32_before(&mut new_payload, v_off, 12, new_size)?;
    Ok(new_payload)
}

fn read_u32_before(bytes: &[u8], offset: usize, distance: usize) -> Result<u32, String> {
    let start = offset
        .checked_sub(distance)
        .ok_or_else(|| format!("offset {offset} is too small for -{distance}"))?;
    let end = start
        .checked_add(4)
        .ok_or_else(|| "u32 read offset overflows".to_string())?;
    let slice = bytes
        .get(start..end)
        .ok_or_else(|| format!("u32 read {start}..{end} is outside payload"))?;
    Ok(u32::from_le_bytes(
        slice.try_into().map_err(|_| "slice size mismatch")?,
    ))
}

fn patch_u32_before(
    bytes: &mut [u8],
    offset: usize,
    distance: usize,
    value: u32,
) -> Result<(), String> {
    let start = offset
        .checked_sub(distance)
        .ok_or_else(|| format!("offset {offset} is too small for -{distance}"))?;
    let end = start
        .checked_add(4)
        .ok_or_else(|| "u32 patch offset overflows".to_string())?;
    let slice = bytes
        .get_mut(start..end)
        .ok_or_else(|| format!("u32 patch {start}..{end} is outside payload"))?;
    slice.copy_from_slice(&value.to_le_bytes());
    Ok(())
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

    let vanilla_bytes = fs::read(&args.vanilla_x64).unwrap_or_else(|e| {
        eprintln!("read vanilla failed: {e}");
        std::process::exit(1);
    });
    let modded_bytes = fs::read(&args.modded_x32).unwrap_or_else(|e| {
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

    println!(
        "vanilla: names={} imports={} exports={}",
        vanilla.names.len(),
        vanilla.imports.len(),
        vanilla.exports.len()
    );
    println!(
        "modded:  names={} imports={} exports={}",
        modded.names.len(),
        modded.imports.len(),
        modded.exports.len()
    );

    println!("\n--- vanilla exports ---");
    for (i, e) in vanilla.exports.iter().enumerate() {
        println!(
            "  [{i:2}] {} class={:?} payload={} bytes",
            e.object_path,
            e.class_name,
            e.payload.len()
        );
    }
    println!("\n--- modded exports ---");
    for (i, e) in modded.exports.iter().enumerate() {
        println!(
            "  [{i:2}] {} class={:?} payload={} bytes",
            e.object_path,
            e.class_name,
            e.payload.len()
        );
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
        let Some(ve) = vanilla
            .exports
            .iter()
            .find(|v| v.object_path == resolved_path)
        else {
            unmatched_modded.push(resolved_path.clone());
            continue;
        };
        // --only-class filter: skip exports whose vanilla class isn't on the
        // allowlist. Used to skip x32→x64 incompatible payloads (Texture2D
        // properties differ between archs and would crash the engine).
        if !args.only_classes.is_empty() {
            let class_str = ve.class_name.as_deref().unwrap_or("");
            if !args
                .only_classes
                .iter()
                .any(|c| class_str == c || class_str.ends_with(c))
            {
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
                .unwrap_or_else(|e| {
                    eprintln!("FAIL: {e}");
                    std::process::exit(8);
                });
            let f_off = find_gfx_offset(&me.payload)
                .ok_or_else(|| format!("modded '{}' has no GFX magic", me.object_path))
                .unwrap_or_else(|e| {
                    eprintln!("FAIL: {e}");
                    std::process::exit(8);
                });
            let old_count = read_u32_before(&ve.payload, v_off, 4).unwrap_or(0);
            let old_size = read_u32_before(&ve.payload, v_off, 12).unwrap_or(0);
            let new_count = read_u32_before(&me.payload, f_off, 4).unwrap_or(0);
            let new_size = new_count.saturating_add(4);
            println!(
                "  gfx-swap '{}': v_off={} f_off={} v_old_count={} new_count={} v_old_size={} new_size={}",
                resolved_path, v_off, f_off, old_count, new_count, old_size, new_size
            );
            build_gfx_swap_payload(&ve.payload, &me.payload).unwrap_or_else(|e| {
                eprintln!("FAIL: {e}");
                std::process::exit(8);
            })
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
    println!(
        "unmatched modded exports:     {} {:?}",
        unmatched_modded.len(),
        unmatched_modded
    );
    if matched == 0 {
        eprintln!(
            "\nFATAL: no matching exports with payload differences — splice would be a no-op."
        );
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
                vanilla.exports.len(),
                vanilla.imports.len(),
                vanilla.names.len()
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
    println!(
        "\nWrote {} bytes to {}",
        patched.len(),
        args.output.display()
    );

    println!("\nself-verify: re-parse output + diff against vanilla...");
    let verify = gpk_package::parse_package(&patched).unwrap_or_else(|e| {
        eprintln!("FAIL: re-parse spliced output: {e}");
        std::process::exit(6);
    });
    println!(
        "  spliced: names={} imports={} exports={} compression_flags={}",
        verify.names.len(),
        verify.imports.len(),
        verify.exports.len(),
        verify.summary.compression_flags
    );
    let diff = gpk_package::compare_packages(&vanilla, &verify);
    println!(
        "  diff vs vanilla: changed={} added={:?} removed={:?}",
        diff.changed_exports.len(),
        diff.added_exports,
        diff.removed_exports
    );
    println!(
        "  diff: name {}→{}, imports {}→{}, exports {}→{}",
        diff.name_count_before,
        diff.name_count_after,
        diff.import_count_before,
        diff.import_count_after,
        diff.export_count_before,
        diff.export_count_after
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gfx_swap_preserves_vanilla_payload_after_embedded_movie() {
        let mut vanilla = Vec::new();
        vanilla.extend_from_slice(b"pref");
        vanilla.extend_from_slice(&(11u32).to_le_bytes());
        vanilla.extend_from_slice(&(0u32).to_le_bytes());
        vanilla.extend_from_slice(&(7u32).to_le_bytes());
        vanilla.extend_from_slice(b"GFX\x09abc");
        vanilla.extend_from_slice(b"TAIL");

        let mut modded = Vec::new();
        modded.extend_from_slice(b"pref");
        modded.extend_from_slice(&(9u32).to_le_bytes());
        modded.extend_from_slice(&(0u32).to_le_bytes());
        modded.extend_from_slice(&(9u32).to_le_bytes());
        modded.extend_from_slice(b"GFX\x09abcde");
        modded.extend_from_slice(b"MODTRAIL");

        let swapped = build_gfx_swap_payload(&vanilla, &modded).unwrap();
        let gfx_offset = find_gfx_offset(&swapped).unwrap();

        assert_eq!(read_u32_before(&swapped, gfx_offset, 4).unwrap(), 9);
        assert_eq!(read_u32_before(&swapped, gfx_offset, 12).unwrap(), 13);
        assert_eq!(&swapped[gfx_offset..gfx_offset + 9], b"GFX\x09abcde");
        assert_eq!(&swapped[gfx_offset + 9..], b"TAIL");
    }
}
