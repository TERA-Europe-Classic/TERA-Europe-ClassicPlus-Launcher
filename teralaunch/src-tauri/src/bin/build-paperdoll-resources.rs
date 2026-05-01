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
        // ALSO emit numeric-coded variants for paperdoll silhouettes — v100
        // engine sends numeric race/sex indices to the SWF, not foglio's letter
        // codes. Without these, the silhouette URI never resolves to our art.
        skin_count += process_paperdoll_silhouettes_numeric(
            &skin_dir, &staging, &mut idx, &mut additions,
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

/// foglio's letter-coded silhouettes → v100's numeric race/sex naming.
/// Convention (best guess from TERA + foglio code patterns):
///   HM=0_0 HW=0_1 KM=1_0 KW=1_1 AM=2_0 AW=2_1 EM=3_0 EW=3_1 PP=4_0 BK=5_0 EL=5_1
/// _2 variants (alt outfits) likewise mapped: HM_2 → 0_0_alt etc. v100 doesn't
/// appear to expose _alt names in PkgMapper, so for now we only emit base codes.
const RACE_NUMERIC_MAP: &[(&str, &str)] = &[
    ("PaperDoll_HM", "PaperDoll_0_0"),
    ("PaperDoll_HW", "PaperDoll_0_1"),
    ("PaperDoll_KM", "PaperDoll_1_0"),
    ("PaperDoll_KW", "PaperDoll_1_1"),
    ("PaperDoll_AM", "PaperDoll_2_0"),
    ("PaperDoll_AW", "PaperDoll_2_1"),
    ("PaperDoll_EM", "PaperDoll_3_0"),
    ("PaperDoll_EW", "PaperDoll_3_1"),
    ("PaperDoll_PP", "PaperDoll_4_0"),
    ("PaperDoll_BK", "PaperDoll_5_0"),
    ("PaperDoll_EL", "PaperDoll_5_1"),
    // PaperDoll2 (secondary instance / "compare with other" SWF)
    ("PaperDoll2_HM", "PaperDoll2_0_0"),
    ("PaperDoll2_HW", "PaperDoll2_0_1"),
    ("PaperDoll2_KM", "PaperDoll2_1_0"),
    ("PaperDoll2_KW", "PaperDoll2_1_1"),
    ("PaperDoll2_AM", "PaperDoll2_2_0"),
    ("PaperDoll2_AW", "PaperDoll2_2_1"),
    ("PaperDoll2_EM", "PaperDoll2_3_0"),
    ("PaperDoll2_EW", "PaperDoll2_3_1"),
    ("PaperDoll2_PP", "PaperDoll2_4_0"),
    ("PaperDoll2_BK", "PaperDoll2_5_0"),
    ("PaperDoll2_EL", "PaperDoll2_5_1"),
];

fn process_paperdoll_silhouettes_numeric(
    skin_dir: &Path,
    staging: &Path,
    idx: &mut u32,
    additions: &mut Vec<mapper_extend::MapperAddition>,
) -> Result<usize, String> {
    let mut count = 0usize;
    for (foglio_letter_name, v100_numeric_name) in RACE_NUMERIC_MAP {
        let dds_path = skin_dir.join(format!("BG_{foglio_letter_name}.dds"));
        if !dds_path.is_file() { continue; }
        let bytes = fs::read(&dds_path)
            .map_err(|e| format!("read DDS {}: {e}", dds_path.display()))?;
        let dds = dds::parse_dds(&bytes)
            .map_err(|e| format!("parse DDS {}: {e}", dds_path.display()))?;
        *idx += 1;
        let composite_uid = format!("modres_skin_n_{:04x}", *idx);
        let composite_filename = format!("modres_paperdoll_skin_n_{:04x}", *idx);
        let texture_object_name = format!("{v100_numeric_name}_dup");
        let composite_object_path = format!("{composite_uid}.{texture_object_name}");

        let gpk_bytes = composite_author::author_composite_slice(
            &dds, &texture_object_name, "S1UIRES_Skin", &composite_object_path,
        ).map_err(|e| format!("author numeric {}: {e}", v100_numeric_name))?;
        let out_path = staging.join(format!("{composite_filename}.gpk"));
        fs::write(&out_path, &gpk_bytes)
            .map_err(|e| format!("write {}: {e}", out_path.display()))?;
        additions.push(mapper_extend::MapperAddition {
            logical_path: format!("S1UIRES_Skin.{v100_numeric_name}"),
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
