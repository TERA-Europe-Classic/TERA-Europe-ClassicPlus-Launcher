//! Extract the RAW (still-compressed) vanilla composite slice for a given
//! logical path and write it to disk. Used by the foglio batch porter to
//! feed splice-x32-payloads — that tool wants the full GPK file with header
//! intact, not the uncompressed body.
//!
//! Usage:
//!   extract-vanilla-slice-raw --game-root <path> --logical <Package.Object> --out <path>

#[allow(dead_code)] #[path = "../services/mods/composite_extract.rs"] mod composite_extract;
#[allow(dead_code)] #[path = "../services/mods/gpk.rs"] mod gpk;

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let mut game_root: Option<PathBuf> = None;
    let mut logical: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut iter = env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--game-root" => game_root = iter.next().map(PathBuf::from),
            "--logical" => logical = iter.next(),
            "--out" => out = iter.next().map(PathBuf::from),
            other => { eprintln!("unknown arg '{other}'"); std::process::exit(1); }
        }
    }
    let game_root = game_root.expect("--game-root required");
    let logical = logical.expect("--logical required");
    let out = out.expect("--out required");

    // Decrypt PkgMapper.clean → composite_object_path
    let cooked = game_root.join(gpk::COOKED_PC_DIR);
    let pkg_clean_bytes = fs::read(cooked.join(gpk::PKG_MAPPER_BACKUP_FILE))
        .unwrap_or_else(|e| { eprintln!("read PkgMapper.clean: {e}"); std::process::exit(2); });
    let pkg_plain = String::from_utf8_lossy(&gpk::decrypt_mapper(&pkg_clean_bytes)).to_string();
    let composite_object_path = pkg_plain
        .split('|')
        .filter_map(|cell| cell.trim().split_once(','))
        .find_map(|(uid, comp)| if uid.eq_ignore_ascii_case(&logical) { Some(comp.trim().to_string()) } else { None })
        .unwrap_or_else(|| { eprintln!("logical path '{logical}' not in PkgMapper.clean"); std::process::exit(3); });
    let composite_uid = composite_object_path.split('.').next().unwrap_or("");

    // Decrypt CompositePackageMapper.clean → MapperEntry
    let comp_bytes = fs::read(cooked.join(gpk::BACKUP_FILE))
        .unwrap_or_else(|e| { eprintln!("read CompositePackageMapper.clean: {e}"); std::process::exit(4); });
    let comp_plain = String::from_utf8_lossy(&gpk::decrypt_mapper(&comp_bytes)).to_string();
    let comp_map = gpk::parse_mapper_strict(&comp_plain).unwrap();
    let entry = comp_map.get(composite_uid)
        .unwrap_or_else(|| { eprintln!("composite_uid '{composite_uid}' not in CompositePackageMapper.clean"); std::process::exit(5); });

    // Read the container, slice at the right offset, write to --out.
    let container_path = cooked.join(format!("{}.gpk", entry.filename));
    let backup_path = container_path.with_extension("gpk.vanilla-bak");
    let source = if backup_path.exists() { backup_path } else { container_path };
    let container = fs::read(&source).unwrap_or_else(|e| { eprintln!("read container {}: {e}", source.display()); std::process::exit(6); });
    let off = entry.offset as usize;
    let size = entry.size as usize;
    let slice = &container[off..off + size];
    fs::write(&out, slice).unwrap_or_else(|e| { eprintln!("write out: {e}"); std::process::exit(7); });

    println!("logical: {logical}");
    println!("composite_uid: {composite_uid}");
    println!("source: {}", source.display());
    println!("offset: {off}, size: {size}");
    println!("wrote: {} ({} bytes)", out.display(), slice.len());
}
