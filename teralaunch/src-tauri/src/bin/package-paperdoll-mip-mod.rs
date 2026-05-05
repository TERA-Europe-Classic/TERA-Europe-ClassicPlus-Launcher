use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[path = "../services/mods/gpk_package.rs"]
mod gpk_package;

#[path = "../services/mods/gpk_resource_inspector.rs"]
mod gpk_resource_inspector;

#[cfg(test)]
#[path = "../services/mods/test_fixtures.rs"]
mod test_fixtures;

const PACKAGE_MAGIC: u32 = 0x9E2A83C1;

fn main() {
    if let Err(err) = run() {
        eprintln!("FAIL: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let mut output = Vec::new();
    let mut offsets = Vec::new();
    for (filename, object_path) in paperdoll_targets() {
        let path = args.candidates_dir.join(filename);
        let bytes =
            fs::read(&path).map_err(|e| format!("failed to read '{}': {e}", path.display()))?;
        let mut bytes = rewrite_package_folder_name(&bytes, &format!("MOD:{object_path}"))?;
        patch_texture_bulk_offsets(&mut bytes)?;
        validate_texture_bulk_offsets(&bytes)?;
        let bytes = recompress_lzo_package(&bytes)?;
        gpk_package::parse_package(&bytes)
            .map_err(|e| format!("rewritten package '{}' does not parse: {e}", path.display()))?;
        offsets.push(output.len() as i32);
        output.extend_from_slice(&bytes);
    }
    let composite_end = output.len() as i32;
    let author_offset = output.len() as i32;
    pack_ansi(&mut output, "TERA Europe ClassicPlus Launcher");
    let name_offset = output.len() as i32;
    pack_ansi(
        &mut output,
        args.container_filename.trim_end_matches(".gpk"),
    );
    let container_offset = output.len() as i32;
    pack_ansi(&mut output, &args.container_filename);
    let offsets_offset = output.len() as i32;
    for offset in &offsets {
        output.extend_from_slice(&offset.to_le_bytes());
    }
    let footer_start = output.len();
    output.extend_from_slice(&PACKAGE_MAGIC.to_le_bytes());
    output.extend_from_slice(&0i32.to_le_bytes());
    output.extend_from_slice(&author_offset.to_le_bytes());
    output.extend_from_slice(&name_offset.to_le_bytes());
    output.extend_from_slice(&container_offset.to_le_bytes());
    output.extend_from_slice(&offsets_offset.to_le_bytes());
    output.extend_from_slice(&(offsets.len() as i32).to_le_bytes());
    output.extend_from_slice(&((output.len() + 8 - composite_end as usize) as i32).to_le_bytes());
    output.extend_from_slice(&PACKAGE_MAGIC.to_le_bytes());

    if let Some(parent) = args.output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "failed to create output directory '{}': {e}",
                parent.display()
            )
        })?;
    }
    fs::write(&args.output_path, output)
        .map_err(|e| format!("failed to write '{}': {e}", args.output_path.display()))?;
    println!(
        "packaged {} PaperDoll resource slices into {} (metadata {} bytes)",
        offsets.len(),
        args.output_path.display(),
        (footer_start + 36).saturating_sub(composite_end as usize)
    );
    Ok(())
}

struct Args {
    candidates_dir: PathBuf,
    output_path: PathBuf,
    container_filename: String,
}

fn parse_args() -> Result<Args, String> {
    let mut args = env::args().skip(1);
    let candidates_dir = args.next().ok_or_else(usage)?;
    let output_path = args.next().ok_or_else(usage)?;
    let container_basename = args
        .next()
        .unwrap_or_else(|| "TMMRestylePaperDoll".to_string());
    if args.next().is_some() {
        return Err(usage());
    }
    Ok(Args {
        candidates_dir: PathBuf::from(candidates_dir),
        output_path: PathBuf::from(output_path),
        container_filename: container_filename(&container_basename)?,
    })
}

fn usage() -> String {
    "usage: package-paperdoll-mip-mod <candidate-dir> <output.gpk> [container-basename]".to_string()
}

fn container_filename(basename: &str) -> Result<String, String> {
    let basename = basename.trim().trim_end_matches(".gpk");
    if basename.is_empty() || !basename.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Err(
            "TMM container basename must contain only Latin letters and numbers".to_string(),
        );
    }
    Ok(format!("{basename}.gpk"))
}

fn paperdoll_targets() -> [(&'static str, &'static str); 11] {
    [
        (
            "PaperDoll_0_0_dup.gpk",
            "ffe86d35_e425ee9e_33ba.PaperDoll_0_0_dup",
        ),
        (
            "PaperDoll_0_1_dup.gpk",
            "ffe86d35_4758f2f8_33b9.PaperDoll_0_1_dup",
        ),
        (
            "PaperDoll_1_0_dup.gpk",
            "ffe86d35_9a62ff60_33b8.PaperDoll_1_0_dup",
        ),
        (
            "PaperDoll_1_1_dup.gpk",
            "ffe86d35_391fe306_33b7.PaperDoll_1_1_dup",
        ),
        (
            "PaperDoll_2_0_dup.gpk",
            "ffe86d35_7136aa7e_33b6.PaperDoll_2_0_dup",
        ),
        (
            "PaperDoll_2_1_dup.gpk",
            "ffe86d35_d24bb618_33b5.PaperDoll_2_1_dup",
        ),
        (
            "PaperDoll_3_0_dup.gpk",
            "ffe86d35_f71bb80_33b4.PaperDoll_3_0_dup",
        ),
        (
            "PaperDoll_3_1_dup.gpk",
            "ffe86d35_ac0ca7e6_33b3.PaperDoll_3_1_dup",
        ),
        (
            "PaperDoll_4_0_dup.gpk",
            "ffe86d35_1a010c57_33b2.PaperDoll_4_0_dup",
        ),
        (
            "PaperDoll_4_1_dup.gpk",
            "ffe86d35_b97c1031_33b1.PaperDoll_4_1_dup",
        ),
        (
            "PaperDoll_5_1_dup.gpk",
            "ffe86d35_c73b01cf_33b0.PaperDoll_5_1_dup",
        ),
    ]
}

fn rewrite_package_folder_name(bytes: &[u8], folder_name: &str) -> Result<Vec<u8>, String> {
    if bytes.len() < 16
        || u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) != PACKAGE_MAGIC
    {
        return Err("input is not a GPK package".to_string());
    }
    let file_version = u16::from_le_bytes([bytes[4], bytes[5]]);
    let old_len = i32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    if old_len <= 0 {
        return Err("package folder name must be an ANSI string".to_string());
    }
    let old_field_len = 4usize
        .checked_add(old_len as usize)
        .ok_or_else(|| "old folder field length overflows usize".to_string())?;
    let old_field_end = 12usize
        .checked_add(old_field_len)
        .ok_or_else(|| "old folder field range overflows usize".to_string())?;
    if old_field_end > bytes.len() {
        return Err("package folder field extends past EOF".to_string());
    }

    let mut out = Vec::with_capacity(bytes.len() + folder_name.len());
    out.extend_from_slice(&bytes[..12]);
    let new_folder_len = folder_name
        .len()
        .checked_add(1)
        .ok_or_else(|| "new folder name length overflows usize".to_string())?;
    out.extend_from_slice(&(new_folder_len as i32).to_le_bytes());
    out.extend_from_slice(folder_name.as_bytes());
    out.push(0);
    out.extend_from_slice(&bytes[old_field_end..]);
    let delta = (4 + new_folder_len) as i64 - old_field_len as i64;
    patch_u32_delta(&mut out, 8, delta)?;

    let package_flags_pos = 12 + 4 + new_folder_len;
    let name_offset_pos = package_flags_pos + 8;
    let export_offset_pos = package_flags_pos + 16;
    let import_offset_pos = package_flags_pos + 24;
    let depends_offset_pos = package_flags_pos + 28;
    patch_u32_delta(&mut out, name_offset_pos, delta)?;
    patch_u32_delta(&mut out, export_offset_pos, delta)?;
    patch_u32_delta(&mut out, import_offset_pos, delta)?;
    patch_u32_delta(&mut out, depends_offset_pos, delta)?;
    if gpk_package::is_x64_file_version(file_version) {
        patch_optional_u32_delta(&mut out, depends_offset_pos + 4, delta)?;
        patch_optional_u32_delta(&mut out, depends_offset_pos + 16, delta)?;
    }
    patch_export_serial_offsets(&mut out, export_offset_pos, delta)?;
    Ok(out)
}

fn patch_texture_bulk_offsets(bytes: &mut [u8]) -> Result<(), String> {
    let package = gpk_package::parse_package(bytes)?;
    let is_x64 = gpk_package::is_x64_file_version(package.summary.file_version);
    for export in package.exports.iter().filter(|export| {
        matches!(
            export.class_name.as_deref(),
            Some("Core.Texture2D") | Some("Core.Engine.Texture2D")
        )
    }) {
        let serial_offset = export
            .serial_offset
            .ok_or_else(|| format!("texture '{}' has no serial offset", export.object_path))?
            as usize;
        for location in
            gpk_resource_inspector::texture_bulk_locations(export, &package.names, is_x64)?
        {
            let expected = serial_offset
                .checked_add(location.payload_offset)
                .ok_or_else(|| "texture bulk payload offset overflows usize".to_string())?;
            let field = serial_offset
                .checked_add(location.offset_in_file_field_offset)
                .ok_or_else(|| "texture bulk offset field overflows usize".to_string())?;
            write_i32_at(bytes, field, expected as i32)?;
        }
    }
    Ok(())
}

fn validate_texture_bulk_offsets(bytes: &[u8]) -> Result<(), String> {
    let package = gpk_package::parse_package(bytes)?;
    let is_x64 = gpk_package::is_x64_file_version(package.summary.file_version);
    for export in package.exports.iter().filter(|export| {
        matches!(
            export.class_name.as_deref(),
            Some("Core.Texture2D") | Some("Core.Engine.Texture2D")
        )
    }) {
        let serial_offset = export
            .serial_offset
            .ok_or_else(|| format!("texture '{}' has no serial offset", export.object_path))?
            as usize;
        for location in
            gpk_resource_inspector::texture_bulk_locations(export, &package.names, is_x64)?
        {
            let expected = serial_offset
                .checked_add(location.payload_offset)
                .ok_or_else(|| "texture bulk payload offset overflows usize".to_string())?;
            if location.offset_in_file != expected as i32 {
                return Err(format!(
                    "texture '{}' bulk offset is {}, expected {}",
                    export.object_path, location.offset_in_file, expected
                ));
            }
        }
    }
    Ok(())
}

fn recompress_lzo_package(bytes: &[u8]) -> Result<Vec<u8>, String> {
    const BLOCK_SIZE: usize = 131072;
    const PRE_CHUNK_FILLER: [u32; 4] = [7, 0, 1, 2];
    let (_, package_flags_pos, compression_flags_pos, chunk_count_pos, name_offset) =
        locate_header_fields(bytes)?;
    let body = bytes
        .get(name_offset..)
        .ok_or_else(|| "name offset is outside package bytes".to_string())?;
    let mut compressed_blocks = Vec::new();
    let mut block_table = Vec::new();
    for chunk in body.chunks(BLOCK_SIZE) {
        let compressed = lzokay::compress::compress(chunk)
            .map_err(|e| format!("failed to LZO-compress package block: {e}"))?;
        block_table.push((compressed.len() as u32, chunk.len() as u32));
        compressed_blocks.push(compressed);
    }

    let compressed_payload_size: usize = compressed_blocks.iter().map(Vec::len).sum();
    let chunk_on_disk_size = 16 + block_table.len() * 8 + compressed_payload_size;
    let chunk_offset = name_offset
        .checked_add(16)
        .ok_or_else(|| "chunk offset overflows usize".to_string())?;
    let mut out = Vec::with_capacity(chunk_offset + chunk_on_disk_size);
    out.extend_from_slice(&bytes[..name_offset]);
    for value in PRE_CHUNK_FILLER {
        out.extend_from_slice(&value.to_le_bytes());
    }

    let mut package_flags = read_u32_at(&out, package_flags_pos)?;
    package_flags |= 0x02000000;
    write_u32_at(&mut out, package_flags_pos, package_flags)?;
    write_u32_at(&mut out, compression_flags_pos, 2)?;
    write_u32_at(&mut out, chunk_count_pos, 1)?;
    let chunk_header = chunk_count_pos + 4;
    write_u32_at(&mut out, chunk_header, name_offset as u32)?;
    write_u32_at(&mut out, chunk_header + 4, body.len() as u32)?;
    write_u32_at(&mut out, chunk_header + 8, chunk_offset as u32)?;
    write_u32_at(&mut out, chunk_header + 12, chunk_on_disk_size as u32)?;

    out.extend_from_slice(&PACKAGE_MAGIC.to_le_bytes());
    out.extend_from_slice(&(BLOCK_SIZE as u32).to_le_bytes());
    out.extend_from_slice(&(compressed_payload_size as u32).to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    for (compressed_len, uncompressed_len) in &block_table {
        out.extend_from_slice(&compressed_len.to_le_bytes());
        out.extend_from_slice(&uncompressed_len.to_le_bytes());
    }
    for block in compressed_blocks {
        out.extend_from_slice(&block);
    }
    Ok(out)
}

fn locate_header_fields(bytes: &[u8]) -> Result<(u16, usize, usize, usize, usize), String> {
    if read_u32_at(bytes, 0)? != PACKAGE_MAGIC {
        return Err("input is not a GPK package".to_string());
    }
    let file_version = u16::from_le_bytes([
        *bytes
            .get(4)
            .ok_or_else(|| "missing file version".to_string())?,
        *bytes
            .get(5)
            .ok_or_else(|| "missing file version".to_string())?,
    ]);
    let mut cursor = 8usize;
    cursor += 4;
    cursor = fstring_end(bytes, cursor)?;
    let package_flags_pos = cursor;
    cursor += 4;
    cursor += 4;
    let name_offset = read_u32_at(bytes, cursor)? as usize;
    cursor += 4;
    cursor += 20;
    if gpk_package::is_x64_file_version(file_version) {
        cursor += 16;
    }
    cursor += 16;
    let generation_count = read_u32_at(bytes, cursor)? as usize;
    cursor += 4 + generation_count.saturating_mul(12) + 8;
    Ok((
        file_version,
        package_flags_pos,
        cursor,
        cursor + 4,
        name_offset,
    ))
}

fn fstring_end(bytes: &[u8], offset: usize) -> Result<usize, String> {
    let len = i32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| "FString length is outside package bytes".to_string())?
            .try_into()
            .map_err(|_| "FString length slice has wrong size".to_string())?,
    );
    if len >= 0 {
        Ok(offset + 4 + len as usize)
    } else {
        Ok(offset + 4 + (-len as usize) * 2)
    }
}

fn patch_export_serial_offsets(
    bytes: &mut [u8],
    export_offset_pos: usize,
    delta: i64,
) -> Result<(), String> {
    let export_count_pos = export_offset_pos - 4;
    let export_count = read_u32_at(bytes, export_count_pos)? as usize;
    let mut cursor = read_u32_at(bytes, export_offset_pos)? as usize;
    for _ in 0..export_count {
        cursor = cursor
            .checked_add(32)
            .ok_or_else(|| "export cursor overflows usize".to_string())?;
        let serial_size = read_u32_at(bytes, cursor)?;
        cursor += 4;
        if serial_size > 0 {
            patch_u32_delta(bytes, cursor, delta)?;
            cursor += 4;
        }
        let _export_flags = read_u32_at(bytes, cursor)?;
        cursor += 4;
        let extra_count = read_u32_at(bytes, cursor)? as usize;
        cursor += 4;
        let _unk4 = read_u32_at(bytes, cursor)?;
        cursor += 4;
        cursor = cursor
            .checked_add(16 + extra_count.saturating_mul(4))
            .ok_or_else(|| "export cursor overflows usize".to_string())?;
        if cursor > bytes.len() {
            return Err("export table extends past EOF while patching serial offsets".to_string());
        }
    }
    Ok(())
}

fn patch_optional_u32_delta(bytes: &mut [u8], pos: usize, delta: i64) -> Result<(), String> {
    if read_u32_at(bytes, pos)? == 0 {
        return Ok(());
    }
    patch_u32_delta(bytes, pos, delta)
}

fn patch_u32_delta(bytes: &mut [u8], pos: usize, delta: i64) -> Result<(), String> {
    let value = read_u32_at(bytes, pos)? as i64;
    let patched = value
        .checked_add(delta)
        .ok_or_else(|| "u32 offset delta overflows i64".to_string())?;
    if !(0..=u32::MAX as i64).contains(&patched) {
        return Err(format!("patched offset {patched} is outside u32 range"));
    }
    bytes[pos..pos + 4].copy_from_slice(&(patched as u32).to_le_bytes());
    Ok(())
}

fn write_i32_at(bytes: &mut [u8], pos: usize, value: i32) -> Result<(), String> {
    let end = pos
        .checked_add(4)
        .ok_or_else(|| "write offset overflows usize".to_string())?;
    let len = bytes.len();
    let data = bytes
        .get_mut(pos..end)
        .ok_or_else(|| format!("write {pos}..{end} is outside {len} bytes"))?;
    data.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u32_at(bytes: &mut [u8], pos: usize, value: u32) -> Result<(), String> {
    let end = pos
        .checked_add(4)
        .ok_or_else(|| "write offset overflows usize".to_string())?;
    let len = bytes.len();
    let data = bytes
        .get_mut(pos..end)
        .ok_or_else(|| format!("write {pos}..{end} is outside {len} bytes"))?;
    data.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn read_u32_at(bytes: &[u8], pos: usize) -> Result<u32, String> {
    let end = pos
        .checked_add(4)
        .ok_or_else(|| "read offset overflows usize".to_string())?;
    let data = bytes
        .get(pos..end)
        .ok_or_else(|| format!("read {pos}..{end} is outside {} bytes", bytes.len()))?;
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

fn pack_ansi(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as i32).to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

#[allow(dead_code)]
fn require_existing_dir(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(format!("'{}' is not a directory", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_filename_accepts_alphanumeric_basename() {
        assert_eq!(
            container_filename("TMMRestylePaperDoll").expect("valid basename"),
            "TMMRestylePaperDoll.gpk"
        );
    }

    #[test]
    fn container_filename_rejects_punctuation() {
        let err = container_filename("foglio1024.restyle-paperdoll.resources-x64")
            .expect_err("punctuation is not TMM-safe");

        assert!(err.contains("Latin letters and numbers"));
    }
}
