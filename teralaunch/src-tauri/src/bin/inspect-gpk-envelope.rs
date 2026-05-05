use std::{env, fs, path::PathBuf};

const PACKAGE_MAGIC: u32 = 0x9E2A83C1;

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
    let bytes = fs::read(&path).map_err(|e| format!("failed to read '{}': {e}", path.display()))?;
    let envelope = inspect_envelope(&bytes)?;
    println!("file={}", path.display());
    println!("file_version={}", envelope.file_version);
    println!("folder={}", envelope.folder);
    println!("package_flags=0x{:08X}", envelope.package_flags);
    println!("name_offset={}", envelope.name_offset);
    println!("compression_flags={}", envelope.compression_flags);
    println!("chunk_count={}", envelope.chunk_count);
    println!("chunk_header={:?}", envelope.chunk_header);
    println!("pre_chunk_filler={:?}", envelope.pre_chunk_filler);
    println!("physical_len={}", bytes.len());
    Ok(())
}

struct Envelope {
    file_version: u16,
    folder: String,
    package_flags: u32,
    name_offset: u32,
    compression_flags: u32,
    chunk_count: u32,
    chunk_header: Option<[u32; 4]>,
    pre_chunk_filler: Option<[u32; 4]>,
}

fn inspect_envelope(bytes: &[u8]) -> Result<Envelope, String> {
    if read_u32(bytes, 0)? != PACKAGE_MAGIC {
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
    let (folder, folder_end) = read_fstring(bytes, cursor)?;
    cursor = folder_end;
    let package_flags = read_u32(bytes, cursor)?;
    cursor += 4;
    cursor += 4;
    let name_offset = read_u32(bytes, cursor)?;
    cursor += 4;
    cursor += 20;
    if file_version >= 0x381 {
        cursor += 16;
    }
    cursor += 16;
    let generation_count = read_u32(bytes, cursor)? as usize;
    cursor += 4 + generation_count.saturating_mul(12) + 8;
    let compression_flags = read_u32(bytes, cursor)?;
    cursor += 4;
    let chunk_count = read_u32(bytes, cursor)?;
    cursor += 4;
    let chunk_header = if chunk_count > 0 {
        Some([
            read_u32(bytes, cursor)?,
            read_u32(bytes, cursor + 4)?,
            read_u32(bytes, cursor + 8)?,
            read_u32(bytes, cursor + 12)?,
        ])
    } else {
        None
    };
    let pre_chunk_filler = chunk_header.and_then(|chunk| {
        let start = name_offset as usize;
        let end = chunk[2] as usize;
        if end.checked_sub(start)? == 16 {
            Some([
                read_u32(bytes, start).ok()?,
                read_u32(bytes, start + 4).ok()?,
                read_u32(bytes, start + 8).ok()?,
                read_u32(bytes, start + 12).ok()?,
            ])
        } else {
            None
        }
    });
    Ok(Envelope {
        file_version,
        folder,
        package_flags,
        name_offset,
        compression_flags,
        chunk_count,
        chunk_header,
        pre_chunk_filler,
    })
}

fn read_fstring(bytes: &[u8], offset: usize) -> Result<(String, usize), String> {
    let len = i32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| "FString length is outside package bytes".to_string())?
            .try_into()
            .map_err(|_| "FString length slice has wrong size".to_string())?,
    );
    if len >= 0 {
        let start = offset + 4;
        let end = start + len as usize;
        let raw = bytes
            .get(start..end)
            .ok_or_else(|| "FString bytes are outside package bytes".to_string())?;
        let nul = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
        Ok((String::from_utf8_lossy(&raw[..nul]).to_string(), end))
    } else {
        let start = offset + 4;
        let end = start + (-len as usize) * 2;
        Ok((format!("<utf16:{} bytes>", end - start), end))
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let data = bytes.get(offset..offset + 4).ok_or_else(|| {
        format!(
            "read {offset}..{} outside {} bytes",
            offset + 4,
            bytes.len()
        )
    })?;
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

fn usage() -> String {
    "usage: inspect-gpk-envelope <package.gpk>".to_string()
}
