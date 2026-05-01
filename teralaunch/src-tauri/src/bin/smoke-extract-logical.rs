//! Smoke test the new extract_vanilla_for_logical_path helper end-to-end
//! against a real game tree. Prints the resolved bytes' length + first 16 hex
//! for visual sanity.

#[allow(dead_code)] #[path = "../services/mods/composite_extract.rs"] mod composite_extract;
#[allow(dead_code)] #[path = "../services/mods/gpk.rs"] mod gpk;

use std::env;
use std::path::PathBuf;

fn main() {
    let mut iter = env::args().skip(1);
    let game_root = PathBuf::from(iter.next().expect("usage: <game-root> <logical-path>"));
    let logical = iter.next().expect("usage: <game-root> <logical-path>");
    let bytes = composite_extract::extract_vanilla_for_logical_path(&game_root, &logical)
        .unwrap_or_else(|e| {
            eprintln!("FAIL: {e}");
            std::process::exit(1);
        });
    println!("logical: {logical}");
    println!("size: {} bytes", bytes.len());
    let prefix: Vec<String> = bytes.iter().take(16).map(|b| format!("{:02x}", b)).collect();
    println!("first 16 bytes: {}", prefix.join(" "));
}
