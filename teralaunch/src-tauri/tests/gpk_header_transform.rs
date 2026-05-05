//! Integration test for the x32/x64 header serializer.
//!
//! Verifies that `serialize_summary` emits bytes that round-trip back to
//! what the parser read from a real x32 GPK fixture, and that the x64
//! variant correctly inserts the 16-byte ImportExportGuids block.

#[path = "../src/services/mods/test_fixtures.rs"]
mod test_fixtures;

#[path = "../src/services/mods/gpk_package.rs"]
mod gpk_package;

use gpk_package::{parse_package, serialize_summary, ArchKind};

#[test]
fn x32_minimap_header_serializes_with_x64_layout() {
    let x32 = include_bytes!("fixtures/minimap_x32.gpk");
    let pkg = parse_package(x32).expect("parse x32");
    assert_eq!(pkg.summary.file_version, 610);

    let mut x64_header = Vec::new();
    serialize_summary(&pkg.summary, ArchKind::X64, &mut x64_header).expect("serialize x64");

    // FileVersion at offset 4-5 LE must be 897 for x64.
    let fv = u16::from_le_bytes([x64_header[4], x64_header[5]]);
    assert_eq!(fv, 897, "x64 conversion sets FileVersion=897");

    // x64 layout has 16 extra header bytes (ImportExportGuidsOffset etc.) that
    // x32 lacks. Re-serialize as x32 and compare lengths.
    let mut x32_header = Vec::new();
    serialize_summary(&pkg.summary, ArchKind::X32, &mut x32_header).expect("serialize x32");
    assert_eq!(x64_header.len(), x32_header.len() + 16);

    // Both must start with PACKAGE_MAGIC.
    assert_eq!(&x32_header[..4], &0x9E2A83C1u32.to_le_bytes());
    assert_eq!(&x64_header[..4], &0x9E2A83C1u32.to_le_bytes());
}

#[test]
fn x32_round_trip_serialize_summary_byte_identity_for_first_n_bytes() {
    // Parse a real x32 GPK, re-serialize summary at x32 arch, compare against
    // the original file's first len(serialized) bytes. They should match
    // byte-for-byte EXCEPT for the HeaderSize at offset 8-11 (which we leave
    // zero — caller will patch later).
    let x32 = include_bytes!("fixtures/minimap_x32.gpk");
    let pkg = parse_package(x32).expect("parse");
    let mut out = Vec::new();
    serialize_summary(&pkg.summary, ArchKind::X32, &mut out).expect("serialize");

    let n = out.len();
    let mut original = x32[..n].to_vec();
    // Mask out HeaderSize bytes at offset 8-11 in BOTH so the comparison ignores it.
    original[8..12].fill(0);
    assert_eq!(
        out.as_slice(),
        original.as_slice(),
        "x32 round-trip header must byte-match original (minus HeaderSize)"
    );
}
