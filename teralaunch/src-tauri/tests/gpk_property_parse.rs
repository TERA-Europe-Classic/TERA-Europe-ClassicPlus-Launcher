//! Integration test for the x32/x64 property-block parser.
//!
//! Uses a real x32 (file_version=610) foglio1024 minimap GPK fixture.
//!
//! The `#[path]` modules bring in `gpk_package.rs` and `gpk_property.rs`
//! directly.  `gpk_package.rs` has internal `#[cfg(test)]` blocks that
//! reference `super::super::test_fixtures`; from inside the inlined module
//! `super::super` resolves to this integration-test crate root.  We satisfy
//! that by providing a re-exported `test_fixtures` module here.

#[allow(dead_code)]
#[path = "../src/services/mods/test_fixtures.rs"]
mod test_fixtures;

#[allow(dead_code)]
#[path = "../src/services/mods/gpk_package.rs"]
mod gpk_package;

#[allow(dead_code)]
#[path = "../src/services/mods/gpk_property.rs"]
mod gpk_property;

use gpk_package::parse_package;
use gpk_property::{parse_properties, write_properties, ArchKind, PropertyValue};

/// Uses the foglio1024 minimap x32 GPK (file_version=610) as a real fixture.
/// Each Texture2D export has 9 properties: SizeX, SizeY, Format,
/// MipTailBaseIdx, SRGB, NeverStream, LODGroup, SourceFilePath,
/// SourceFileTimestamp, followed by a "None" terminator.
#[test]
fn parses_x32_minimap_property_block() {
    let gpk_bytes = include_bytes!("fixtures/minimap_x32.gpk");
    let pkg = parse_package(gpk_bytes).expect("parse minimap x32 gpk");

    assert_eq!(pkg.summary.file_version, 610, "fixture must be x32");

    // Find the first Texture2D export.
    let target = pkg
        .exports
        .iter()
        .find(|e| {
            matches!(
                e.class_name.as_deref(),
                Some("Core.Engine.Texture2D") | Some("Core.Texture2D")
            )
        })
        .expect("minimap gpk must have at least one Texture2D export");

    // Strip the 4-byte NetIndex prefix before the property block.
    assert!(
        target.payload.len() >= 4,
        "export payload too small to contain NetIndex"
    );
    let prop_block = &target.payload[4..];

    let props =
        parse_properties(prop_block, ArchKind::X32, &pkg.names).expect("parse x32 properties");

    let last = props.last().expect("at least one property returned");
    assert_eq!(last.name, "None", "property block must terminate with None");

    // 9 real properties + 1 None terminator.
    assert!(
        props.len() >= 2,
        "expected at least one real property + None terminator, got {}",
        props.len()
    );

    // The first property must be SizeX (IntProperty).
    let first = &props[0];
    assert_eq!(first.name, "SizeX");
    assert_eq!(first.type_name, "IntProperty");
    assert!(
        matches!(first.value, PropertyValue::Int(_)),
        "SizeX must parse as Int, got {:?}",
        first.value
    );

    // SRGB and NeverStream are BoolProperty (4 bytes on x32).
    let srgb = props.iter().find(|p| p.name == "SRGB").expect("SRGB prop");
    assert_eq!(srgb.type_name, "BoolProperty");
    assert!(
        matches!(srgb.value, PropertyValue::Bool(_)),
        "SRGB must parse as Bool"
    );

    // Format is ByteProperty with a name-value (size=8 on x32, no enum prefix).
    let format = props
        .iter()
        .find(|p| p.name == "Format")
        .expect("Format prop");
    assert_eq!(format.type_name, "ByteProperty");
    // On x32 there is no enum_type prefix; name_value should be Some(_).
    assert!(
        matches!(
            &format.value,
            PropertyValue::Byte {
                enum_type: None,
                name_value: Some(_),
                ..
            }
        ),
        "x32 ByteProperty must have no enum_type and a name_value, got {:?}",
        format.value
    );

    // SourceFilePath is StrProperty and must not be empty.
    let sfp = props
        .iter()
        .find(|p| p.name == "SourceFilePath")
        .expect("SourceFilePath prop");
    assert_eq!(sfp.type_name, "StrProperty");
    assert!(
        matches!(&sfp.value, PropertyValue::Str(s) if !s.is_empty()),
        "SourceFilePath must be a non-empty string, got {:?}",
        sfp.value
    );
}

/// Byte length of the value portion as it sits on disk (the `size` header field).
/// Mirrors `compute_value_size` in the writer — kept here so the test can
/// calculate how many bytes the parser actually consumed.
fn value_disk_size(value: &PropertyValue) -> usize {
    match value {
        PropertyValue::Int(_) | PropertyValue::Float(_) | PropertyValue::Object(_) => 4,
        PropertyValue::Bool(_) => 0,
        PropertyValue::Byte {
            name_value: Some(_),
            ..
        } => 8,
        PropertyValue::Byte {
            name_value: None, ..
        } => 1,
        PropertyValue::Name(_) => 8,
        PropertyValue::Str(s) => 4 + s.len() + 1,
        PropertyValue::Struct { raw, .. } => raw.len(),
        PropertyValue::Array(raw) => raw.len(),
        PropertyValue::None => 0,
    }
}

/// Extra bytes the parser reads *outside* the `size` header field.
fn value_extra_bytes(value: &PropertyValue, arch: ArchKind) -> usize {
    match value {
        // BoolProperty: on x32 the value is 4 bytes; on x64 it is 1 byte.
        // Neither is accounted for in the `size` field (size == 0 for Bool).
        PropertyValue::Bool(_) => match arch {
            ArchKind::X32 => 4,
            ArchKind::X64 => 1,
        },
        // StructProperty: the 8-byte inner_type name-index prefix sits outside `size`.
        PropertyValue::Struct { .. } => 8,
        // ByteProperty on x64: 8-byte enum_type prefix sits outside `size`.
        PropertyValue::Byte { enum_type: Some(_), .. } => 8,
        _ => 0,
    }
}

#[test]
fn x32_property_block_writes_back_byte_identical() {
    let gpk_bytes = include_bytes!("fixtures/minimap_x32.gpk");
    let pkg = parse_package(gpk_bytes).expect("parse minimap x32 gpk");

    let target = pkg
        .exports
        .iter()
        .find(|e| {
            matches!(
                e.class_name.as_deref(),
                Some("Core.Engine.Texture2D") | Some("Core.Texture2D")
            )
        })
        .expect("find Texture2D export");

    let prop_block = &target.payload[4..]; // strip 4-byte NetIndex
    let props =
        parse_properties(prop_block, ArchKind::X32, &pkg.names).expect("parse x32 properties");

    // Compute the number of bytes the parser consumed for this property block.
    // Non-None properties: 24 bytes header + size field bytes + any extra bytes.
    // None terminator: 8 bytes (just the name index).
    let mut consumed = 8usize; // None terminator
    for p in props.iter().filter(|p| p.name != "None") {
        consumed += 24; // header: i64 name + i64 type + i32 size + i32 array_index
        consumed += value_disk_size(&p.value, );
        consumed += value_extra_bytes(&p.value, ArchKind::X32);
    }
    let original = &prop_block[..consumed];

    let mut out = Vec::with_capacity(original.len());
    write_properties(&props, ArchKind::X32, &pkg.names, &mut out).expect("write properties");

    assert_eq!(
        out.as_slice(),
        original,
        "round-trip must be byte-identical"
    );
}
