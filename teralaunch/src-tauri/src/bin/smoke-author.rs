//! Smoke test for the composite-slice author. Reads a DDS file, calls
//! `author_composite_slice`, and writes the resulting GPK to disk. Pair with
//! `inspect-gpk-resources` to verify the output round-trips.

use std::{env, fs, path::PathBuf};

#[path = "../services/mods/dds.rs"]
mod dds;

#[path = "../services/mods/gpk_package.rs"]
mod gpk_package;

#[path = "../services/mods/gpk_resource_inspector.rs"]
mod gpk_resource_inspector;

#[path = "../services/mods/texture_encoder.rs"]
mod texture_encoder;

#[path = "../services/mods/composite_author.rs"]
mod composite_author;

fn main() {
    if let Err(err) = run() {
        eprintln!("FAIL: {err}");
        std::process::exit(1);
    }
}

struct Args {
    dds_path: PathBuf,
    texture_name: String,
    parent_package: String,
    composite: String,
    out_path: PathBuf,
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let dds_bytes = fs::read(&args.dds_path)
        .map_err(|e| format!("read dds '{}': {e}", args.dds_path.display()))?;
    let dds_image = dds::parse_dds(&dds_bytes)?;
    let bytes = composite_author::author_composite_slice(
        &dds_image,
        &args.texture_name,
        &args.parent_package,
        &args.composite,
    )?;
    if let Some(parent) = args.out_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("create parent '{}': {e}", parent.display()))?;
    }
    fs::write(&args.out_path, &bytes)
        .map_err(|e| format!("write '{}': {e}", args.out_path.display()))?;
    println!(
        "wrote {} bytes to {}",
        bytes.len(),
        args.out_path.display()
    );
    Ok(())
}

fn parse_args() -> Result<Args, String> {
    let mut dds_path: Option<PathBuf> = None;
    let mut texture_name: Option<String> = None;
    let mut parent_package: Option<String> = None;
    let mut composite: Option<String> = None;
    let mut out_path: Option<PathBuf> = None;
    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        let value = iter
            .next()
            .ok_or_else(|| format!("flag '{arg}' requires a value"))?;
        match arg.as_str() {
            "--dds" => dds_path = Some(PathBuf::from(value)),
            "--texture-name" => texture_name = Some(value),
            "--parent-package" => parent_package = Some(value),
            "--composite" => composite = Some(value),
            "--out" => out_path = Some(PathBuf::from(value)),
            other => return Err(format!("unknown flag '{other}'")),
        }
    }
    Ok(Args {
        dds_path: dds_path.ok_or_else(usage)?,
        texture_name: texture_name.ok_or_else(usage)?,
        parent_package: parent_package.ok_or_else(usage)?,
        composite: composite.ok_or_else(usage)?,
        out_path: out_path.ok_or_else(usage)?,
    })
}

fn usage() -> String {
    "usage: smoke-author --dds <path> --texture-name <name> --parent-package <name> --composite <obj-path> --out <gpk-path>".to_string()
}
