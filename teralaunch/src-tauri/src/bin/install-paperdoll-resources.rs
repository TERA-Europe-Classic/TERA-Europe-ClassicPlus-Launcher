use std::env;
use std::fs;
use std::path::PathBuf;

#[allow(dead_code)] #[path = "../services/mods/gpk.rs"] mod gpk;
#[allow(dead_code)] #[path = "../services/mods/mapper_extend.rs"] mod mapper_extend;

const USAGE: &str = "install-paperdoll-resources --game-root <path> --staging <dir>";

fn main() {
    if let Err(e) = run() {
        eprintln!("FAIL: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut iter = env::args().skip(1);
    let mut game_root: Option<PathBuf> = None;
    let mut staging: Option<PathBuf> = None;
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--game-root" => game_root = iter.next().map(PathBuf::from),
            "--staging" => staging = iter.next().map(PathBuf::from),
            "-h" | "--help" => { println!("{USAGE}"); std::process::exit(0); }
            other => return Err(format!("unknown arg '{other}'\n{USAGE}")),
        }
    }
    let game_root = game_root.ok_or("--game-root required")?;
    let staging = staging.ok_or("--staging required")?;

    // Pre-check: CookedPC + .clean files + manifest + staged GPKs.
    let cooked = game_root.join(gpk::COOKED_PC_DIR);
    if !cooked.is_dir() {
        return Err(format!("CookedPC dir missing: {}", cooked.display()));
    }
    ensure_clean_baseline(&cooked, gpk::MAPPER_FILE, gpk::BACKUP_FILE)?;
    ensure_clean_baseline(&cooked, gpk::PKG_MAPPER_FILE, gpk::PKG_MAPPER_BACKUP_FILE)?;

    let manifest_path = staging.join("install-manifest.json");
    let json_text = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("read manifest at {}: {e}", manifest_path.display()))?;
    let entries: Vec<serde_json::Value> = serde_json::from_str(&json_text)
        .map_err(|e| format!("parse manifest: {e}"))?;

    // Build additions and verify each staged GPK exists.
    let mut additions: Vec<mapper_extend::MapperAddition> = Vec::with_capacity(entries.len());
    for e in &entries {
        let composite_filename = e["composite_filename"].as_str()
            .ok_or("manifest entry missing composite_filename")?
            .to_string();
        let staged_path = staging.join(format!("{composite_filename}.gpk"));
        if !staged_path.is_file() {
            return Err(format!("staged GPK missing: {}", staged_path.display()));
        }
        additions.push(mapper_extend::MapperAddition {
            logical_path: e["logical_path"].as_str().ok_or("missing logical_path")?.into(),
            composite_uid: e["composite_uid"].as_str().ok_or("missing composite_uid")?.into(),
            composite_object_path: e["composite_object_path"].as_str()
                .ok_or("missing composite_object_path")?.into(),
            composite_filename,
            composite_offset: e["composite_offset"].as_i64().unwrap_or(0),
            composite_size: e["composite_size"].as_i64().unwrap_or(0),
        });
    }

    // Copy each GPK into CookedPC.
    for add in &additions {
        let src = staging.join(format!("{}.gpk", add.composite_filename));
        let dst = cooked.join(format!("{}.gpk", add.composite_filename));
        gpk::copy_atomic(&src, &dst)
            .map_err(|e| format!("copy {} -> {}: {e}", src.display(), dst.display()))?;
    }

    // Extend mappers.
    mapper_extend::extend_mappers(&game_root, &additions)?;

    println!("installed {} resources", additions.len());
    Ok(())
}

fn ensure_clean_baseline(cooked: &std::path::Path, live_name: &str, clean_name: &str) -> Result<(), String> {
    let live = cooked.join(live_name);
    let clean = cooked.join(clean_name);
    if !live.is_file() {
        return Err(format!("live file missing: {}", live.display()));
    }
    if !clean.is_file() {
        eprintln!("note: {} missing — copying {} as the vanilla baseline before install",
            clean.display(), live.display());
        fs::copy(&live, &clean)
            .map_err(|e| format!("copy {} -> {}: {e}", live.display(), clean.display()))?;
    }
    Ok(())
}
