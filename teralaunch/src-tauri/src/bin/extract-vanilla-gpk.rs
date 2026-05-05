use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "../services/mods/gpk.rs"]
pub mod gpk;

#[path = "../services/mods/composite_extract.rs"]
mod composite_extract;

const USAGE: &str = "extract-vanilla-gpk --game-root <path> (--package <name> | --object-path <path>) --out <path>\n       extract-vanilla-gpk --game-root <path> --find <term>";

enum Target {
    Package(String),
    ObjectPath(String),
}

struct CliArgs {
    game_root: PathBuf,
    command: Command,
}

enum Command {
    Extract { target: Target, out: PathBuf },
    Find { term: String },
}

fn parse_args() -> Result<CliArgs, String> {
    let mut game_root: Option<PathBuf> = None;
    let mut package: Option<String> = None;
    let mut object_path: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut find: Option<String> = None;

    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--game-root" => game_root = iter.next().map(PathBuf::from),
            "--package" => package = iter.next(),
            "--object-path" => object_path = iter.next(),
            "--out" => out = iter.next().map(PathBuf::from),
            "--find" => find = iter.next(),
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("Unknown arg '{other}'")),
        }
    }

    let command = if let Some(term) = find {
        if package.is_some() || object_path.is_some() || out.is_some() {
            return Err("Use --find without --package, --object-path, or --out".into());
        }
        Command::Find { term }
    } else {
        let target = match (package, object_path) {
            (Some(package), None) => Target::Package(package),
            (None, Some(object_path)) => Target::ObjectPath(object_path),
            (Some(_), Some(_)) => return Err("Use --package or --object-path, not both".into()),
            (None, None) => return Err("--package or --object-path is required".into()),
        };
        Command::Extract {
            target,
            out: out.ok_or("--out is required")?,
        }
    };

    Ok(CliArgs {
        game_root: game_root.ok_or("--game-root is required")?,
        command,
    })
}

fn run(args: CliArgs) -> Result<String, String> {
    match args.command {
        Command::Extract { target, out } => extract_to_file(&args.game_root, target, out),
        Command::Find { term } => find_mapper_matches(&args.game_root, &term),
    }
}

fn extract_to_file(game_root: &Path, target: Target, out: PathBuf) -> Result<String, String> {
    let bytes = match target {
        Target::Package(package) => {
            composite_extract::extract_vanilla_for_package_name(game_root, &package)?
        }
        Target::ObjectPath(object_path) => {
            composite_extract::extract_vanilla_for_object_path(game_root, &object_path)?
        }
    };
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create output directory {}: {e}",
                    parent.display()
                )
            })?;
        }
    }
    fs::write(&out, &bytes).map_err(|e| format!("Failed to write {}: {e}", out.display()))?;
    Ok(format!("extracted-bytes: {}", bytes.len()))
}

fn find_mapper_matches(game_root: &Path, term: &str) -> Result<String, String> {
    let cooked_pc = game_root.join(gpk::COOKED_PC_DIR);
    let needle = term.to_ascii_lowercase();
    let mut out = Vec::new();
    for mapper in [gpk::BACKUP_FILE, gpk::PKG_MAPPER_BACKUP_FILE] {
        let path = cooked_pc.join(mapper);
        let bytes =
            fs::read(&path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        let plain = String::from_utf8_lossy(&gpk::decrypt_mapper(&bytes)).to_string();
        let mut count = 0usize;
        for cell in plain.split(['|', '!']) {
            let trimmed = cell.trim();
            if trimmed.to_ascii_lowercase().contains(&needle) {
                out.push(format!("{mapper}: {trimmed}"));
                count += 1;
                if count >= 40 {
                    out.push(format!("{mapper}: ... truncated after {count} matches"));
                    break;
                }
            }
        }
    }
    if out.is_empty() {
        Ok(format!("no matches for '{term}'"))
    } else {
        Ok(out.join("\n"))
    }
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("{err}\n{USAGE}");
            std::process::exit(2);
        }
    };

    match run(args) {
        Ok(message) => println!("{message}"),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}

// `crate::services::mods::*` shim so `gpk_package`'s `#[cfg(test)] mod tests`
// re-exports keep resolving when this bin compiles. The bin doesn't run those
// tests (`gpk_package::tests` is feature-gated behind `lib-tests`), but cargo
// still parses the module and walks `use` paths during clippy. Suppress the
// per-item dead-code warnings the shim helpers emit.
#[cfg(test)]
#[allow(dead_code, unused_imports)]
mod services {
    pub mod mods {
        pub use crate::gpk;

        pub mod test_fixtures {
            pub fn build_boss_window_test_package(marker: [u8; 4], _compressed: bool) -> Vec<u8> {
                marker.to_vec()
            }
        }
    }
}
