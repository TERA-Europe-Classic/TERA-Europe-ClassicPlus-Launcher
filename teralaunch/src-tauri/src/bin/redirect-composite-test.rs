//! One-shot: apply install_composite_redirect's redirect to an
//! already-deployed file. Used for differential testing.
//!
//! Usage: redirect-composite-test --game-root <path> --target-object-path <path> --deployed-filename <name>

#[path = "../services/mods/gpk.rs"] mod gpk;

use std::env;
use std::path::PathBuf;

fn main() {
    let mut iter = env::args().skip(1);
    let mut game_root: Option<PathBuf> = None;
    let mut target_path: Option<String> = None;
    let mut deployed_name: Option<String> = None;
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--game-root" => game_root = iter.next().map(PathBuf::from),
            "--target-object-path" => target_path = iter.next(),
            "--deployed-filename" => deployed_name = iter.next(),
            other => { eprintln!("unknown arg '{other}'"); std::process::exit(1); }
        }
    }
    let game_root = game_root.expect("--game-root required");
    let target_path = target_path.expect("--target-object-path required");
    let deployed_name = deployed_name.expect("--deployed-filename required");

    // Just apply the mapper redirect — file is already deployed
    let cooked = game_root.join("S1Game/CookedPC");
    let dat = cooked.join("CompositePackageMapper.dat");
    let bytes = std::fs::read(&dat).expect("read");
    let plain = String::from_utf8_lossy(&gpk::decrypt_mapper(&bytes)).to_string();
    let mut map = gpk::parse_mapper(&plain);

    let tail = target_path.rsplit('.').next().expect("no dot");
    let folder_name = if tail.ends_with("_dup") { tail.to_string() } else { format!("{tail}_dup") };
    let suffix = format!(".{folder_name}");
    let file_size = std::fs::metadata(cooked.join(&deployed_name)).expect("stat").len() as i64;

    let mut rewritten = 0;
    for (_, entry) in map.iter_mut() {
        if entry.object_path.ends_with(&suffix) || entry.object_path == folder_name {
            entry.filename = deployed_name.clone();
            entry.offset = 0;
            entry.size = file_size;
            rewritten += 1;
        }
    }
    println!("rewritten {rewritten} composite entries");
    let new_plain = gpk::serialize_mapper(&map);
    let new_enc = gpk::encrypt_mapper(new_plain.as_bytes());
    std::fs::write(&dat, &new_enc).expect("write");
    println!("wrote: {}", dat.display());
}
