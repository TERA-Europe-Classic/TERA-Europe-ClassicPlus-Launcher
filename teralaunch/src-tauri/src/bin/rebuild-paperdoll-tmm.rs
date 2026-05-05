//! Rebuild RestylePaperdoll.gpk with a proper TMM footer so it installs
//! via tmm::install_gpk through the launcher's catalog flow.
//!
//! Usage:
//!   rebuild-paperdoll-tmm --inner <path-to-existing-RestylePaperdoll.gpk> --out <path-to-new-RestylePaperdoll.gpk>

#[path = "../services/mods/gpk.rs"] mod gpk;
#[path = "../services/mods/gpk_package.rs"] mod gpk_package;
#[path = "../services/mods/mapper_extend.rs"] mod mapper_extend;
#[path = "../services/mods/tmm_wrap.rs"] mod tmm_wrap;

use std::env;
use std::fs;
use std::path::PathBuf;

use tmm_wrap::{wrap_as_tmm, TmmComposite, TmmModSpec};

fn main() {
    if let Err(e) = run() {
        eprintln!("FAIL: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut inner: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut iter = env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--inner" => inner = iter.next().map(PathBuf::from),
            "--out" => out = iter.next().map(PathBuf::from),
            other => return Err(format!("unknown arg '{other}'")),
        }
    }
    let inner = inner.ok_or("--inner required")?;
    let out = out.ok_or("--out required")?;

    let inner_bytes = fs::read(&inner).map_err(|e| format!("read inner: {e}"))?;
    println!("inner size: {} bytes", inner_bytes.len());

    let spec = TmmModSpec {
        container: "RestylePaperdoll.gpk".to_string(),
        mod_name: "Foglio's PaperDoll Restyle (x64 port)".to_string(),
        mod_author: "foglio1024".to_string(),
        composites: vec![TmmComposite { bytes: inner_bytes }],
    };
    let wrapped = wrap_as_tmm(&spec)?;
    fs::write(&out, &wrapped).map_err(|e| format!("write out: {e}"))?;
    println!("wrapped size: {} bytes", wrapped.len());

    // Round-trip verify with parse_mod_file
    let parsed = gpk::parse_mod_file(&wrapped)?;
    println!("--- round-trip parse ---");
    println!("container: {:?}", parsed.container);
    println!("mod_name: {:?}", parsed.mod_name);
    println!("mod_author: {:?}", parsed.mod_author);
    println!("packages: {}", parsed.packages.len());
    for p in &parsed.packages {
        println!("  - object_path: {:?}, offset: {}, size: {}", p.object_path, p.offset, p.size);
    }
    if parsed.container.is_empty() {
        return Err("round-trip: container is empty (footer not parsed)".into());
    }
    if parsed.packages.is_empty() || parsed.packages[0].object_path.is_empty() {
        return Err("round-trip: composite package object_path is empty".into());
    }
    println!("OK");
    Ok(())
}
