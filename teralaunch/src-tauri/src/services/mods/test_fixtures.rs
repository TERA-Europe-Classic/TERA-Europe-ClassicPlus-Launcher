//! Shared GPK fixture helpers for unit tests.
//!
//! Centralised so `gpk_patch_applier::tests`, `patch_derivation::tests`, and
//! the per-feature integration tests under `tests/` can build the same
//! synthetic boss-window-shaped package without duplicating the byte-level
//! layout.

#![cfg(test)]
#![allow(dead_code)]

const PACKAGE_MAGIC: u32 = 0x9E2A83C1;
const IMPORT_ENTRY_LEN: u32 = 28;
const EXPORT_ENTRY_LEN_WITH_SERIAL_OFFSET: u32 = 68;

pub fn build_boss_window_test_package(
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
    patch_u32(&mut bytes, raw_name_count_pos, header_size + names.len() as u32);

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

    patch_u32(&mut tail, export0_serial_size_pos + 4, export0_payload_offset);
    if let Some(export1_serial_size_pos) = export1_serial_size_pos {
        patch_u32(&mut tail, export1_serial_size_pos + 4, export1_payload_offset);
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

fn patch_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
