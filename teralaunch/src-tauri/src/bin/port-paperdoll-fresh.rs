// port-paperdoll-fresh — clean-slate authoring of an x64 TERA paperdoll mod GPK
// from the loose foglio mod.gfx + the v100 vanilla composite slice.
//
// Inputs:
//   --vanilla-x64 <path>   uncompressed standalone GPK extracted from the
//                          composite container (FileVersion 897, MOD: folder set).
//   --mod-swf <path>       the loose foglio mod.gfx file.
//   --target-export <name> the GFxMovieInfo export name to swap (e.g. "PaperDoll_dup"
//                          or "PaperDoll"). The script will list candidates and pick
//                          the matching one.
//   --output <path>        where to write the modded GPK.
//
// What it does:
//   1. Read vanilla bytes; parse_package().
//   2. Locate the GFxMovieInfo export by name (assert class).
//   3. Inside that export's payload, find the "GFX" magic and read
//      ArrayProperty count (gfx-4) + Size (gfx-12).
//   4. Splice: new_payload = prefix(<gfx) + mod_swf + suffix(>=gfx+old_count).
//      Patch count = mod_swf.len, Size = mod_swf.len+4.
//   5. Build a PatchManifest with one ReplaceExportPayload op carrying the new payload.
//   6. apply_manifest() → patched bytes. Output is uncompressed.
//   7. Self-verify: reparse output, confirm name/import/export counts unchanged,
//      target export's payload bytes contain mod_swf, FileVersion still 897,
//      MOD: folder preserved.
//   8. Write output. Print summary.
//
// What it does NOT do:
//   - Compression (output is uncompressed, matching what TMM/installer expects).
//   - Mapper editing (a separate install binary handles that).
//   - TMM v2 footer (not required for the launcher's own install path).

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

const USAGE: &str =
    "port-paperdoll-fresh --vanilla-x64 <path> --mod-swf <path> --target-export <name> --output <path>";

struct CliArgs {
    vanilla_x64: PathBuf,
    mod_swf: PathBuf,
    target_export: String,
    output: PathBuf,
}

fn parse_args() -> Result<CliArgs, String> {
    let mut vanilla_x64: Option<PathBuf> = None;
    let mut mod_swf: Option<PathBuf> = None;
    let mut target_export: Option<String> = None;
    let mut output: Option<PathBuf> = None;

    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--vanilla-x64" => vanilla_x64 = iter.next().map(PathBuf::from),
            "--mod-swf" => mod_swf = iter.next().map(PathBuf::from),
            "--target-export" => target_export = iter.next(),
            "--output" => output = iter.next().map(PathBuf::from),
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("Unknown arg '{other}'")),
        }
    }

    Ok(CliArgs {
        vanilla_x64: vanilla_x64.ok_or("--vanilla-x64 is required")?,
        mod_swf: mod_swf.ok_or("--mod-swf is required")?,
        target_export: target_export.ok_or("--target-export is required")?,
        output: output.ok_or("--output is required")?,
    })
}

fn find_gfx_offset(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|w| {
        w[0] == b'G' && w[1] == b'F' && w[2] == b'X' && (w[3] >= 0x07 && w[3] <= 0x0C)
    })
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| format!("u32 read at {offset} out of range"))?;
    Ok(u32::from_le_bytes(slice.try_into().unwrap()))
}

fn write_u32_le(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), String> {
    let slice = bytes
        .get_mut(offset..offset + 4)
        .ok_or_else(|| format!("u32 write at {offset} out of range"))?;
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

    println!("== port-paperdoll-fresh ==");
    println!("vanilla:    {}", args.vanilla_x64.display());
    println!("mod_swf:    {}", args.mod_swf.display());
    println!("target:     {}", args.target_export);
    println!("output:     {}", args.output.display());
    println!();

    let vanilla_bytes = fs::read(&args.vanilla_x64).unwrap_or_else(|e| {
        eprintln!("FAIL read vanilla: {e}");
        std::process::exit(2);
    });
    let mod_swf = fs::read(&args.mod_swf).unwrap_or_else(|e| {
        eprintln!("FAIL read mod_swf: {e}");
        std::process::exit(2);
    });

    println!("vanilla size: {} bytes", vanilla_bytes.len());
    println!("mod_swf size: {} bytes", mod_swf.len());

    if mod_swf.len() < 16 || &mod_swf[..3] != b"GFX" {
        eprintln!("FAIL: mod_swf does not start with 'GFX' magic — not a Scaleform GFx file");
        std::process::exit(3);
    }
    let swf_version = mod_swf[3];
    if !(0x07..=0x0C).contains(&swf_version) {
        eprintln!(
            "FAIL: mod_swf has unexpected GFx version byte 0x{swf_version:02X} (expected 0x07-0x0C)"
        );
        std::process::exit(3);
    }
    println!("mod_swf magic ok: GFX version 0x{swf_version:02X}");

    // Decompress vanilla if needed (paperdoll_dup x64 slice is uncompressed; this is
    // a no-op for that input). The applier requires uncompressed input.
    let uncompressed_vanilla = gpk_package::extract_uncompressed_package_bytes(&vanilla_bytes)
        .unwrap_or_else(|e| {
            eprintln!("FAIL decompress vanilla: {e}");
            std::process::exit(4);
        });
    println!("uncompressed vanilla: {} bytes", uncompressed_vanilla.len());

    let parsed = gpk_package::parse_package(&uncompressed_vanilla).unwrap_or_else(|e| {
        eprintln!("FAIL parse vanilla: {e}");
        std::process::exit(5);
    });
    println!(
        "parsed: file_version={} names={} imports={} exports={} folder={:?} compression={}",
        parsed.summary.file_version,
        parsed.names.len(),
        parsed.imports.len(),
        parsed.exports.len(),
        parsed.summary.package_name,
        parsed.summary.compression_flags
    );

    if parsed.summary.file_version != 897 {
        eprintln!(
            "FAIL: vanilla file_version {} != 897. This binary expects an x64 vanilla.",
            parsed.summary.file_version
        );
        std::process::exit(6);
    }

    // List GFxMovieInfo exports for diagnostic clarity.
    println!("\n--- GFxMovieInfo exports ---");
    let mut gfx_exports: Vec<&gpk_package::GpkExportEntry> = Vec::new();
    for e in parsed.exports.iter() {
        if e.class_name.as_deref() == Some("Core.GFxUI.GFxMovieInfo") {
            println!(
                "  '{}' payload={} bytes (offset={:?})",
                e.object_path,
                e.payload.len(),
                e.serial_offset
            );
            gfx_exports.push(e);
        }
    }
    if gfx_exports.is_empty() {
        eprintln!("FAIL: no GFxMovieInfo exports in vanilla");
        std::process::exit(7);
    }

    // Find the target export.
    let target = gfx_exports
        .iter()
        .find(|e| e.object_path == args.target_export)
        .or_else(|| {
            // Try suffix match (e.g. "PaperDoll" matches "S1UI_PaperDoll.PaperDoll").
            gfx_exports
                .iter()
                .find(|e| e.object_path.ends_with(&format!(".{}", args.target_export)))
        })
        .copied()
        .unwrap_or_else(|| {
            eprintln!(
                "FAIL: target export '{}' not in GFxMovieInfo list. Candidates: {:?}",
                args.target_export,
                gfx_exports.iter().map(|e| &e.object_path).collect::<Vec<_>>()
            );
            std::process::exit(8);
        });

    println!(
        "\nselected target: '{}' payload={} bytes",
        target.object_path,
        target.payload.len()
    );

    // Find GFX magic inside the target's payload.
    let gfx_off = find_gfx_offset(&target.payload).unwrap_or_else(|| {
        eprintln!("FAIL: target export payload contains no 'GFX' magic");
        std::process::exit(9);
    });
    if gfx_off < 12 {
        eprintln!(
            "FAIL: GFX magic at offset {gfx_off} in payload, but ArrayProperty header needs at least 12 bytes before it"
        );
        std::process::exit(10);
    }
    let old_count = read_u32_le(&target.payload, gfx_off - 4).unwrap_or_else(|e| {
        eprintln!("FAIL: read old count: {e}");
        std::process::exit(11);
    });
    let old_size = read_u32_le(&target.payload, gfx_off - 12).unwrap_or_else(|e| {
        eprintln!("FAIL: read old size: {e}");
        std::process::exit(11);
    });
    let old_array_index = read_u32_le(&target.payload, gfx_off - 8).unwrap_or(0);

    println!(
        "  GFX magic at payload offset {gfx_off} (count={old_count} size={old_size} array_index={old_array_index})"
    );

    let old_end = gfx_off
        .checked_add(old_count as usize)
        .ok_or("count overflow")
        .unwrap_or_else(|e| {
            eprintln!("FAIL: {e}");
            std::process::exit(12);
        });
    if old_end > target.payload.len() {
        eprintln!(
            "FAIL: vanilla GFX section [gfx_off={gfx_off}, end={old_end}] exceeds payload size {}",
            target.payload.len()
        );
        std::process::exit(13);
    }

    // Synthesize the new payload.
    let new_count = mod_swf.len() as u32;
    let new_size = (mod_swf.len() as u32)
        .checked_add(4)
        .ok_or("size overflow")
        .unwrap_or_else(|e| {
            eprintln!("FAIL: {e}");
            std::process::exit(14);
        });

    let mut new_payload = Vec::with_capacity(gfx_off + mod_swf.len() + (target.payload.len() - old_end));
    new_payload.extend_from_slice(&target.payload[..gfx_off]);
    new_payload.extend_from_slice(&mod_swf);
    new_payload.extend_from_slice(&target.payload[old_end..]);
    write_u32_le(&mut new_payload, gfx_off - 4, new_count).unwrap();
    write_u32_le(&mut new_payload, gfx_off - 12, new_size).unwrap();

    println!(
        "  new payload: {} bytes (delta {:+} vs {})",
        new_payload.len(),
        new_payload.len() as isize - target.payload.len() as isize,
        target.payload.len()
    );

    // Verify the splice round-trips: re-find GFX in new payload, lengths consistent.
    let verify_gfx_off = find_gfx_offset(&new_payload).unwrap_or_else(|| {
        eprintln!("FAIL: post-splice payload contains no 'GFX' magic");
        std::process::exit(15);
    });
    if verify_gfx_off != gfx_off {
        eprintln!(
            "FAIL: post-splice GFX at {verify_gfx_off}, expected {gfx_off}"
        );
        std::process::exit(15);
    }
    let verify_count = read_u32_le(&new_payload, gfx_off - 4).unwrap();
    let verify_size = read_u32_le(&new_payload, gfx_off - 12).unwrap();
    if verify_count != new_count || verify_size != new_size {
        eprintln!(
            "FAIL: count/size mismatch after splice (want count={new_count} size={new_size}, got count={verify_count} size={verify_size})"
        );
        std::process::exit(15);
    }
    println!("  post-splice verification ok (count={verify_count} size={verify_size})");

    // Build a manifest with one ReplaceExportPayload op.
    let manifest = PatchManifest {
        schema_version: 2,
        mod_id: "foglio1024.restyle-paperdoll.fresh".to_string(),
        title: "Foglio Restyle PaperDoll (x64 fresh port)".to_string(),
        target_package: format!("{}.gpk", parsed.summary.package_name),
        patch_family: PatchFamily::UiLayout,
        reference: ReferenceBaseline {
            source_patch_label: "v100.02 vanilla composite slice".into(),
            package_fingerprint: format!(
                "exports:{}|imports:{}|names:{}",
                parsed.exports.len(),
                parsed.imports.len(),
                parsed.names.len()
            ),
            provenance: None,
        },
        compatibility: CompatibilityPolicy {
            require_exact_package_fingerprint: false,
            require_all_exports_present: false,
            forbid_name_or_import_expansion: false,
        },
        exports: vec![ExportPatch {
            object_path: target.object_path.clone(),
            class_name: target.class_name.clone(),
            reference_export_fingerprint: target.payload_fingerprint.clone(),
            target_export_fingerprint: Some(target.payload_fingerprint.clone()),
            operation: ExportPatchOperation::ReplaceExportPayload,
            new_class_name: None,
            replacement_payload_hex: hex_lower(&new_payload),
        }],
        import_patches: Vec::new(),
        name_patches: Vec::new(),
        notes: vec!["Authored by port-paperdoll-fresh; loose mod.gfx splice".into()],
    };

    let patched = gpk_patch_applier::apply_manifest(&uncompressed_vanilla, &manifest)
        .unwrap_or_else(|e| {
            eprintln!("FAIL apply_manifest: {e}");
            std::process::exit(16);
        });

    println!(
        "\napply_manifest produced {} bytes (delta {:+} vs vanilla {})",
        patched.len(),
        patched.len() as isize - uncompressed_vanilla.len() as isize,
        uncompressed_vanilla.len()
    );

    // Self-verify the output.
    let verify = gpk_package::parse_package(&patched).unwrap_or_else(|e| {
        eprintln!("FAIL re-parse output: {e}");
        std::process::exit(17);
    });

    if verify.summary.file_version != 897 {
        eprintln!(
            "FAIL: output file_version {} != 897",
            verify.summary.file_version
        );
        std::process::exit(18);
    }
    if verify.summary.package_name != parsed.summary.package_name {
        eprintln!(
            "FAIL: folder name changed ({} -> {})",
            parsed.summary.package_name, verify.summary.package_name
        );
        std::process::exit(18);
    }
    if verify.names.len() != parsed.names.len()
        || verify.imports.len() != parsed.imports.len()
        || verify.exports.len() != parsed.exports.len()
    {
        eprintln!(
            "FAIL: count drift names {}->{}, imports {}->{}, exports {}->{}",
            parsed.names.len(),
            verify.names.len(),
            parsed.imports.len(),
            verify.imports.len(),
            parsed.exports.len(),
            verify.exports.len()
        );
        std::process::exit(18);
    }

    let new_target = verify
        .exports
        .iter()
        .find(|e| e.object_path == target.object_path)
        .unwrap_or_else(|| {
            eprintln!("FAIL: target export disappeared from output");
            std::process::exit(18);
        });
    if new_target.payload != new_payload {
        eprintln!(
            "FAIL: post-write target payload differs from in-memory new_payload (lens {} vs {})",
            new_target.payload.len(),
            new_payload.len()
        );
        std::process::exit(18);
    }
    let result_gfx_off = find_gfx_offset(&new_target.payload).unwrap_or_else(|| {
        eprintln!("FAIL: output target payload has no GFX magic");
        std::process::exit(18);
    });
    let result_count = read_u32_le(&new_target.payload, result_gfx_off - 4).unwrap();
    let result_swf_slice =
        &new_target.payload[result_gfx_off..result_gfx_off + result_count as usize];
    if result_swf_slice != mod_swf.as_slice() {
        eprintln!("FAIL: output's SWF section does not match mod_swf bytes");
        std::process::exit(18);
    }

    // Bounds: every export's serial_offset+serial_size must fit in the file.
    for e in verify.exports.iter() {
        let off = match e.serial_offset {
            Some(o) => o as u64,
            None => continue, // exports with size 0 may have no offset
        };
        let end = off.saturating_add(e.serial_size as u64);
        if end > patched.len() as u64 {
            eprintln!(
                "FAIL: export '{}' overruns file (offset={} size={} end={} file_len={})",
                e.object_path,
                off,
                e.serial_size,
                end,
                patched.len()
            );
            std::process::exit(18);
        }
    }
    println!("self-verify: PASS");

    fs::write(&args.output, &patched).unwrap_or_else(|e| {
        eprintln!("FAIL write output: {e}");
        std::process::exit(19);
    });
    println!(
        "\nwrote {} bytes to {}",
        patched.len(),
        args.output.display()
    );
    println!("DONE");
}
