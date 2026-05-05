use std::{env, fs, path::PathBuf};

#[path = "../services/mods/gpk_package.rs"]
mod gpk_package;

#[path = "../services/mods/gpk_resource_inspector.rs"]
mod gpk_resource_inspector;

#[cfg(test)]
#[path = "../services/mods/test_fixtures.rs"]
mod test_fixtures;

fn main() {
    if let Err(err) = run() {
        eprintln!("FAIL: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let target_bytes = fs::read(&args.target_path).map_err(|e| {
        format!(
            "failed to read target '{}': {e}",
            args.target_path.display()
        )
    })?;
    let source_bytes = fs::read(&args.source_path).map_err(|e| {
        format!(
            "failed to read source '{}': {e}",
            args.source_path.display()
        )
    })?;
    let target_bytes = gpk_package::extract_uncompressed_package_bytes(&target_bytes)?;
    let source_bytes = gpk_package::extract_uncompressed_package_bytes(&source_bytes)?;
    let target_package = gpk_package::parse_package(&target_bytes)?;
    let source_package = gpk_package::parse_package(&source_bytes)?;

    let target_is_x64 = gpk_package::is_x64_file_version(target_package.summary.file_version);
    let source_is_x64 = gpk_package::is_x64_file_version(source_package.summary.file_version);
    let target_export = single_texture_export(&target_package, "target")?;
    let target_texture = single_texture_inspection(&target_package, "target")?;
    let target_basename = source_basename(&target_texture.source_file_path).ok_or_else(|| {
        format!(
            "target texture '{}' has no source file path",
            target_texture.object_path
        )
    })?;
    let source_export = matching_source_export(&source_package, &target_basename, source_is_x64)?;
    let rewritten_payload = gpk_resource_inspector::replace_texture_first_mip_pixels(
        target_export,
        &target_package.names,
        target_is_x64,
        source_export,
        &source_package.names,
        source_is_x64,
    )?;
    if rewritten_payload.len() != target_export.payload.len() {
        return Err(format!(
            "rewritten payload changed size from {} to {} bytes",
            target_export.payload.len(),
            rewritten_payload.len()
        ));
    }

    let serial_offset = target_export.serial_offset.ok_or_else(|| {
        format!(
            "target texture '{}' has no serial offset",
            target_export.object_path
        )
    })? as usize;
    let serial_end = serial_offset
        .checked_add(target_export.serial_size as usize)
        .ok_or_else(|| "target export serial range overflows usize".to_string())?;
    if target_export.serial_size as usize != rewritten_payload.len() {
        return Err(format!(
            "target export serial size {} differs from payload size {}",
            target_export.serial_size,
            rewritten_payload.len()
        ));
    }
    let mut output_bytes = target_bytes;
    let target_range = output_bytes
        .get_mut(serial_offset..serial_end)
        .ok_or_else(|| "target export serial range is outside package bytes".to_string())?;
    target_range.copy_from_slice(&rewritten_payload);
    if let Some(parent) = args.output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "failed to create output directory '{}': {e}",
                parent.display()
            )
        })?;
    }
    fs::write(&args.output_path, output_bytes).map_err(|e| {
        format!(
            "failed to write output '{}': {e}",
            args.output_path.display()
        )
    })?;

    println!(
        "rewrote target={} source={} basename={} output={}",
        target_export.object_path,
        source_export.object_path,
        target_basename,
        args.output_path.display()
    );
    Ok(())
}

struct Args {
    target_path: PathBuf,
    source_path: PathBuf,
    output_path: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let mut args = env::args().skip(1);
    let target_path = args.next().ok_or_else(usage)?;
    let source_path = args.next().ok_or_else(usage)?;
    let output_path = args.next().ok_or_else(usage)?;
    if args.next().is_some() {
        return Err(usage());
    }
    Ok(Args {
        target_path: PathBuf::from(target_path),
        source_path: PathBuf::from(source_path),
        output_path: PathBuf::from(output_path),
    })
}

fn usage() -> String {
    "usage: rewrite-paperdoll-mip <target-x64-slice.gpk> <source-s1uires-skin.gpk> <output.gpk>"
        .to_string()
}

fn single_texture_export<'a>(
    package: &'a gpk_package::GpkPackage,
    label: &str,
) -> Result<&'a gpk_package::GpkExportEntry, String> {
    let textures: Vec<_> = package
        .exports
        .iter()
        .filter(|export| {
            matches!(
                export.class_name.as_deref(),
                Some("Core.Texture2D") | Some("Core.Engine.Texture2D")
            )
        })
        .collect();
    if textures.len() != 1 {
        return Err(format!(
            "{label} package must contain exactly one Texture2D export, found {}",
            textures.len()
        ));
    }
    Ok(textures[0])
}

fn single_texture_inspection(
    package: &gpk_package::GpkPackage,
    label: &str,
) -> Result<gpk_resource_inspector::TextureExportInspection, String> {
    let textures = gpk_resource_inspector::inspect_texture_exports(package)?;
    if textures.len() != 1 {
        return Err(format!(
            "{label} package must inspect exactly one Texture2D export, found {}",
            textures.len()
        ));
    }
    Ok(textures[0].clone())
}

fn matching_source_export<'a>(
    package: &'a gpk_package::GpkPackage,
    target_basename: &str,
    source_is_x64: bool,
) -> Result<&'a gpk_package::GpkExportEntry, String> {
    let textures = gpk_resource_inspector::inspect_texture_exports(package)?;
    let mut matches = Vec::new();
    for texture in textures {
        if source_basename(&texture.source_file_path).as_deref() == Some(target_basename) {
            matches.push(texture.object_path);
        }
    }
    if matches.len() != 1 {
        return Err(format!(
            "source package must contain exactly one Texture2D with source basename '{target_basename}', found {}",
            matches.len()
        ));
    }
    let object_path = &matches[0];
    let export = package
        .exports
        .iter()
        .find(|export| export.object_path == *object_path)
        .ok_or_else(|| {
            format!("matched source texture '{object_path}' is missing from export table")
        })?;
    let inspection = gpk_resource_inspector::inspect_texture_exports(package)?
        .into_iter()
        .find(|texture| texture.object_path == *object_path)
        .ok_or_else(|| format!("matched source texture '{object_path}' is not inspectable"))?;
    let source_mip = inspection
        .first_mip
        .ok_or_else(|| format!("matched source texture '{object_path}' has no first mip"))?;
    if source_is_x64 && source_mip.flags != 0 {
        return Err(format!(
            "matched source texture '{object_path}' is x64 but first mip is not uncompressed"
        ));
    }
    Ok(export)
}

fn source_basename(source_file_path: &Option<String>) -> Option<String> {
    source_file_path.as_ref().and_then(|value| {
        value
            .rsplit(['\\', '/'])
            .next()
            .filter(|name| !name.is_empty())
            .map(|name| name.to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_basename_normalizes_windows_paths() {
        assert_eq!(
            source_basename(&Some(
                "D:\\UI\\Resource\\TGA\\PaperDoll_0_0.tga".to_string()
            )),
            Some("paperdoll_0_0.tga".to_string())
        );
    }
}
