//! Smoke test for the DDS parser. Reads a `.dds` file and prints its
//! width, height, fourCC format, and mip count.

use std::{env, fs, path::PathBuf};

#[allow(dead_code)]
#[path = "../services/mods/dds.rs"]
mod dds;

use dds::{parse_dds, DdsPixelFormat};

fn main() {
    if let Err(err) = run() {
        eprintln!("FAIL: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let path = PathBuf::from(args.next().ok_or_else(usage)?);
    if args.next().is_some() {
        return Err(usage());
    }
    let bytes =
        fs::read(&path).map_err(|e| format!("failed to read '{}': {e}", path.display()))?;
    let dds = parse_dds(&bytes)?;
    let fmt = match dds.format {
        DdsPixelFormat::Dxt1 => "DXT1",
        DdsPixelFormat::Dxt3 => "DXT3",
        DdsPixelFormat::Dxt5 => "DXT5",
    };
    println!(
        "{} x {} {}, {} mips",
        dds.width,
        dds.height,
        fmt,
        dds.mips.len()
    );
    Ok(())
}

fn usage() -> String {
    "usage: smoke-dds <path/to/file.dds>".to_string()
}
