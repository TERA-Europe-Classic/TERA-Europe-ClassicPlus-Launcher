//! Integration tests for the composite-redirect install path.
//!
//! Uses the `#[path]` inlining pattern so the test crate sees module-private
//! helpers without needing a Tauri build.

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

use gpk_transform::{transform_x32_to_x64_with, CompressionMode};
use std::fs;
use tempfile::TempDir;

#[test]
fn composite_redirect_deploys_under_target_object_path_tail_filename() {
    // Build a realistic x64 payload from the x32 fixture.
    let x32_src = include_bytes!("fixtures/minimap_x32.gpk");
    let payload = transform_x32_to_x64_with(x32_src, CompressionMode::Lzo)
        .expect("transform fixture to x64");

    let game_root = TempDir::new().expect("tmpdir");
    let cooked = game_root.path().join("S1Game/CookedPC");
    fs::create_dir_all(&cooked).unwrap();

    // Seed a CompositePackageMapper.dat that contains the entry we want to
    // verify gets redirected: ffe86d35_317168d3_ec.Message_I1CF_dup
    let composite_plain =
        b"some_container?ffe86d35_317168d3_ec.Message_I1CF_dup,ffe86d35_317168d3_ec,620986,21615,|!";
    let composite_enc = gpk::encrypt_mapper(composite_plain);
    fs::write(cooked.join("CompositePackageMapper.dat"), &composite_enc).unwrap();
    fs::write(cooked.join("CompositePackageMapper.clean"), &composite_enc).unwrap();

    // PkgMapper files required by ensure_backup / legacy install helpers.
    let empty_mapper = gpk::encrypt_mapper(b"");
    fs::write(cooked.join("PkgMapper.dat"), &empty_mapper).unwrap();
    fs::write(cooked.join("PkgMapper.clean"), &empty_mapper).unwrap();

    // Write the payload to a temp source file.
    let src_path = game_root.path().join("source.gpk");
    fs::write(&src_path, &payload).unwrap();

    // --- function under test ---
    let deployed = gpk::install_composite_redirect(
        game_root.path(),
        &src_path,
        "S1UI_Message.Message_I1CF",
        "test.mod-happy-path",
    )
    .expect("install_composite_redirect should succeed");

    // Assertion 1: returned filename is the _dup-suffixed name.
    assert_eq!(deployed, "Message_I1CF_dup.gpk");

    // Assertion 2: file landed at CookedPC/Message_I1CF_dup.gpk.
    let installed = cooked.join("Message_I1CF_dup.gpk");
    assert!(installed.exists(), "file must be written to CookedPC");
    let installed_size = fs::metadata(&installed).unwrap().len();

    // Assertion 3: CompositePackageMapper now redirects the entry.
    let composite_after = fs::read(cooked.join("CompositePackageMapper.dat")).unwrap();
    let composite_text =
        String::from_utf8_lossy(&gpk::decrypt_mapper(&composite_after)).to_string();
    let row = composite_text
        .split('|')
        .find(|r| r.contains("Message_I1CF_dup"))
        .expect("redirect row must exist in patched mapper");

    assert!(
        row.contains("Message_I1CF_dup.gpk") || row.contains("Message_I1CF_dup,"),
        "redirect must reference our filename; row: {row}"
    );
    assert!(row.contains(",0,"), "redirect offset must be 0; row: {row}");
    assert!(
        row.contains(&format!(",{installed_size},")),
        "redirect size must equal file size {installed_size}; row: {row}"
    );
}

#[test]
fn composite_redirect_does_not_double_suffix_when_tail_already_ends_with_dup() {
    let x32_src = include_bytes!("fixtures/minimap_x32.gpk");
    let payload = transform_x32_to_x64_with(x32_src, CompressionMode::Lzo).unwrap();

    let game_root = TempDir::new().expect("tmpdir");
    let cooked = game_root.path().join("S1Game/CookedPC");
    fs::create_dir_all(&cooked).unwrap();

    // Seed mappers with the _dup target already present.
    let composite_plain =
        b"container?abc.Message_I1CF_dup,abc,0,100,|!";
    let composite_enc = gpk::encrypt_mapper(composite_plain);
    fs::write(cooked.join("CompositePackageMapper.dat"), &composite_enc).unwrap();
    fs::write(cooked.join("CompositePackageMapper.clean"), &composite_enc).unwrap();
    let empty_mapper = gpk::encrypt_mapper(b"");
    fs::write(cooked.join("PkgMapper.dat"), &empty_mapper).unwrap();
    fs::write(cooked.join("PkgMapper.clean"), &empty_mapper).unwrap();

    let src_path = game_root.path().join("source.gpk");
    fs::write(&src_path, &payload).unwrap();

    // Pass a target_object_path whose tail already ends in _dup.
    let deployed = gpk::install_composite_redirect(
        game_root.path(),
        &src_path,
        "S1UI_Message.Message_I1CF_dup",
        "test.mod-no-double-dup",
    )
    .expect("should succeed when tail already has _dup");

    // Must NOT produce Message_I1CF_dup_dup.gpk.
    assert_eq!(deployed, "Message_I1CF_dup.gpk");
    assert!(cooked.join("Message_I1CF_dup.gpk").exists());
}

#[test]
fn composite_redirect_rejects_target_object_path_with_no_tail() {
    let game_root = TempDir::new().expect("tmpdir");
    let src_path = game_root.path().join("source.gpk");
    fs::write(&src_path, b"anything").unwrap();

    let err = gpk::install_composite_redirect(game_root.path(), &src_path, "NoDotsHere", "some.mod")
        .expect_err("must reject target_object_path with no dot");
    assert!(
        err.contains("no tail"),
        "wrong error message: {err}"
    );
}

/// Simulate the corruption scenario: a mod was installed via the old
/// dropin+mapper_extend path (leaving `modres_*` rows in both mapper files),
/// then uninstalled without cleaning those rows, then reinstalled via
/// composite_redirect. The pre-flight cleanup in install_composite_redirect
/// must remove the stale modres rows before writing the redirect.
#[test]
fn idempotent_install_composite_redirect_after_prior_dropin() {
    let x32_src = include_bytes!("fixtures/minimap_x32.gpk");
    let payload = transform_x32_to_x64_with(x32_src, CompressionMode::Lzo)
        .expect("transform fixture to x64");

    let game_root = TempDir::new().expect("tmpdir");
    let cooked = game_root.path().join("S1Game/CookedPC");
    fs::create_dir_all(&cooked).unwrap();

    // .clean files represent the vanilla (uncorrupted) baseline.
    let vanilla_comp_plain =
        b"some_container?ffe86d35_317168d3_ec.Message_I1CF_dup,ffe86d35_317168d3_ec,620986,21615,|!";
    let vanilla_comp_enc = gpk::encrypt_mapper(vanilla_comp_plain);
    fs::write(cooked.join("CompositePackageMapper.clean"), &vanilla_comp_enc).unwrap();

    let vanilla_pkg_plain =
        b"S1UI_Message.Message_I1CF,ffe86d35_317168d3_ec.Message_I1CF_dup|";
    let vanilla_pkg_enc = gpk::encrypt_mapper(vanilla_pkg_plain);
    fs::write(cooked.join("PkgMapper.clean"), &vanilla_pkg_enc).unwrap();

    // .dat files represent the corrupted live state left by the old dropin+mapper_extend path.
    // PkgMapper has the logical path pointing at the modres_ composite uid.
    let corrupted_pkg_plain =
        b"S1UI_Message.Message_I1CF,modres_artexlib_lancer_gigachad_block.Message_I1CF_dup|";
    fs::write(
        cooked.join("PkgMapper.dat"),
        gpk::encrypt_mapper(corrupted_pkg_plain),
    )
    .unwrap();

    // CompositePackageMapper has both the vanilla row AND a stray modres_ row.
    let corrupted_comp_plain = b"some_container?ffe86d35_317168d3_ec.Message_I1CF_dup,ffe86d35_317168d3_ec,620986,21615,|!\
LancerGigaChadBlock?modres_artexlib_lancer_gigachad_block.Message_I1CF_dup,modres_artexlib_lancer_gigachad_block,0,268245,|!";
    fs::write(
        cooked.join("CompositePackageMapper.dat"),
        gpk::encrypt_mapper(corrupted_comp_plain),
    )
    .unwrap();

    let src_path = game_root.path().join("source.gpk");
    fs::write(&src_path, &payload).unwrap();

    let deployed = gpk::install_composite_redirect(
        game_root.path(),
        &src_path,
        "S1UI_Message.Message_I1CF",
        "artexlib.lancer-gigachad-block",
    )
    .expect("install_composite_redirect must succeed even after prior dropin state");

    assert_eq!(deployed, "Message_I1CF_dup.gpk");

    // (1) PkgMapper must no longer point at the modres_ composite uid.
    let pm_after = String::from_utf8_lossy(
        &gpk::decrypt_mapper(&fs::read(cooked.join("PkgMapper.dat")).unwrap()),
    )
    .to_string();
    assert!(
        !pm_after.contains("modres_artexlib_lancer_gigachad_block"),
        "modres_ row must be removed from PkgMapper; got: {pm_after}"
    );

    // (2) CompositePackageMapper must not contain the stray modres_ row.
    let cm_after = String::from_utf8_lossy(
        &gpk::decrypt_mapper(&fs::read(cooked.join("CompositePackageMapper.dat")).unwrap()),
    )
    .to_string();
    assert!(
        !cm_after.contains("modres_artexlib_lancer_gigachad_block"),
        "modres_ row must be removed from CompositePackageMapper; got: {cm_after}"
    );

    // (3) The redirect entry must be present (offset 0 since it was just installed).
    assert!(
        cm_after.contains(",0,"),
        "redirect entry with offset 0 must exist; got: {cm_after}"
    );
    assert!(
        cooked.join("Message_I1CF_dup.gpk").exists(),
        "deployed file must exist in CookedPC"
    );
}
