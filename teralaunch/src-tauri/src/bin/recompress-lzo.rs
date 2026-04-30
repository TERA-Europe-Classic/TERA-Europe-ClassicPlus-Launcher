// Re-compress an uncompressed-and-spliced GPK back into LZO-chunked form
// so the v100.02 engine accepts it for composite-routed loads.
//
// Format (mirrors gpk_package::decompress_chunk in reverse):
//   header (verbatim through chunk-table area, with patches below)
//   compression_flags=2, chunk_count=1, chunk_header_0=(name_offset, body_size, name_offset, on_disk_chunk_size)
//   package_flags |= 0x02000000 (PKG_Compressed)
//   body region replaced with chunk-on-disk:
//     u32 signature 0x9E2A83C1
//     u32 block_size = 131072
//     u32 total_compressed_payload_size
//     u32 chunk_uncompressed_size
//     N × (u32 compressedSize, u32 uncompressedSize)
//     N × [LZO-compressed block bytes]

use std::env;
use std::fs;

const PACKAGE_MAGIC: u32 = 0x9E2A83C1;
const X64_THRESHOLD: u16 = 0x381;
const BLOCK_SIZE: usize = 131072;

fn read_u16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}
fn read_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn write_u32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

fn read_fstring_end(bytes: &[u8], off: usize) -> usize {
    let len = i32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
    if len == 0 {
        off + 4
    } else if len > 0 {
        off + 4 + len as usize
    } else {
        off + 4 + (-len as usize) * 2
    }
}

fn locate_header_fields(bytes: &[u8]) -> (usize, usize, usize, usize, u32) {
    // returns (package_flags_pos, compression_flags_pos, chunk_count_pos, name_offset, _name_offset_value_unused)
    let mut c = 0;
    assert_eq!(read_u32(bytes, c), PACKAGE_MAGIC);
    c += 4;
    let file_ver = read_u16(bytes, c);
    c += 2;
    let _lic = read_u16(bytes, c);
    c += 2;
    let is_x64 = file_ver >= X64_THRESHOLD;
    c += 4; // header_size
    c = read_fstring_end(bytes, c); // folder_name
    let pkg_flags_pos = c;
    c += 4;
    c += 4; // raw_name_count
    let name_offset = read_u32(bytes, c) as usize;
    c += 4;
    c += 4 * 5; // export_count, export_offset, import_count, import_offset, depends_offset
    if is_x64 {
        c += 16;
    } // ImportExportGuids block
    c += 16; // FGuid
    let gen_count = read_u32(bytes, c) as usize;
    c += 4;
    c += gen_count * 12;
    c += 4; // engine_version
    c += 4; // cooker_version
    let comp_flags_pos = c;
    c += 4;
    let chunk_count_pos = c;
    (
        pkg_flags_pos,
        comp_flags_pos,
        chunk_count_pos,
        name_offset,
        0,
    )
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: recompress-lzo <input-uncompressed.gpk> <output.gpk>");
        std::process::exit(1);
    }
    let bytes = fs::read(&args[1]).expect("read input");

    let (pkg_flags_pos, comp_flags_pos, chunk_count_pos, name_offset, _) =
        locate_header_fields(&bytes);
    println!("name_offset = {name_offset}");
    println!("pkg_flags_pos = {pkg_flags_pos}, comp_flags_pos = {comp_flags_pos}, chunk_count_pos = {chunk_count_pos}");

    let body = &bytes[name_offset..];
    let body_len = body.len();
    println!(
        "body_len = {body_len} bytes; will produce {} blocks of <= {BLOCK_SIZE}",
        body_len.div_ceil(BLOCK_SIZE)
    );

    // LZO-compress each block
    let mut compressed_blocks: Vec<Vec<u8>> = Vec::new();
    let mut block_table: Vec<(u32, u32)> = Vec::new();
    for chunk in body.chunks(BLOCK_SIZE) {
        let cmp = lzokay::compress::compress(chunk).expect("lzo compress");
        block_table.push((cmp.len() as u32, chunk.len() as u32));
        compressed_blocks.push(cmp);
    }

    let total_compressed_payload: usize = compressed_blocks.iter().map(|b| b.len()).sum();
    let chunk_header_size = 16 + 8 * block_table.len();
    let chunk_on_disk_size = chunk_header_size + total_compressed_payload;
    println!("compressed body: {total_compressed_payload} bytes; chunk_on_disk_size: {chunk_on_disk_size}");

    // Build chunk-on-disk
    let mut chunk_blob = Vec::with_capacity(chunk_on_disk_size);
    chunk_blob.extend_from_slice(&PACKAGE_MAGIC.to_le_bytes());
    chunk_blob.extend_from_slice(&(BLOCK_SIZE as u32).to_le_bytes());
    chunk_blob.extend_from_slice(&(total_compressed_payload as u32).to_le_bytes());
    chunk_blob.extend_from_slice(&(body_len as u32).to_le_bytes());
    for (c, u) in &block_table {
        chunk_blob.extend_from_slice(&c.to_le_bytes());
        chunk_blob.extend_from_slice(&u.to_le_bytes());
    }
    for b in &compressed_blocks {
        chunk_blob.extend_from_slice(b);
    }
    assert_eq!(chunk_blob.len(), chunk_on_disk_size);

    // Build output: header (everything up to name_offset) + chunk_blob.
    // Patch header fields:
    //   - package_flags |= 0x02000000 (PKG_Compressed)
    //   - compression_flags = 2 (LZO)
    //   - chunk_count = 1
    //   - chunk_header_0 = (name_offset, body_len, name_offset, chunk_on_disk_size)
    // Vanilla LZO files have a 16-byte gap between name_offset and the
    // chunk-on-disk start, containing values [7, 0, 1, 5]. Their meaning
    // isn't documented but the engine appears to require them — without
    // the gap (or with zero filler) the engine rejects the package and
    // crashes at load. Mirroring vanilla literal works.
    const PRE_CHUNK_FILLER: [u32; 4] = [7, 0, 1, 5];
    let chunk_offset = name_offset + 16;

    let mut out = Vec::with_capacity(chunk_offset + chunk_on_disk_size);
    out.extend_from_slice(&bytes[..name_offset]);
    for v in PRE_CHUNK_FILLER {
        out.extend_from_slice(&v.to_le_bytes());
    }

    let mut pkg_flags = read_u32(&out, pkg_flags_pos);
    pkg_flags |= 0x02000000;
    write_u32(&mut out, pkg_flags_pos, pkg_flags);

    write_u32(&mut out, comp_flags_pos, 2);
    write_u32(&mut out, chunk_count_pos, 1);

    // chunk_header_0 lives at chunk_count_pos + 4
    let ch0 = chunk_count_pos + 4;
    write_u32(&mut out, ch0, name_offset as u32); // uncompressed_offset
    write_u32(&mut out, ch0 + 4, body_len as u32); // uncompressed_size
    write_u32(&mut out, ch0 + 8, chunk_offset as u32); // compressed_offset (file offset = name_offset + 16)
    write_u32(&mut out, ch0 + 12, chunk_on_disk_size as u32); // compressed_size

    out.extend_from_slice(&chunk_blob);
    fs::write(&args[2], &out).expect("write output");
    println!("wrote {} bytes to {}", out.len(), args[2]);
}
