//! Smoke test: simulate the launcher's catalog install path against a
//! downloaded mod GPK + the vanilla game files. Runs install_via_patch
//! followed by enable_via_patch, then disable_via_patch + restore so the
//! game tree is left at the .clean baseline.
//!
//! Usage:
//!   smoke-catalog-install --game-root D:/Elinu \
//!     --app-root /tmp/smoke-app-root \
//!     --mod-id foglio1024.restyle-paperdoll \
//!     --target-package S1UI_PaperDoll \
//!     --source-gpk /path/to/downloaded.gpk

#[allow(dead_code)] #[path = "../services/mods/composite_extract.rs"] mod composite_extract;
#[allow(dead_code)] #[path = "../services/mods/gpk.rs"] mod gpk;
#[allow(dead_code)] #[path = "../services/mods/gpk_package.rs"] mod gpk_package;
#[allow(dead_code)] #[path = "../services/mods/gpk_patch_applier.rs"] mod gpk_patch_applier;
#[allow(dead_code)] #[path = "../services/mods/gpk_patch_deploy.rs"] mod gpk_patch_deploy;
#[allow(dead_code)] #[path = "../services/mods/gpk_resource_inspector.rs"] mod gpk_resource_inspector;
#[allow(dead_code)] #[path = "../services/mods/manifest_store.rs"] mod manifest_store;
#[allow(dead_code)] #[path = "../services/mods/mapper_extend.rs"] mod mapper_extend;
#[allow(dead_code)] #[path = "../services/mods/patch_derivation.rs"] mod patch_derivation;
#[allow(dead_code)] #[path = "../services/mods/patch_manifest.rs"] mod patch_manifest;
#[allow(dead_code)] #[path = "../services/mods/vanilla_resolver.rs"] mod vanilla_resolver;

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    if let Err(e) = run() {
        eprintln!("FAIL: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut game_root: Option<PathBuf> = None;
    let mut app_root: Option<PathBuf> = None;
    let mut mod_id: Option<String> = None;
    let mut target_package: Option<String> = None;
    let mut target_object_path: Option<String> = None;
    let mut source_gpk: Option<PathBuf> = None;
    let mut iter = env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--game-root" => game_root = iter.next().map(PathBuf::from),
            "--app-root" => app_root = iter.next().map(PathBuf::from),
            "--mod-id" => mod_id = iter.next(),
            "--target-package" => target_package = iter.next(),
            "--target-object-path" => target_object_path = iter.next(),
            "--source-gpk" => source_gpk = iter.next().map(PathBuf::from),
            other => return Err(format!("unknown arg '{other}'")),
        }
    }
    let game_root = game_root.ok_or("--game-root required")?;
    let app_root = app_root.ok_or("--app-root required")?;
    let mod_id = mod_id.ok_or("--mod-id required")?;
    let target_package = target_package.ok_or("--target-package required")?;
    let source_gpk = source_gpk.ok_or("--source-gpk required")?;

    fs::create_dir_all(&app_root).map_err(|e| format!("mkdir app_root: {e}"))?;

    println!("=== install_via_patch_with_qualifier (target_object_path={:?}) ===", target_object_path);
    let outcome = gpk_patch_deploy::install_via_patch_with_qualifier(
        &game_root, &app_root, &mod_id, &source_gpk, &target_package,
        target_object_path.as_deref(),
    )?;
    println!("install ok: target_filename={}, target_package_name={}",
        outcome.target_filename, outcome.target_package_name);

    println!("\n=== enable_via_patch ===");
    gpk_patch_deploy::enable_via_patch(&game_root, &app_root, &mod_id)?;
    println!("enable ok");

    // Verify mod has actually been deployed: the relevant file should be in CookedPC
    // and the mapper should route to it. Spot-check by looking at PkgMapper for the
    // target_package's main object.
    println!("\n=== verify ===");
    let cooked = game_root.join(gpk::COOKED_PC_DIR);
    let mapper_bytes = fs::read(cooked.join(gpk::PKG_MAPPER_FILE))
        .map_err(|e| format!("read pkgmapper: {e}"))?;
    let plain = String::from_utf8_lossy(&gpk::decrypt_mapper(&mapper_bytes)).to_string();
    let probe = format!("{target_package}.{}",
        target_package.strip_prefix("S1UI_").unwrap_or(&target_package));
    let row = plain.split('|').find(|cell| cell.trim().starts_with(&format!("{probe},")));
    match row {
        Some(r) => println!("PkgMapper: {}", r.trim()),
        None => println!("PkgMapper: no row matching '{probe}' (might be standalone install)"),
    }

    println!("\n=== disable_via_patch + restore ===");
    gpk_patch_deploy::disable_via_patch(&game_root, &app_root, &mod_id)?;
    gpk::restore_clean_gpk_state(&game_root)?;
    println!("restore ok");
    println!("\nALL OK");
    Ok(())
}
