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

use gpk_dropin_install::{install_dropin, install_dropin_with_mapper, uninstall_dropin};

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

#[test]
fn install_dropin_with_mapper_registers_logical_paths() {
    use gpk_transform::{transform_x32_to_x64_with, CompressionMode};

    let x32_src = include_bytes!("fixtures/minimap_x32.gpk");
    let payload = transform_x32_to_x64_with(x32_src, CompressionMode::Lzo)
        .expect("transform fixture to x64");

    let game_root = tempfile::TempDir::new().expect("tmpdir");
    let cooked = game_root.path().join("S1Game/CookedPC");
    fs::create_dir_all(&cooked).unwrap();

    // Seed empty-but-valid mapper files so mapper_extend doesn't fail on
    // missing file. Also create .clean variants as the rollback baseline.
    let empty_dat = gpk::encrypt_mapper(b"");
    for name in &[
        "PkgMapper.dat",
        "PkgMapper.clean",
        "CompositePackageMapper.dat",
        "CompositePackageMapper.clean",
    ] {
        fs::write(cooked.join(name), &empty_dat).unwrap();
    }

    let registered = install_dropin_with_mapper(
        game_root.path(),
        "test.dropin.minimap",
        "Minimap_Mod.gpk",
        &payload,
        None,
    )
    .expect("install_dropin_with_mapper ok");

    // The minimap fixture contains Texture2D exports; at least one must be
    // registered.
    assert!(
        !registered.is_empty(),
        "expected at least one registered logical path, got 0"
    );

    // Every registered path must contain a dot (PackageName.ObjectName).
    for path in &registered {
        assert!(path.contains('.'), "logical path missing dot: {path}");
    }

    // The GPK file itself must be present in CookedPC.
    assert!(
        cooked.join("Minimap_Mod.gpk").exists(),
        "dropin file must be written to CookedPC"
    );

    // PkgMapper.dat must contain the registered paths.
    let pm_bytes = fs::read(cooked.join("PkgMapper.dat")).unwrap();
    let pm_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&pm_bytes)).to_string();
    for path in registered.iter().take(3) {
        assert!(
            pm_text.contains(path.as_str()),
            "PkgMapper.dat missing '{path}'; full text: {pm_text}"
        );
    }

    // CompositePackageMapper.dat must contain the composite filename.
    let cm_bytes = fs::read(cooked.join("CompositePackageMapper.dat")).unwrap();
    let cm_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&cm_bytes)).to_string();
    assert!(
        cm_text.contains("Minimap_Mod"),
        "CompositePackageMapper.dat missing 'Minimap_Mod'; full text: {cm_text}"
    );

    // .clean files must remain unchanged (rollback baseline).
    let pmc_bytes = fs::read(cooked.join("PkgMapper.clean")).unwrap();
    let pmc_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&pmc_bytes)).to_string();
    assert!(
        pmc_text.is_empty(),
        "PkgMapper.clean must remain empty (vanilla baseline); got: {pmc_text}"
    );
}

#[test]
fn install_dropin_with_mapper_refuses_unsafe_filename() {
    let game_root = tempfile::TempDir::new().expect("tmpdir");
    let payload = b"fake";
    let err = install_dropin_with_mapper(
        game_root.path(),
        "some.mod",
        "../escape.gpk",
        payload,
        None,
    )
    .expect_err("must refuse unsafe filename");
    assert!(
        err.contains("unsafe target filename"),
        "wrong error: {err}"
    );
}

#[test]
fn install_dropin_with_mapper_fails_on_non_gpk_payload() {
    let game_root = tempfile::TempDir::new().expect("tmpdir");
    let cooked = game_root.path().join("S1Game/CookedPC");
    fs::create_dir_all(&cooked).unwrap();
    let err = install_dropin_with_mapper(
        game_root.path(),
        "some.mod",
        "Fake.gpk",
        b"not a gpk",
        None,
    )
    .expect_err("must fail on invalid GPK payload");
    assert!(
        err.contains("dropin parse"),
        "wrong error message: {err}"
    );
}

#[test]
fn dropin_with_target_object_path_uses_it_as_logical_path() {
    use gpk_transform::{transform_x32_to_x64_with, CompressionMode};
    let x32_src = include_bytes!("fixtures/minimap_x32.gpk");
    let payload = transform_x32_to_x64_with(x32_src, CompressionMode::Lzo)
        .expect("transform fixture to x64");

    let game_root = tempfile::TempDir::new().expect("tmpdir");
    let cooked = game_root.path().join("S1Game/CookedPC");
    fs::create_dir_all(&cooked).unwrap();
    let empty_dat = gpk::encrypt_mapper(b"");
    for name in &[
        "PkgMapper.dat",
        "PkgMapper.clean",
        "CompositePackageMapper.dat",
        "CompositePackageMapper.clean",
    ] {
        fs::write(cooked.join(name), &empty_dat).unwrap();
    }

    // Pick the first interesting export from the minimap fixture to build a
    // target_object_path that the matcher can find.
    let pkg = gpk_package::parse_package(&payload).expect("parse synth payload");
    let first_interesting = pkg
        .exports
        .iter()
        .find(|e| {
            e.class_name.as_deref().map(|c| {
                let base = c.rsplit('.').next().unwrap_or(c);
                matches!(
                    base,
                    "Texture2D"
                        | "StaticMesh"
                        | "SkeletalMesh"
                        | "GFxMovieInfo"
                        | "AnimSet"
                        | "AnimNodeBlendList"
                        | "Material"
                        | "MaterialInstanceConstant"
                        | "PhysicsAsset"
                        | "ParticleSystem"
                        | "SoundCue"
                        | "SoundNodeWave"
                )
            }).unwrap_or(false)
        })
        .expect("at least one interesting export in minimap fixture");

    // Strip _dup suffix if present — target_object_path uses the canonical name.
    let canonical = first_interesting
        .object_name
        .strip_suffix("_dup")
        .unwrap_or(&first_interesting.object_name)
        .to_string();
    let target_object_path = format!("S1UI_Test_Pkg.{canonical}");

    let registered = install_dropin_with_mapper(
        game_root.path(),
        "test.dropin.target_path",
        "Synth_Mod.gpk",
        &payload,
        Some(&target_object_path),
    )
    .expect("install ok");

    // The first registered logical path MUST be exactly target_object_path.
    assert_eq!(
        &registered[0], &target_object_path,
        "primary mapper addition must use target_object_path as its logical_path"
    );

    // The on-disk PkgMapper.dat must contain the override row.
    let pm_bytes = fs::read(cooked.join("PkgMapper.dat")).unwrap();
    let pm_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&pm_bytes)).to_string();
    assert!(
        pm_text.contains(&target_object_path),
        "PkgMapper.dat must register {target_object_path}; full text: {pm_text}"
    );

    // The dropin file must be present in CookedPC.
    assert!(
        cooked.join("Synth_Mod.gpk").exists(),
        "dropin file must be written to CookedPC"
    );
}
