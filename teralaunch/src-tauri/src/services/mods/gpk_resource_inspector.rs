//! Resource-level inspection for GPK export payloads.
//!
//! This module is intentionally read-only. It exists to prove that x64
//! resource packages can be understood before any tool is allowed to generate
//! replacement `Texture2D` bytes.
//!
//! Most functions are consumed by developer bins, not the main binary, so dead
//! code is expected here.
#![allow(dead_code)]

use super::gpk_package::{GpkExportEntry, GpkNameEntry, GpkPackage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureExportInspection {
    pub object_path: String,
    pub property_count: usize,
    pub native_data_offset: usize,
    pub native_data_size: usize,
    pub source_art_size: i32,
    pub source_file_path: Option<String>,
    pub mip_count: i32,
    pub first_mip: Option<MipInspection>,
    pub cached_mip_count: Option<i32>,
    pub max_cached_resolution: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MipInspection {
    pub flags: u32,
    pub element_count: i32,
    pub size_on_disk: i32,
    pub size_x: i32,
    pub size_y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedirectorExportInspection {
    pub object_path: String,
    pub target_index: i32,
    pub target_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MipBulkLocation {
    pub payload_offset: usize,
    pub payload_len: usize,
    pub offset_in_file_field_offset: usize,
    pub offset_in_file: i32,
}

pub fn first_mip_bulk_location(
    export: &GpkExportEntry,
    names: &[GpkNameEntry],
    is_x64: bool,
) -> Result<MipBulkLocation, String> {
    let native_data_offset = locate_property_terminator(export, names, is_x64)?.native_data_offset;
    locate_first_mip_payload(&export.payload, native_data_offset).map(|mip| MipBulkLocation {
        payload_offset: mip.payload_offset,
        payload_len: mip.payload_len,
        offset_in_file_field_offset: mip.offset_in_file_field_offset,
        offset_in_file: mip.offset_in_file,
    })
}

pub fn texture_bulk_locations(
    export: &GpkExportEntry,
    names: &[GpkNameEntry],
    is_x64: bool,
) -> Result<Vec<MipBulkLocation>, String> {
    let native_data_offset = locate_property_terminator(export, names, is_x64)?.native_data_offset;
    locate_texture_bulk_payloads(&export.payload, native_data_offset, is_x64)
}

pub fn inspect_texture_exports(
    package: &GpkPackage,
) -> Result<Vec<TextureExportInspection>, String> {
    let mut textures = Vec::new();
    let is_x64 = package.summary.file_version >= super::gpk_package::X64_VERSION_THRESHOLD;

    for export in &package.exports {
        if !is_texture_class(export.class_name.as_deref()) {
            continue;
        }
        textures.push(inspect_texture_export(export, &package.names, is_x64)?);
    }

    Ok(textures)
}

fn is_texture_class(class_name: Option<&str>) -> bool {
    matches!(
        class_name,
        Some("Core.Texture2D") | Some("Core.Engine.Texture2D")
    )
}

pub fn inspect_redirector_exports(
    package: &GpkPackage,
) -> Result<Vec<RedirectorExportInspection>, String> {
    let mut redirectors = Vec::new();
    let is_x64 = package.summary.file_version >= super::gpk_package::X64_VERSION_THRESHOLD;

    for export in &package.exports {
        if export.class_name.as_deref() != Some("Core.ObjectRedirector") {
            continue;
        }
        let native_data_offset =
            locate_property_terminator(export, &package.names, is_x64)?.native_data_offset;
        let mut cursor = native_data_offset;
        let target_index = read_i32(&export.payload, &mut cursor).map_err(|e| {
            format!(
                "ObjectRedirector '{}' is missing target object index: {e}",
                export.object_path
            )
        })?;
        redirectors.push(RedirectorExportInspection {
            object_path: export.object_path.clone(),
            target_index,
            target_path: resolve_object_index(package, target_index),
        });
    }

    Ok(redirectors)
}

/// Decode a Texture2D's first-mip pixels along with its dimensions and
/// element count, regardless of any target. Used by extractors that emit
/// fresh GPKs at the source's native dimensions (no in-place replacement).
pub fn decode_first_mip(
    export: &GpkExportEntry,
    names: &[GpkNameEntry],
    is_x64: bool,
) -> Result<(Vec<u8>, i32, i32, i32), String> {
    let native_offset = locate_property_terminator(export, names, is_x64)?.native_data_offset;
    let mip = locate_first_mip_payload(&export.payload, native_offset)?;
    let pixels = decode_mip_pixels(&export.payload, &mip)?;
    Ok((
        pixels,
        mip.inspection.size_x,
        mip.inspection.size_y,
        mip.inspection.element_count,
    ))
}

pub fn replace_texture_first_mip_pixels(
    target_export: &GpkExportEntry,
    target_names: &[GpkNameEntry],
    target_is_x64: bool,
    source_export: &GpkExportEntry,
    source_names: &[GpkNameEntry],
    source_is_x64: bool,
) -> Result<Vec<u8>, String> {
    let target_native_offset =
        locate_property_terminator(target_export, target_names, target_is_x64)?.native_data_offset;
    let source_native_offset =
        locate_property_terminator(source_export, source_names, source_is_x64)?.native_data_offset;
    let target_mip = locate_first_mip_payload(&target_export.payload, target_native_offset)?;
    let source_mip = locate_first_mip_payload(&source_export.payload, source_native_offset)?;

    if target_mip.inspection.flags != 0 {
        return Err(format!(
            "Target texture '{}' first mip uses unsupported bulk flags 0x{:X}; expected uncompressed 0x0",
            target_export.object_path, target_mip.inspection.flags
        ));
    }
    if target_mip.inspection.size_x != source_mip.inspection.size_x
        || target_mip.inspection.size_y != source_mip.inspection.size_y
    {
        return Err(format!(
            "Texture mip dimensions differ: target {}x{}, source {}x{}",
            target_mip.inspection.size_x,
            target_mip.inspection.size_y,
            source_mip.inspection.size_x,
            source_mip.inspection.size_y
        ));
    }
    if target_mip.inspection.element_count != source_mip.inspection.element_count {
        return Err(format!(
            "Texture mip element counts differ: target {}, source {}",
            target_mip.inspection.element_count, source_mip.inspection.element_count
        ));
    }

    let source_pixels = decode_mip_pixels(&source_export.payload, &source_mip)?;
    if source_pixels.len() != target_mip.payload_len {
        return Err(format!(
            "Decoded source mip has {} bytes, target slot has {} bytes",
            source_pixels.len(),
            target_mip.payload_len
        ));
    }

    let mut rewritten = target_export.payload.clone();
    let end = target_mip
        .payload_offset
        .checked_add(target_mip.payload_len)
        .ok_or_else(|| "target mip payload range overflows usize".to_string())?;
    rewritten[target_mip.payload_offset..end].copy_from_slice(&source_pixels);
    Ok(rewritten)
}

fn resolve_object_index(package: &GpkPackage, object_index: i32) -> Option<String> {
    if object_index > 0 {
        package
            .exports
            .get(object_index as usize - 1)
            .map(|export| export.object_path.clone())
    } else if object_index < 0 {
        package
            .imports
            .get((-object_index) as usize - 1)
            .map(|import| import.object_path.clone())
    } else {
        None
    }
}

fn inspect_texture_export(
    export: &GpkExportEntry,
    names: &[GpkNameEntry],
    is_x64: bool,
) -> Result<TextureExportInspection, String> {
    locate_native_data(export, names, is_x64)
}

struct NativeDataLocation {
    property_count: usize,
    native_data_offset: usize,
}

fn locate_property_terminator(
    export: &GpkExportEntry,
    names: &[GpkNameEntry],
    is_x64: bool,
) -> Result<NativeDataLocation, String> {
    let mut cursor = 0usize;
    read_exact(&export.payload, &mut cursor, 4).map_err(|e| {
        format!(
            "Export '{}' is missing NetIndex before properties: {e}",
            export.object_path
        )
    })?;

    let mut property_count = 0usize;
    loop {
        let property_name_index = read_u64(&export.payload, &mut cursor)?;
        let property_name = resolve_name(names, property_name_index)?;
        if property_name == "None" {
            break;
        }

        let property_type_index = read_u64(&export.payload, &mut cursor)?;
        let property_type = resolve_name(names, property_type_index)?;
        let property_size = read_u32(&export.payload, &mut cursor)? as usize;
        let _array_index = read_u32(&export.payload, &mut cursor)?;
        let value_size = texture_property_value_size(property_type, property_size, is_x64)?;
        read_exact(&export.payload, &mut cursor, value_size)?;
        property_count += 1;
    }

    Ok(NativeDataLocation {
        property_count,
        native_data_offset: cursor,
    })
}

fn locate_native_data(
    export: &GpkExportEntry,
    names: &[GpkNameEntry],
    is_x64: bool,
) -> Result<TextureExportInspection, String> {
    let location = locate_property_terminator(export, names, is_x64)?;
    let native = inspect_texture_native(&export.payload[location.native_data_offset..], is_x64)?;

    Ok(TextureExportInspection {
        object_path: export.object_path.clone(),
        property_count: location.property_count,
        native_data_offset: location.native_data_offset,
        native_data_size: export
            .payload
            .len()
            .saturating_sub(location.native_data_offset),
        source_art_size: native.source_art_size,
        source_file_path: native.source_file_path,
        mip_count: native.mip_count,
        first_mip: native.first_mip,
        cached_mip_count: native.cached_mip_count,
        max_cached_resolution: native.max_cached_resolution,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextureNativeInspection {
    source_art_size: i32,
    source_file_path: Option<String>,
    mip_count: i32,
    first_mip: Option<MipInspection>,
    cached_mip_count: Option<i32>,
    max_cached_resolution: Option<i32>,
}

fn inspect_texture_native(bytes: &[u8], is_x64: bool) -> Result<TextureNativeInspection, String> {
    let mut cursor = 0usize;
    let source_art = read_bulk_metadata(bytes, &mut cursor)?;
    skip_embedded_bulk_payload(bytes, &mut cursor, &source_art)?;
    let source_file_path = read_fstring(bytes, &mut cursor)?;
    let mips = read_mip_array_metadata(bytes, &mut cursor)?;

    if !is_x64 {
        return Ok(TextureNativeInspection {
            source_art_size: source_art.element_count,
            source_file_path,
            mip_count: mips.count,
            first_mip: mips.first_mip,
            cached_mip_count: None,
            max_cached_resolution: None,
        });
    }

    read_exact(bytes, &mut cursor, 16)?;
    let cached_mips = read_mip_array_metadata(bytes, &mut cursor)?;
    let max_cached_resolution = read_i32(bytes, &mut cursor)?;

    Ok(TextureNativeInspection {
        source_art_size: source_art.element_count,
        source_file_path,
        mip_count: mips.count,
        first_mip: mips.first_mip,
        cached_mip_count: Some(cached_mips.count),
        max_cached_resolution: Some(max_cached_resolution),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BulkMetadata {
    flags: u32,
    element_count: i32,
    size_on_disk: i32,
    offset_in_file_field_offset: usize,
    offset_in_file: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FirstMipPayload {
    inspection: MipInspection,
    payload_offset: usize,
    payload_len: usize,
    offset_in_file_field_offset: usize,
    offset_in_file: i32,
}

fn locate_first_mip_payload(
    export_payload: &[u8],
    native_data_offset: usize,
) -> Result<FirstMipPayload, String> {
    let mut cursor = native_data_offset;
    let source_art = read_bulk_metadata(export_payload, &mut cursor)?;
    skip_embedded_bulk_payload(export_payload, &mut cursor, &source_art)?;
    let _source_file_path = read_fstring(export_payload, &mut cursor)?;
    let count = read_i32(export_payload, &mut cursor)?;
    if count < 1 {
        return Err("Texture2D has no primary mips to replace".to_string());
    }
    if count > 32 {
        return Err(format!(
            "Texture2D mip count {count} is outside supported range 0..=32"
        ));
    }
    let bulk = read_bulk_metadata(export_payload, &mut cursor)?;
    let payload_offset = cursor;
    let payload_len = embedded_bulk_payload_len(&bulk)?;
    read_exact(export_payload, &mut cursor, payload_len)?;
    let size_x = read_i32(export_payload, &mut cursor)?;
    let size_y = read_i32(export_payload, &mut cursor)?;
    Ok(FirstMipPayload {
        inspection: MipInspection {
            flags: bulk.flags,
            element_count: bulk.element_count,
            size_on_disk: bulk.size_on_disk,
            size_x,
            size_y,
        },
        payload_offset,
        payload_len,
        offset_in_file_field_offset: bulk.offset_in_file_field_offset,
        offset_in_file: bulk.offset_in_file,
    })
}

fn locate_texture_bulk_payloads(
    export_payload: &[u8],
    native_data_offset: usize,
    is_x64: bool,
) -> Result<Vec<MipBulkLocation>, String> {
    let mut cursor = native_data_offset;
    let mut locations = Vec::new();

    let source_art = read_bulk_metadata(export_payload, &mut cursor)?;
    push_bulk_location(&mut locations, &source_art, cursor)?;
    skip_embedded_bulk_payload(export_payload, &mut cursor, &source_art)?;
    let _source_file_path = read_fstring(export_payload, &mut cursor)?;

    read_mip_bulk_locations(export_payload, &mut cursor, &mut locations)?;

    if is_x64 {
        read_exact(export_payload, &mut cursor, 16)?;
        read_mip_bulk_locations(export_payload, &mut cursor, &mut locations)?;
        let _max_cached_resolution = read_i32(export_payload, &mut cursor)?;
    }

    Ok(locations)
}

fn read_mip_bulk_locations(
    bytes: &[u8],
    cursor: &mut usize,
    locations: &mut Vec<MipBulkLocation>,
) -> Result<(), String> {
    let count = read_i32(bytes, cursor)?;
    if !(0..=32).contains(&count) {
        return Err(format!(
            "Texture2D mip count {count} is outside supported range 0..=32"
        ));
    }
    for _ in 0..count {
        let bulk = read_bulk_metadata(bytes, cursor)?;
        push_bulk_location(locations, &bulk, *cursor)?;
        skip_embedded_bulk_payload(bytes, cursor, &bulk)?;
        let _size_x = read_i32(bytes, cursor)?;
        let _size_y = read_i32(bytes, cursor)?;
    }
    Ok(())
}

fn push_bulk_location(
    locations: &mut Vec<MipBulkLocation>,
    bulk: &BulkMetadata,
    payload_offset: usize,
) -> Result<(), String> {
    locations.push(MipBulkLocation {
        payload_offset,
        payload_len: embedded_bulk_payload_len(bulk)?,
        offset_in_file_field_offset: bulk.offset_in_file_field_offset,
        offset_in_file: bulk.offset_in_file,
    });
    Ok(())
}

fn decode_mip_pixels(export_payload: &[u8], mip: &FirstMipPayload) -> Result<Vec<u8>, String> {
    const BULK_COMPRESSED_LZO: u32 = 0x10;
    let end = mip
        .payload_offset
        .checked_add(mip.payload_len)
        .ok_or_else(|| "source mip payload range overflows usize".to_string())?;
    let encoded = export_payload
        .get(mip.payload_offset..end)
        .ok_or_else(|| "source mip payload range is outside export payload".to_string())?;
    match mip.inspection.flags {
        0 => Ok(encoded.to_vec()),
        BULK_COMPRESSED_LZO => {
            decompress_lzo_texture_blocks(encoded, mip.inspection.element_count as usize)
        }
        other => Err(format!(
            "Unsupported source texture mip bulk flags 0x{other:X}; expected 0x0 or 0x10"
        )),
    }
}

fn decompress_lzo_texture_blocks(bytes: &[u8], expected_len: usize) -> Result<Vec<u8>, String> {
    const CHUNK_BLOCK_SIGNATURE: u32 = 0x9E2A83C1;
    let mut cursor = 0usize;
    let signature = read_u32(bytes, &mut cursor)?;
    if signature != CHUNK_BLOCK_SIGNATURE {
        return Err(format!(
            "Texture mip compressed block has invalid signature {signature:08X}"
        ));
    }
    let block_size = read_u32(bytes, &mut cursor)? as usize;
    let _compressed_payload_size = read_u32(bytes, &mut cursor)? as usize;
    let uncompressed_size = read_u32(bytes, &mut cursor)? as usize;
    if uncompressed_size != expected_len {
        return Err(format!(
            "Texture mip compressed block expands to {uncompressed_size} bytes, expected {expected_len}"
        ));
    }
    let block_count = uncompressed_size.div_ceil(block_size.max(1));
    let mut blocks = Vec::with_capacity(block_count);
    for _ in 0..block_count {
        let compressed_size = read_u32(bytes, &mut cursor)? as usize;
        let block_uncompressed_size = read_u32(bytes, &mut cursor)? as usize;
        blocks.push((compressed_size, block_uncompressed_size));
    }

    let mut out = Vec::with_capacity(uncompressed_size);
    for (compressed_size, block_uncompressed_size) in blocks {
        let compressed = read_exact(bytes, &mut cursor, compressed_size)?;
        let mut block = vec![0u8; block_uncompressed_size];
        let written = lzokay::decompress::decompress(compressed, &mut block)
            .map_err(|e| format!("Failed to decompress LZO texture mip block: {e}"))?;
        block.truncate(written);
        if block.len() != block_uncompressed_size {
            return Err(format!(
                "Texture mip LZO block decompressed to {} bytes, expected {}",
                block.len(),
                block_uncompressed_size
            ));
        }
        out.extend_from_slice(&block);
    }
    if out.len() != uncompressed_size {
        return Err(format!(
            "Texture mip decompressed to {} bytes, expected {uncompressed_size}",
            out.len()
        ));
    }
    Ok(out)
}

struct MipArrayInspection {
    count: i32,
    first_mip: Option<MipInspection>,
}

fn read_mip_array_metadata(bytes: &[u8], cursor: &mut usize) -> Result<MipArrayInspection, String> {
    let count = read_i32(bytes, cursor)?;
    if !(0..=32).contains(&count) {
        return Err(format!(
            "Texture2D mip count {count} is outside supported range 0..=32"
        ));
    }
    let mut first_mip = None;
    for _ in 0..count {
        let bulk = read_bulk_metadata(bytes, cursor)?;
        skip_embedded_bulk_payload(bytes, cursor, &bulk)?;
        let size_x = read_i32(bytes, cursor)?;
        let size_y = read_i32(bytes, cursor)?;
        if first_mip.is_none() {
            first_mip = Some(MipInspection {
                flags: bulk.flags,
                element_count: bulk.element_count,
                size_on_disk: bulk.size_on_disk,
                size_x,
                size_y,
            });
        }
    }
    Ok(MipArrayInspection { count, first_mip })
}

fn read_bulk_metadata(bytes: &[u8], cursor: &mut usize) -> Result<BulkMetadata, String> {
    let flags = read_u32(bytes, cursor)?;
    let element_count = read_i32(bytes, cursor)?;
    let size_on_disk = read_i32(bytes, cursor)?;
    let offset_in_file_field_offset = *cursor;
    let offset_in_file = read_i32(bytes, cursor)?;
    if element_count < 0 {
        return Err(format!(
            "Texture2D bulk element count {element_count} is negative"
        ));
    }
    if size_on_disk < -1 {
        return Err(format!(
            "Texture2D bulk size on disk {size_on_disk} is invalid"
        ));
    }
    Ok(BulkMetadata {
        flags,
        element_count,
        size_on_disk,
        offset_in_file_field_offset,
        offset_in_file,
    })
}

fn skip_embedded_bulk_payload(
    bytes: &[u8],
    cursor: &mut usize,
    bulk: &BulkMetadata,
) -> Result<(), String> {
    const BULK_STORE_IN_SEPARATE_FILE: u32 = 0x01;
    if bulk.flags & BULK_STORE_IN_SEPARATE_FILE != 0 {
        return Ok(());
    }
    let payload_len = embedded_bulk_payload_len(bulk)?;
    read_exact(bytes, cursor, payload_len)?;
    Ok(())
}

fn embedded_bulk_payload_len(bulk: &BulkMetadata) -> Result<usize, String> {
    if bulk.size_on_disk >= 0 {
        Ok(bulk.size_on_disk as usize)
    } else if bulk.element_count >= 0 {
        Ok(bulk.element_count as usize)
    } else {
        Err(format!(
            "Texture2D bulk element count {} cannot define an embedded payload length",
            bulk.element_count
        ))
    }
}

fn read_fstring(bytes: &[u8], cursor: &mut usize) -> Result<Option<String>, String> {
    let len = read_i32(bytes, cursor)?;
    if len == 0 {
        return Ok(None);
    }
    if len > 0 {
        let raw = read_exact(bytes, cursor, len as usize)?;
        let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
        return Ok(Some(String::from_utf8_lossy(&raw[..end]).to_string()));
    }
    let chars = len
        .checked_abs()
        .ok_or_else(|| "FString length overflows i32".to_string())? as usize;
    let raw = read_exact(bytes, cursor, chars.saturating_mul(2))?;
    let mut words = Vec::with_capacity(chars);
    for chunk in raw.chunks_exact(2) {
        words.push(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    let end = words
        .iter()
        .position(|word| *word == 0)
        .unwrap_or(words.len());
    Ok(Some(String::from_utf16_lossy(&words[..end])))
}

fn texture_property_value_size(
    property_type: &str,
    property_size: usize,
    is_x64: bool,
) -> Result<usize, String> {
    match property_type {
        "IntProperty" | "FloatProperty" | "ObjectProperty" => Ok(4),
        "BoolProperty" => Ok(if is_x64 { 1 } else { 4 }),
        "NameProperty" => Ok(8),
        "StrProperty" | "ArrayProperty" => Ok(property_size),
        "StructProperty" => Ok(8usize
            .checked_add(property_size)
            .ok_or_else(|| "StructProperty value size overflows usize".to_string())?),
        "ByteProperty" => {
            let byte_value_size = if property_size == 1 { 1 } else { 8 };
            if is_x64 {
                Ok(8 + byte_value_size)
            } else {
                Ok(byte_value_size)
            }
        }
        other => Err(format!(
            "Unsupported Texture2D property type '{other}' while locating native texture data"
        )),
    }
}

fn resolve_name(names: &[GpkNameEntry], index: u64) -> Result<&str, String> {
    let name_index = (index & 0xFFFF_FFFF) as usize;
    names
        .get(name_index)
        .map(|entry| entry.name.as_str())
        .ok_or_else(|| format!("Name index {name_index} is outside name table"))
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, String> {
    let data = read_exact(bytes, cursor, 4)?;
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

fn read_i32(bytes: &[u8], cursor: &mut usize) -> Result<i32, String> {
    let data = read_exact(bytes, cursor, 4)?;
    Ok(i32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

fn read_u64(bytes: &[u8], cursor: &mut usize) -> Result<u64, String> {
    let data = read_exact(bytes, cursor, 8)?;
    Ok(u64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]))
}

fn read_exact<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8], String> {
    let end = cursor
        .checked_add(len)
        .ok_or_else(|| "read offset overflows usize".to_string())?;
    let slice = bytes.get(*cursor..end).ok_or_else(|| {
        format!(
            "read {cursor}..{end} is outside payload of {} bytes",
            bytes.len()
        )
    })?;
    *cursor = end;
    Ok(slice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locates_native_texture_data_after_x64_properties() {
        let names = names(&[
            "None",
            "TextureFileCacheName",
            "NameProperty",
            "TextureFileCacheGuid",
            "StructProperty",
            "Guid",
        ]);
        let mut payload = Vec::new();
        payload.extend_from_slice(&0u32.to_le_bytes());
        write_property_header(&mut payload, 1, 2, 8, 0);
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes());
        write_property_header(&mut payload, 3, 4, 16, 0);
        payload.extend_from_slice(&5u64.to_le_bytes());
        payload.extend_from_slice(&[0xAB; 16]);
        payload.extend_from_slice(&0u64.to_le_bytes());
        let native_offset = payload.len();
        write_empty_x64_texture_native(&mut payload);

        let export = texture_export("S1UIRES_Skin.PaperDoll_HighElf_F", payload);

        let inspection = inspect_texture_export(&export, &names, true).expect("inspect texture");

        assert_eq!(inspection.object_path, "S1UIRES_Skin.PaperDoll_HighElf_F");
        assert_eq!(inspection.property_count, 2);
        assert_eq!(inspection.native_data_offset, native_offset);
        assert_eq!(inspection.native_data_size, 49);
        assert_eq!(inspection.source_art_size, 0);
        assert_eq!(inspection.source_file_path, Some(String::new()));
        assert_eq!(inspection.mip_count, 0);
        assert_eq!(inspection.first_mip, None);
        assert_eq!(inspection.cached_mip_count, Some(0));
        assert_eq!(inspection.max_cached_resolution, Some(0));
    }

    #[test]
    fn rejects_unknown_property_type_before_native_texture_data() {
        let names = names(&["None", "Mystery", "UnsupportedProperty"]);
        let mut payload = Vec::new();
        payload.extend_from_slice(&0u32.to_le_bytes());
        write_property_header(&mut payload, 1, 2, 4, 0);
        payload.extend_from_slice(&[0xEF; 4]);

        let export = texture_export("S1UIRES_Skin.BadTexture", payload);

        let err =
            inspect_texture_export(&export, &names, true).expect_err("unknown type must fail");

        assert!(err.contains("Unsupported Texture2D property type 'UnsupportedProperty'"));
    }

    #[test]
    fn reads_object_redirector_target_after_property_terminator() {
        let names = names(&["None"]);
        let mut payload = Vec::new();
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload.extend_from_slice(&0u64.to_le_bytes());
        payload.extend_from_slice(&(-7i32).to_le_bytes());

        let export = GpkExportEntry {
            class_index: 0,
            super_index: 0,
            package_index: 0,
            object_name: "PaperDoll_HighElf_F".to_string(),
            object_path: "S1UIRES_Skin.PaperDoll_HighElf_F".to_string(),
            class_name: Some("Core.ObjectRedirector".to_string()),
            serial_size: payload.len() as u32,
            serial_offset: Some(0),
            export_flags: 0,
            payload,
            payload_fingerprint: "sha256:test".to_string(),
            unk1: 0,
            unk2: 0,
            unk4: 0,
            guid: [0u8; 16],
            unk_extra_ints: Vec::new(),
        };
        let package = GpkPackage {
            summary: super::super::gpk_package::GpkPackageSummary {
                file_version: 897,
                license_version: 17,
                package_name: "S1UIRES_Skin".to_string(),
                package_flags: 0,
                name_count: 1,
                name_offset: 0,
                export_count: 1,
                export_offset: 0,
                import_count: 0,
                import_offset: 0,
                depends_offset: 0,
                compression_flags: 0,
                guid: [0u8; 16],
                generations: vec![],
                engine_version: 0,
                cooker_version: 0,
                import_export_guids_offset: None,
                import_guids_count: None,
                export_guids_count: None,
                thumbnail_table_offset: None,
            },
            names,
            imports: Vec::new(),
            exports: vec![export],
        };

        let redirectors = inspect_redirector_exports(&package).expect("inspect redirector");

        assert_eq!(redirectors.len(), 1);
        assert_eq!(
            redirectors[0].object_path,
            "S1UIRES_Skin.PaperDoll_HighElf_F"
        );
        assert_eq!(redirectors[0].target_index, -7);
        assert_eq!(redirectors[0].target_path, None);
    }

    #[test]
    fn replaces_uncompressed_x64_mip_pixels_from_lzo_source_without_changing_layout() {
        let original_pixels = vec![0x11; 16];
        let replacement_pixels = vec![0xA5; 16];
        let mut target_payload = Vec::new();
        target_payload.extend_from_slice(&0u32.to_le_bytes());
        target_payload.extend_from_slice(&0u64.to_le_bytes());
        write_x64_texture_native_with_mip(
            &mut target_payload,
            "target.tga",
            0,
            &original_pixels,
            4,
            4,
        );

        let compressed_replacement = lzo_texture_block(&replacement_pixels);
        let mut source_payload = Vec::new();
        source_payload.extend_from_slice(&0u32.to_le_bytes());
        source_payload.extend_from_slice(&0u64.to_le_bytes());
        write_x32_texture_native_with_compressed_mip(
            &mut source_payload,
            "source.tga",
            &compressed_replacement,
            replacement_pixels.len(),
            4,
            4,
        );

        let names = names(&["None"]);
        let target_export = texture_export("Package.PaperDoll_0_0_dup", target_payload.clone());
        let source_export = texture_export("S1UIRES_Skin.PaperDoll_0", source_payload);

        let rewritten = replace_texture_first_mip_pixels(
            &target_export,
            &names,
            true,
            &source_export,
            &names,
            false,
        )
        .expect("replace mip pixels");

        assert_eq!(rewritten.len(), target_payload.len());
        assert_ne!(rewritten, target_payload);
        let export = texture_export("Package.PaperDoll_0_0_dup", rewritten.clone());
        let inspection = inspect_texture_export(&export, &names, true).expect("inspect rewritten");
        assert_eq!(
            inspection.first_mip,
            Some(MipInspection {
                flags: 0,
                element_count: original_pixels.len() as i32,
                size_on_disk: original_pixels.len() as i32,
                size_x: 4,
                size_y: 4,
            })
        );
        assert!(rewritten
            .windows(replacement_pixels.len())
            .any(|window| window == replacement_pixels));
        assert!(!rewritten
            .windows(original_pixels.len())
            .any(|window| window == original_pixels));
    }

    #[test]
    fn locates_first_mip_bulk_offset_field() {
        let names = names(&["None"]);
        let pixels = vec![0x42; 16];
        let mut payload = Vec::new();
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload.extend_from_slice(&0u64.to_le_bytes());
        write_x64_texture_native_with_mip(&mut payload, "target.tga", 0, &pixels, 4, 4);
        let export = texture_export("Package.PaperDoll_0_0_dup", payload);

        let location = first_mip_bulk_location(&export, &names, true).expect("locate bulk");

        assert_eq!(location.payload_len, pixels.len());
        assert_eq!(location.offset_in_file, 0);
        assert!(location.offset_in_file_field_offset < location.payload_offset);
    }

    #[test]
    fn locates_source_art_and_mip_bulk_offset_fields() {
        let names = names(&["None"]);
        let pixels = vec![0x42; 16];
        let mut payload = Vec::new();
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload.extend_from_slice(&0u64.to_le_bytes());
        write_x64_texture_native_with_mip(&mut payload, "target.tga", 0, &pixels, 4, 4);
        let export = texture_export("Package.PaperDoll_0_0_dup", payload);

        let locations = texture_bulk_locations(&export, &names, true).expect("locate all bulk");

        assert_eq!(locations.len(), 2);
        assert_eq!(locations[0].payload_len, 0);
        assert_eq!(locations[1].payload_len, pixels.len());
        assert!(
            locations[0].offset_in_file_field_offset < locations[1].offset_in_file_field_offset
        );
    }

    fn names(values: &[&str]) -> Vec<GpkNameEntry> {
        values
            .iter()
            .map(|name| GpkNameEntry {
                name: (*name).to_string(),
                flags: 0,
            })
            .collect()
    }

    fn texture_export(object_path: &str, payload: Vec<u8>) -> GpkExportEntry {
        GpkExportEntry {
            class_index: 0,
            super_index: 0,
            package_index: 0,
            object_name: object_path
                .rsplit('.')
                .next()
                .unwrap_or(object_path)
                .to_string(),
            object_path: object_path.to_string(),
            class_name: Some("Core.Texture2D".to_string()),
            serial_size: payload.len() as u32,
            serial_offset: Some(0),
            export_flags: 0,
            payload,
            payload_fingerprint: "sha256:test".to_string(),
            unk1: 0,
            unk2: 0,
            unk4: 0,
            guid: [0u8; 16],
            unk_extra_ints: Vec::new(),
        }
    }

    fn write_property_header(
        bytes: &mut Vec<u8>,
        name_index: u64,
        type_index: u64,
        size: u32,
        array_index: u32,
    ) {
        bytes.extend_from_slice(&name_index.to_le_bytes());
        bytes.extend_from_slice(&type_index.to_le_bytes());
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.extend_from_slice(&array_index.to_le_bytes());
    }

    fn write_empty_x64_texture_native(bytes: &mut Vec<u8>) {
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&1i32.to_le_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 16]);
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
    }

    fn write_x64_texture_native_with_mip(
        bytes: &mut Vec<u8>,
        source_file_path: &str,
        flags: u32,
        pixels: &[u8],
        size_x: i32,
        size_y: i32,
    ) {
        write_empty_bulk(bytes);
        write_fstring(bytes, source_file_path);
        bytes.extend_from_slice(&1i32.to_le_bytes());
        write_bulk(bytes, flags, pixels.len() as i32, pixels.len() as i32);
        bytes.extend_from_slice(pixels);
        bytes.extend_from_slice(&size_x.to_le_bytes());
        bytes.extend_from_slice(&size_y.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 16]);
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
    }

    fn write_x32_texture_native_with_compressed_mip(
        bytes: &mut Vec<u8>,
        source_file_path: &str,
        compressed_pixels: &[u8],
        uncompressed_len: usize,
        size_x: i32,
        size_y: i32,
    ) {
        write_empty_bulk(bytes);
        write_fstring(bytes, source_file_path);
        bytes.extend_from_slice(&1i32.to_le_bytes());
        write_bulk(
            bytes,
            0x10,
            uncompressed_len as i32,
            compressed_pixels.len() as i32,
        );
        bytes.extend_from_slice(compressed_pixels);
        bytes.extend_from_slice(&size_x.to_le_bytes());
        bytes.extend_from_slice(&size_y.to_le_bytes());
    }

    fn write_empty_bulk(bytes: &mut Vec<u8>) {
        write_bulk(bytes, 0, 0, 0);
    }

    fn write_bulk(bytes: &mut Vec<u8>, flags: u32, element_count: i32, size_on_disk: i32) {
        bytes.extend_from_slice(&flags.to_le_bytes());
        bytes.extend_from_slice(&element_count.to_le_bytes());
        bytes.extend_from_slice(&size_on_disk.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
    }

    fn write_fstring(bytes: &mut Vec<u8>, value: &str) {
        bytes.extend_from_slice(&((value.len() + 1) as i32).to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
        bytes.push(0);
    }

    fn lzo_texture_block(payload: &[u8]) -> Vec<u8> {
        let compressed = lzokay::compress::compress(payload).expect("compress replacement fixture");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0x9E2A83C1u32.to_le_bytes());
        bytes.extend_from_slice(&131072u32.to_le_bytes());
        bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&compressed);
        bytes
    }
}
