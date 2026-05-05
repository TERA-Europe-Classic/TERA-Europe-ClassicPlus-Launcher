//! Shift-aware variant of splice-x32-payloads --gfx-swap, for vanilla
//! packages whose layout has tables-in-the-middle (some bodies after the
//! export table). The standard `gpk_patch_applier::apply_manifest` rejects
//! these as "Package layout is not suitable" because both its
//! tables-before-bodies and bodies-before-tables paths assume a contiguous
//! body region.
//!
//! Strategy: take the uncompressed vanilla bytes, locate the target export's
//! body, build the new payload via build_gfx_swap_payload (same as the
//! original splice tool), splice it in, then shift every byte that lives
//! at file_offset > (target_serial_offset + target_serial_size) by
//! `delta = new_payload.len() - old_serial_size`. After the shift, fix up
//! every header offset and export-table SerialOffset that points past the
//! target body.
//!
//! Usage:
//!   splice-shift-aware --vanilla-x64 <path> --modded-x32 <path> --output <path>
//!     --target-export <object_path> [--rename A=B ...]

#[path = "../services/mods/gpk_package.rs"] mod gpk_package;

use std::env;
use std::fs;
use std::path::PathBuf;

// Documented header magic from the GPK format spec; kept as a named
// constant so future work that needs to validate the input package can
// reference it directly rather than re-deriving the value.
#[allow(dead_code)]
const PACKAGE_MAGIC: u32 = 0x9E2A_83C1;

fn main() {
    if let Err(e) = run() {
        eprintln!("FAIL: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut vanilla: Option<PathBuf> = None;
    let mut modded: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut target_export: Option<String> = None;
    let mut renames: Vec<(String, String)> = Vec::new();
    let mut iter = env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--vanilla-x64" => vanilla = iter.next().map(PathBuf::from),
            "--modded-x32" => modded = iter.next().map(PathBuf::from),
            "--output" => output = iter.next().map(PathBuf::from),
            "--target-export" => target_export = iter.next(),
            "--rename" => {
                let v = iter.next().ok_or("--rename needs A=B")?;
                let (a, b) = v.split_once('=').ok_or("--rename A=B format")?;
                renames.push((a.to_string(), b.to_string()));
            }
            other => return Err(format!("unknown arg '{other}'")),
        }
    }
    let vanilla_path = vanilla.ok_or("--vanilla-x64 required")?;
    let modded_path = modded.ok_or("--modded-x32 required")?;
    let output_path = output.ok_or("--output required")?;
    let target_export = target_export.ok_or("--target-export required (full export object_path in vanilla)")?;

    let vanilla_raw = fs::read(&vanilla_path).map_err(|e| format!("read vanilla: {e}"))?;
    let modded_raw = fs::read(&modded_path).map_err(|e| format!("read modded: {e}"))?;
    let vanilla_bytes = gpk_package::extract_uncompressed_package_bytes(&vanilla_raw)
        .map_err(|e| format!("decompress vanilla: {e}"))?;
    let modded_bytes = gpk_package::extract_uncompressed_package_bytes(&modded_raw)
        .map_err(|e| format!("decompress modded: {e}"))?;

    let vanilla_pkg = gpk_package::parse_package(&vanilla_bytes).map_err(|e| format!("parse vanilla: {e}"))?;
    let modded_pkg = gpk_package::parse_package(&modded_bytes).map_err(|e| format!("parse modded: {e}"))?;

    // Find target export in vanilla
    let target = vanilla_pkg.exports.iter().find(|e| e.object_path == target_export)
        .ok_or_else(|| format!("target export '{target_export}' not in vanilla. Available: {}",
            vanilla_pkg.exports.iter().map(|e| e.object_path.as_str()).collect::<Vec<_>>().join(", ")))?;
    let target_serial_offset = target.serial_offset.ok_or("target has no serial_offset")? as usize;
    let target_serial_size = target.serial_size as usize;

    // Apply renames to find matching modded export
    let rename_map: std::collections::HashMap<&str, &str> = renames.iter()
        .map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let modded_export = modded_pkg.exports.iter().find(|e| {
        let resolved = rename_map.get(e.object_path.as_str()).copied().unwrap_or(e.object_path.as_str());
        resolved == target_export
    }).ok_or_else(|| format!("no modded export matches target '{target_export}' (after renames)"))?;

    // Build new payload via gfx-swap logic
    let new_payload = build_gfx_swap_payload(&target.payload, &modded_export.payload)?;
    println!("vanilla payload size: {}", target_serial_size);
    println!("new payload size: {}", new_payload.len());
    let delta: i64 = new_payload.len() as i64 - target_serial_size as i64;
    println!("delta: {delta}");

    // In-place splice + shift
    let body_end = target_serial_offset + target_serial_size;
    let mut out = Vec::with_capacity((vanilla_bytes.len() as i64 + delta).max(0) as usize);
    out.extend_from_slice(&vanilla_bytes[..target_serial_offset]);
    out.extend_from_slice(&new_payload);
    out.extend_from_slice(&vanilla_bytes[body_end..]);

    if delta != 0 {
        // Patch every export's SerialOffset > target's serial_offset (after the body that grew)
        // by adding delta. Walk the export table and rewrite the SerialOffset field of each
        // export whose serial_offset is > target_serial_offset.
        patch_export_table_offsets(&mut out, &vanilla_pkg, target_serial_offset, delta)?;

        // Patch header offsets: import_offset, export_offset, depends_offset, name_offset
        // (only if they come AFTER target body end). Also any chunk-header / x64 field
        // pointers that are post-body.
        patch_header_offsets(&mut out, &vanilla_bytes, body_end, delta)?;

        // Update HeaderSize if it changed (it shouldn't, since header is before bodies).
        // No-op here for clarity.
    }

    fs::write(&output_path, &out).map_err(|e| format!("write output: {e}"))?;
    println!("wrote {} bytes to {}", out.len(), output_path.display());

    // Self-verify
    let verify = gpk_package::parse_package(&out).map_err(|e| format!("re-parse output: {e}"))?;
    println!("self-verify ok: names={} imports={} exports={}",
        verify.names.len(), verify.imports.len(), verify.exports.len());
    Ok(())
}

fn find_gfx_offset(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|w| {
        w[0] == b'G' && w[1] == b'F' && w[2] == b'X' && (0x07..=0x10).contains(&w[3])
    })
}

fn read_u32_le_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}

fn build_gfx_swap_payload(vanilla: &[u8], modded: &[u8]) -> Result<Vec<u8>, String> {
    let v_off = find_gfx_offset(vanilla).ok_or("vanilla payload has no GFX magic")?;
    let m_off = find_gfx_offset(modded).ok_or("modded payload has no GFX magic")?;
    let vanilla_count = read_u32_le_at(vanilla, v_off - 4) as usize;
    let modded_count = read_u32_le_at(modded, m_off - 4) as usize;
    let vanilla_end = v_off + vanilla_count;
    let modded_end = m_off + modded_count;
    if vanilla_end > vanilla.len() || modded_end > modded.len() {
        return Err("GFX byte count overflows payload".into());
    }
    let mut out = Vec::with_capacity(v_off + modded_count + (vanilla.len() - vanilla_end));
    out.extend_from_slice(&vanilla[..v_off]);
    out.extend_from_slice(&modded[m_off..modded_end]);
    out.extend_from_slice(&vanilla[vanilla_end..]);
    let new_count = modded_count as u32;
    let new_size = (modded_count + 4) as u32;
    out[v_off - 4..v_off].copy_from_slice(&new_count.to_le_bytes());
    out[v_off - 12..v_off - 8].copy_from_slice(&new_size.to_le_bytes());
    Ok(out)
}

fn patch_export_table_offsets(
    out: &mut [u8],
    pkg: &gpk_package::GpkPackage,
    target_serial_offset: usize,
    delta: i64,
) -> Result<(), String> {
    let export_offset = pkg.summary.export_offset as usize;
    // After header offsets shift, the export table itself MAY have moved.
    // For an in-place shift we need to compute where the export table NOW
    // sits and where each export entry's SerialOffset field is. Tables
    // shift by `delta` if they were after the body. Compute the new export
    // table base.
    let new_export_table_base = if export_offset > target_serial_offset {
        (export_offset as i64 + delta) as usize
    } else {
        export_offset
    };

    // Each x64 export entry in the file is variable size — base 64/68 bytes
    // + UnkExtraInts (UnkHeaderCount * 4). We have to walk them in the
    // SAME order parse_package walks them and rewrite serial_offset values.
    // Simpler: rewrite by re-parsing exports' on-disk layout from `out`.
    //
    // For Phase 1 of this tool we assume the target body is in the
    // before-tables region and the export table moved. Walk via the
    // already-parsed pkg.exports[], but use the post-shift positions.

    // Walk the original export table bytes in order and locate each
    // export entry's start, then their SerialOffset field at offset+44 (x64).
    // x64 export base layout:
    //   ClassIndex(4) SuperIndex(4) PackageIndex(4) ObjectNameIndex(4)
    //   Unk1(8) Unk2(8) SerialSize(4) SerialOffset(4)        ← bytes 36..40
    //   Unk3(4) UnkHeaderCount(4) Unk4(4) Guid(16) UnkExtraInts(UnkHeaderCount*4)
    // ObjectNameIndex per parser is i32 (4 bytes) in x64 too — total 64 + 4 (serial_offset
    // present when serial_size > 0) + UnkExtraInts.

    // We need to find each export's location in the OUTPUT bytes. Step through.
    let mut cursor = new_export_table_base;
    for export in &pkg.exports {
        let entry_start = cursor;
        // ClassIndex .. ObjectNameIndex = 16 bytes
        cursor += 16;
        // Unk1 + Unk2 = 16 bytes
        cursor += 16;
        // SerialSize (4)
        cursor += 4;
        // SerialOffset (4) — present when SerialSize > 0
        if export.serial_size > 0 {
            // Patch it if the original serial_offset was past our target body
            let orig_offset = export.serial_offset.unwrap_or(0);
            if (orig_offset as usize) > target_serial_offset {
                let new_off = (orig_offset as i64 + delta) as u32;
                out[cursor..cursor + 4].copy_from_slice(&new_off.to_le_bytes());
            }
            cursor += 4;
        }
        // Unk3 (4) + UnkHeaderCount (4) + Unk4 (4) = 12 bytes
        let unk_header_count = u32::from_le_bytes(out[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        cursor += 12;
        // Guid (16)
        cursor += 16;
        // UnkExtraInts (UnkHeaderCount * 4)
        cursor += unk_header_count * 4;
        let _ = entry_start;
    }
    Ok(())
}

fn patch_header_offsets(
    out: &mut [u8],
    vanilla_bytes: &[u8],
    body_end: usize,
    delta: i64,
) -> Result<(), String> {
    // Header layout (x64) — re-read from original vanilla_bytes positions, patch the
    // i32 values in `out` in place. Each position holds an absolute file offset which
    // shifts by delta if it was > body_end.
    //
    // Position of each offset field (matching parse_package):
    //   - magic(4) + file_version(4) + header_size(4) = 12
    //   - folder_name FString = variable
    //   - package_flags(4) = +4
    //   - name_count(4) + name_offset(4) = +8
    //   - export_count(4) + export_offset(4) = +8
    //   - import_count(4) + import_offset(4) = +8
    //   - depends_offset(4) = +4
    //   - x64 only: ImportExportGuidsOffset(4) + ImportGuidsCount(4) +
    //     ExportGuidsCount(4) + ThumbnailTableOffset(4) = +16
    let folder_len_field = u32::from_le_bytes(vanilla_bytes[12..16].try_into().unwrap()) as i32;
    let folder_bytes = if folder_len_field > 0 {
        4 + folder_len_field as usize
    } else if folder_len_field < 0 {
        4 + ((-folder_len_field as usize) * 2)
    } else {
        4
    };
    let mut p = 12 + folder_bytes; // package_flags pos
    p += 4; // skip flags
    p += 4; // skip name_count
    let name_off_pos = p; p += 4;
    p += 4; // skip export_count
    let export_off_pos = p; p += 4;
    p += 4; // skip import_count
    let import_off_pos = p; p += 4;
    let depends_off_pos = p; p += 4;
    // x64 fields
    let ie_guids_off_pos = p; p += 4;
    let _import_guids_count_pos = p; p += 4;
    let _export_guids_count_pos = p; p += 4;
    // The trailing `p += 4` keeps the cursor-advance pattern uniform
    // across every offset slot. Removing it would diverge from the
    // header layout documented above; the linter flags the final
    // increment as never read.
    #[allow(unused_assignments)]
    let thumb_off_pos = { let pos = p; p += 4; pos };

    for off_pos in [name_off_pos, export_off_pos, import_off_pos, depends_off_pos,
                    ie_guids_off_pos, thumb_off_pos] {
        let v = u32::from_le_bytes(vanilla_bytes[off_pos..off_pos+4].try_into().unwrap()) as usize;
        if v > body_end && v != 0 {
            let new_v = (v as i64 + delta) as u32;
            out[off_pos..off_pos+4].copy_from_slice(&new_v.to_le_bytes());
        }
    }
    Ok(())
}
