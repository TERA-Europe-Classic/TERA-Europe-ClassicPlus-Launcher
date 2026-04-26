// Test helper: just decompress a GPK via extract_uncompressed_package_bytes
// and write the output. Lets us isolate whether engine-load failures are
// caused by the decompression header tweaks vs the splice itself.

use std::env;
use std::fs;

#[allow(dead_code)]
#[path = "../services/mods/gpk_package.rs"]
mod gpk_package;

#[allow(dead_code)]
#[path = "../services/mods/patch_manifest.rs"]
mod patch_manifest;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../services/mods/test_fixtures.rs"]
mod test_fixtures;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: decompress-only <input.gpk> <output.gpk>");
        std::process::exit(1);
    }
    let bytes = fs::read(&args[1]).unwrap();
    let out = gpk_package::extract_uncompressed_package_bytes(&bytes).unwrap();
    fs::write(&args[2], &out).unwrap();
    println!("wrote {} bytes (input {} bytes)", out.len(), bytes.len());
}
