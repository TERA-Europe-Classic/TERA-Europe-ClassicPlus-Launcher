//! Integration test for the x32→x64 GPK transformer.
//!
//! Verifies that `transform_x32_to_x64` produces a file that `parse_package`
//! accepts as x64, with the same export/import/name counts as the original.

#[path = "../src/services/mods/test_fixtures.rs"]
mod test_fixtures;

#[path = "../src/services/mods/gpk_package.rs"]
mod gpk_package;

#[allow(unused_imports)]
#[path = "../src/services/mods/gpk_property.rs"]
mod gpk_property;

#[allow(unused_imports)]
#[path = "../src/services/mods/gpk_transform.rs"]
mod gpk_transform;

use gpk_package::parse_package;
use gpk_property::{parse_properties, ArchKind};
use gpk_transform::{transform_x32_to_x64, transform_x32_to_x64_with, CompressionMode};

#[test]
fn x32_minimap_transforms_to_parseable_x64() {
    let x32 = include_bytes!("fixtures/minimap_x32.gpk");
    let x64 = transform_x32_to_x64(x32).expect("transform");

    let pkg = parse_package(&x64).expect("parse transformed");
    assert_eq!(pkg.summary.file_version, 897, "must produce x64 file version");

    let original = parse_package(x32).expect("parse original");
    assert_eq!(
        pkg.summary.export_count,
        original.summary.export_count,
        "export count must be preserved"
    );
    assert_eq!(
        pkg.summary.name_count,
        original.summary.name_count,
        "name count must be preserved"
    );
    assert_eq!(
        pkg.summary.import_count,
        original.summary.import_count,
        "import count must be preserved"
    );

    // Spot-check: first Texture2D export should have a parseable x64 property block.
    let first_tex = pkg
        .exports
        .iter()
        .find(|e| {
            matches!(
                e.class_name.as_deref(),
                Some("Core.Engine.Texture2D") | Some("Core.Texture2D")
            )
        })
        .expect("at least one Texture2D survives");
    let prop_block = &first_tex.payload[4..];
    let props = parse_properties(prop_block, ArchKind::X64, &pkg.names)
        .expect("parse transformed property block as x64");
    assert!(
        props.iter().any(|p| p.name == "None"),
        "must terminate with None"
    );
}

#[test]
fn x32_minimap_transforms_to_lzo_compressed_x64_round_trips_on_parse() {
    let x32 = include_bytes!("fixtures/minimap_x32.gpk");
    let x64_lzo =
        transform_x32_to_x64_with(x32, CompressionMode::Lzo).expect("transform lzo");

    // The compressed output must be parseable and report LZO compression.
    let pkg = parse_package(&x64_lzo).expect("parse compressed transformed");
    assert_eq!(pkg.summary.file_version, 897, "must produce x64 file version");
    assert_eq!(pkg.summary.compression_flags, 2, "must report LZO");

    // Same export/import/name counts as original input.
    let original = parse_package(x32).expect("parse original");
    assert_eq!(
        pkg.summary.export_count,
        original.summary.export_count,
        "export count must be preserved"
    );
    assert_eq!(
        pkg.summary.name_count,
        original.summary.name_count,
        "name count must be preserved"
    );

    // Spot-check: Texture2D export survives parse-after-decompress.
    let first_tex = pkg
        .exports
        .iter()
        .find(|e| matches!(e.class_name.as_deref(), Some("Texture2D")))
        .or_else(|| {
            pkg.exports.iter().find(|e| {
                matches!(
                    e.class_name.as_deref(),
                    Some("Core.Engine.Texture2D") | Some("Core.Texture2D")
                )
            })
        })
        .expect("Texture2D survives");
    assert!(!first_tex.payload.is_empty(), "payload non-empty after roundtrip");
}
