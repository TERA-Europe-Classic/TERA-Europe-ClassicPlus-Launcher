// install-loose-dds — author a fresh composite slice from a single loose
// DDS file, route it via PkgMapper REPLACE-by-logical-path. Used for
// per-texture targeted overrides (e.g. shared SlotComponent atlases).
//
// Usage:
//   install-loose-dds --dds <path> --logical <package.texture_name>
//                     --uid-prefix <prefix> --filename-prefix <prefix>
//                     --game-root <path> --staging <dir>

use std::env;
use std::fs;
use std::path::PathBuf;

#[path = "../services/mods/dds.rs"] mod dds;
#[path = "../services/mods/gpk.rs"] mod gpk;
#[path = "../services/mods/gpk_package.rs"] mod gpk_package;
#[path = "../services/mods/gpk_resource_inspector.rs"] mod gpk_resource_inspector;
#[path = "../services/mods/texture_encoder.rs"] mod texture_encoder;
#[path = "../services/mods/composite_author.rs"] mod composite_author;
#[path = "../services/mods/mapper_extend.rs"] mod mapper_extend;

const USAGE: &str = "install-loose-dds --dds <path> --logical <package.texture> --uid-prefix <p> \
    --filename-prefix <p> --game-root <path> --staging <dir>";

fn main() {
    if let Err(e) = run() {
        eprintln!("FAIL: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut dds_path: Option<PathBuf> = None;
    let mut logical: Option<String> = None;
    let mut uid_prefix: Option<String> = None;
    let mut filename_prefix: Option<String> = None;
    let mut game_root: Option<PathBuf> = None;
    let mut staging: Option<PathBuf> = None;
    let mut iter = env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--dds" => dds_path = iter.next().map(PathBuf::from),
            "--logical" => logical = iter.next(),
            "--uid-prefix" => uid_prefix = iter.next(),
            "--filename-prefix" => filename_prefix = iter.next(),
            "--game-root" => game_root = iter.next().map(PathBuf::from),
            "--staging" => staging = iter.next().map(PathBuf::from),
            "-h" | "--help" => { println!("{USAGE}"); std::process::exit(0); }
            other => return Err(format!("unknown arg '{other}'\n{USAGE}")),
        }
    }
    let dds_path = dds_path.ok_or("--dds required")?;
    let logical = logical.ok_or("--logical required")?;
    let uid_prefix = uid_prefix.ok_or("--uid-prefix required")?;
    let filename_prefix = filename_prefix.ok_or("--filename-prefix required")?;
    let game_root = game_root.ok_or("--game-root required")?;
    let staging = staging.ok_or("--staging required")?;
    fs::create_dir_all(&staging).map_err(|e| format!("mkdir staging: {e}"))?;

    let (parent_package, texture_name) = logical.rsplit_once('.')
        .ok_or_else(|| format!("--logical must be <package>.<texture>; got '{logical}'"))?;

    let bytes = fs::read(&dds_path).map_err(|e| format!("read DDS: {e}"))?;
    let dds_img = dds::parse_dds(&bytes).map_err(|e| format!("parse DDS: {e}"))?;
    println!("DDS: {}x{} format={:?} mips={}",
        dds_img.width, dds_img.height, dds_img.format, dds_img.mips.len());

    let composite_uid = format!("{uid_prefix}_0001");
    let composite_filename = format!("{filename_prefix}_0001");
    let texture_object_name = format!("{texture_name}_dup");
    let composite_object_path = format!("{composite_uid}.{texture_object_name}");

    let gpk_bytes = composite_author::author_composite_slice(
        &dds_img, &texture_object_name, parent_package, &composite_object_path,
    ).map_err(|e| format!("author: {e}"))?;
    let out_path = staging.join(format!("{composite_filename}.gpk"));
    fs::write(&out_path, &gpk_bytes)
        .map_err(|e| format!("write {}: {e}", out_path.display()))?;

    let addition = mapper_extend::MapperAddition {
        logical_path: logical.clone(),
        composite_uid,
        composite_object_path,
        composite_filename,
        composite_offset: 0,
        composite_size: gpk_bytes.len() as i64,
    };

    let manifest_path = staging.join("install-manifest.json");
    let json = serde_json::to_string_pretty(&[serde_json::json!({
        "logical_path": addition.logical_path,
        "composite_uid": addition.composite_uid,
        "composite_object_path": addition.composite_object_path,
        "composite_filename": addition.composite_filename,
        "composite_offset": addition.composite_offset,
        "composite_size": addition.composite_size,
    })]).map_err(|e| format!("serialize manifest: {e}"))?;
    fs::write(&manifest_path, json)
        .map_err(|e| format!("write manifest: {e}"))?;

    let _ = game_root; // game-root reserved for future preflight checks
    println!("wrote {} ({} bytes) and manifest to {}",
        out_path.display(), gpk_bytes.len(), staging.display());
    Ok(())
}
