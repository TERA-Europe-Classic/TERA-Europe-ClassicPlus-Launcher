//! Minimal TERA GPK package parser supporting both Classic (x32, FileVersion
//! 610) and v100.02 (x64, FileVersion 897) layouts.
//!
//! Read-only package-analysis seam used by the patch converter and the patch
//! applier: package summary, name table, import table, export table,
//! object-path resolution, and export payload extraction. Does not write
//! packages directly — the applier rebuilds via slice+concatenate so the
//! header is preserved verbatim.
//!
//! x32 vs x64 differences this parser handles:
//! - x64 inserts 16 extra header bytes after `DependsOffset` (per
//!   TeraCoreLib FStructs.cpp: `ImportExportGuidsOffset`, `ImportGuidsCount`,
//!   `ExportGuidsCount`, `ThumbnailTableOffset`).
//! - x32 stores `NameCount` as `count + name_offset` when cooked; x64 stores
//!   the raw count.
//!
//! Property-body differences (BoolProperty 1 vs 4 bytes, ByteProperty
//! enumType prefix) only matter when parsing inside an export payload, which
//! we do not do — the applier replaces payloads byte-for-byte.

use sha2::{Digest, Sha256};
use std::io::Read;

const PACKAGE_MAGIC: u32 = 0x9E2A83C1;
pub const X64_VERSION_THRESHOLD: u16 = 0x381;
const CHUNK_BLOCK_SIGNATURE: u32 = PACKAGE_MAGIC;

/// Reads the FileVersion (u16 LE at offset 4) from a GPK header. Returns
/// `None` if the buffer is too small or doesn't start with the package
/// magic. Used by the install flow to refuse arch-mismatched mods before
/// attempting derivation.
pub fn read_file_version(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 8 {
        return None;
    }
    let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if magic != PACKAGE_MAGIC {
        return None;
    }
    Some(u16::from_le_bytes([bytes[4], bytes[5]]))
}

/// Returns true for FileVersion >= 0x381 (TERA v100.02 / Modern x64).
pub fn is_x64_file_version(file_version: u16) -> bool {
    file_version >= X64_VERSION_THRESHOLD
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChunkHeader {
    uncompressed_offset: u32,
    uncompressed_size: u32,
    compressed_offset: u32,
    compressed_size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpkPackage {
    pub summary: GpkPackageSummary,
    pub names: Vec<GpkNameEntry>,
    pub imports: Vec<GpkImportEntry>,
    pub exports: Vec<GpkExportEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpkPackageSummary {
    pub file_version: u16,
    pub license_version: u16,
    pub package_name: String,
    pub package_flags: u32,
    pub name_count: u32,
    pub name_offset: u32,
    pub export_count: u32,
    pub export_offset: u32,
    pub import_count: u32,
    pub import_offset: u32,
    pub depends_offset: u32,
    pub compression_flags: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpkNameEntry {
    pub name: String,
    pub flags: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpkImportEntry {
    pub class_package_name: String,
    pub class_name: String,
    pub owner_index: i32,
    pub object_name: String,
    pub object_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpkExportEntry {
    pub class_index: i32,
    pub super_index: i32,
    pub package_index: i32,
    pub object_name: String,
    pub object_path: String,
    pub class_name: Option<String>,
    pub serial_size: u32,
    pub serial_offset: Option<u32>,
    pub export_flags: u32,
    pub payload: Vec<u8>,
    pub payload_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpkPackageDiff {
    pub name_count_before: usize,
    pub name_count_after: usize,
    pub import_count_before: usize,
    pub import_count_after: usize,
    pub export_count_before: usize,
    pub export_count_after: usize,
    pub changed_exports: Vec<ChangedExport>,
    pub removed_exports: Vec<String>,
    pub added_exports: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedExport {
    pub object_path: String,
    pub class_before: Option<String>,
    pub class_after: Option<String>,
    pub payload_fingerprint_before: String,
    pub payload_fingerprint_after: String,
}

pub fn compare_packages(reference: &GpkPackage, modded: &GpkPackage) -> GpkPackageDiff {
    let mut reference_exports = std::collections::BTreeMap::new();
    for export in &reference.exports {
        reference_exports.insert(export.object_path.clone(), export);
    }

    let mut modded_exports = std::collections::BTreeMap::new();
    for export in &modded.exports {
        modded_exports.insert(export.object_path.clone(), export);
    }

    let mut changed_exports = Vec::new();
    let mut removed_exports = Vec::new();
    let mut added_exports = Vec::new();

    for (object_path, reference_export) in &reference_exports {
        match modded_exports.get(object_path) {
            Some(modded_export)
                if reference_export.class_name != modded_export.class_name
                    || reference_export.payload_fingerprint
                        != modded_export.payload_fingerprint =>
            {
                changed_exports.push(ChangedExport {
                    object_path: object_path.clone(),
                    class_before: reference_export.class_name.clone(),
                    class_after: modded_export.class_name.clone(),
                    payload_fingerprint_before: reference_export.payload_fingerprint.clone(),
                    payload_fingerprint_after: modded_export.payload_fingerprint.clone(),
                });
            }
            Some(_) => {}
            None => removed_exports.push(object_path.clone()),
        }
    }

    for object_path in modded_exports.keys() {
        if !reference_exports.contains_key(object_path) {
            added_exports.push(object_path.clone());
        }
    }

    GpkPackageDiff {
        name_count_before: reference.names.len(),
        name_count_after: modded.names.len(),
        import_count_before: reference.imports.len(),
        import_count_after: modded.imports.len(),
        export_count_before: reference.exports.len(),
        export_count_after: modded.exports.len(),
        changed_exports,
        removed_exports,
        added_exports,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawExport {
    class_index: i32,
    super_index: i32,
    package_index: i32,
    object_name_index: i32,
    serial_size: u32,
    serial_offset: Option<u32>,
    export_flags: u32,
}

/// Returns the uncompressed full-file representation of a GPK. For
/// uncompressed inputs this is `bytes.to_vec()`; for ZLIB/LZO chunked
/// packages it walks the chunk table and reassembles the body. Used by
/// offline tooling that needs to feed `apply_manifest` (which only
/// accepts uncompressed inputs).
pub fn extract_uncompressed_package_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() < 32 {
        return Err("GPK package is too small to contain a valid header".into());
    }
    let mut cursor = 0usize;
    let tag = read_u32_le(bytes, &mut cursor)?;
    if tag != PACKAGE_MAGIC {
        return Err(format!(
            "GPK package has invalid magic {:08X}; expected {:08X}",
            tag, PACKAGE_MAGIC
        ));
    }
    let file_version = read_u16_le(bytes, &mut cursor)?;
    let _license_version = read_u16_le(bytes, &mut cursor)?;
    let is_x64 = is_x64_file_version(file_version);
    let _header_size = read_u32_le(bytes, &mut cursor)?;
    let _package_name = read_fstring(bytes, &mut cursor)?;
    let package_flags_pos = cursor;
    let _package_flags = read_u32_le(bytes, &mut cursor)?;
    let _raw_name_count = read_u32_le(bytes, &mut cursor)?;
    let name_offset = read_u32_le(bytes, &mut cursor)?;
    let _export_count = read_u32_le(bytes, &mut cursor)?;
    let _export_offset = read_u32_le(bytes, &mut cursor)?;
    let _import_count = read_u32_le(bytes, &mut cursor)?;
    let _import_offset = read_u32_le(bytes, &mut cursor)?;
    let _depends_offset = read_u32_le(bytes, &mut cursor)?;
    if is_x64 {
        skip_exact(bytes, &mut cursor, 16)?;
    }
    skip_exact(bytes, &mut cursor, 16)?;
    let generation_count = read_u32_le(bytes, &mut cursor)? as usize;
    skip_exact(bytes, &mut cursor, generation_count.saturating_mul(12))?;
    let _engine_version = read_u32_le(bytes, &mut cursor)?;
    let _cooker_version = read_u32_le(bytes, &mut cursor)?;
    let compression_flags_pos = cursor;
    let compression_flags = read_u32_le(bytes, &mut cursor)?;
    let chunk_count_pos = cursor;
    let chunk_count = read_u32_le(bytes, &mut cursor)? as usize;
    let chunk_headers = read_chunk_headers(bytes, &mut cursor, chunk_count)?;
    match compression_flags {
        0 => Ok(bytes.to_vec()),
        1 | 2 => {
            let mut out = decompress_package_body(bytes, name_offset, compression_flags, &chunk_headers)?;
            // Patch the header so subsequent parsers treat the result as
            // uncompressed: compression_flags=0, chunk_count=0. The
            // chunk_headers bytes after chunk_count remain as in-file
            // padding before name_offset; they're harmless since
            // chunk_count=0 means parsers don't read them.
            if compression_flags_pos + 4 <= out.len() {
                out[compression_flags_pos..compression_flags_pos + 4]
                    .copy_from_slice(&0u32.to_le_bytes());
            }
            if chunk_count_pos + 4 <= out.len() {
                out[chunk_count_pos..chunk_count_pos + 4]
                    .copy_from_slice(&0u32.to_le_bytes());
            }
            // Strip the PKG_Compressed bit (0x02000000) from package_flags so
            // the engine doesn't expect a compressed body it won't find.
            if package_flags_pos + 4 <= out.len() {
                let mut flags = u32::from_le_bytes([
                    out[package_flags_pos], out[package_flags_pos + 1],
                    out[package_flags_pos + 2], out[package_flags_pos + 3],
                ]);
                flags &= !0x02000000;
                out[package_flags_pos..package_flags_pos + 4]
                    .copy_from_slice(&flags.to_le_bytes());
            }
            Ok(out)
        }
        other => Err(format!(
            "Unsupported GPK package compression flag {other}; expected 0 (none), 1 (zlib), or 2 (lzo)"
        )),
    }
}

pub fn parse_package(bytes: &[u8]) -> Result<GpkPackage, String> {
    if bytes.len() < 32 {
        return Err("GPK package is too small to contain a valid header".into());
    }

    let mut cursor = 0usize;
    let tag = read_u32_le(bytes, &mut cursor)?;
    if tag != PACKAGE_MAGIC {
        return Err(format!(
            "GPK package has invalid magic {:08X}; expected {:08X}",
            tag, PACKAGE_MAGIC
        ));
    }

    let file_version = read_u16_le(bytes, &mut cursor)?;
    let license_version = read_u16_le(bytes, &mut cursor)?;
    let is_x64 = is_x64_file_version(file_version);

    let _header_size = read_u32_le(bytes, &mut cursor)?;
    let package_name = read_fstring(bytes, &mut cursor)?;
    let package_flags = read_u32_le(bytes, &mut cursor)?;

    let raw_name_count = read_u32_le(bytes, &mut cursor)?;
    let name_offset = read_u32_le(bytes, &mut cursor)?;
    let export_count = read_u32_le(bytes, &mut cursor)?;
    let export_offset = read_u32_le(bytes, &mut cursor)?;
    let import_count = read_u32_le(bytes, &mut cursor)?;
    let import_offset = read_u32_le(bytes, &mut cursor)?;
    let depends_offset = read_u32_le(bytes, &mut cursor)?;

    if is_x64 {
        // x64 (v100.02) inserts 4 extra u32 fields here per TeraCoreLib
        // FStructs.cpp: ImportExportGuidsOffset, ImportGuidsCount,
        // ExportGuidsCount, ThumbnailTableOffset (16 bytes total).
        skip_exact(bytes, &mut cursor, 16)?;
    }

    skip_exact(bytes, &mut cursor, 16)?; // FGuid

    let generation_count = read_u32_le(bytes, &mut cursor)? as usize;
    skip_exact(bytes, &mut cursor, generation_count.saturating_mul(12))?;

    let _engine_version = read_u32_le(bytes, &mut cursor)?;
    let _cooker_version = read_u32_le(bytes, &mut cursor)?;
    let compression_flags = read_u32_le(bytes, &mut cursor)?;
    let chunk_count = read_u32_le(bytes, &mut cursor)? as usize;
    let chunk_headers = read_chunk_headers(bytes, &mut cursor, chunk_count)?;

    let working_bytes = match compression_flags {
        0 => bytes.to_vec(),
        1 | 2 => decompress_package_body(bytes, name_offset, compression_flags, &chunk_headers)?,
        other => {
            return Err(format!(
                "Unsupported GPK package compression flag {other}; expected 0 (none), 1 (zlib), or 2 (lzo)"
            ));
        }
    };

    // x32 (Classic) cooked packages store NameCount as `count + name_offset`;
    // x64 (Modern) stores the raw count.
    let name_count = if is_x64 {
        raw_name_count
    } else {
        raw_name_count.saturating_sub(name_offset)
    };
    let names = parse_names(&working_bytes, name_offset as usize, name_count as usize)?;
    let imports = parse_imports(
        &working_bytes,
        import_offset as usize,
        import_count as usize,
        &names,
    )?;
    let exports = parse_exports(
        &working_bytes,
        export_offset as usize,
        export_count as usize,
        &names,
        &imports,
    )?;

    Ok(GpkPackage {
        summary: GpkPackageSummary {
            file_version,
            license_version,
            package_name,
            package_flags,
            name_count,
            name_offset,
            export_count,
            export_offset,
            import_count,
            import_offset,
            depends_offset,
            compression_flags,
        },
        names,
        imports,
        exports,
    })
}

fn read_chunk_headers(
    bytes: &[u8],
    cursor: &mut usize,
    count: usize,
) -> Result<Vec<ChunkHeader>, String> {
    let mut headers = Vec::with_capacity(count);
    for _ in 0..count {
        headers.push(ChunkHeader {
            uncompressed_offset: read_u32_le(bytes, cursor)?,
            uncompressed_size: read_u32_le(bytes, cursor)?,
            compressed_offset: read_u32_le(bytes, cursor)?,
            compressed_size: read_u32_le(bytes, cursor)?,
        });
    }
    Ok(headers)
}

fn decompress_package_body(
    bytes: &[u8],
    name_offset: u32,
    compression_flags: u32,
    chunk_headers: &[ChunkHeader],
) -> Result<Vec<u8>, String> {
    let prefix_len = name_offset as usize;
    if prefix_len > bytes.len() {
        return Err(format!(
            "Name offset {} extends past EOF {} while rebuilding compressed package",
            prefix_len,
            bytes.len()
        ));
    }

    let mut rebuilt = bytes[..prefix_len].to_vec();
    let mut total_len = prefix_len;
    for header in chunk_headers {
        let end = header
            .uncompressed_offset
            .checked_add(header.uncompressed_size)
            .ok_or_else(|| "Compressed chunk uncompressed range overflows u32".to_string())?
            as usize;
        total_len = total_len.max(end);
    }
    rebuilt.resize(total_len, 0);

    for header in chunk_headers {
        let chunk = decompress_chunk(bytes, header, compression_flags)?;
        if chunk.len() != header.uncompressed_size as usize {
            return Err(format!(
                "Decompressed chunk size mismatch at offset {}: expected {}, got {}",
                header.uncompressed_offset,
                header.uncompressed_size,
                chunk.len()
            ));
        }
        let start = header.uncompressed_offset as usize;
        let end = start + chunk.len();
        rebuilt[start..end].copy_from_slice(&chunk);
    }

    Ok(rebuilt)
}

fn decompress_chunk(
    bytes: &[u8],
    header: &ChunkHeader,
    compression_flags: u32,
) -> Result<Vec<u8>, String> {
    let mut cursor = header.compressed_offset as usize;
    let signature = read_u32_le(bytes, &mut cursor)?;
    if signature != CHUNK_BLOCK_SIGNATURE {
        return Err(format!(
            "Compressed chunk at {} has invalid signature {:08X}",
            header.compressed_offset, signature
        ));
    }

    let block_size = read_u32_le(bytes, &mut cursor)? as usize;
    let _compressed_payload_size = read_u32_le(bytes, &mut cursor)? as usize;
    let chunk_uncompressed_size = read_u32_le(bytes, &mut cursor)? as usize;
    let expected_blocks = chunk_uncompressed_size.div_ceil(block_size.max(1));

    let mut blocks = Vec::with_capacity(expected_blocks);
    for _ in 0..expected_blocks {
        let compressed_size = read_u32_le(bytes, &mut cursor)? as usize;
        let uncompressed_size = read_u32_le(bytes, &mut cursor)? as usize;
        blocks.push((compressed_size, uncompressed_size));
    }

    let mut out = Vec::with_capacity(chunk_uncompressed_size);
    for (compressed_size, uncompressed_size) in blocks {
        let compressed = read_slice(bytes, &mut cursor, compressed_size)?;
        let block = match compression_flags {
            1 => decompress_zlib_block(compressed, uncompressed_size)?,
            2 => decompress_lzo_block(compressed, uncompressed_size)?,
            other => {
                return Err(format!(
                    "Unsupported chunk compression flag {other} while decoding package"
                ));
            }
        };
        if block.len() != uncompressed_size {
            return Err(format!(
                "Compressed block size mismatch: expected {}, got {}",
                uncompressed_size,
                block.len()
            ));
        }
        out.extend_from_slice(&block);
    }

    if out.len() != chunk_uncompressed_size {
        return Err(format!(
            "Chunk decompressed to {} bytes, expected {}",
            out.len(),
            chunk_uncompressed_size
        ));
    }
    Ok(out)
}

fn decompress_zlib_block(input: &[u8], expected_len: usize) -> Result<Vec<u8>, String> {
    let mut decoder = flate2::read::ZlibDecoder::new(input);
    let mut out = Vec::with_capacity(expected_len);
    decoder
        .read_to_end(&mut out)
        .map_err(|e| format!("Failed to decompress zlib GPK block: {e}"))?;
    Ok(out)
}

fn decompress_lzo_block(input: &[u8], expected_len: usize) -> Result<Vec<u8>, String> {
    let mut out = vec![0u8; expected_len];
    let written = lzokay::decompress::decompress(input, &mut out)
        .map_err(|e| format!("Failed to decompress lzo GPK block: {e}"))?;
    out.truncate(written);
    Ok(out)
}

fn parse_names(bytes: &[u8], mut offset: usize, count: usize) -> Result<Vec<GpkNameEntry>, String> {
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        let name = read_fstring(bytes, &mut offset)?;
        let flags = read_u64_le(bytes, &mut offset)?;
        names.push(GpkNameEntry { name, flags });
    }
    Ok(names)
}

fn parse_imports(
    bytes: &[u8],
    mut offset: usize,
    count: usize,
    names: &[GpkNameEntry],
) -> Result<Vec<GpkImportEntry>, String> {
    let mut raw = Vec::with_capacity(count);
    for _ in 0..count {
        let class_package_index = read_u64_le(bytes, &mut offset)?;
        let class_name_index = read_u64_le(bytes, &mut offset)?;
        let owner_index = read_i32_le(bytes, &mut offset)?;
        let object_name_index = read_u64_le(bytes, &mut offset)?;
        raw.push((
            class_package_index,
            class_name_index,
            owner_index,
            object_name_index,
        ));
    }

    let mut imports = Vec::with_capacity(count);
    for (idx, (class_package_index, class_name_index, owner_index, object_name_index)) in
        raw.iter().copied().enumerate()
    {
        let class_package_name = resolve_name(names, class_package_index)?;
        let class_name = resolve_name(names, class_name_index)?;
        let object_name = resolve_name(names, object_name_index)?;
        let object_path = build_import_path(&raw, names, idx, 0)?;
        imports.push(GpkImportEntry {
            class_package_name,
            class_name,
            owner_index,
            object_name,
            object_path,
        });
    }
    Ok(imports)
}

fn parse_exports(
    bytes: &[u8],
    mut offset: usize,
    count: usize,
    names: &[GpkNameEntry],
    imports: &[GpkImportEntry],
) -> Result<Vec<GpkExportEntry>, String> {
    let mut raw = Vec::with_capacity(count);
    for _ in 0..count {
        let class_index = read_i32_le(bytes, &mut offset)?;
        let super_index = read_i32_le(bytes, &mut offset)?;
        let package_index = read_i32_le(bytes, &mut offset)?;
        let object_name_index = read_i32_le(bytes, &mut offset)?;
        let _unk1 = read_u64_le(bytes, &mut offset)?;
        let _unk2 = read_u64_le(bytes, &mut offset)?;
        let serial_size = read_u32_le(bytes, &mut offset)?;
        let serial_offset = if serial_size > 0 {
            Some(read_u32_le(bytes, &mut offset)?)
        } else {
            None
        };
        let export_flags = read_u32_le(bytes, &mut offset)?;
        let unk_header_count = read_u32_le(bytes, &mut offset)? as usize;
        let _unk4 = read_u32_le(bytes, &mut offset)?;
        skip_exact(bytes, &mut offset, 16)?; // guid
        skip_exact(bytes, &mut offset, unk_header_count.saturating_mul(4))?;
        raw.push(RawExport {
            class_index,
            super_index,
            package_index,
            object_name_index,
            serial_size,
            serial_offset,
            export_flags,
        });
    }

    let mut exports = Vec::with_capacity(count);
    for idx in 0..raw.len() {
        let item = &raw[idx];
        let object_name = resolve_name_from_i32(names, item.object_name_index)?;
        let object_path = build_export_path(&raw, imports, names, idx, 0)?;
        let class_name = resolve_object_ref_name(item.class_index, &raw, imports, names, 0).ok();
        let payload = if let Some(serial_offset) = item.serial_offset {
            let start = serial_offset as usize;
            let end = start
                .checked_add(item.serial_size as usize)
                .ok_or_else(|| {
                    format!("Export '{}' payload offset overflows usize", object_path)
                })?;
            if end > bytes.len() {
                return Err(format!(
                    "Export '{}' payload extends past EOF ({}..{} of {})",
                    object_path,
                    start,
                    end,
                    bytes.len()
                ));
            }
            bytes[start..end].to_vec()
        } else {
            Vec::new()
        };
        let payload_fingerprint = format!("sha256:{}", hex_lower(&Sha256::digest(&payload)));
        exports.push(GpkExportEntry {
            class_index: item.class_index,
            super_index: item.super_index,
            package_index: item.package_index,
            object_name,
            object_path,
            class_name,
            serial_size: item.serial_size,
            serial_offset: item.serial_offset,
            export_flags: item.export_flags,
            payload,
            payload_fingerprint,
        });
    }

    Ok(exports)
}

fn build_import_path(
    raw: &[(u64, u64, i32, u64)],
    names: &[GpkNameEntry],
    idx: usize,
    depth: usize,
) -> Result<String, String> {
    if depth > 32 {
        return Err("Import owner chain exceeds recursion limit".into());
    }
    let (class_package_index, _, owner_index, object_name_index) = raw[idx];
    let object_name = resolve_name(names, object_name_index)?;
    if owner_index == 0 {
        let class_package = resolve_name(names, class_package_index)?;
        if class_package.is_empty() || class_package == object_name {
            return Ok(object_name);
        }
        return Ok(format!("{class_package}.{object_name}"));
    }
    if owner_index > 0 {
        return Err(format!(
            "Import '{}' references export owner index {}, which this minimal parser does not support yet",
            object_name, owner_index
        ));
    }
    let parent = owner_index
        .checked_neg()
        .and_then(|v| v.checked_sub(1))
        .ok_or_else(|| format!("Invalid import owner index {}", owner_index))?
        as usize;
    let parent_path = build_import_path(raw, names, parent, depth + 1)?;
    Ok(format!("{parent_path}.{object_name}"))
}

fn build_export_path(
    raw_exports: &[impl ExportLike],
    imports: &[GpkImportEntry],
    names: &[GpkNameEntry],
    idx: usize,
    depth: usize,
) -> Result<String, String> {
    if depth > 32 {
        return Err("Export owner chain exceeds recursion limit".into());
    }
    let export = &raw_exports[idx];
    let object_name = resolve_name_from_i32(names, export.object_name_index())?;
    match export.package_index() {
        0 => Ok(object_name),
        owner if owner < 0 => {
            let parent = owner
                .checked_neg()
                .and_then(|v| v.checked_sub(1))
                .ok_or_else(|| format!("Invalid export owner index {owner}"))?
                as usize;
            let parent_path = imports
                .get(parent)
                .ok_or_else(|| format!("Export owner import {} out of range", parent))?
                .object_path
                .clone();
            Ok(format!("{parent_path}.{object_name}"))
        }
        owner => {
            let parent = owner
                .checked_sub(1)
                .ok_or_else(|| format!("Invalid export owner index {owner}"))?
                as usize;
            let parent_path = build_export_path(raw_exports, imports, names, parent, depth + 1)?;
            Ok(format!("{parent_path}.{object_name}"))
        }
    }
}

fn resolve_object_ref_name(
    object_index: i32,
    raw_exports: &[impl ExportLike],
    imports: &[GpkImportEntry],
    names: &[GpkNameEntry],
    depth: usize,
) -> Result<String, String> {
    if depth > 32 {
        return Err("Object reference recursion limit exceeded".into());
    }
    if object_index == 0 {
        return Err("Object reference points to none".into());
    }
    if object_index < 0 {
        let idx = object_index
            .checked_neg()
            .and_then(|v| v.checked_sub(1))
            .ok_or_else(|| format!("Invalid import object index {object_index}"))?
            as usize;
        return imports
            .get(idx)
            .map(|i| i.object_path.clone())
            .ok_or_else(|| format!("Import object index {} out of range", idx));
    }

    let idx = object_index
        .checked_sub(1)
        .ok_or_else(|| format!("Invalid export object index {object_index}"))?
        as usize;
    let export = raw_exports
        .get(idx)
        .ok_or_else(|| format!("Export object index {} out of range", idx))?;
    build_export_path(raw_exports, imports, names, idx, depth + 1)
        .or_else(|_| resolve_name_from_i32(names, export.object_name_index()))
}

fn resolve_name(names: &[GpkNameEntry], encoded_index: u64) -> Result<String, String> {
    let idx = encoded_index as u32 as usize;
    names
        .get(idx)
        .map(|entry| entry.name.clone())
        .ok_or_else(|| format!("Name index {} out of range", idx))
}

fn resolve_name_from_i32(names: &[GpkNameEntry], index: i32) -> Result<String, String> {
    let idx = usize::try_from(index).map_err(|_| format!("Negative name index {}", index))?;
    names
        .get(idx)
        .map(|entry| entry.name.clone())
        .ok_or_else(|| format!("Name index {} out of range", idx))
}

fn read_fstring(bytes: &[u8], cursor: &mut usize) -> Result<String, String> {
    let len = read_i32_le(bytes, cursor)?;
    if len == 0 {
        return Ok(String::new());
    }
    if len > 0 {
        let len = len as usize;
        let data = read_slice(bytes, cursor, len)?;
        let without_null = data.strip_suffix(&[0]).unwrap_or(data);
        return Ok(String::from_utf8_lossy(without_null).to_string());
    }

    let units = (-len) as usize;
    let data = read_slice(bytes, cursor, units.saturating_mul(2))?;
    let mut buf = Vec::with_capacity(units);
    for chunk in data.chunks_exact(2) {
        buf.push(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    if let Some(0) = buf.last().copied() {
        let _ = buf.pop();
    }
    Ok(String::from_utf16_lossy(&buf))
}

fn read_slice<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8], String> {
    let end = cursor
        .checked_add(len)
        .ok_or_else(|| "Cursor overflow while reading GPK bytes".to_string())?;
    if end > bytes.len() {
        return Err("Unexpected EOF while reading GPK bytes".into());
    }
    let slice = &bytes[*cursor..end];
    *cursor = end;
    Ok(slice)
}

fn skip_exact(bytes: &[u8], cursor: &mut usize, len: usize) -> Result<(), String> {
    let _ = read_slice(bytes, cursor, len)?;
    Ok(())
}

fn read_u16_le(bytes: &[u8], cursor: &mut usize) -> Result<u16, String> {
    let data = read_slice(bytes, cursor, 2)?;
    Ok(u16::from_le_bytes([data[0], data[1]]))
}

fn read_u32_le(bytes: &[u8], cursor: &mut usize) -> Result<u32, String> {
    let data = read_slice(bytes, cursor, 4)?;
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

fn read_i32_le(bytes: &[u8], cursor: &mut usize) -> Result<i32, String> {
    let data = read_slice(bytes, cursor, 4)?;
    Ok(i32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

fn read_u64_le(bytes: &[u8], cursor: &mut usize) -> Result<u64, String> {
    let data = read_slice(bytes, cursor, 8)?;
    Ok(u64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

trait ExportLike {
    fn package_index(&self) -> i32;
    fn object_name_index(&self) -> i32;
}

impl ExportLike for RawExport {
    fn package_index(&self) -> i32 {
        self.package_index
    }

    fn object_name_index(&self) -> i32 {
        self.object_name_index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::ZlibEncoder, Compression};
    use std::io::Write;

    const IMPORT_ENTRY_LEN: u32 = 28;
    const EXPORT_ENTRY_LEN_WITH_SERIAL_OFFSET: u32 = 68;

    enum CompressionFlavor {
        None,
        Zlib,
        Lzo,
        Unsupported(u32),
    }

    #[test]
    fn parses_minimal_uncompressed_classic_package() {
        let bytes = build_test_package(CompressionFlavor::None);
        let package = parse_package(&bytes).expect("parse synthetic package");

        assert_eq!(package.summary.file_version, 610);
        assert_eq!(package.summary.package_name, "S1UI_GageBoss");
        assert_eq!(package.names.len(), 6);
        assert_eq!(package.imports.len(), 2);
        assert_eq!(package.exports.len(), 2);

        assert_eq!(package.imports[0].object_path, "Core.GFxMovieInfo");
        assert_eq!(package.imports[1].object_path, "Core.ObjectRedirector");

        assert_eq!(package.exports[0].object_path, "GageBoss");
        assert_eq!(
            package.exports[0].class_name.as_deref(),
            Some("Core.GFxMovieInfo")
        );
        assert_eq!(package.exports[0].payload, vec![0x10, 0x11, 0x12, 0x13]);

        assert_eq!(package.exports[1].object_path, "GageBoss.GageBoss_I1C");
        assert_eq!(
            package.exports[1].class_name.as_deref(),
            Some("Core.ObjectRedirector")
        );
        assert_eq!(package.exports[1].payload, vec![0x20, 0x21, 0x22, 0x23]);
        assert!(package.exports[1]
            .payload_fingerprint
            .starts_with("sha256:"));
    }

    #[test]
    fn parses_zlib_compressed_classic_package() {
        let bytes = build_test_package(CompressionFlavor::Zlib);
        let package = parse_package(&bytes).expect("parse zlib-compressed package");

        assert_eq!(package.summary.compression_flags, 1);
        assert_eq!(package.exports.len(), 2);
        assert_eq!(package.exports[0].payload, vec![0x10, 0x11, 0x12, 0x13]);
        assert_eq!(package.exports[1].payload, vec![0x20, 0x21, 0x22, 0x23]);
    }

    #[test]
    fn parses_lzo_compressed_classic_package() {
        let bytes = build_test_package(CompressionFlavor::Lzo);
        let package = parse_package(&bytes).expect("parse lzo-compressed package");

        assert_eq!(package.summary.compression_flags, 2);
        assert_eq!(package.imports[0].object_path, "Core.GFxMovieInfo");
        assert_eq!(package.exports[1].object_path, "GageBoss.GageBoss_I1C");
    }

    #[test]
    fn rejects_unsupported_compression_flag() {
        let bytes = build_test_package(CompressionFlavor::Unsupported(4));
        let err = parse_package(&bytes).expect_err("unsupported compression must fail closed");
        assert!(err.contains("Unsupported GPK package compression flag"));
    }

    #[test]
    fn parses_x64_modern_package() {
        // v100.02 vanilla files have FileVersion 897 and 16 extra header
        // bytes between depends_offset and FGuid. The parser must accept
        // them and produce the same logical structure as the x32 fixture.
        let bytes = super::super::test_fixtures::build_x64_boss_window_test_package(
            [0x10, 0x11, 0x12, 0x13],
            true,
        );
        let package = parse_package(&bytes).expect("parse x64 package");

        assert_eq!(package.summary.file_version, 897);
        assert_eq!(package.names.len(), 6);
        assert_eq!(package.imports.len(), 2);
        assert_eq!(package.exports.len(), 2);
        assert_eq!(package.exports[0].object_path, "GageBoss");
        assert_eq!(package.exports[0].payload, vec![0x10, 0x11, 0x12, 0x13]);
        assert_eq!(package.exports[1].object_path, "GageBoss.GageBoss_I1C");
    }

    #[test]
    fn read_file_version_returns_classic_for_x32_fixture() {
        let bytes = build_test_package(CompressionFlavor::None);
        assert_eq!(read_file_version(&bytes), Some(610));
        assert!(!is_x64_file_version(610));
    }

    #[test]
    fn read_file_version_returns_modern_for_x64_fixture() {
        let bytes = super::super::test_fixtures::build_x64_boss_window_test_package(
            [0x10; 4],
            false,
        );
        assert_eq!(read_file_version(&bytes), Some(897));
        assert!(is_x64_file_version(897));
    }

    #[test]
    fn read_file_version_rejects_non_gpk_bytes() {
        assert_eq!(read_file_version(b""), None);
        assert_eq!(read_file_version(b"not a gpk file at all"), None);
    }

    #[test]
    fn compares_boss_window_like_structural_diff() {
        let reference = parse_package(&build_variant_package(TestPackageOptions {
            compression: CompressionFlavor::None,
            export0_payload: [0x10, 0x11, 0x12, 0x13],
            include_redirector_export: true,
        }))
        .expect("parse reference package");
        let modded = parse_package(&build_variant_package(TestPackageOptions {
            compression: CompressionFlavor::None,
            export0_payload: [0x90, 0x91, 0x92, 0x93],
            include_redirector_export: false,
        }))
        .expect("parse modded package");

        let diff = compare_packages(&reference, &modded);

        assert_eq!(diff.import_count_before, 2);
        assert_eq!(diff.import_count_after, 2);
        assert_eq!(diff.export_count_before, 2);
        assert_eq!(diff.export_count_after, 1);
        assert_eq!(diff.changed_exports.len(), 1);
        assert_eq!(diff.changed_exports[0].object_path, "GageBoss");
        assert_eq!(
            diff.removed_exports,
            vec!["GageBoss.GageBoss_I1C".to_string()]
        );
        assert!(diff.added_exports.is_empty());
    }

    fn build_test_package(compression: CompressionFlavor) -> Vec<u8> {
        build_variant_package(TestPackageOptions {
            compression,
            export0_payload: [0x10, 0x11, 0x12, 0x13],
            include_redirector_export: true,
        })
    }

    struct TestPackageOptions {
        compression: CompressionFlavor,
        export0_payload: [u8; 4],
        include_redirector_export: bool,
    }

    fn build_variant_package(options: TestPackageOptions) -> Vec<u8> {
        let names = [
            "Core",
            "GFxMovieInfo",
            "ObjectRedirector",
            "GageBoss",
            "GageBoss_I1C",
            "S1UI_GageBoss",
        ];

        let mut bytes = Vec::new();

        bytes.extend_from_slice(&PACKAGE_MAGIC.to_le_bytes());
        bytes.extend_from_slice(&610u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        let header_size_pos = bytes.len();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        write_fstring_ascii(&mut bytes, "S1UI_GageBoss");
        bytes.extend_from_slice(&0x0000_8000u32.to_le_bytes());
        let raw_name_count_pos = bytes.len();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let name_offset_pos = bytes.len();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        let export_offset_pos = bytes.len();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        let import_offset_pos = bytes.len();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let depends_offset_pos = bytes.len();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 16]);
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&6u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&4206u32.to_le_bytes());
        bytes.extend_from_slice(&76u32.to_le_bytes());
        let compression_pos = bytes.len();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let chunk_count_pos = bytes.len();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let chunk_table_pos = bytes.len();
        let reserves_chunk_table = !matches!(
            options.compression,
            CompressionFlavor::None | CompressionFlavor::Unsupported(_)
        );
        if reserves_chunk_table {
            bytes.extend_from_slice(&[0u8; 16]);
        }

        let header_size = bytes.len() as u32;
        patch_u32(&mut bytes, header_size_pos, header_size);
        patch_u32(&mut bytes, name_offset_pos, header_size);
        patch_u32(
            &mut bytes,
            raw_name_count_pos,
            header_size + names.len() as u32,
        );

        let name_offset = header_size;
        let mut names_blob = Vec::new();
        for name in names {
            write_fstring_ascii(&mut names_blob, name);
            names_blob.extend_from_slice(&0u64.to_le_bytes());
        }

        let mut tail = Vec::new();

        let import_offset = header_size + names_blob_len(&names) as u32;
        patch_u32(&mut bytes, import_offset_pos, import_offset);
        write_import(&mut tail, 0, 1, 0, 1);
        write_import(&mut tail, 0, 2, 0, 2);

        let export_offset = import_offset + 2 * IMPORT_ENTRY_LEN;
        patch_u32(&mut bytes, export_offset_pos, export_offset);
        let export0_serial_size_pos = write_export_header(&mut tail, -1, 0, 0, 3, 4);
        let export1_serial_size_pos = if options.include_redirector_export {
            Some(write_export_header(&mut tail, -2, 0, 1, 4, 4))
        } else {
            None
        };

        let export_count = if options.include_redirector_export {
            2
        } else {
            1
        };
        patch_u32(&mut bytes, export_offset_pos - 4, export_count);

        let depends_offset = export_offset + export_count * EXPORT_ENTRY_LEN_WITH_SERIAL_OFFSET;
        patch_u32(&mut bytes, depends_offset_pos, depends_offset);
        tail.extend_from_slice(&0u32.to_le_bytes());
        if options.include_redirector_export {
            tail.extend_from_slice(&0u32.to_le_bytes());
        }

        let tail_start_offset = import_offset;
        let export0_payload_offset = tail_start_offset + tail.len() as u32;
        tail.extend_from_slice(&options.export0_payload);
        let export1_payload_offset = tail_start_offset + tail.len() as u32;
        if options.include_redirector_export {
            tail.extend_from_slice(&[0x20, 0x21, 0x22, 0x23]);
        }

        patch_u32(
            &mut tail,
            export0_serial_size_pos + 4,
            export0_payload_offset,
        );
        if let Some(export1_serial_size_pos) = export1_serial_size_pos {
            patch_u32(
                &mut tail,
                export1_serial_size_pos + 4,
                export1_payload_offset,
            );
        }

        match options.compression {
            CompressionFlavor::None => {
                bytes.extend_from_slice(&names_blob);
                bytes.extend_from_slice(&tail);
            }
            CompressionFlavor::Zlib => {
                let mut body = names_blob;
                body.extend_from_slice(&tail);
                let compressed = compress_zlib(&body);
                patch_u32(&mut bytes, compression_pos, 1);
                patch_u32(&mut bytes, chunk_count_pos, 1);
                let chunk_total_size = 16 + 8 + compressed.len() as u32;
                let compressed_offset = bytes.len() as u32;
                patch_u32(&mut bytes, chunk_table_pos, name_offset);
                patch_u32(&mut bytes, chunk_table_pos + 4, body.len() as u32);
                patch_u32(&mut bytes, chunk_table_pos + 8, compressed_offset);
                patch_u32(&mut bytes, chunk_table_pos + 12, chunk_total_size);
                bytes.extend_from_slice(&PACKAGE_MAGIC.to_le_bytes());
                bytes.extend_from_slice(&131072u32.to_le_bytes());
                bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&compressed);
            }
            CompressionFlavor::Lzo => {
                let mut body = names_blob;
                body.extend_from_slice(&tail);
                let compressed = lzokay::compress::compress(&body).expect("compress lzo test body");
                patch_u32(&mut bytes, compression_pos, 2);
                patch_u32(&mut bytes, chunk_count_pos, 1);
                let chunk_total_size = 16 + 8 + compressed.len() as u32;
                let compressed_offset = bytes.len() as u32;
                patch_u32(&mut bytes, chunk_table_pos, name_offset);
                patch_u32(&mut bytes, chunk_table_pos + 4, body.len() as u32);
                patch_u32(&mut bytes, chunk_table_pos + 8, compressed_offset);
                patch_u32(&mut bytes, chunk_table_pos + 12, chunk_total_size);
                bytes.extend_from_slice(&PACKAGE_MAGIC.to_le_bytes());
                bytes.extend_from_slice(&131072u32.to_le_bytes());
                bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&compressed);
            }
            CompressionFlavor::Unsupported(flag) => {
                patch_u32(&mut bytes, compression_pos, flag);
                patch_u32(&mut bytes, chunk_count_pos, 0);
                bytes.extend_from_slice(&names_blob);
                bytes.extend_from_slice(&tail);
            }
        }

        let _ = name_offset;
        bytes
    }

    fn compress_zlib(input: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(input).expect("write zlib input");
        encoder.finish().expect("finish zlib compression")
    }

    fn names_blob_len(names: &[&str]) -> usize {
        names.iter().map(|name| 4 + name.len() + 1 + 8).sum()
    }

    fn write_import(
        bytes: &mut Vec<u8>,
        class_package_index: u32,
        class_name_index: u32,
        owner_index: i32,
        object_name_index: u32,
    ) {
        bytes.extend_from_slice(&(class_package_index as u64).to_le_bytes());
        bytes.extend_from_slice(&(class_name_index as u64).to_le_bytes());
        bytes.extend_from_slice(&owner_index.to_le_bytes());
        bytes.extend_from_slice(&(object_name_index as u64).to_le_bytes());
    }

    fn write_export_header(
        bytes: &mut Vec<u8>,
        class_index: i32,
        super_index: i32,
        package_index: i32,
        object_name_index: i32,
        serial_size: u32,
    ) -> usize {
        bytes.extend_from_slice(&class_index.to_le_bytes());
        bytes.extend_from_slice(&super_index.to_le_bytes());
        bytes.extend_from_slice(&package_index.to_le_bytes());
        bytes.extend_from_slice(&object_name_index.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        let serial_size_pos = bytes.len();
        bytes.extend_from_slice(&serial_size.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 16]);
        serial_size_pos
    }

    fn write_fstring_ascii(bytes: &mut Vec<u8>, value: &str) {
        let len = (value.len() + 1) as i32;
        bytes.extend_from_slice(&len.to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
        bytes.push(0);
    }

    fn patch_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}
