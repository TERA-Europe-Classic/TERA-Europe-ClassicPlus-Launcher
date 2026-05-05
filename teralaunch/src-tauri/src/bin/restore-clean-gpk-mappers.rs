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
    let container = args.next();
    if args.next().is_some() {
        return Err(usage());
    }

    let cooked_pc = game_root.join(gpk::COOKED_PC_DIR);
    gpk::restore_clean_gpk_state(&game_root)?;
    if let Some(container) = container {
        let path = cooked_pc.join(&container);
        if path.exists() {
            fs::remove_file(&path)
                .map_err(|e| format!("failed to remove '{}': {e}", path.display()))?;
            println!("removed {}", path.display());
        }
    }
    println!(
        "restored clean GPK mapper/container state in {}",
        cooked_pc.display()
    );
    Ok(())
}

fn usage() -> String {
    "usage: restore-clean-gpk-mappers <game-root> [container-to-remove.gpk]".to_string()
}
