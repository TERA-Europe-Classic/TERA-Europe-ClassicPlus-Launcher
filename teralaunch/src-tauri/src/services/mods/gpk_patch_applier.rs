// Shared between the main launcher bin and several experimental tooling
// bins via `#[path = ...]` includes; each compilation context exercises
// a different subset, so any single bin sees the rest as "dead".
#![allow(dead_code)]

//! Narrow launcher-side curated patch application.
//!
//! This first implementation slice intentionally supports only the reviewed
//! Boss Window-style shape on classic uncompressed packages:
//!
//! - `replace_export_payload`
//! - `remove_export`
//! - no import patches
//! - no name patches
//! - no export creation
//! - no class changes

use std::collections::{BTreeMap, BTreeSet};

use super::{
    gpk_package,
    patch_manifest::{ExportPatchOperation, PatchManifest},
};

#[derive(Debug, Clone)]
struct HeaderLayout {
    export_count_pos: usize,
    depends_offset_pos: usize,
    export_offset: u32,
    export_count: u32,
    depends_offset: u32,
    compression_flags: u32,
}

#[derive(Debug, Clone)]
struct RawExportLayout {
    start: usize,
    end: usize,
    has_serial_offset: bool,
}

pub fn apply_manifest(package_bytes: &[u8], manifest: &PatchManifest) -> Result<Vec<u8>, String> {
    if !manifest.import_patches.is_empty() {
        return Err(
            "Launcher-side curated GPK patch application does not support import patches yet"
                .into(),
        );
    }
    if !manifest.name_patches.is_empty() {
        return Err(
            "Launcher-side curated GPK patch application does not support name patches yet".into(),
        );
    }

    let package = gpk_package::parse_package(package_bytes)?;
    if package.summary.compression_flags != 0 {
        return Err("Launcher-side curated GPK patch application currently supports only uncompressed classic packages".into());
    }

    let header = parse_header_layout(package_bytes)?;
    if header.export_offset != package.summary.export_offset
        || header.export_count != package.summary.export_count
    {
        return Err(
            "Package header export summary drifted during parse; refusing curated patch apply"
                .into(),
        );
    }

    let raw_exports = parse_raw_export_layouts(
        package_bytes,
        header.export_offset as usize,
        header.export_count as usize,
    )?;
    if raw_exports.len() != package.exports.len() {
        return Err(format!(
            "Raw export layout count {} does not match parsed export count {}",
            raw_exports.len(),
            package.exports.len()
        ));
    }

    let mut requested = BTreeMap::new();
    for export in &manifest.exports {
        if requested
            .insert(export.object_path.clone(), export)
            .is_some()
        {
            return Err(format!(
                "Manifest contains duplicate export patch entries for '{}'",
                export.object_path
            ));
        }
        if matches!(
            export.operation,
            ExportPatchOperation::ReplaceExportClassAndPayload
                | ExportPatchOperation::PatchProperties
        ) {
            return Err(format!(
                "Launcher-side curated GPK patch application does not support {:?} yet",
                export.operation
            ));
        }
    }

    let mut matched = BTreeSet::new();
    let mut kept = Vec::new();
    for (parsed, raw) in package.exports.iter().zip(raw_exports.iter()) {
        let patch = requested.get(&parsed.object_path);
        match patch.map(|p| p.operation) {
            Some(ExportPatchOperation::RemoveExport) => {
                validate_patch_target(parsed, patch.expect("patch exists"))?;
                matched.insert(parsed.object_path.clone());
            }
            Some(ExportPatchOperation::ReplaceExportPayload) => {
                let patch = patch.expect("patch exists");
                validate_patch_target(parsed, patch)?;
                let payload = decode_hex(&patch.replacement_payload_hex)?;
                if payload.is_empty() && parsed.serial_offset.is_some() {
                    return Err(format!(
                        "Replacing '{}' with an empty payload is not supported in the narrow applier slice",
                        parsed.object_path
                    ));
                }
                matched.insert(parsed.object_path.clone());
                kept.push((parsed, raw, payload));
            }
            None => kept.push((parsed, raw, parsed.payload.clone())),
            Some(other) => {
                return Err(format!(
                    "Unsupported export patch operation in narrow applier slice: {:?}",
                    other
                ))
            }
        }
    }

    for object_path in requested.keys() {
        if !matched.contains(object_path)
            && package
                .exports
                .iter()
                .any(|e| &e.object_path == object_path)
        {
            continue;
        }
        if !package
            .exports
            .iter()
            .any(|e| &e.object_path == object_path)
        {
            return Err(format!(
                "Manifest export patch references missing object_path '{}'",
                object_path
            ));
        }
    }

    let export_table_end = raw_exports
        .last()
        .map_or(header.export_offset as usize, |entry| entry.end);
    let first_payload_offset = package
        .exports
        .iter()
        .filter_map(|export| export.serial_offset)
        .map(|offset| offset as usize)
        .min()
        .unwrap_or(package_bytes.len());
    let payload_region_end = package
        .exports
        .iter()
        .filter_map(|export| {
            export
                .serial_offset
                .map(|offset| (offset as usize) + export.serial_size as usize)
        })
        .max()
        .unwrap_or(first_payload_offset);

    if export_table_end > first_payload_offset || payload_region_end > package_bytes.len() {
        return Err("Package layout is not suitable for the narrow curated applier slice".into());
    }

    let prefix = &package_bytes[..header.export_offset as usize];
    let middle = &package_bytes[export_table_end..first_payload_offset];
    let suffix = &package_bytes[payload_region_end..];

    let new_export_table_len: usize = kept.iter().map(|(_, raw, _)| raw.end - raw.start).sum();
    let new_depends_offset = header.export_offset as usize + new_export_table_len;
    let payload_start = new_depends_offset + middle.len();

    let mut export_table = Vec::with_capacity(new_export_table_len);
    let mut payload_bytes = Vec::new();
    let mut current_payload_offset = payload_start;
    for (parsed, raw, payload) in kept {
        let mut entry = package_bytes[raw.start..raw.end].to_vec();
        if payload.is_empty() {
            patch_u32(&mut entry, 32, 0);
        } else {
            if !raw.has_serial_offset {
                return Err(format!(
                    "Export '{}' has no serial_offset slot; the narrow curated applier slice cannot rewrite it safely",
                    parsed.object_path
                ));
            }
            patch_u32(&mut entry, 32, payload.len() as u32);
            patch_u32(&mut entry, 36, current_payload_offset as u32);
            current_payload_offset += payload.len();
        }
        export_table.extend_from_slice(&entry);
        payload_bytes.extend_from_slice(&payload);
    }

    let mut rebuilt = Vec::with_capacity(
        prefix.len() + export_table.len() + middle.len() + payload_bytes.len() + suffix.len(),
    );
    rebuilt.extend_from_slice(prefix);
    rebuilt.extend_from_slice(&export_table);
    rebuilt.extend_from_slice(middle);
    rebuilt.extend_from_slice(&payload_bytes);
    rebuilt.extend_from_slice(suffix);

    patch_u32(
        &mut rebuilt,
        header.export_count_pos,
        (export_table.len() / (raw_exports[0].end - raw_exports[0].start)) as u32,
    );
    patch_u32(
        &mut rebuilt,
        header.depends_offset_pos,
        new_depends_offset as u32,
    );

    let reparsed = gpk_package::parse_package(&rebuilt)?;
    if reparsed.exports.len()
        != manifest
            .exports
            .iter()
            .filter(|patch| !matches!(patch.operation, ExportPatchOperation::RemoveExport))
            .count()
            + package
                .exports
                .iter()
                .filter(|export| !requested.contains_key(&export.object_path))
                .count()
    {
        return Err("Reparsed export count did not match rebuilt export table count".into());
    }
    Ok(rebuilt)
}

fn validate_patch_target(
    parsed: &gpk_package::GpkExportEntry,
    patch: &super::patch_manifest::ExportPatch,
) -> Result<(), String> {
    if let Some(class_name) = &patch.class_name {
        if parsed.class_name.as_deref() != Some(class_name.as_str()) {
            return Err(format!(
                "Export '{}' class mismatch: manifest expects {:?}, package has {:?}",
                parsed.object_path, patch.class_name, parsed.class_name
            ));
        }
    }
    if let Some(target_fingerprint) = &patch.target_export_fingerprint {
        if &parsed.payload_fingerprint != target_fingerprint {
            return Err(format!(
                "Export '{}' fingerprint mismatch: manifest expects {}, package has {}",
                parsed.object_path, target_fingerprint, parsed.payload_fingerprint
            ));
        }
    }
    Ok(())
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if value.len() % 2 != 0 {
        return Err("Hex payload must have even length".into());
    }
    let mut out = Vec::with_capacity(value.len() / 2);
    let bytes = value.as_bytes();
    for idx in (0..bytes.len()).step_by(2) {
        let chunk = std::str::from_utf8(&bytes[idx..idx + 2])
            .map_err(|e| format!("Invalid UTF-8 in hex payload: {e}"))?;
        let byte = u8::from_str_radix(chunk, 16)
            .map_err(|e| format!("Invalid hex payload byte '{chunk}': {e}"))?;
        out.push(byte);
    }
    Ok(out)
}

fn parse_header_layout(bytes: &[u8]) -> Result<HeaderLayout, String> {
    let mut cursor = 0usize;
    let tag = read_u32_le(bytes, &mut cursor)?;
    if tag != 0x9E2A83C1 {
        return Err(format!("GPK package has invalid magic {:08X}", tag));
    }
    let file_version = read_u16_le(bytes, &mut cursor)?;
    let _license_version = read_u16_le(bytes, &mut cursor)?;
    let is_x64 = gpk_package::is_x64_file_version(file_version);
    let _header_size = read_u32_le(bytes, &mut cursor)?;
    let _package_name = read_fstring(bytes, &mut cursor)?;
    let _package_flags = read_u32_le(bytes, &mut cursor)?;
    let _raw_name_count = read_u32_le(bytes, &mut cursor)?;
    let _name_offset = read_u32_le(bytes, &mut cursor)?;
    let export_count_pos = cursor;
    let export_count = read_u32_le(bytes, &mut cursor)?;
    let export_offset = read_u32_le(bytes, &mut cursor)?;
    let _import_count = read_u32_le(bytes, &mut cursor)?;
    let _import_offset = read_u32_le(bytes, &mut cursor)?;
    let depends_offset_pos = cursor;
    let depends_offset = read_u32_le(bytes, &mut cursor)?;
    if is_x64 {
        // Skip ImportExportGuidsOffset + ImportGuidsCount + ExportGuidsCount
        // + ThumbnailTableOffset (16 bytes) before FGuid.
        skip_exact(bytes, &mut cursor, 16)?;
    }
    skip_exact(bytes, &mut cursor, 16)?;
    let generation_count = read_u32_le(bytes, &mut cursor)? as usize;
    skip_exact(bytes, &mut cursor, generation_count.saturating_mul(12))?;
    let _engine_version = read_u32_le(bytes, &mut cursor)?;
    let _cooker_version = read_u32_le(bytes, &mut cursor)?;
    let compression_flags = read_u32_le(bytes, &mut cursor)?;
    Ok(HeaderLayout {
        export_count_pos,
        depends_offset_pos,
        export_offset,
        export_count,
        depends_offset,
        compression_flags,
    })
}

fn parse_raw_export_layouts(
    bytes: &[u8],
    mut offset: usize,
    count: usize,
) -> Result<Vec<RawExportLayout>, String> {
    let mut raw = Vec::with_capacity(count);
    for _ in 0..count {
        let start = offset;
        let _class_index = read_i32_le(bytes, &mut offset)?;
        let _super_index = read_i32_le(bytes, &mut offset)?;
        let _package_index = read_i32_le(bytes, &mut offset)?;
        let _object_name_index = read_i32_le(bytes, &mut offset)?;
        let _unk1 = read_u64_le(bytes, &mut offset)?;
        let _unk2 = read_u64_le(bytes, &mut offset)?;
        let serial_size = read_u32_le(bytes, &mut offset)?;
        let has_serial_offset = serial_size > 0;
        if has_serial_offset {
            let _serial_offset = read_u32_le(bytes, &mut offset)?;
        }
        let _export_flags = read_u32_le(bytes, &mut offset)?;
        let unk_header_count = read_u32_le(bytes, &mut offset)? as usize;
        let _unk4 = read_u32_le(bytes, &mut offset)?;
        skip_exact(bytes, &mut offset, 16)?;
        skip_exact(bytes, &mut offset, unk_header_count.saturating_mul(4))?;
        raw.push(RawExportLayout {
            start,
            end: offset,
            has_serial_offset,
        });
    }
    Ok(raw)
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

fn patch_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[cfg(all(test, feature = "lib-tests"))]
mod tests {
    use super::super::patch_manifest;
    use super::*;

    const PACKAGE_MAGIC: u32 = 0x9E2A83C1;
    const IMPORT_ENTRY_LEN: u32 = 28;
    const EXPORT_ENTRY_LEN_WITH_SERIAL_OFFSET: u32 = 68;

    #[test]
    fn boss_window_slice_applies_payload_replace_and_export_remove() {
        let package_bytes = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], true);
        let parsed = gpk_package::parse_package(&package_bytes).expect("parse source package");
        let manifest = boss_window_manifest(&parsed);

        let rebuilt = apply_manifest(&package_bytes, &manifest).expect("apply manifest");
        let reparsed = gpk_package::parse_package(&rebuilt).expect("parse rebuilt package");

        assert_eq!(reparsed.summary.compression_flags, 0);
        assert_eq!(reparsed.exports.len(), 1);
        assert_eq!(reparsed.imports.len(), 2);
        assert_eq!(reparsed.exports[0].object_path, "GageBoss");
        assert_eq!(reparsed.exports[0].payload, vec![0x90, 0x91, 0x92, 0x93]);
    }

    #[test]
    fn apply_manifest_rejects_import_patches_in_narrow_slice() {
        let package_bytes = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], true);
        let parsed = gpk_package::parse_package(&package_bytes).expect("parse source package");
        let mut manifest = boss_window_manifest(&parsed);
        manifest.import_patches.push(patch_manifest::ImportPatch {
            operation: patch_manifest::ImportPatchOperation::RemoveImport,
            import_path: "Core.Texture2D".into(),
            class_package: None,
            class_name: None,
        });

        let err =
            apply_manifest(&package_bytes, &manifest).expect_err("import patches must fail closed");
        assert!(err.contains("does not support import patches yet"));
    }

    #[test]
    fn apply_manifest_rejects_class_change_in_narrow_slice() {
        let package_bytes = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], true);
        let parsed = gpk_package::parse_package(&package_bytes).expect("parse source package");
        let mut manifest = boss_window_manifest(&parsed);
        manifest.exports[0].operation =
            patch_manifest::ExportPatchOperation::ReplaceExportClassAndPayload;
        manifest.exports[0].new_class_name = Some("Core.Texture2D".into());

        let err =
            apply_manifest(&package_bytes, &manifest).expect_err("class changes must fail closed");
        assert!(err.contains("does not support ReplaceExportClassAndPayload yet"));
    }

    #[test]
    fn x64_package_round_trips_through_applier() {
        // v100.02 boss-window-shaped fixture (FileVersion 897, 16 extra
        // header bytes). parse_header_layout must agree with the parser
        // about where the header ends so the rebuild slice math is correct.
        let package_bytes = super::super::test_fixtures::build_x64_boss_window_test_package(
            [0x10, 0x11, 0x12, 0x13],
            true,
        );
        let parsed = gpk_package::parse_package(&package_bytes).expect("parse x64 source");
        let manifest = boss_window_manifest(&parsed);

        let rebuilt = apply_manifest(&package_bytes, &manifest).expect("apply x64 manifest");
        let reparsed = gpk_package::parse_package(&rebuilt).expect("parse x64 rebuilt");

        assert_eq!(reparsed.summary.file_version, 897);
        assert_eq!(reparsed.exports.len(), 1);
        assert_eq!(reparsed.exports[0].object_path, "GageBoss");
        assert_eq!(reparsed.exports[0].payload, vec![0x90, 0x91, 0x92, 0x93]);
    }

    fn boss_window_manifest(parsed: &gpk_package::GpkPackage) -> patch_manifest::PatchManifest {
        let main = parsed
            .exports
            .iter()
            .find(|export| export.object_path == "GageBoss")
            .expect("main export present");
        let redirector = parsed
            .exports
            .iter()
            .find(|export| export.object_path == "GageBoss.GageBoss_I1C")
            .expect("redirector export present");
        patch_manifest::PatchManifest {
            schema_version: 2,
            mod_id: "foglio1024.ui-remover-boss-window".into(),
            title: "UI Remover: Boss Window".into(),
            target_package: "S1UI_GageBoss.gpk".into(),
            patch_family: patch_manifest::PatchFamily::UiLayout,
            reference: patch_manifest::ReferenceBaseline {
                source_patch_label: "converter-candidate".into(),
                package_fingerprint: "exports:2|imports:2|names:6".into(),
                provenance: None,
            },
            compatibility: patch_manifest::CompatibilityPolicy {
                require_exact_package_fingerprint: true,
                require_all_exports_present: false,
                forbid_name_or_import_expansion: false,
            },
            exports: vec![
                patch_manifest::ExportPatch {
                    object_path: "GageBoss".into(),
                    class_name: Some("Core.GFxMovieInfo".into()),
                    reference_export_fingerprint: main.payload_fingerprint.clone(),
                    target_export_fingerprint: Some(main.payload_fingerprint.clone()),
                    operation: patch_manifest::ExportPatchOperation::ReplaceExportPayload,
                    new_class_name: None,
                    replacement_payload_hex: "90919293".into(),
                },
                patch_manifest::ExportPatch {
                    object_path: "GageBoss.GageBoss_I1C".into(),
                    class_name: Some("Core.ObjectRedirector".into()),
                    reference_export_fingerprint: redirector.payload_fingerprint.clone(),
                    target_export_fingerprint: Some(redirector.payload_fingerprint.clone()),
                    operation: patch_manifest::ExportPatchOperation::RemoveExport,
                    new_class_name: None,
                    replacement_payload_hex: String::new(),
                },
            ],
            import_patches: vec![],
            name_patches: vec![],
            notes: vec!["candidate".into()],
        }
    }

    fn build_boss_window_test_package(
        export0_payload: [u8; 4],
        include_redirector_export: bool,
    ) -> Vec<u8> {
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
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());

        let header_size = bytes.len() as u32;
        patch_u32(&mut bytes, header_size_pos, header_size);
        patch_u32(&mut bytes, name_offset_pos, header_size);
        patch_u32(
            &mut bytes,
            raw_name_count_pos,
            header_size + names.len() as u32,
        );

        let mut names_blob = Vec::new();
        for name in names {
            write_fstring_ascii(&mut names_blob, name);
            names_blob.extend_from_slice(&0u64.to_le_bytes());
        }

        let import_offset = header_size + names_blob_len(&names) as u32;
        patch_u32(&mut bytes, import_offset_pos, import_offset);

        let mut tail = Vec::new();
        write_import(&mut tail, 0, 1, 0, 1);
        write_import(&mut tail, 0, 2, 0, 2);

        let export_offset = import_offset + 2 * IMPORT_ENTRY_LEN;
        patch_u32(&mut bytes, export_offset_pos, export_offset);
        let export0_serial_size_pos = write_export_header(&mut tail, -1, 0, 0, 3, 4);
        let export1_serial_size_pos = if include_redirector_export {
            Some(write_export_header(&mut tail, -2, 0, 1, 4, 4))
        } else {
            None
        };

        let export_count = if include_redirector_export { 2 } else { 1 };
        patch_u32(&mut bytes, export_offset_pos - 4, export_count);

        let depends_offset = export_offset + export_count * EXPORT_ENTRY_LEN_WITH_SERIAL_OFFSET;
        patch_u32(&mut bytes, depends_offset_pos, depends_offset);
        tail.extend_from_slice(&0u32.to_le_bytes());
        if include_redirector_export {
            tail.extend_from_slice(&0u32.to_le_bytes());
        }

        let tail_start_offset = import_offset;
        let export0_payload_offset = tail_start_offset + tail.len() as u32;
        tail.extend_from_slice(&export0_payload);
        let export1_payload_offset = tail_start_offset + tail.len() as u32;
        if include_redirector_export {
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

        bytes.extend_from_slice(&names_blob);
        bytes.extend_from_slice(&tail);
        bytes
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
}
