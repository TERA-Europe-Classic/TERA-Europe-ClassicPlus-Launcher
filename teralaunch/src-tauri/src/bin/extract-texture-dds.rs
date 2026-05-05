//! Extract a Texture2D's first-mip pixel data as a DDS file from any GPK
//! source: a standalone GPK file OR a vanilla composite slice resolved by
//! logical path. Used for visual diagnosis of mod vs vanilla textures.

#[path = "../services/mods/composite_extract.rs"] mod composite_extract;
#[path = "../services/mods/gpk.rs"] mod gpk;
#[path = "../services/mods/gpk_package.rs"] mod gpk_package;
#[path = "../services/mods/gpk_property.rs"] mod gpk_property;
#[path = "../services/mods/gpk_resource_inspector.rs"] mod gpk_resource_inspector;

use std::env;
use std::fs;
use std::path::PathBuf;

const USAGE: &str = "extract-texture-dds [--game-root <path> --logical <Pkg.Object> | --gpk <path> --object <Name>] --out <path.dds>";

fn main() {
    if let Err(e) = run() {
        eprintln!("FAIL: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut game_root: Option<PathBuf> = None;
    let mut logical: Option<String> = None;
    let mut gpk_path: Option<PathBuf> = None;
    let mut object_name: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut iter = env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--game-root" => game_root = iter.next().map(PathBuf::from),
            "--logical" => logical = iter.next(),
            "--gpk" => gpk_path = iter.next().map(PathBuf::from),
            "--object" => object_name = iter.next(),
            "--out" => out = iter.next().map(PathBuf::from),
            other => return Err(format!("unknown arg '{other}'\n{USAGE}")),
        }
    }
    let out = out.ok_or_else(|| format!("--out required\n{USAGE}"))?;

    let bytes = if let (Some(gr), Some(lp)) = (game_root.as_ref(), logical.as_ref()) {
        composite_extract::extract_vanilla_for_logical_path(gr, lp)?
    } else if let Some(p) = gpk_path.as_ref() {
        fs::read(p).map_err(|e| format!("read {}: {e}", p.display()))?
    } else {
        return Err(format!("provide --game-root + --logical OR --gpk\n{USAGE}"));
    };

    let pkg = gpk_package::parse_package(&bytes)?;
    let is_x64 = pkg.summary.file_version >= gpk_package::X64_VERSION_THRESHOLD;

    // Pick the export: explicit --object, or fall back to the trailing part of --logical, or first Texture2D.
    let want = object_name.as_deref()
        .or_else(|| logical.as_deref().and_then(|s| s.rsplit('.').next()))
        .map(|s| s.to_string());
    let export = pkg.exports.iter().find(|e| {
        let is_tex = e.class_name.as_deref().map(|c| c.rsplit('.').next() == Some("Texture2D")).unwrap_or(false);
        if !is_tex { return false; }
        match &want {
            Some(w) => e.object_name == *w
                || e.object_name == format!("{w}_dup")
                || e.object_name.strip_suffix("_dup") == Some(w),
            None => true,
        }
    }).ok_or_else(|| format!("no Texture2D export matching {want:?}"))?;

    let (pixels, w, h, _elements) = gpk_resource_inspector::decode_first_mip(export, &pkg.names, is_x64)?;

    // For TERA UI textures the engine uses DXT5 (compressed). decode_first_mip
    // returns the raw mip payload (whatever format the texture stores). For
    // 256x256 DXT5 that's 65536 bytes and we wrap it in a DDS header. If the
    // size doesn't match DXT5, just dump as-is and warn.
    let dxt5_expected = ((w as usize).max(1) * (h as usize).max(1)).max(16);
    println!("export: {} ({}x{}, {} bytes mip)", export.object_name, w, h, pixels.len());
    println!("dxt5_expected_block_bytes: {dxt5_expected}");

    let dds = make_dds_dxt5(w as u32, h as u32, &pixels);
    fs::write(&out, &dds).map_err(|e| format!("write {}: {e}", out.display()))?;
    println!("wrote: {} ({} bytes)", out.display(), dds.len());
    Ok(())
}

fn make_dds_dxt5(width: u32, height: u32, dxt: &[u8]) -> Vec<u8> {
    let mut h = vec![0u8; 128];
    h[0..4].copy_from_slice(b"DDS ");
    write_u32(&mut h, 4, 124);
    write_u32(&mut h, 8, 0x000A1007);
    write_u32(&mut h, 12, height);
    write_u32(&mut h, 16, width);
    write_u32(&mut h, 20, dxt.len() as u32);
    write_u32(&mut h, 28, 1);
    write_u32(&mut h, 76, 32);
    write_u32(&mut h, 80, 0x4);
    h[84..88].copy_from_slice(b"DXT5");
    write_u32(&mut h, 108, 0x401008);
    let mut out = h;
    out.extend_from_slice(dxt);
    out
}

fn write_u32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
