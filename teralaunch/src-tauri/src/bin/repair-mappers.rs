//! Admin/recovery tool: restore PkgMapper.dat and CompositePackageMapper.dat
//! from their .clean baselines, then remove any stray artexlib dropin files
//! left by the previous dropin+mapper-extend install path.
//!
//! Safe to run when composite_patch mods are installed — those never touch
//! the mapper files directly.
//!
//! Usage:
//!   repair-mappers --game-root D:/Elinu

#[allow(dead_code)]
#[path = "../services/mods/gpk.rs"]
mod gpk;

use std::{env, fs, path::PathBuf};

fn main() {
    if let Err(err) = run() {
        eprintln!("FAIL: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut iter = env::args().skip(1);
    let mut game_root: Option<PathBuf> = None;
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--game-root" => {
                game_root = iter.next().map(PathBuf::from);
            }
            other => {
                return Err(format!(
                    "unknown argument '{other}'\nusage: repair-mappers --game-root <path>"
                ));
            }
        }
    }
    let game_root = game_root.ok_or(
        "missing required argument --game-root <path>".to_string(),
    )?;

    let cooked = game_root.join(gpk::COOKED_PC_DIR);

    // Restore PkgMapper.dat and CompositePackageMapper.dat from .clean baselines.
    let pairs = [
        (
            cooked.join(gpk::PKG_MAPPER_FILE),
            cooked.join(gpk::PKG_MAPPER_BACKUP_FILE),
        ),
        (
            cooked.join(gpk::MAPPER_FILE),
            cooked.join(gpk::BACKUP_FILE),
        ),
    ];
    for (live, clean) in &pairs {
        if !clean.exists() {
            return Err(format!(
                "baseline '{}' not found — cannot repair",
                clean.display()
            ));
        }
        fs::copy(clean, live)
            .map_err(|e| format!("failed to copy '{}' → '{}': {e}", clean.display(), live.display()))?;
        println!("restored: {}", live.display());
    }

    // Remove stray artexlib dropin files left by the old dropin+mapper-extend
    // install path. These are safe to delete — re-installing the mod via the
    // launcher with the fixed composite_redirect strategy will redeploy them
    // under the correct filename.
    let strays = [
        "LancerGigaChadBlock.gpk",
        "BrawlerChadBlocking.gpk",
        "GucciBackpack.gpk",
        "PinkValkyrieHelmet.gpk",
    ];
    for stray in strays {
        let p = cooked.join(stray);
        if p.exists() {
            fs::remove_file(&p)
                .map_err(|e| format!("failed to remove '{}': {e}", p.display()))?;
            println!("removed stray: {}", p.display());
        }
    }

    println!(
        "\nRepair complete. Uninstall and reinstall affected mods via the launcher to redeploy under the fixed path."
    );
    Ok(())
}
