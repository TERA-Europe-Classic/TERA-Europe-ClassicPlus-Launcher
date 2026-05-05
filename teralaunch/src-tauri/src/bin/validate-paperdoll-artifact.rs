use std::{env, fs, path::PathBuf};

#[path = "../services/mods/gpk.rs"]
mod gpk;

#[path = "../services/mods/gpk_package.rs"]
mod gpk_package;

#[path = "../services/mods/gpk_resource_inspector.rs"]
mod gpk_resource_inspector;

fn main() {
    if let Err(err) = run() {
        eprintln!("FAIL: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let artifact = PathBuf::from(args.next().ok_or_else(usage)?);
    if args.next().is_some() {
        return Err(usage());
    }
    let bytes =
        fs::read(&artifact).map_err(|e| format!("failed to read '{}': {e}", artifact.display()))?;
    let modfile = gpk::parse_mod_file(&bytes)?;
    if modfile.packages.len() != 11 {
        return Err(format!(
            "expected 11 PaperDoll packages, found {}",
            modfile.packages.len()
        ));
    }
    for package in &modfile.packages {
        let start = package.offset as usize;
        let end = start
            .checked_add(package.size as usize)
            .ok_or_else(|| format!("package '{}' range overflows usize", package.object_path))?;
        let slice = bytes.get(start..end).ok_or_else(|| {
            format!(
                "package '{}' range is outside artifact",
                package.object_path
            )
        })?;
        let parsed = gpk_package::parse_package(slice)?;
        if parsed.summary.file_version < gpk_package::X64_VERSION_THRESHOLD {
            return Err(format!("package '{}' is not x64", package.object_path));
        }
        if parsed.summary.compression_flags != 2 {
            return Err(format!(
                "package '{}' compression flag is {}, expected 2",
                package.object_path, parsed.summary.compression_flags
            ));
        }
        let textures = gpk_resource_inspector::inspect_texture_exports(&parsed)?;
        if textures.len() != 1 {
            return Err(format!(
                "package '{}' has {} Texture2D exports, expected 1",
                package.object_path,
                textures.len()
            ));
        }
        let export = parsed
            .exports
            .iter()
            .find(|export| {
                matches!(
                    export.class_name.as_deref(),
                    Some("Core.Texture2D") | Some("Core.Engine.Texture2D")
                )
            })
            .ok_or_else(|| {
                format!(
                    "package '{}' Texture2D export is missing",
                    package.object_path
                )
            })?;
        let serial_offset = export.serial_offset.ok_or_else(|| {
            format!(
                "package '{}' texture has no serial offset",
                package.object_path
            )
        })? as usize;
        for location in gpk_resource_inspector::texture_bulk_locations(export, &parsed.names, true)?
        {
            let expected = serial_offset + location.payload_offset;
            if location.offset_in_file != expected as i32 {
                return Err(format!(
                    "package '{}' bulk offset is {}, expected {}",
                    package.object_path, location.offset_in_file, expected
                ));
            }
        }
        println!(
            "ok package={} size={} comp={} bulk_offsets=ok",
            package.object_path, package.size, parsed.summary.compression_flags
        );
    }
    Ok(())
}

fn usage() -> String {
    "usage: validate-paperdoll-artifact <artifact.gpk>".to_string()
}
