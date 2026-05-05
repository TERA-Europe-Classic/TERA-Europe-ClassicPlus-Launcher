// revert-pkg-mapper-rows — surgically revert specific PkgMapper logical
// paths back to their .clean baseline content, leaving every other live
// row untouched. Used to undo a single bad override without nuking the
// whole mod state.
//
// Usage:
//   revert-pkg-mapper-rows <game-root> <logical_path>...

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::PathBuf;

#[path = "../services/mods/gpk.rs"] mod gpk;

fn main() {
    if let Err(e) = run() {
        eprintln!("FAIL: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let game_root = PathBuf::from(args.next().ok_or("game_root required")?);
    let logical_paths: Vec<String> = args.collect();
    if logical_paths.is_empty() {
        return Err("at least one logical path required".into());
    }

    let cooked = game_root.join(gpk::COOKED_PC_DIR);
    let clean_path = cooked.join("PkgMapper.clean");
    let live_path = cooked.join(gpk::PKG_MAPPER_FILE);

    let clean_bytes = fs::read(&clean_path).map_err(|e| format!("read clean: {e}"))?;
    let live_bytes = fs::read(&live_path).map_err(|e| format!("read live: {e}"))?;
    let clean_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&clean_bytes)).to_string();
    let live_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&live_bytes)).to_string();

    // Build clean lookup: logical → "composite_uid.composite_object"
    let mut clean_rows: HashMap<String, String> = HashMap::new();
    for cell in clean_text.split('|') {
        let cell = cell.trim();
        if cell.is_empty() { continue; }
        if let Some(comma) = cell.find(',') {
            clean_rows.insert(cell[..comma].to_string(), cell[comma + 1..].to_string());
        }
    }

    // Walk live rows in order; substitute matching logicals with .clean values.
    let target_set: HashSet<&str> = logical_paths.iter().map(|s| s.as_str()).collect();
    let mut new_rows: Vec<String> = Vec::new();
    let mut reverted = 0usize;
    let mut not_in_clean: Vec<&str> = Vec::new();
    for row in live_text.split('|') {
        let trimmed = row.trim();
        if trimmed.is_empty() { continue; }
        let comma = match trimmed.find(',') {
            Some(c) => c,
            None => { new_rows.push(trimmed.to_string()); continue; }
        };
        let logical = &trimmed[..comma];
        if target_set.contains(logical) {
            match clean_rows.get(logical) {
                Some(clean_value) => {
                    new_rows.push(format!("{},{}", logical, clean_value));
                    reverted += 1;
                    println!("reverted {logical}");
                    continue;
                }
                None => {
                    not_in_clean.push(logical);
                    new_rows.push(trimmed.to_string());
                    continue;
                }
            }
        }
        new_rows.push(trimmed.to_string());
    }

    let new_text = format!("{}|", new_rows.join("|"));
    let encrypted = gpk::encrypt_mapper(new_text.as_bytes());
    fs::write(&live_path, encrypted).map_err(|e| format!("write live: {e}"))?;

    println!("reverted {} rows", reverted);
    if !not_in_clean.is_empty() {
        println!("warning: these logical paths had no .clean baseline (left as-is):");
        for p in not_in_clean { println!("  {}", p); }
    }
    let unmatched: Vec<&str> = target_set.iter()
        .filter(|t| !live_text.split('|').any(|r| r.trim().starts_with(&format!("{},", t))))
        .copied()
        .collect();
    if !unmatched.is_empty() {
        println!("warning: these logical paths were not present in live PkgMapper:");
        for p in unmatched { println!("  {}", p); }
    }
    Ok(())
}
