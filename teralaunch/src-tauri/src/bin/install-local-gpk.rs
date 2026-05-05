use std::{env, path::PathBuf};

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
    let source_gpk = PathBuf::from(args.next().ok_or_else(usage)?);
    if args.next().is_some() {
        return Err(usage());
    }

    let modfile = gpk::install_gpk(&game_root, &source_gpk)?;
    println!(
        "installed container={} packages={}",
        modfile.container,
        modfile.packages.len()
    );
    for package in modfile.packages {
        println!("package={} size={}", package.object_path, package.size);
    }
    Ok(())
}

fn usage() -> String {
    "usage: install-local-gpk <game-root> <source.gpk>".to_string()
}
