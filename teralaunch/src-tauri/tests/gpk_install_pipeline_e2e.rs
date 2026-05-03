//! End-to-end integration proof: x32 source GPK → x32→x64 transform with LZO
//! compression → drop-in install to a temp game tree → parse_package on the
//! deposited file. If this passes, the launcher's runtime install path for
//! Type-D x32 mods is provably correct.

#[path = "../src/services/mods/test_fixtures.rs"]
mod test_fixtures;

#[allow(dead_code, unused_imports)]
#[path = "../src/services/mods/gpk.rs"]
mod gpk;

#[allow(dead_code, unused_imports)]
#[path = "../src/services/mods/gpk_package.rs"]
mod gpk_package;

#[allow(unused_imports)]
#[path = "../src/services/mods/gpk_property.rs"]
mod gpk_property;

#[allow(unused_imports)]
#[path = "../src/services/mods/gpk_transform.rs"]
mod gpk_transform;

#[allow(dead_code, unused_imports)]
#[path = "../src/services/mods/mapper_extend.rs"]
mod mapper_extend;

#[allow(dead_code, unused_imports)]
#[path = "../src/services/mods/gpk_dropin_install.rs"]
mod gpk_dropin_install;

use std::fs;
use tempfile::TempDir;

use gpk_dropin_install::{install_dropin, uninstall_dropin};
use gpk_package::parse_package;
use gpk_transform::{transform_x32_to_x64_with, CompressionMode};

#[test]
fn x32_source_transforms_compresses_and_drops_in_to_parseable_file() {
    // Step 1: Load an x32 source mod (mimics what the launcher downloads from
    // a foreign catalog URL).
    let x32_source = include_bytes!("fixtures/minimap_x32.gpk");

    // Verify the input is genuinely x32 — sanity check that we're testing the
    // path we think we are.
    let original = parse_package(x32_source).expect("parse source");
    assert_eq!(original.summary.file_version, 610, "fixture must be x32");
    let original_export_count = original.summary.export_count;

    // Step 2: Transform x32 → x64 with LZO compression (what try_deploy_gpk
    // does at install time when deploy_strategy=dropin and source is x32).
    let x64_payload = transform_x32_to_x64_with(x32_source, CompressionMode::Lzo)
        .expect("transformer must succeed on a valid x32 GPK");

    // Step 3: Install via dropin to a temp game tree.
    let game_root = TempDir::new().expect("tmpdir");
    let mod_id = "test.foglio.minimap";
    let target_filename = "Minimap_Mod.gpk";
    install_dropin(game_root.path(), mod_id, target_filename, &x64_payload)
        .expect("dropin install must succeed");

    let deposited = game_root.path().join("S1Game/CookedPC").join(target_filename);
    assert!(deposited.exists(), "deposited file must exist on disk");
    let on_disk = fs::read(&deposited).expect("read deposited");
    assert_eq!(
        on_disk.len(),
        x64_payload.len(),
        "deposited bytes must match transformed payload exactly"
    );
    assert_eq!(
        on_disk, x64_payload,
        "deposited bytes must match transformed payload byte-for-byte"
    );

    // Step 4: Re-parse the deposited file as if the engine were loading it.
    // parse_package exercises the full v100 GPK reader pipeline (header +
    // compression decode + name table + import table + export table +
    // per-export payload extraction), so a successful parse means the file is
    // structurally a valid v100 GPK.
    let reparsed = parse_package(&on_disk).expect("deposited file must parse as x64");
    assert_eq!(
        reparsed.summary.file_version,
        897,
        "deposited file must report x64"
    );
    assert_eq!(
        reparsed.summary.compression_flags,
        2,
        "deposited file must report LZO"
    );
    assert_eq!(
        reparsed.summary.export_count,
        original_export_count,
        "export count must be preserved through transform → install → reparse"
    );

    // Step 5: Spot-check that a real Texture2D export survives the full
    // pipeline with payload intact.
    let tex = reparsed
        .exports
        .iter()
        .find(|e| matches!(e.class_name.as_deref(), Some("Texture2D")))
        .or_else(|| {
            reparsed.exports.iter().find(|e| {
                matches!(
                    e.class_name.as_deref(),
                    Some("Core.Engine.Texture2D") | Some("Core.Texture2D")
                )
            })
        })
        .expect("at least one Texture2D survives");
    assert!(
        !tex.payload.is_empty(),
        "Texture2D payload must be non-empty after the full pipeline"
    );

    // Step 6: Uninstall and verify clean removal.
    uninstall_dropin(game_root.path(), mod_id, target_filename)
        .expect("uninstall must succeed");
    assert!(
        !deposited.exists(),
        "uninstall must remove the deposited file"
    );
}

#[test]
fn dropin_install_already_x64_passthrough_round_trips() {
    // Mirror the artexlib path: the source is already x64. Skip the
    // transformer; dropin-install verbatim. The launcher does this when the
    // input file_version >= 0x381.
    //
    // We don't have a small x64 fixture committed. Synthesize one by
    // transforming the x32 fixture to x64 first, then treat that as the
    // "already-x64 source" and re-install.
    let x32 = include_bytes!("fixtures/minimap_x32.gpk");
    let synthetic_x64 = transform_x32_to_x64_with(x32, CompressionMode::Lzo)
        .expect("synthesize x64 source");

    let game_root = TempDir::new().expect("tmpdir");
    install_dropin(
        game_root.path(),
        "test.synth.x64",
        "Synth_X64.gpk",
        &synthetic_x64,
    )
    .expect("dropin must succeed for already-x64 input");

    let deposited = game_root.path().join("S1Game/CookedPC/Synth_X64.gpk");
    let reparsed =
        parse_package(&fs::read(&deposited).unwrap()).expect("reparse");
    assert_eq!(
        reparsed.summary.file_version,
        897,
        "synthesized x64 must report x64 file version"
    );
    assert_eq!(
        reparsed.summary.compression_flags,
        2,
        "synthesized x64 must report LZO"
    );
}
