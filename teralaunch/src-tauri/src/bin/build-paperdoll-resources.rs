use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[allow(dead_code)] #[path = "../services/mods/dds.rs"] mod dds;
#[allow(dead_code)] #[path = "../services/mods/gpk_package.rs"] mod gpk_package;
#[allow(dead_code)] #[path = "../services/mods/gpk_resource_inspector.rs"] mod gpk_resource_inspector;
#[allow(dead_code)] #[path = "../services/mods/texture_encoder.rs"] mod texture_encoder;
#[allow(dead_code)] #[path = "../services/mods/composite_author.rs"] mod composite_author;
#[allow(dead_code)] #[path = "../services/mods/gpk.rs"] mod gpk;
#[allow(dead_code)] #[path = "../services/mods/mapper_extend.rs"] mod mapper_extend;

const USAGE: &str = "build-paperdoll-resources --foglio-root <path> --staging <dir>";

fn main() {
    if let Err(e) = run() {
        eprintln!("FAIL: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut foglio_root: Option<PathBuf> = None;
    let mut staging: Option<PathBuf> = None;
    let mut iter = env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--foglio-root" => foglio_root = iter.next().map(PathBuf::from),
            "--staging" => staging = iter.next().map(PathBuf::from),
            "-h" | "--help" => { println!("{USAGE}"); std::process::exit(0); }
            other => return Err(format!("unknown arg '{other}'\n{USAGE}")),
        }
    }
    let foglio_root = foglio_root.ok_or("--foglio-root required")?;
    let staging = staging.ok_or("--staging required")?;
    fs::create_dir_all(&staging).map_err(|e| format!("create staging: {e}"))?;

    let mut additions: Vec<mapper_extend::MapperAddition> = Vec::new();
    let mut idx = 0u32;
    let mut skin_count = 0usize;
    let mut comp_count = 0usize;

    let skin_dir = foglio_root.join("RES_Skin");
    if skin_dir.is_dir() {
        skin_count = process_dir(
            &skin_dir, &staging, &mut idx, &mut additions,
            "modres_skin", "modres_paperdoll_skin", "S1UIRES_Skin",
            /* strip_bg_prefix = */ true,
        )?;
    }
    let component_dir = foglio_root.join("RES_Component");
    if component_dir.is_dir() {
        comp_count = process_dir(
            &component_dir, &staging, &mut idx, &mut additions,
            "modres_comp", "modres_paperdoll_comp", "S1UIRES_Component",
            /* strip_bg_prefix = */ false,
        )?;
    }

    let manifest_path = staging.join("install-manifest.json");
    let json_entries: Vec<serde_json::Value> = additions.iter().map(|a| serde_json::json!({
        "logical_path": a.logical_path,
        "composite_uid": a.composite_uid,
        "composite_object_path": a.composite_object_path,
        "composite_filename": a.composite_filename,
        "composite_offset": a.composite_offset,
        "composite_size": a.composite_size,
    })).collect();
    let json = serde_json::to_string_pretty(&json_entries)
        .map_err(|e| format!("serialize manifest: {e}"))?;
    fs::write(&manifest_path, json)
        .map_err(|e| format!("write manifest: {e}"))?;

    println!("processed {skin_count} RES_Skin entries, {comp_count} RES_Component entries");
    println!("wrote {} GPKs + manifest to {}", additions.len(), staging.display());
    Ok(())
}

fn process_dir(
    dir: &Path,
    staging: &Path,
    idx: &mut u32,
    additions: &mut Vec<mapper_extend::MapperAddition>,
    uid_prefix: &str,
    filename_prefix: &str,
    parent_package: &str,
    strip_bg_prefix: bool,
) -> Result<usize, String> {
    let mut count = 0usize;
    for entry in fs::read_dir(dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("dir entry: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("dds") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str())
            .ok_or_else(|| format!("bad stem for {}", path.display()))?;
        let texture_name = if strip_bg_prefix {
            stem.strip_prefix("BG_").unwrap_or(stem).to_string()
        } else {
            stem.to_string()
        };

        let bytes = fs::read(&path)
            .map_err(|e| format!("read DDS {}: {e}", path.display()))?;
        let dds = dds::parse_dds(&bytes)
            .map_err(|e| format!("parse DDS {}: {e}", path.display()))?;

        *idx += 1;
        let composite_uid = format!("{uid_prefix}_{:04x}", *idx);
        let composite_filename = format!("{filename_prefix}_{:04x}", *idx);
        // The texture export's object_name MUST equal the part-after-dot of the
        // composite_object_path, otherwise engine-side hierarchical lookup
        // fails silently. Vanilla v100 composite slices follow this convention
        // (e.g. composite_object_path "<uid>.PaperDoll_I147_dup" pairs with
        // texture object_name "PaperDoll_I147_dup"). We append "_dup" here.
        let texture_object_name = format!("{texture_name}_dup");
        let composite_object_path = format!("{composite_uid}.{texture_object_name}");

        let gpk_bytes = composite_author::author_composite_slice(
            &dds, &texture_object_name, parent_package, &composite_object_path,
        ).map_err(|e| format!("author {}: {e}", path.display()))?;

        let out_path = staging.join(format!("{composite_filename}.gpk"));
        fs::write(&out_path, &gpk_bytes)
            .map_err(|e| format!("write {}: {e}", out_path.display()))?;

        additions.push(mapper_extend::MapperAddition {
            logical_path: format!("{parent_package}.{texture_name}"),
            composite_uid,
            composite_object_path,
            composite_filename,
            composite_offset: 0,
            composite_size: gpk_bytes.len() as i64,
        });
        count += 1;
    }
    Ok(count)
}
