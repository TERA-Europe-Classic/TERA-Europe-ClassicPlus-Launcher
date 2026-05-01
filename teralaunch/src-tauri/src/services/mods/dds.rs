//! Minimal DDS reader for DXT1 / DXT3 / DXT5 textures.
//!
//! Parses the `DDSURFACEDESC2` header (Microsoft DirectX SDK) and slices the
//! raw block-compressed pixel payload into per-mip `Vec<u8>`. Strict YAGNI:
//! no encoding, no non-DXT formats, no DX10 extended header.
//!
//! Layout reference:
//! - Bytes 0..4   : magic `"DDS "` (`0x20534444`).
//! - Bytes 4..128 : 124-byte header (`DDSURFACEDESC2`).
//! - Bytes 128..  : block-compressed pixel data, mip 0 first.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DdsPixelFormat {
    Dxt1,
    Dxt3,
    Dxt5,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdsImage {
    pub width: u32,
    pub height: u32,
    pub format: DdsPixelFormat,
    /// One Vec per mip level (index 0 = primary). Always at least one entry.
    pub mips: Vec<Vec<u8>>,
}

const DDS_MAGIC: &[u8; 4] = b"DDS ";
const HEADER_LEN: usize = 124;
const PIXEL_DATA_OFFSET: usize = 4 + HEADER_LEN;

// Field offsets within the file (magic + header). Header field offsets per the
// Microsoft `DDSURFACEDESC2` layout.
const HEIGHT_OFFSET: usize = 4 + 8;
const WIDTH_OFFSET: usize = 4 + 12;
const MIP_COUNT_OFFSET: usize = 4 + 24;
// Pixel format struct begins after: dwSize(4) + dwFlags(4) + dwHeight(4) +
// dwWidth(4) + dwPitchOrLinearSize(4) + dwDepth(4) + dwMipMapCount(4) +
// dwReserved1[11](44) = 72 bytes of header. Plus 4 magic bytes = file offset 76.
const PIXEL_FORMAT_OFFSET: usize = 4 + 72;

const DDPF_FOURCC: u32 = 0x0000_0004;

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, String> {
    bytes
        .get(offset..offset + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| format!("DDS truncated: cannot read u32 at offset {offset}"))
}

pub fn parse_dds(bytes: &[u8]) -> Result<DdsImage, String> {
    if bytes.len() < PIXEL_DATA_OFFSET {
        return Err(format!(
            "DDS truncated: need at least {PIXEL_DATA_OFFSET} bytes, got {}",
            bytes.len()
        ));
    }
    if &bytes[0..4] != DDS_MAGIC {
        return Err("DDS magic missing or invalid (expected 'DDS ' at offset 0)".to_string());
    }

    let height = read_u32_le(bytes, HEIGHT_OFFSET)?;
    let width = read_u32_le(bytes, WIDTH_OFFSET)?;
    let raw_mip_count = read_u32_le(bytes, MIP_COUNT_OFFSET)?;
    let mip_count = if raw_mip_count == 0 { 1 } else { raw_mip_count };

    let pf_flags = read_u32_le(bytes, PIXEL_FORMAT_OFFSET + 4)?;
    if pf_flags & DDPF_FOURCC == 0 {
        return Err("DDS unsupported: pixel format lacks DDPF_FOURCC flag".to_string());
    }
    let fourcc = &bytes[PIXEL_FORMAT_OFFSET + 8..PIXEL_FORMAT_OFFSET + 12];

    // 4x4 block sizes per S3TC: DXT1 = 8B (color only),
    // DXT3/5 = 16B (8B alpha + 8B color).
    let (format, block_bytes) = match fourcc {
        b"DXT1" => (DdsPixelFormat::Dxt1, 8usize),
        b"DXT3" => (DdsPixelFormat::Dxt3, 16usize),
        b"DXT5" => (DdsPixelFormat::Dxt5, 16usize),
        other => {
            return Err(format!(
                "DDS unsupported fourCC: {:?} (only DXT1/DXT3/DXT5 supported)",
                String::from_utf8_lossy(other)
            ));
        }
    };

    let mut mips: Vec<Vec<u8>> = Vec::with_capacity(mip_count as usize);
    let mut cursor = PIXEL_DATA_OFFSET;
    let mut mip_w = width.max(1);
    let mut mip_h = height.max(1);

    for level in 0..mip_count {
        let blocks_x = mip_w.div_ceil(4) as usize;
        let blocks_y = mip_h.div_ceil(4) as usize;
        let mip_size = blocks_x * blocks_y * block_bytes;
        let end = cursor
            .checked_add(mip_size)
            .ok_or_else(|| format!("DDS mip {level} size overflow"))?;
        if end > bytes.len() {
            return Err(format!(
                "DDS truncated: mip {level} needs {mip_size} bytes from offset {cursor}, file is {} bytes",
                bytes.len()
            ));
        }
        mips.push(bytes[cursor..end].to_vec());
        cursor = end;
        mip_w = (mip_w / 2).max(1);
        mip_h = (mip_h / 2).max(1);
    }

    Ok(DdsImage {
        width,
        height,
        format,
        mips,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_synthetic_dds(width: u32, height: u32, fourcc: &[u8; 4], pixels: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"DDS ");                           // magic
        buf.extend_from_slice(&124u32.to_le_bytes());             // size
        buf.extend_from_slice(&0x0002_1007u32.to_le_bytes());     // flags: caps|height|width|pf|linsize
        buf.extend_from_slice(&height.to_le_bytes());
        buf.extend_from_slice(&width.to_le_bytes());
        buf.extend_from_slice(&(pixels.len() as u32).to_le_bytes()); // pitch_or_linear_size
        buf.extend_from_slice(&0u32.to_le_bytes());               // depth
        buf.extend_from_slice(&1u32.to_le_bytes());               // mip_map_count
        buf.extend_from_slice(&[0u8; 11 * 4]);                    // reserved1
        // pixel format struct (32 bytes)
        buf.extend_from_slice(&32u32.to_le_bytes());              // pf size
        buf.extend_from_slice(&0x0000_0004u32.to_le_bytes());     // pf flags = DDPF_FOURCC
        buf.extend_from_slice(fourcc);                            // fourCC
        buf.extend_from_slice(&[0u8; 5 * 4]);                     // RGB bit count + masks (zero for DXT)
        buf.extend_from_slice(&0x1000u32.to_le_bytes());          // caps = TEXTURE
        buf.extend_from_slice(&[0u8; 3 * 4]);                     // caps2/3/4
        buf.extend_from_slice(&0u32.to_le_bytes());               // reserved2
        buf.extend_from_slice(pixels);
        buf
    }

    #[test]
    fn parses_dxt1_dds() {
        let pixels = vec![0xAA; 32]; // 8x8 DXT1 = 8 bytes per 4x4 block * 2x2 blocks = 32 bytes
        let bytes = build_synthetic_dds(8, 8, b"DXT1", &pixels);
        let dds = parse_dds(&bytes).unwrap();
        assert_eq!(dds.width, 8);
        assert_eq!(dds.height, 8);
        assert_eq!(dds.format, DdsPixelFormat::Dxt1);
        assert_eq!(dds.mips.len(), 1);
        assert_eq!(dds.mips[0], pixels);
    }

    #[test]
    fn parses_dxt5_dds() {
        let pixels = vec![0xBB; 64]; // 8x8 DXT5 = 16 bytes per block * 4 blocks = 64 bytes
        let bytes = build_synthetic_dds(8, 8, b"DXT5", &pixels);
        let dds = parse_dds(&bytes).unwrap();
        assert_eq!(dds.format, DdsPixelFormat::Dxt5);
        assert_eq!(dds.mips[0], pixels);
    }

    #[test]
    fn parses_dxt3_dds() {
        let pixels = vec![0xCCu8; 64]; // 8x8 DXT3 = 16 bytes per 4x4 block * 4 blocks = 64 bytes
        let bytes = build_synthetic_dds(8, 8, b"DXT3", &pixels);
        let dds = parse_dds(&bytes).unwrap();
        assert_eq!(dds.format, DdsPixelFormat::Dxt3);
        assert_eq!(dds.mips[0], pixels);
    }

    #[test]
    fn rejects_missing_magic() {
        let mut bytes = vec![0u8; 200];
        bytes[..4].copy_from_slice(b"NOPE");
        let err = parse_dds(&bytes).unwrap_err();
        assert!(err.contains("DDS magic"));
    }

    #[test]
    fn rejects_unsupported_format() {
        let bytes = build_synthetic_dds(8, 8, b"NOPE", &[0u8; 32]);
        let err = parse_dds(&bytes).unwrap_err();
        assert!(err.contains("unsupported"));
    }
}
