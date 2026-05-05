// smoke-texture-swap — minimal validator: try to swap ONE specific texture
// from foglio x32 source into vanilla x64 target. Reports success/failure
// per texture name. Runs no FS writes.

use std::env;
use std::fs;
use std::path::PathBuf;

#[path = "../services/mods/gpk_package.rs"]
mod gpk_package;
#[path = "../services/mods/gpk_resource_inspector.rs"]
mod gpk_resource_inspector;

const USAGE: &str =
    "smoke-texture-swap --vanilla-x64 <path> --mod-x32 <path> [--texture <name>]";

fn main() {
    if let Err(e) = run() {
        eprintln!("FAIL: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut vanilla_x64: Option<PathBuf> = None;
    let mut mod_x32: Option<PathBuf> = None;
    let mut texture: Option<String> = None;

    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--vanilla-x64" => vanilla_x64 = iter.next().map(PathBuf::from),
            "--mod-x32" => mod_x32 = iter.next().map(PathBuf::from),
            "--texture" => texture = iter.next(),
            other => return Err(format!("Unknown arg '{other}'\n{USAGE}")),
        }
    }
    let vanilla_path = vanilla_x64.ok_or("--vanilla-x64 required")?;
    let x32_path = mod_x32.ok_or("--mod-x32 required")?;

    let v_bytes = gpk_package::extract_uncompressed_package_bytes(&fs::read(&vanilla_path)
        .map_err(|e| format!("read vanilla: {e}"))?)?;
    let s_bytes = gpk_package::extract_uncompressed_package_bytes(&fs::read(&x32_path)
        .map_err(|e| format!("read x32: {e}"))?)?;

    let v_pkg = gpk_package::parse_package(&v_bytes).map_err(|e| format!("parse vanilla: {e}"))?;
    let s_pkg = gpk_package::parse_package(&s_bytes).map_err(|e| format!("parse x32: {e}"))?;

    println!("vanilla x64: file_version={}, names={}, exports={}",
        v_pkg.summary.file_version, v_pkg.names.len(), v_pkg.exports.len());
    println!("foglio x32:  file_version={}, names={}, exports={}",
        s_pkg.summary.file_version, s_pkg.names.len(), s_pkg.exports.len());
    println!();

    let v_is_x64 = v_pkg.summary.file_version >= gpk_package::X64_VERSION_THRESHOLD;
    let s_is_x64 = s_pkg.summary.file_version >= gpk_package::X64_VERSION_THRESHOLD;

    let v_textures: Vec<&_> = v_pkg.exports.iter()
        .filter(|e| matches!(e.class_name.as_deref(),
            Some("Core.Texture2D") | Some("Core.Engine.Texture2D")))
        .collect();
    let s_textures: Vec<&_> = s_pkg.exports.iter()
        .filter(|e| matches!(e.class_name.as_deref(),
            Some("Core.Texture2D") | Some("Core.Engine.Texture2D")))
        .collect();
    println!("vanilla Texture2D count: {}", v_textures.len());
    println!("foglio  Texture2D count: {}", s_textures.len());
    println!();

    // Build name->export maps. Match by "object_name" (the name component
    // after the last dot). Try both with and without "_dup" suffix.
    fn short_name(path: &str) -> &str {
        path.rsplit('.').next().unwrap_or(path)
    }
    let s_by_name: std::collections::HashMap<String, &_> = s_textures.iter()
        .map(|e| (short_name(&e.object_path).to_string(), *e))
        .collect();

    let mut tried = 0usize;
    let mut matched = 0usize;
    let mut succeeded = 0usize;
    let mut errs: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for v_export in &v_textures {
        let v_short = short_name(&v_export.object_path);
        if let Some(filter) = &texture {
            if v_short != filter && !v_short.contains(filter) { continue; }
        }
        tried += 1;
        // Try exact match first, then strip "_dup" suffix from target.
        let v_short_stripped = v_short.strip_suffix("_dup").unwrap_or(v_short);
        let s_export = s_by_name.get(v_short)
            .or_else(|| s_by_name.get(v_short_stripped))
            .copied();
        let Some(s_export) = s_export else {
            *errs.entry("no source match".to_string()).or_insert(0) += 1;
            if texture.is_some() {
                println!("  '{v_short}': no foglio match");
            }
            continue;
        };
        matched += 1;

        match gpk_resource_inspector::replace_texture_first_mip_pixels(
            v_export, &v_pkg.names, v_is_x64,
            s_export, &s_pkg.names, s_is_x64,
        ) {
            Ok(rewritten) => {
                succeeded += 1;
                let differs = rewritten != v_export.payload;
                let mip = gpk_resource_inspector::first_mip_bulk_location(
                    v_export, &v_pkg.names, v_is_x64).ok();
                let mip_differs = if let Some(loc) = &mip {
                    let end = loc.payload_offset + loc.payload_len;
                    rewritten[loc.payload_offset..end] != v_export.payload[loc.payload_offset..end]
                } else { false };
                if !mip_differs { *errs.entry("swap was no-op (mip identical)".into()).or_insert(0) += 1; }
                if texture.is_some() {
                    println!("  '{v_short}': OK  (payload_changed={differs}, mip_changed={mip_differs})");
                }
            }
            Err(e) => {
                let key = e.split(';').next().unwrap_or(&e).to_string();
                *errs.entry(key).or_insert(0) += 1;
                if texture.is_some() {
                    println!("  '{v_short}': FAIL: {e}");
                }
            }
        }
    }

    println!("\nSummary:");
    println!("  Texture2D in target: {}", v_textures.len());
    println!("  Tried (filtered):    {tried}");
    println!("  Name-matched:        {matched}");
    println!("  Swap succeeded:      {succeeded}");
    if !errs.is_empty() {
        println!("\nError reasons (count):");
        let mut sorted: Vec<_> = errs.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (e, n) in sorted {
            println!("  {n:4}× {e}");
        }
    }
    Ok(())
}
