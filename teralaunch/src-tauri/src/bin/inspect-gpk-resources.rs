use std::collections::BTreeMap;
use std::{env, fs, path::PathBuf};

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
    let package_path = parse_args()?;
    let bytes = fs::read(&package_path)
        .map_err(|e| format!("failed to read '{}': {e}", package_path.display()))?;
    let package = gpk_package::parse_package(&bytes)?;
    let textures = gpk_resource_inspector::inspect_texture_exports(&package)?;
    let redirectors = gpk_resource_inspector::inspect_redirector_exports(&package)?;

    println!("package={}", package.summary.package_name);
    println!("file_version={}", package.summary.file_version);
    println!("license_version={}", package.summary.license_version);
    println!("names={}", package.names.len());
    println!("imports={}", package.imports.len());
    println!("exports={}", package.exports.len());
    let mut class_counts = BTreeMap::new();
    for export in &package.exports {
        let class_name = export.class_name.as_deref().unwrap_or("<unresolved>");
        *class_counts.entry(class_name).or_insert(0usize) += 1;
    }
    for (class_name, count) in class_counts {
        println!("class={class_name} count={count}");
    }
    println!("texture_exports={}", textures.len());
    for texture in textures {
        println!(
            "texture={} properties={} native_offset={} native_size={} source_art_size={} source_file_path={} mip_count={} first_mip={} cached_mip_count={} max_cached_resolution={}",
            texture.object_path,
            texture.property_count,
            texture.native_data_offset,
            texture.native_data_size,
            texture.source_art_size,
            texture.source_file_path.as_deref().unwrap_or("<none>"),
            texture.mip_count,
            texture
                .first_mip
                .as_ref()
                .map(|mip| format!(
                    "flags:0x{:X},elements:{},disk:{},size:{}x{}",
                    mip.flags, mip.element_count, mip.size_on_disk, mip.size_x, mip.size_y
                ))
                .unwrap_or_else(|| "<none>".to_string()),
            texture
                .cached_mip_count
                .map(|value| value.to_string())
                .unwrap_or_else(|| "<none>".to_string()),
            texture
                .max_cached_resolution
                .map(|value| value.to_string())
                .unwrap_or_else(|| "<none>".to_string())
        );
    }
    println!("redirector_exports={}", redirectors.len());
    for redirector in redirectors {
        println!(
            "redirector={} target_index={} target={}",
            redirector.object_path,
            redirector.target_index,
            redirector.target_path.as_deref().unwrap_or("<unresolved>")
        );
    }

    Ok(())
}

fn parse_args() -> Result<PathBuf, String> {
    let mut args = env::args().skip(1);
    let package_path = args
        .next()
        .ok_or_else(|| "usage: inspect-gpk-resources <package.gpk>".to_string())?;
    if args.next().is_some() {
        return Err("usage: inspect-gpk-resources <package.gpk>".to_string());
    }
    Ok(PathBuf::from(package_path))
}
