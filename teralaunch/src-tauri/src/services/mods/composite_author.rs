//! Composite-slice GPK authoring.
//!
//! Produces a single-texture standalone GPK (header + name/import/export/
//! depends tables + export bodies) that mirrors the vanilla v100 composite
//! slice template `S1UI_PaperDoll.PaperDoll_I147_dup.gpk` byte-for-byte
//! enough for `gpk_package::parse_package` to accept it and for
//! `gpk_resource_inspector::first_mip_bulk_location` to locate the
//! texture's mip pixels.
//!
//! Layout (mirroring I147_dup, x64 / FileVersion 897):
//! - Header: magic, file/license version, header_size, FString folder
//!   (`"MOD:" + composite_object_path`), package_flags, name/export/import
//!   table counts and offsets, depends offset, x64-only 16-byte
//!   ImportExportGuids/ThumbnailTable filler, FGuid, generations,
//!   engine/cooker version, compression flags, chunk count, package source,
//!   additional packages to cook count, texture allocations count.
//! - Name table: includes everything `texture_encoder::encode_texture2d_body`
//!   needs plus `None`, `Core`, `Engine`, `Package`, `Class`,
//!   `ObjectReferencer`, `Texture2D`, `ReferencedObjects`, `ArrayProperty`,
//!   the composite prefix, the parent package name, and the texture name.
//! - Imports (5 entries, 28 bytes each):
//!   - 0: Core.Package
//!   - 1: Core.Engine.ObjectReferencer
//!   - 2: Core.Engine.Texture2D
//!   - 3: Core (the literal Core package)
//!   - 4: Engine
//! - Exports (3 entries):
//!   - 0: ObjectReferencer (class=-2, outer=0, name=ObjectReferencer)
//!   - 1: Package wrapper (class=-1, outer=0, name=composite-prefix,
//!     unk_header_count=1, extra=package_flags, guid=zero)
//!   - 2: Texture2D (class=-3, outer=0, name=texture_name)
//! - Depends: 3 zeroed i32s.
//! - Body order: export 0 → export 1 → export 2.
//!
//! After all bytes are written, the FByteBulkData `offset_in_file` fields in
//! the Texture2D body are patched to absolute file offsets via
//! `gpk_resource_inspector::texture_bulk_locations`, matching how the engine
//! locates mip pixels when streaming.

#![allow(dead_code)]
// Task 3 output. Tasks 4-8 (mapper integration, end-to-end install,
// in-game smoke) are the consumers; remove this allow when the install
// pipeline calls `author_composite_slice` directly.

use super::dds::DdsImage;
use super::gpk_package::{is_x64_file_version, X64_VERSION_THRESHOLD};
use super::gpk_resource_inspector;
use super::texture_encoder::{encode_texture2d_body, NameTableBuilder};

const PACKAGE_MAGIC: u32 = 0x9E2A83C1;
const FILE_VERSION: u16 = X64_VERSION_THRESHOLD;
const LICENSE_VERSION: u16 = 17;
/// Engine version stamped in vanilla I147_dup.
const ENGINE_VERSION: u32 = 13249;
/// Cooker version stamped in vanilla I147_dup.
const COOKER_VERSION: u32 = 142;
/// `package_flags` from vanilla I147_dup. Carries the flags the runtime needs
/// to treat this slice as a standalone package (PKG_AllowDownload | PKG_Cooked
/// | PKG_StandaloneExport variants confirmed against the template).
const DEFAULT_PACKAGE_FLAGS: u32 = 0x20880009;
/// `package_source` value from vanilla I147_dup. The engine doesn't validate
/// this against any external value — we copy the template's so the file
/// matches vanilla shape.
const PACKAGE_SOURCE: u32 = 0xF78F931A;
/// Sentinel value for `ImportExportGuidsOffset` used by vanilla I147_dup to
/// indicate "no guid table". (i32 = -1 cast to u32 = 0xFFFFFFFF.)
const NO_GUID_TABLE: u32 = 0xFFFFFFFF;
/// Bytes per import entry (i64 class_pkg + i64 class_name + i32 outer +
/// i64 object_name = 28).
const IMPORT_ENTRY_LEN: usize = 28;
/// Bytes per export entry when serial_size > 0 and unk_header_count = 0
/// (i32×4 + u64×2 + u32×3 [serial_size, serial_offset, export_flags] +
/// u32×2 [unk_header_count, unk4] + 16 guid = 68).
const EXPORT_ENTRY_BASE_LEN: usize = 68;

/// Build a complete single-texture composite GPK.
///
/// The output is a standalone `.gpk` file consumable by
/// `gpk_package::parse_package`. The Texture2D body is produced by
/// `texture_encoder::encode_texture2d_body`; this module wraps it with the
/// surrounding header / name / import / export / depends sections.
pub fn author_composite_slice(
    dds: &DdsImage,
    texture_name: &str,
    parent_package_name: &str,
    composite_object_path: &str,
) -> Result<Vec<u8>, String> {
    if texture_name.is_empty() {
        return Err("texture_name must not be empty".to_string());
    }
    if parent_package_name.is_empty() {
        return Err("parent_package_name must not be empty".to_string());
    }
    let composite_prefix = composite_object_path
        .split_once('.')
        .map(|(prefix, _)| prefix)
        .ok_or_else(|| {
            format!(
                "composite_object_path '{composite_object_path}' must contain a '.' separating prefix from object name"
            )
        })?;
    if composite_prefix.is_empty() {
        return Err(format!(
            "composite_object_path '{composite_object_path}' has empty prefix"
        ));
    }

    // 1. Encode the texture body. This interns its own property names into
    // `name_builder`; we add structural names afterwards.
    let mut name_builder = NameTableBuilder::new();
    let texture_body = encode_texture2d_body(dds, &mut name_builder)?;

    // 2. Intern structural names. The encoder already added "None"; intern
    // again is idempotent.
    let none_idx = name_builder.intern("None");
    let core_idx = name_builder.intern("Core");
    let engine_idx = name_builder.intern("Engine");
    let package_idx = name_builder.intern("Package");
    let class_idx = name_builder.intern("Class");
    let object_referencer_idx = name_builder.intern("ObjectReferencer");
    let texture2d_idx = name_builder.intern("Texture2D");
    let referenced_objects_idx = name_builder.intern("ReferencedObjects");
    let array_property_idx = name_builder.intern("ArrayProperty");
    let composite_prefix_idx = name_builder.intern(composite_prefix);
    // Parent package name interned for downstream tooling that may scan the
    // name table for cross-references (e.g. mapper integration in Task 4).
    // Texture's outer is still 0 — the parent_package_name isn't used in the
    // export hierarchy here, mirroring I147_dup.
    let _parent_package_name_idx = name_builder.intern(parent_package_name);
    let texture_name_idx = name_builder.intern(texture_name);

    let names = name_builder.into_entries();

    // 3. Build import table bytes (5 entries, mirroring I147_dup).
    //    import[0]: Core.Package        (class_pkg=Core, class_name=Class, outer=-4=>import[3]=Core,   object_name=Package)
    //    import[1]: Core.Engine.ObjectReferencer (class_pkg=Core, class_name=Class, outer=-5=>import[4]=Engine, object_name=ObjectReferencer)
    //    import[2]: Core.Engine.Texture2D       (class_pkg=Core, class_name=Class, outer=-5=>import[4]=Engine, object_name=Texture2D)
    //    import[3]: Core                        (class_pkg=Core, class_name=Package, outer=0,                 object_name=Core)
    //    import[4]: Engine                      (class_pkg=Core, class_name=Package, outer=0,                 object_name=Engine)
    let mut import_bytes = Vec::with_capacity(5 * IMPORT_ENTRY_LEN);
    write_import(
        &mut import_bytes,
        core_idx,
        class_idx,
        -4,
        package_idx,
    );
    write_import(
        &mut import_bytes,
        core_idx,
        class_idx,
        -5,
        object_referencer_idx,
    );
    write_import(
        &mut import_bytes,
        core_idx,
        class_idx,
        -5,
        texture2d_idx,
    );
    write_import(&mut import_bytes, core_idx, package_idx, 0, core_idx);
    write_import(&mut import_bytes, core_idx, package_idx, 0, engine_idx);

    // 4. Encode export bodies (texture body already encoded above).
    let object_referencer_body = encode_object_referencer_body(
        referenced_objects_idx,
        array_property_idx,
        none_idx,
        // ObjectReferencer's array references export[0] (self, =1) and
        // export[2] (Texture2D, =3) per vanilla I147_dup convention. Mirror
        // it: count=2, refs=[1, 3].
        &[1, 3],
    );
    let package_wrapper_body = encode_package_wrapper_body(none_idx);

    // 5. Layout pass: compute every section's absolute file offset.
    //
    // Order (matches I147_dup section ordering for tables; bodies follow
    // immediately after depends):
    //   header → name table → import table → export table → depends → bodies
    let header_size = compute_header_size(composite_object_path);
    let name_offset = header_size;

    let mut names_blob = Vec::new();
    for entry in &names {
        write_fstring_ascii(&mut names_blob, &entry.name);
        names_blob.extend_from_slice(&entry.flags.to_le_bytes());
    }

    let import_offset = name_offset + names_blob.len();
    let export_offset = import_offset + import_bytes.len();
    let export_table_size =
        2 * EXPORT_ENTRY_BASE_LEN + (EXPORT_ENTRY_BASE_LEN + 4); // export[1] has unk_header_count=1
    let depends_offset = export_offset + export_table_size;
    let depends_size = 3 * 4;
    let body0_offset = depends_offset + depends_size;
    let body1_offset = body0_offset + object_referencer_body.len();
    let body2_offset = body1_offset + package_wrapper_body.len();
    let total_size = body2_offset + texture_body.len();

    if name_offset > u32::MAX as usize
        || import_offset > u32::MAX as usize
        || export_offset > u32::MAX as usize
        || depends_offset > u32::MAX as usize
        || total_size > u32::MAX as usize
    {
        return Err("composite slice byte offsets do not fit in u32".to_string());
    }

    // 6. Build the byte buffer.
    let mut out = Vec::with_capacity(total_size);

    // Header.
    out.extend_from_slice(&PACKAGE_MAGIC.to_le_bytes());
    out.extend_from_slice(&FILE_VERSION.to_le_bytes());
    out.extend_from_slice(&LICENSE_VERSION.to_le_bytes());
    // header_size: vanilla stamps name_offset + name_table_size here. We do
    // the same for shape-fidelity even though parse_package only treats it
    // as the FString-folder offset boundary.
    let header_size_value = (name_offset + names_blob.len()) as u32;
    out.extend_from_slice(&header_size_value.to_le_bytes());
    write_fstring_ascii(&mut out, &format!("MOD:{}", composite_object_path));
    out.extend_from_slice(&DEFAULT_PACKAGE_FLAGS.to_le_bytes());
    out.extend_from_slice(&(names.len() as u32).to_le_bytes());
    out.extend_from_slice(&(name_offset as u32).to_le_bytes());
    out.extend_from_slice(&3u32.to_le_bytes()); // export_count
    out.extend_from_slice(&(export_offset as u32).to_le_bytes());
    out.extend_from_slice(&5u32.to_le_bytes()); // import_count
    out.extend_from_slice(&(import_offset as u32).to_le_bytes());
    out.extend_from_slice(&(depends_offset as u32).to_le_bytes());
    // x64 16-byte filler (ImportExportGuidsOffset, ImportGuidsCount,
    // ExportGuidsCount, ThumbnailTableOffset). I147_dup uses
    // ImportExportGuidsOffset=-1 (NO_GUID_TABLE) and zero for the other 3.
    debug_assert!(is_x64_file_version(FILE_VERSION));
    out.extend_from_slice(&NO_GUID_TABLE.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    // FGuid (16 bytes). Zeroed — vanilla I147_dup uses a non-zero GUID, but
    // the parser doesn't validate it, and zero is the default for fresh
    // composite slices.
    out.extend_from_slice(&[0u8; 16]);
    // Generations: 1 generation.
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&3u32.to_le_bytes()); // gen[0].export_count
    out.extend_from_slice(&(names.len() as u32).to_le_bytes()); // gen[0].name_count
    out.extend_from_slice(&0u32.to_le_bytes()); // gen[0].net_object_count
    // Engine + cooker versions.
    out.extend_from_slice(&ENGINE_VERSION.to_le_bytes());
    out.extend_from_slice(&COOKER_VERSION.to_le_bytes());
    // Compression: none, zero chunks.
    out.extend_from_slice(&0u32.to_le_bytes()); // compression_flags
    out.extend_from_slice(&0u32.to_le_bytes()); // chunk_count
    // Tail-of-header trio: package_source, additional_packages_to_cook
    // count, texture_allocations count. All zero array-counts in vanilla
    // I147_dup; package_source is the fixed sentinel.
    out.extend_from_slice(&PACKAGE_SOURCE.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // additional_packages_to_cook count = 0
    out.extend_from_slice(&0u32.to_le_bytes()); // texture_allocations count = 0

    debug_assert_eq!(
        out.len(),
        name_offset,
        "header layout produced unexpected name_offset"
    );

    // Name table.
    out.extend_from_slice(&names_blob);

    // Import table.
    debug_assert_eq!(out.len(), import_offset);
    out.extend_from_slice(&import_bytes);

    // Export table.
    debug_assert_eq!(out.len(), export_offset);
    write_export_header(
        &mut out,
        -2, // class_index → import[1] = ObjectReferencer
        0,  // super_index
        0,  // outer
        object_referencer_idx as i32,
        object_referencer_body.len() as u32,
        body0_offset as u32,
        0, // export_flags
        0, // unk_header_count
        &[],
    );
    write_export_header(
        &mut out,
        -1, // class_index → import[0] = Package
        0,
        0,
        composite_prefix_idx as i32,
        package_wrapper_body.len() as u32,
        body1_offset as u32,
        0, // export_flags (vanilla uses 1; not validated by parser)
        1, // unk_header_count = 1 → 4 extra bytes after guid
        &[DEFAULT_PACKAGE_FLAGS],
    );
    write_export_header(
        &mut out,
        -3, // class_index → import[2] = Texture2D
        0,
        0,
        texture_name_idx as i32,
        texture_body.len() as u32,
        body2_offset as u32,
        0,
        0,
        &[],
    );

    // Depends table.
    debug_assert_eq!(out.len(), depends_offset);
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());

    // Export bodies.
    debug_assert_eq!(out.len(), body0_offset);
    out.extend_from_slice(&object_referencer_body);
    debug_assert_eq!(out.len(), body1_offset);
    out.extend_from_slice(&package_wrapper_body);
    debug_assert_eq!(out.len(), body2_offset);
    out.extend_from_slice(&texture_body);
    debug_assert_eq!(out.len(), total_size);

    // 7. Patch FByteBulkData `offset_in_file` fields inside the Texture2D
    // body to hold absolute file offsets. The encoder writes 0 there — at
    // this point we know each mip's payload absolute offset because the
    // body is at `body2_offset` and the inspector's
    // `texture_bulk_locations` returns offsets relative to the body.
    patch_bulk_offsets(&mut out, body2_offset, &names)?;

    Ok(out)
}

/// Encode the body of an ObjectReferencer export.
///
/// Layout (matches I147_dup ObjectReferencer body, 48 bytes for 2 refs):
/// - i32 NetIndex = -1
/// - i64 ReferencedObjects name index
/// - i64 ArrayProperty type index
/// - i32 property size = 4 + 4*refs.len()
/// - i32 array_index = 0
/// - i32 array_count
/// - i32×N (each = export-table 1-based index)
/// - i64 None terminator
fn encode_object_referencer_body(
    referenced_objects_idx: u64,
    array_property_idx: u64,
    none_idx: u64,
    refs: &[i32],
) -> Vec<u8> {
    let array_payload_size = 4 + 4 * refs.len() as u32; // count + entries
    let mut body = Vec::with_capacity(40 + 4 * refs.len());
    body.extend_from_slice(&(-1i32).to_le_bytes()); // NetIndex
    body.extend_from_slice(&referenced_objects_idx.to_le_bytes());
    body.extend_from_slice(&array_property_idx.to_le_bytes());
    body.extend_from_slice(&array_payload_size.to_le_bytes());
    body.extend_from_slice(&0i32.to_le_bytes()); // array_index
    body.extend_from_slice(&(refs.len() as i32).to_le_bytes()); // count
    for r in refs {
        body.extend_from_slice(&r.to_le_bytes());
    }
    body.extend_from_slice(&none_idx.to_le_bytes()); // None terminator
    body
}

/// Encode the body of a Package wrapper export.
///
/// Layout (matches I147_dup Package wrapper body, 12 bytes):
/// - i32 NetIndex = -1
/// - i64 None name index (no properties, just the terminator)
fn encode_package_wrapper_body(none_idx: u64) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    body.extend_from_slice(&(-1i32).to_le_bytes());
    body.extend_from_slice(&none_idx.to_le_bytes());
    body
}

#[allow(clippy::too_many_arguments)]
fn write_export_header(
    out: &mut Vec<u8>,
    class_index: i32,
    super_index: i32,
    outer_index: i32,
    object_name_index: i32,
    serial_size: u32,
    serial_offset: u32,
    export_flags: u32,
    unk_header_count: u32,
    extra_ints: &[u32],
) {
    debug_assert_eq!(unk_header_count as usize, extra_ints.len());
    out.extend_from_slice(&class_index.to_le_bytes());
    out.extend_from_slice(&super_index.to_le_bytes());
    out.extend_from_slice(&outer_index.to_le_bytes());
    out.extend_from_slice(&object_name_index.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes()); // unk1
    out.extend_from_slice(&0u64.to_le_bytes()); // unk2
    out.extend_from_slice(&serial_size.to_le_bytes());
    if serial_size > 0 {
        out.extend_from_slice(&serial_offset.to_le_bytes());
    }
    out.extend_from_slice(&export_flags.to_le_bytes());
    out.extend_from_slice(&unk_header_count.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // unk4
    out.extend_from_slice(&[0u8; 16]); // guid (zero — parser does not validate)
    for v in extra_ints {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn write_import(
    out: &mut Vec<u8>,
    class_package_idx: u64,
    class_name_idx: u64,
    outer_index: i32,
    object_name_idx: u64,
) {
    out.extend_from_slice(&class_package_idx.to_le_bytes());
    out.extend_from_slice(&class_name_idx.to_le_bytes());
    out.extend_from_slice(&outer_index.to_le_bytes());
    out.extend_from_slice(&object_name_idx.to_le_bytes());
}

fn write_fstring_ascii(out: &mut Vec<u8>, value: &str) {
    let len_with_nul = (value.len() + 1) as i32;
    out.extend_from_slice(&len_with_nul.to_le_bytes());
    out.extend_from_slice(value.as_bytes());
    out.push(0);
}

fn compute_header_size(composite_object_path: &str) -> usize {
    // Magic(4) + FileVersion(2) + LicenseVersion(2) + HeaderSize(4) = 12
    // FString folder = 4 length prefix + folder bytes + nul terminator
    // PackageFlags(4) + NameCount(4) + NameOffset(4) + ExportCount(4) +
    // ExportOffset(4) + ImportCount(4) + ImportOffset(4) + DependsOffset(4)
    // = 32
    // x64 16-byte filler
    // FGuid(16)
    // GenCount(4) + 1 generation × 12 bytes = 16
    // EngineVersion(4) + CookerVersion(4) = 8
    // CompressionFlags(4) + ChunkCount(4) = 8
    // PackageSource(4) + AdditionalPkgsCount(4) + TextureAllocsCount(4) = 12
    let folder_len = 4 + ("MOD:".len() + composite_object_path.len() + 1);
    12 + folder_len + 32 + 16 + 16 + 16 + 8 + 8 + 12
}

/// Patch every FByteBulkData `offset_in_file` field in the Texture2D body so
/// it holds the absolute file offset of that bulk's payload. Vanilla x64
/// packages stamp these as the runtime-streaming target; even though the
/// inspector's first_mip_bulk_location does not validate them, downstream
/// engine-side mip streaming depends on accurate offsets. The encoder writes
/// 0 by default; we patch them here once we know the body's absolute offset.
fn patch_bulk_offsets(
    out: &mut [u8],
    body_offset: usize,
    names: &[super::gpk_package::GpkNameEntry],
) -> Result<(), String> {
    use super::gpk_package::GpkExportEntry;
    let body_len = out.len() - body_offset;
    let body = out[body_offset..].to_vec();
    let temp_export = GpkExportEntry {
        class_index: 0,
        super_index: 0,
        package_index: 0,
        object_name: String::new(),
        object_path: String::new(),
        class_name: Some("Core.Engine.Texture2D".to_string()),
        serial_size: body_len as u32,
        serial_offset: Some(body_offset as u32),
        export_flags: 0,
        payload: body,
        payload_fingerprint: String::new(),
    };
    let locations = gpk_resource_inspector::texture_bulk_locations(&temp_export, names, true)
        .map_err(|e| format!("locating texture bulk fields for offset patching: {e}"))?;
    for loc in locations {
        let abs_offset = body_offset + loc.payload_offset;
        if abs_offset > i32::MAX as usize {
            return Err("absolute mip offset does not fit in i32".to_string());
        }
        let field_abs = body_offset + loc.offset_in_file_field_offset;
        out[field_abs..field_abs + 4]
            .copy_from_slice(&(abs_offset as i32).to_le_bytes());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authors_a_single_texture_composite_slice() {
        use super::super::dds::DdsPixelFormat;
        use super::super::{gpk_package, gpk_resource_inspector};
        let pixels = vec![0xD7u8; 64];
        let dds = super::super::dds::DdsImage {
            width: 8,
            height: 8,
            format: DdsPixelFormat::Dxt5,
            mips: vec![pixels.clone()],
        };
        let bytes = author_composite_slice(
            &dds,
            "PaperDoll_HighElf_F",
            "S1UIRES_Skin",
            "modres_a1b2c3d4_1.PaperDoll_HighElf_F_dup",
        )
        .expect("author slice");

        let pkg = gpk_package::parse_package(&bytes).expect("re-parse");
        assert_eq!(pkg.summary.file_version, 897);
        assert!(pkg.summary.package_name.starts_with("MOD:"));
        assert!(pkg.exports.iter().any(|e| matches!(
            e.class_name.as_deref(),
            Some("Core.Engine.Texture2D") | Some("Core.Texture2D")
        )));

        let tex = pkg
            .exports
            .iter()
            .find(|e| e.object_path.ends_with("PaperDoll_HighElf_F"))
            .expect("texture export");
        let mip = gpk_resource_inspector::first_mip_bulk_location(tex, &pkg.names, true)
            .expect("locate mip");
        let mip_bytes = &tex.payload[mip.payload_offset..mip.payload_offset + mip.payload_len];
        assert_eq!(mip_bytes, pixels.as_slice());
    }

    #[test]
    fn authored_slice_with_dxt1_round_trips() {
        use super::super::dds::DdsPixelFormat;
        use super::super::{gpk_package, gpk_resource_inspector};
        let pixels = vec![0xC3u8; 32];
        let dds = super::super::dds::DdsImage {
            width: 8,
            height: 8,
            format: DdsPixelFormat::Dxt1,
            mips: vec![pixels.clone()],
        };
        let bytes = author_composite_slice(
            &dds,
            "Test_Tex",
            "S1UIRES_Skin",
            "modres_test_1.Test_Tex_dup",
        )
        .expect("author");
        let pkg = gpk_package::parse_package(&bytes).expect("re-parse");
        let tex = pkg
            .exports
            .iter()
            .find(|e| e.object_path.ends_with("Test_Tex"))
            .expect("tex");
        let mip = gpk_resource_inspector::first_mip_bulk_location(tex, &pkg.names, true)
            .expect("mip");
        let mip_bytes = &tex.payload[mip.payload_offset..mip.payload_offset + mip.payload_len];
        assert_eq!(mip_bytes, pixels.as_slice());
    }
}
