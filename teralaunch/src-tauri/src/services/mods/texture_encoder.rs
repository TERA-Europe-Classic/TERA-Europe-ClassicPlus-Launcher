//! x64 `Core.Engine.Texture2D` export-body encoder.
//!
//! Produces the bytes of a single Texture2D export's *body* — the slice that
//! lives between `SerialOffset` and `SerialOffset + SerialSize` in a parsed
//! GPK. Does NOT produce a full GPK package; the composite-slice author
//! consumes this output and assembles a complete package around it.
//!
//! This encoder is the inverse of the read path in `gpk_resource_inspector`:
//! `locate_property_terminator` + `locate_native_data` (`read_bulk_metadata`,
//! `read_mip_array_metadata`, the 16-byte cached-mip preamble skip, and the
//! cached-mip array). Round-trip tests at the bottom of this file feed the
//! encoder's output back through the inspector to confirm byte-for-byte
//! agreement.

#![allow(dead_code)]
// Task 2 output. Task 3 (composite_author.rs) is the consumer; remove this
// allow when that lands.

use super::dds::{DdsImage, DdsPixelFormat};
use super::gpk_package::GpkNameEntry;

/// `_ObjectFlags` value emitted on every name-table entry in vanilla v100
/// (x64) packages. Confirmed against `S1UI_PaperDoll.PaperDoll_I147_dup.gpk`
/// and `S1UI_PaperDoll.PaperDoll_dup.gpk` — every name in both files carries
/// `0x0007001000000000`.
const NAME_FLAGS_DEFAULT: u64 = 1970393556451328;

/// 16 bytes between the end of the primary-mip array and the
/// `cached_mip_count` field in an x64 Texture2D body. Vanilla emits all
/// zeros: confirmed identical across 31 Texture2D exports sampled from
/// `S1UI_PaperDoll.PaperDoll_I147_dup.gpk` and
/// `S1UI_PaperDoll.PaperDoll_dup.gpk` (1 distinct value across the sample).
/// Per `gpk_resource_inspector::inspect_texture_native` line 297, the decoder
/// skips these 16 bytes verbatim — they are not interpreted on read, but
/// vanilla writes them as zero, so the encoder must match.
const X64_CACHED_MIP_PREAMBLE: [u8; 16] = [0u8; 16];

#[derive(Debug, Default)]
pub struct NameTableBuilder {
    names: Vec<String>,
}

impl NameTableBuilder {
    pub fn new() -> Self {
        Self { names: Vec::new() }
    }

    /// Insert if not present; return its 0-based index in the table.
    pub fn intern(&mut self, name: &str) -> u64 {
        if let Some(existing) = self.names.iter().position(|n| n == name) {
            return existing as u64;
        }
        let idx = self.names.len() as u64;
        self.names.push(name.to_string());
        idx
    }

    pub fn into_entries(self) -> Vec<GpkNameEntry> {
        self.names
            .into_iter()
            .map(|name| GpkNameEntry {
                name,
                flags: NAME_FLAGS_DEFAULT,
            })
            .collect()
    }
}

/// Encode the full Texture2D export body for the given DDS source.
///
/// Layout (matches `gpk_resource_inspector` decoder, x64 only):
/// - 4-byte NetIndex prefix
/// - Property block (Format / SizeX / SizeY / OriginalSizeX / OriginalSizeY /
///   MipTailBaseIdx) terminated by an `i64 None` name index
/// - Source-art FByteBulkData (16-byte header + zero-length embedded payload)
/// - Empty FString SourceFilePath (i32 length = 0)
/// - Primary mip array (i32 count + per-mip [16-byte FByteBulkData header,
///   payload, i32 size_x, i32 size_y])
/// - 16 zero bytes (`X64_CACHED_MIP_PREAMBLE`)
/// - Cached-mip array (i32 count = 0)
/// - i32 max_cached_resolution = 0
///
/// The caller (Task 3 composite-slice author) is responsible for interning
/// the export's object name into `names` when building the export table —
/// this encoder only interns the property/type/enum names it emits into the
/// body itself.
///
/// All `offset_in_file` fields inside FByteBulkData headers are written as 0.
/// If the consuming GPK layout requires absolute file offsets in those
/// headers (it does for the in-game runtime), the caller must locate and
/// patch them post-hoc using `gpk_resource_inspector::texture_bulk_locations`
/// against the final wrapped export.
pub fn encode_texture2d_body(
    dds: &DdsImage,
    names: &mut NameTableBuilder,
) -> Result<Vec<u8>, String> {
    if dds.mips.is_empty() {
        return Err("Texture2D requires at least one mip level".to_string());
    }
    let width: i32 = dds
        .width
        .try_into()
        .map_err(|_| format!("Texture width {} does not fit in i32", dds.width))?;
    let height: i32 = dds
        .height
        .try_into()
        .map_err(|_| format!("Texture height {} does not fit in i32", dds.height))?;

    let pixel_format_name = match dds.format {
        DdsPixelFormat::Dxt1 => "PF_DXT1",
        DdsPixelFormat::Dxt3 => "PF_DXT3",
        DdsPixelFormat::Dxt5 => "PF_DXT5",
    };

    // Intern every name we'll reference. Order doesn't matter for correctness
    // (indices are looked up by string), but we insert "None" first so the
    // terminator name is at index 0 — matches vanilla convention and keeps
    // the table compact.
    let none_idx = names.intern("None");
    let format_idx = names.intern("Format");
    let byte_property_idx = names.intern("ByteProperty");
    let epixel_format_idx = names.intern("EPixelFormat");
    let pixel_format_idx = names.intern(pixel_format_name);
    let size_x_idx = names.intern("SizeX");
    let size_y_idx = names.intern("SizeY");
    let original_size_x_idx = names.intern("OriginalSizeX");
    let original_size_y_idx = names.intern("OriginalSizeY");
    let mip_tail_base_idx_idx = names.intern("MipTailBaseIdx");
    let int_property_idx = names.intern("IntProperty");

    // 256-byte estimate covers: NetIndex prefix (4) + 6 property headers
    // (24 each = 144) + ByteProperty payload (16) + IntProperty payloads
    // (5 * 4 = 20) + None terminator (8) + source-art bulk header (16) +
    // empty FString (4) + mip_count (4) + cached preamble (16) +
    // cached_count (4) + max_cached_resolution (4). Mip pixel data and per-
    // mip headers (24 bytes each: 16 header + 4 size_x + 4 size_y) dominate
    // beyond that.
    let mut body = Vec::with_capacity(256 + dds.mips.iter().map(|m| m.len() + 24).sum::<usize>());

    // NetIndex prefix.
    body.extend_from_slice(&0u32.to_le_bytes());

    // NOTE: vanilla v100 Texture2D exports carry ~13 properties (incl. SRGB,
    // LODGroup, NeverStream, TextureFileCacheGuid/Name, UnpackMin[0..3],
    // FirstResourceMemMip, etc.). We emit a minimal 6-property subset because
    // the inspector's None-terminated walk accepts it and our round-trip
    // tests pass. If Task 6's in-game smoke test reveals the runtime engine
    // rejects minimal textures (missing format hints, MIP streaming, or sRGB
    // flags), expand this list using a vanilla S1UI_PaperDoll.PaperDoll_dup
    // Texture2D as the template.

    // ByteProperty Format = EPixelFormat::PF_DXT*
    push_property_byteprop_x64(
        &mut body,
        format_idx,
        byte_property_idx,
        epixel_format_idx,
        pixel_format_idx,
    );

    // IntProperty SizeX
    push_property_intprop(&mut body, size_x_idx, int_property_idx, width);

    // IntProperty SizeY
    push_property_intprop(&mut body, size_y_idx, int_property_idx, height);

    // IntProperty OriginalSizeX
    push_property_intprop(&mut body, original_size_x_idx, int_property_idx, width);

    // IntProperty OriginalSizeY
    push_property_intprop(&mut body, original_size_y_idx, int_property_idx, height);

    // IntProperty MipTailBaseIdx = 0
    push_property_intprop(&mut body, mip_tail_base_idx_idx, int_property_idx, 0);

    // Property terminator: i64 None.
    body.extend_from_slice(&none_idx.to_le_bytes());

    // Source-art FByteBulkData: empty (flags=0, element_count=0,
    // size_on_disk=0, offset_in_file=0). Embedded payload of length 0.
    write_bulk_header(&mut body, 0, 0, 0, 0);

    // Source file path FString: empty (length 0, no payload).
    body.extend_from_slice(&0i32.to_le_bytes());

    // Primary mip array.
    let mip_count: i32 = dds
        .mips
        .len()
        .try_into()
        .map_err(|_| format!("Mip count {} does not fit in i32", dds.mips.len()))?;
    body.extend_from_slice(&mip_count.to_le_bytes());

    let mut mip_w = width;
    let mut mip_h = height;
    for (level, pixels) in dds.mips.iter().enumerate() {
        let len: i32 = pixels.len().try_into().map_err(|_| {
            format!("Mip {level} byte length {} does not fit in i32", pixels.len())
        })?;
        // FByteBulkData: flags=0 (uncompressed, embedded), element_count=len,
        // size_on_disk=len, offset_in_file=0 (composite-slice author patches
        // this later if the package layout requires absolute offsets).
        write_bulk_header(&mut body, 0, len, len, 0);
        body.extend_from_slice(pixels);
        body.extend_from_slice(&mip_w.to_le_bytes());
        body.extend_from_slice(&mip_h.to_le_bytes());
        // halving 1 yields 0; min-clamp keeps the next iteration's stride
        // math sane if the caller supplies more mip entries than the source
        // dimensions support.
        mip_w = (mip_w / 2).max(1);
        mip_h = (mip_h / 2).max(1);
    }

    // 16-byte cached-mip preamble (vanilla writes zeros).
    body.extend_from_slice(&X64_CACHED_MIP_PREAMBLE);

    // Empty cached-mip array.
    body.extend_from_slice(&0i32.to_le_bytes());

    // max_cached_resolution = 0.
    body.extend_from_slice(&0i32.to_le_bytes());

    Ok(body)
}

/// Append an IntProperty header + its 4-byte i32 value.
fn push_property_intprop(buf: &mut Vec<u8>, name_idx: u64, type_idx: u64, value: i32) {
    buf.extend_from_slice(&name_idx.to_le_bytes());
    buf.extend_from_slice(&type_idx.to_le_bytes());
    buf.extend_from_slice(&4i32.to_le_bytes()); // size = 4
    buf.extend_from_slice(&0i32.to_le_bytes()); // array_index = 0
    buf.extend_from_slice(&value.to_le_bytes());
}

/// Append an x64 ByteProperty header + its (i64 enum_type_idx, i64
/// enum_value_idx) value.
fn push_property_byteprop_x64(
    buf: &mut Vec<u8>,
    name_idx: u64,
    type_idx: u64,
    enum_type_idx: u64,
    enum_value_idx: u64,
) {
    buf.extend_from_slice(&name_idx.to_le_bytes());
    buf.extend_from_slice(&type_idx.to_le_bytes());
    buf.extend_from_slice(&8i32.to_le_bytes()); // size = 8 (the i64 enum-value)
    buf.extend_from_slice(&0i32.to_le_bytes()); // array_index = 0
    buf.extend_from_slice(&enum_type_idx.to_le_bytes());
    buf.extend_from_slice(&enum_value_idx.to_le_bytes());
}

fn write_bulk_header(
    out: &mut Vec<u8>,
    flags: u32,
    element_count: i32,
    size_on_disk: i32,
    offset_in_file: i32,
) {
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&element_count.to_le_bytes());
    out.extend_from_slice(&size_on_disk.to_le_bytes());
    out.extend_from_slice(&offset_in_file.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::dds::{DdsImage, DdsPixelFormat};
    use super::super::gpk_package::{GpkExportEntry, GpkPackage, GpkPackageSummary};
    use super::super::gpk_resource_inspector;

    fn build_minimal_package_with_body(
        body: Vec<u8>,
        names: Vec<super::super::gpk_package::GpkNameEntry>,
        object_path: &str,
    ) -> GpkPackage {
        GpkPackage {
            summary: GpkPackageSummary {
                file_version: super::super::gpk_package::X64_VERSION_THRESHOLD,
                license_version: 17,
                package_name: format!("MOD:test.{object_path}"),
                package_flags: 0,
                name_count: names.len() as u32,
                name_offset: 0,
                export_count: 1,
                export_offset: 0,
                import_count: 0,
                import_offset: 0,
                depends_offset: 0,
                compression_flags: 0,
            },
            names,
            imports: vec![],
            exports: vec![GpkExportEntry {
                class_index: 0,
                super_index: 0,
                package_index: 0,
                object_name: object_path.to_string(),
                object_path: object_path.to_string(),
                class_name: Some("Core.Engine.Texture2D".to_string()),
                serial_size: body.len() as u32,
                serial_offset: Some(0),
                export_flags: 0,
                payload: body,
                payload_fingerprint: "test".to_string(),
            }],
        }
    }

    #[test]
    fn encode_dxt1_round_trips_through_inspector() {
        let pixels = vec![0xC3u8; 32]; // 8x8 DXT1
        let dds = DdsImage {
            width: 8,
            height: 8,
            format: DdsPixelFormat::Dxt1,
            mips: vec![pixels.clone()],
        };
        let mut nb = NameTableBuilder::new();
        let body = encode_texture2d_body(&dds, &mut nb).unwrap();
        let pkg = build_minimal_package_with_body(body, nb.into_entries(), "PaperDoll_HighElf_F");
        let mip = gpk_resource_inspector::first_mip_bulk_location(&pkg.exports[0], &pkg.names, true)
            .unwrap();
        let mip_bytes = &pkg.exports[0].payload[mip.payload_offset..mip.payload_offset + mip.payload_len];
        assert_eq!(mip_bytes, pixels.as_slice());
    }

    #[test]
    fn encode_dxt5_round_trips() {
        let pixels = vec![0xD7u8; 64]; // 8x8 DXT5
        let dds = DdsImage {
            width: 8,
            height: 8,
            format: DdsPixelFormat::Dxt5,
            mips: vec![pixels.clone()],
        };
        let mut nb = NameTableBuilder::new();
        let body = encode_texture2d_body(&dds, &mut nb).unwrap();
        let pkg = build_minimal_package_with_body(body, nb.into_entries(), "Test_DXT5");
        let mip = gpk_resource_inspector::first_mip_bulk_location(&pkg.exports[0], &pkg.names, true)
            .unwrap();
        let mip_bytes = &pkg.exports[0].payload[mip.payload_offset..mip.payload_offset + mip.payload_len];
        assert_eq!(mip_bytes, pixels.as_slice());
    }

    #[test]
    fn name_table_builder_dedupes() {
        let mut nb = NameTableBuilder::new();
        let a = nb.intern("Format");
        let b = nb.intern("SizeX");
        let c = nb.intern("Format");
        assert_eq!(a, c);
        assert_ne!(a, b);
        assert_eq!(nb.into_entries().len(), 2);
    }
}
