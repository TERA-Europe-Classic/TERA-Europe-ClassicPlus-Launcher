// Tasks 8-9 will wire runtime consumers (drop-in install + Tauri command
// routing). Until then, suppress dead-code warnings on the public API.
#![allow(dead_code)]

//! Transform a Classic (x32, FileVersion 610) GPK into a Modern (x64,
//! FileVersion 897) GPK by re-encoding the header and every export's
//! property block. Output can be uncompressed (compression_flags = 0) or
//! LZO-compressed (compression_flags = 2, chunked body).

use super::gpk_package::{
    parse_package, serialize_summary, serialize_summary_with_chunks, ChunkHeader,
    GpkExportEntry, GpkImportEntry, GpkNameEntry,
};
use super::gpk_package::ArchKind as PkgArch;
use super::gpk_property::{parse_properties_with_consumed, write_properties, ArchKind};

const LZO_BLOCK_SIZE: usize = 131_072; // 128 KiB per block
const LZO_CHUNK_CAP: usize = LZO_BLOCK_SIZE * 256; // 32 MiB per chunk (256 blocks max)

/// Output compression mode for `transform_x32_to_x64_with`.
pub enum CompressionMode {
    /// No compression (compression_flags = 0). Equivalent to calling
    /// `transform_x32_to_x64` directly.
    None,
    /// LZO chunked compression (compression_flags = 2). Body is split into
    /// 32 MiB chunks of 128 KiB blocks, each block LZO-compressed.
    Lzo,
}

/// Transform an x32 (FileVersion 610) GPK into an x64 (FileVersion 897) GPK
/// with selectable output compression.
///
/// `None` is identical to calling `transform_x32_to_x64`.
/// `Lzo` chunks the body with 32 MiB chunks / 128 KiB blocks and writes
/// compression_flags=2.
pub fn transform_x32_to_x64_with(
    x32_bytes: &[u8],
    mode: CompressionMode,
) -> Result<Vec<u8>, String> {
    let uncompressed = transform_x32_to_x64_inner(x32_bytes)?;
    match mode {
        CompressionMode::None => Ok(uncompressed),
        CompressionMode::Lzo => compress_lzo(uncompressed),
    }
}

/// Transform an x32 (FileVersion 610) GPK into an x64 (FileVersion 897) GPK.
///
/// Re-encodes every export's property block from x32 to x64 format (BoolProperty
/// shrinks from 4 bytes to 1; ByteProperty gains an 8-byte enumType prefix).
/// Trailing payload bytes (mip tables, sound data) are preserved verbatim.
/// Output is uncompressed regardless of input compression.
pub fn transform_x32_to_x64(x32_bytes: &[u8]) -> Result<Vec<u8>, String> {
    transform_x32_to_x64_with(x32_bytes, CompressionMode::None)
}

fn transform_x32_to_x64_inner(x32_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let pkg = parse_package(x32_bytes)?;
    if pkg.summary.file_version >= 0x381 {
        return Err("input is already x64".to_string());
    }

    // Step 1: re-encode each export's property block.
    let mut new_payloads: Vec<Vec<u8>> = Vec::with_capacity(pkg.exports.len());
    for export in &pkg.exports {
        new_payloads.push(reencode_export_payload(export, &pkg.names)?);
    }

    // Step 2: lay out the new x64 file.
    let mut summary = pkg.summary.clone();
    summary.compression_flags = 0;
    summary.file_version = 897;
    // Safe defaults for x64 GUID-table fields when converting from x32.
    summary.import_export_guids_offset = Some(summary.depends_offset);
    summary.import_guids_count = Some(0);
    summary.export_guids_count = Some(0);
    summary.thumbnail_table_offset = Some(0);

    // Probe header size — varies because PackageName is variable-length.
    let mut header_probe = Vec::new();
    serialize_summary(&summary, PkgArch::X64, &mut header_probe)?;
    let header_size = header_probe.len() as u32;

    let name_table = serialize_name_table(&pkg.names);
    let import_table = serialize_import_table(&pkg.imports, &pkg.names)?;

    let depends_table_size = (pkg.exports.len() as u32) * 4;

    // Compute layout offsets.
    summary.name_offset = header_size;
    summary.name_count = pkg.names.len() as u32;
    let import_offset = summary.name_offset + name_table.len() as u32;
    summary.import_offset = import_offset;
    let export_offset = import_offset + import_table.len() as u32;
    summary.export_offset = export_offset;

    // Probe export table to learn its length (serial_offset values are wrong
    // in the probe but the per-entry sizes are correct).
    let probe_export_table =
        serialize_export_table(&pkg.exports, &new_payloads, &pkg.names, 0)?;
    let depends_offset = export_offset + probe_export_table.len() as u32;
    summary.depends_offset = depends_offset;
    summary.import_export_guids_offset = Some(depends_offset);
    let body_base = depends_offset + depends_table_size;

    // Final export table with correct serial_offsets.
    let final_export_table =
        serialize_export_table(&pkg.exports, &new_payloads, &pkg.names, body_base)?;
    if final_export_table.len() != probe_export_table.len() {
        return Err("export table size mismatch between probe and final".into());
    }

    // Re-emit the summary with patched offsets.
    let mut header = Vec::new();
    serialize_summary(&summary, PkgArch::X64, &mut header)?;
    if header.len() as u32 != header_size {
        return Err("header size changed between probe and final".into());
    }
    // Patch HeaderSize at offset 8-11.
    header[8..12].copy_from_slice(&header_size.to_le_bytes());

    let bodies_size: u32 = new_payloads.iter().map(|p| p.len() as u32).sum();
    let mut out = Vec::with_capacity(
        (header_size
            + name_table.len() as u32
            + import_table.len() as u32
            + final_export_table.len() as u32
            + depends_table_size
            + bodies_size) as usize,
    );
    out.extend_from_slice(&header);
    out.extend_from_slice(&name_table);
    out.extend_from_slice(&import_table);
    out.extend_from_slice(&final_export_table);
    out.extend_from_slice(&vec![0u8; depends_table_size as usize]);
    for body in &new_payloads {
        out.extend_from_slice(body);
    }
    Ok(out)
}

/// Re-encode a single export payload from x32 to x64 property encoding.
///
/// Layout inside an export payload:
///   [4 bytes NetIndex] [property block...] [trailing bytes: mip table, etc.]
///
/// The property block always ends with a "None" name index (8 bytes). Trailing
/// bytes are arch-agnostic and are copied verbatim.
fn reencode_export_payload(
    export: &GpkExportEntry,
    names: &[GpkNameEntry],
) -> Result<Vec<u8>, String> {
    let payload = &export.payload;
    if payload.len() < 4 {
        // Too small to contain a NetIndex — preserve as-is.
        return Ok(payload.clone());
    }
    let net_index = i32::from_le_bytes(payload[..4].try_into().unwrap());
    let body = &payload[4..];

    // Parse the property block and record how many bytes it consumed.
    // On parse failure (e.g. raw-data exports with no valid property header)
    // fall back to copying the payload verbatim.
    let (parsed, consumed) = match parse_properties_with_consumed(body, ArchKind::X32, names) {
        Ok(result) => result,
        Err(_) => return Ok(payload.clone()),
    };
    let trailing = &body[consumed..];

    let mut new_body = Vec::with_capacity(payload.len());
    new_body.extend_from_slice(&net_index.to_le_bytes());
    write_properties(&parsed, ArchKind::X64, names, &mut new_body)?;
    new_body.extend_from_slice(trailing);
    Ok(new_body)
}

/// Serialize the name table.
///
/// Each entry: `i32 length+1` (ASCII, positive) + ascii bytes + null + `i64 flags`.
fn serialize_name_table(names: &[GpkNameEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    for entry in names {
        let len = (entry.name.len() + 1) as i32;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(entry.name.as_bytes());
        out.push(0u8);
        out.extend_from_slice(&entry.flags.to_le_bytes());
    }
    out
}

/// Serialize the import table.
///
/// Each entry is fixed 28 bytes:
/// `i64 class_package_name_idx | i64 class_name_idx | i32 owner_index | i64 object_name_idx`
fn serialize_import_table(
    imports: &[GpkImportEntry],
    names: &[GpkNameEntry],
) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(imports.len() * 28);
    for imp in imports {
        let cp_idx = find_name(names, &imp.class_package_name)?;
        let cn_idx = find_name(names, &imp.class_name)?;
        let on_idx = find_name(names, &imp.object_name)?;
        out.extend_from_slice(&cp_idx.to_le_bytes());
        out.extend_from_slice(&cn_idx.to_le_bytes());
        out.extend_from_slice(&imp.owner_index.to_le_bytes());
        out.extend_from_slice(&on_idx.to_le_bytes());
    }
    Ok(out)
}

/// Serialize the export table.
///
/// Per-entry layout (variable size):
/// ```
/// i32 class_index | i32 super_index | i32 package_index
/// i32 object_name_idx  (note: i32, not i64)
/// i64 unk1 | i64 unk2
/// i32 serial_size | i32 serial_offset  (serial_offset only present if serial_size > 0)
/// i32 export_flags | i32 unk_header_count | i32 unk4
/// bytes[16] guid
/// bytes[unk_header_count * 4] unk_extra_ints
/// ```
///
/// `body_base` is the file offset at which the first export body begins.
/// Pass 0 for a probe call (to measure table size without correct offsets).
fn serialize_export_table(
    exports: &[GpkExportEntry],
    new_payloads: &[Vec<u8>],
    names: &[GpkNameEntry],
    body_base: u32,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut current_offset = body_base;

    for (export, payload) in exports.iter().zip(new_payloads.iter()) {
        let on_idx = find_name_i32(names, &export.object_name)?;
        let serial_size = payload.len() as u32;

        out.extend_from_slice(&export.class_index.to_le_bytes());
        out.extend_from_slice(&export.super_index.to_le_bytes());
        out.extend_from_slice(&export.package_index.to_le_bytes());
        out.extend_from_slice(&on_idx.to_le_bytes());
        out.extend_from_slice(&export.unk1.to_le_bytes());
        out.extend_from_slice(&export.unk2.to_le_bytes());
        out.extend_from_slice(&serial_size.to_le_bytes());
        if serial_size > 0 {
            out.extend_from_slice(&current_offset.to_le_bytes());
        }
        out.extend_from_slice(&export.export_flags.to_le_bytes());
        out.extend_from_slice(&(export.unk_extra_ints.len() as u32).to_le_bytes());
        out.extend_from_slice(&export.unk4.to_le_bytes());
        out.extend_from_slice(&export.guid);
        for &v in &export.unk_extra_ints {
            out.extend_from_slice(&v.to_le_bytes());
        }

        current_offset = current_offset.saturating_add(serial_size);
    }
    Ok(out)
}

/// Find a name's i64 index for import-table entries.
fn find_name(names: &[GpkNameEntry], wanted: &str) -> Result<i64, String> {
    names
        .iter()
        .position(|e| e.name == wanted)
        .map(|i| i as i64)
        .ok_or_else(|| format!("name '{wanted}' not found in name table"))
}

/// Find a name's i32 index for export-table ObjectName field.
fn find_name_i32(names: &[GpkNameEntry], wanted: &str) -> Result<i32, String> {
    names
        .iter()
        .position(|e| e.name == wanted)
        .map(|i| i as i32)
        .ok_or_else(|| format!("name '{wanted}' not found in name table"))
}

// ---------------------------------------------------------------------------
// LZO compression path (Task 6)
// ---------------------------------------------------------------------------

const CHUNK_BLOCK_SIGNATURE: u32 = 0x9E2A83C1;

/// Take a fully-assembled uncompressed x64 GPK and rewrite it with an
/// LZO-chunked body (compression_flags = 2).
///
/// Layout of the returned bytes:
///   [header with chunk_count + chunk table]
///   [chunk data blocks, back-to-back]
///
/// The `name_offset` from the uncompressed file marks the start of the logical
/// body. The header prefix is re-serialised with updated compression_flags,
/// chunk_count, and chunk table; everything from name_offset onward is split
/// into 32 MiB chunks of 128 KiB LZO-compressed blocks.
fn compress_lzo(uncompressed: Vec<u8>) -> Result<Vec<u8>, String> {
    let pkg = parse_package(&uncompressed)?;
    let name_offset = pkg.summary.name_offset as usize;

    if name_offset > uncompressed.len() {
        return Err(format!(
            "name_offset {name_offset} exceeds uncompressed GPK length {}",
            uncompressed.len()
        ));
    }

    let body = &uncompressed[name_offset..];

    // Compress each 32 MiB chunk.
    let chunk_bodies: Vec<Vec<u8>> = body
        .chunks(LZO_CHUNK_CAP)
        .map(compress_one_chunk)
        .collect::<Result<Vec<_>, _>>()?;

    // Build the updated summary.
    let mut summary = pkg.summary.clone();
    summary.compression_flags = 2;
    // Set the PKG_Compressed bit (0x02000000).
    summary.package_flags |= 0x02000000;

    // Compute header + chunk-table size. serialize_summary emits chunk_count=0,
    // so base_header_size is the pre-table size; chunk table adds chunk_count*16.
    let chunk_count = chunk_bodies.len();
    let mut header_probe = Vec::new();
    serialize_summary(&summary, PkgArch::X64, &mut header_probe)?;
    let base_header_size = header_probe.len();
    let chunk_table_size = chunk_count * 16;
    let total_header_size = base_header_size + chunk_table_size;

    // Compute per-chunk header entries.
    let mut chunk_headers: Vec<ChunkHeader> = Vec::with_capacity(chunk_count);
    let mut compressed_offset = total_header_size as u32;
    let mut uncompressed_body_offset = 0usize;

    for chunk_body in &chunk_bodies {
        let unc_size = body[uncompressed_body_offset..].len().min(LZO_CHUNK_CAP) as u32;
        chunk_headers.push(ChunkHeader {
            // absolute offset in the logical uncompressed file
            uncompressed_offset: (name_offset + uncompressed_body_offset) as u32,
            uncompressed_size: unc_size,
            compressed_offset,
            compressed_size: chunk_body.len() as u32,
        });
        compressed_offset = compressed_offset
            .checked_add(chunk_body.len() as u32)
            .ok_or("compressed offset overflow")?;
        uncompressed_body_offset += LZO_CHUNK_CAP;
    }

    // Serialize header + chunk table.
    let total_data_size: usize = chunk_bodies.iter().map(|c| c.len()).sum();
    let mut out = Vec::with_capacity(total_header_size + total_data_size);
    serialize_summary_with_chunks(&summary, PkgArch::X64, &chunk_headers, &mut out)?;
    // Patch HeaderSize at offset 8-11.
    let hsize = total_header_size as u32;
    out[8..12].copy_from_slice(&hsize.to_le_bytes());

    if out.len() != total_header_size {
        return Err(format!(
            "header+chunk-table size mismatch: expected {total_header_size}, got {}",
            out.len()
        ));
    }

    for chunk_body in &chunk_bodies {
        out.extend_from_slice(chunk_body);
    }

    Ok(out)
}

/// Compress one chunk into the on-disk format read by `decompress_chunk`:
///
/// ```text
/// u32  signature        = CHUNK_BLOCK_SIGNATURE
/// u32  block_size       = 131072
/// u32  compressed_size  = sum(block.compressed_size)
/// u32  uncompressed_size = chunk.len()
/// [per block: u32 compressed_size, u32 uncompressed_size]
/// [concatenated compressed block data]
/// ```
fn compress_one_chunk(chunk: &[u8]) -> Result<Vec<u8>, String> {
    let block_size = LZO_BLOCK_SIZE;
    let blocks_raw: Vec<(&[u8], Vec<u8>)> = chunk
        .chunks(block_size)
        .map(|block| {
            let compressed = lzokay::compress::compress(block)
                .map_err(|e| format!("LZO compress failed: {e}"))?;
            Ok((block, compressed))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let total_compressed: usize = blocks_raw.iter().map(|(_, c)| c.len()).sum();
    let header_bytes = 4 + 4 + 4 + 4 + blocks_raw.len() * 8;

    let mut out = Vec::with_capacity(header_bytes + total_compressed);

    out.extend_from_slice(&CHUNK_BLOCK_SIGNATURE.to_le_bytes());
    out.extend_from_slice(&(block_size as u32).to_le_bytes());
    out.extend_from_slice(&(total_compressed as u32).to_le_bytes());
    out.extend_from_slice(&(chunk.len() as u32).to_le_bytes());

    for (block, compressed) in &blocks_raw {
        out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        out.extend_from_slice(&(block.len() as u32).to_le_bytes());
    }

    for (_, compressed) in &blocks_raw {
        out.extend_from_slice(compressed);
    }

    Ok(out)
}
