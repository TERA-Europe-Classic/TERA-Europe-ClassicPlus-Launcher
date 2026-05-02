//! Integration tests for the drop-in CookedPC install path.
//!
//! Uses the `#[path]` inlining pattern (same as `gpk_property_parse.rs` and
//! `gpk_transform_x32_to_x64.rs`) so the test crate sees the module-private
//! helpers directly without needing a Tauri build.

#[path = "../src/services/mods/test_fixtures.rs"]
mod test_fixtures;

#[allow(dead_code, unused_imports)]
#[path = "../src/services/mods/gpk.rs"]
mod gpk;

#[allow(dead_code, unused_imports)]
#[path = "../src/services/mods/gpk_dropin_install.rs"]
mod gpk_dropin_install;

use gpk_dropin_install::{install_dropin, uninstall_dropin};

use std::fs;

#[test]
fn dropin_writes_gpk_to_cookedpc_and_removes_on_uninstall() {
    let game_root = tempfile::tempdir().expect("tmpdir");
    let mod_id = "artexlib.gray-college-backpack";
    let target_filename = "GucciBackpack.gpk";
    let payload = b"FAKE GPK PAYLOAD";

    install_dropin(game_root.path(), mod_id, target_filename, payload).expect("install ok");

    let installed = game_root.path().join("S1Game/CookedPC").join(target_filename);
    assert!(installed.exists(), "dropin file must land in CookedPC");
    let on_disk = fs::read(&installed).expect("read installed");
    assert_eq!(on_disk, payload);

    uninstall_dropin(game_root.path(), mod_id, target_filename).expect("uninstall ok");
    assert!(!installed.exists(), "uninstall removes the file");
}

#[test]
fn dropin_refuses_to_overwrite_existing_file() {
    let game_root = tempfile::tempdir().expect("tmpdir");
    let target_filename = "PreExisting.gpk";
    let cooked = game_root.path().join("S1Game/CookedPC");
    fs::create_dir_all(&cooked).unwrap();
    fs::write(cooked.join(target_filename), b"vanilla").unwrap();

    let err = install_dropin(game_root.path(), "some.mod", target_filename, b"new")
        .expect_err("must refuse overwrite");
    assert!(err.contains("refusing to overwrite"), "actual error: {err}");
}

#[test]
fn dropin_uninstall_is_idempotent_when_file_missing() {
    let game_root = tempfile::tempdir().expect("tmpdir");
    uninstall_dropin(game_root.path(), "some.mod", "Missing.gpk")
        .expect("missing file is not an error");
}
