use std::{env, fs, path::PathBuf};

#[path = "../services/mods/gpk.rs"]
mod gpk;

fn main() {
    if let Err(err) = run() {
        eprintln!("FAIL: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let game_root = PathBuf::from(args.next().ok_or_else(usage)?);
    let term = args.next().ok_or_else(usage)?.to_ascii_lowercase();
    if args.next().is_some() {
        return Err(usage());
    }

    let cooked_pc = game_root.join(gpk::COOKED_PC_DIR);
    print_composite_mapper_hits(&cooked_pc, &term)?;
    print_pkg_mapper_hits(&cooked_pc, &term)?;
    Ok(())
}

fn print_composite_mapper_hits(cooked_pc: &std::path::Path, term: &str) -> Result<(), String> {
    let mapper = gpk::MAPPER_FILE;
    let path = cooked_pc.join(mapper);
    let encrypted =
        fs::read(&path).map_err(|e| format!("failed to read '{}': {e}", path.display()))?;
    let plain = String::from_utf8_lossy(&gpk::decrypt_mapper(&encrypted)).to_string();
    let map = gpk::parse_mapper(&plain);
    let mut hits: Vec<_> = map
        .values()
        .filter(|entry| {
            entry.object_path.to_ascii_lowercase().contains(term)
                || entry.composite_name.to_ascii_lowercase().contains(term)
                || entry.filename.to_ascii_lowercase().contains(term)
        })
        .collect();
    hits.sort_by(|left, right| {
        left.filename
            .cmp(&right.filename)
            .then(left.offset.cmp(&right.offset))
            .then(left.composite_name.cmp(&right.composite_name))
    });
    for entry in hits {
        println!(
            "{mapper}: file={} object={} composite={} offset={} size={}",
            entry.filename, entry.object_path, entry.composite_name, entry.offset, entry.size
        );
    }
    Ok(())
}

fn print_pkg_mapper_hits(cooked_pc: &std::path::Path, term: &str) -> Result<(), String> {
    let mapper = gpk::PKG_MAPPER_FILE;
    let path = cooked_pc.join(mapper);
    let encrypted =
        fs::read(&path).map_err(|e| format!("failed to read '{}': {e}", path.display()))?;
    let plain = String::from_utf8_lossy(&gpk::decrypt_mapper(&encrypted)).to_string();
    for cell in plain.split('|') {
        let trimmed = cell.trim();
        if trimmed.to_ascii_lowercase().contains(term) {
            println!("{mapper}: {trimmed}");
        }
    }
    Ok(())
}

fn usage() -> String {
    "usage: find-current-gpk-mapper <game-root> <term>".to_string()
}
