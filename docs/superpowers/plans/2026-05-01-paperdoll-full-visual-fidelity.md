# PaperDoll Full Visual Fidelity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make foglio1024/tera-restyle PaperDoll render in v100.02 client visually 1:1 with foglio's intended look — including modded race silhouettes (`S1UIRES_Skin.PaperDoll_<race>_<sex>`), modded UI component atlases (`S1UIRES_Component.*`), and any AS2 fixes required for v100's slot-list event API.

**Architecture:**
1. Verify Hypothesis A (SWF API drift) with controlled-state screenshots before any AS2 work, so we don't waste cycles fixing a non-issue.
2. Build an **x64 Texture2D encoder** (`encoder.rs`) that produces `Texture2D` export bytes from a DDS file. The existing `gpk_resource_inspector` is read-only / pixel-swap-only; we need a from-scratch encoder for new textures whose names don't exist in vanilla.
3. Build a **composite-slice authoring** module (`composite_author.rs`) that emits a single-texture standalone GPK (FileVersion 897, MOD: folder, 1 Texture2D export + supporting ObjectReferencer + Package exports), modeled byte-for-byte after vanilla composite slices like `S1UI_PaperDoll.PaperDoll_I147_dup.gpk`.
4. Build a **mapper extender** (`mapper_extend.rs`) that adds new rows to `PkgMapper.dat` and `CompositePackageMapper.dat` for new logical-name → composite-UID → file mappings.
5. Pipeline foglio's `tera-restyle/RES_Skin/BG_PaperDoll_*.dds` and `RES_Component/*.dds` files through the encoder + author + extender to produce a deployable resource pack.
6. If Phase 0 confirms Hypothesis A, decompile `mod.gfx` with JPEXS, patch the slot-list listener's AS2 to handle v100's data shape, recompile, re-author the paperdoll GPK with the patched SWF.
7. Install the full pack (modded paperdoll + new silhouette slices + new component slices + extended mappers), smoke-test in game, iterate.

**Tech Stack:** Rust 2021 (encoder/author/extender as new modules under `services/mods/`). External tools used by humans only: JPEXS Free Flash Decompiler (for SWF AS2 inspection in Phase 7). No new Cargo dependencies for Rust code — we have `lzokay` for LZO; everything else is byte serialization we own.

**Out of scope for this plan:**
- Generalizing the encoder/author to **non-Texture2D classes** (StaticMesh, SoundCue, etc.). Paperdoll resources are textures-only.
- **TFC-backed textures** (FByteBulkData flag 0x01 = StoreInSeparateFile). Foglio's silhouettes ship inline; if we ever hit a TFC-required texture, treat it as a separate plan.
- Cube maps, mip-streaming above mip 0, and DXT-uncommon formats (BC4/BC5/BC6/BC7). Foglio's DDS files are DXT1/DXT5, which is what TERA's TextureFormatPixelFormat enum routes to PF_DXT1/PF_DXT5.
- A **single mod-file footer** (FILEMOD v2 trailer) wrapping the whole pack. Each new resource ships as a standalone GPK + mapper edits, same way TMM installs work.

---

## File Structure

| Path | Status | Responsibility |
|---|---|---|
| `teralaunch/src-tauri/src/services/mods/dds.rs` | new | Parse a DDS file: pixel format (DXT1/DXT5/uncompressed), dimensions, mip count, raw pixel bytes per mip. Read-only; no encoding back to DDS. |
| `teralaunch/src-tauri/src/services/mods/texture_encoder.rs` | new | Encode a `Texture2D` export body: properties (Format/SizeX/SizeY/MipCount/SourceFilePath/etc.) + native section + mip array (uncompressed; flag 0x0). Inputs: DDS-derived dimensions/format/pixel bytes + a name-table builder that can resolve property/type names to indices. |
| `teralaunch/src-tauri/src/services/mods/composite_author.rs` | new | Build a complete single-texture standalone GPK (header + name table + import table + export table + depends + bodies). Take a name (e.g. `PaperDoll_HighElf_F`), parent package (e.g. `S1UIRES_Skin`), composite UID, and a `Texture2D` body from `texture_encoder`. Emit bytes ready to drop into CookedPC. |
| `teralaunch/src-tauri/src/services/mods/mapper_extend.rs` | new | Decrypt `PkgMapper.dat` + `CompositePackageMapper.dat`, add new rows, encrypt + atomic-write back. Also create `.clean` backups for any rows that didn't exist before (so disable can restore correctly). |
| `teralaunch/src-tauri/src/services/mods/mod.rs` | modify | `pub mod` declarations for the four new modules. |
| `teralaunch/src-tauri/src/bin/build-paperdoll-resources.rs` | new | CLI: scan `tera-restyle/RES_Skin/` and `RES_Component/` for `BG_PaperDoll_*.dds` and component DDS files, build standalone GPKs for each via the new modules, emit them into a staging directory + an install-manifest JSON. |
| `teralaunch/src-tauri/src/bin/install-paperdoll-resources.rs` | new | CLI: read the install-manifest, copy GPKs into `D:\Elinu\S1Game\CookedPC\`, extend mappers via `mapper_extend`. Reverse companion `uninstall-paperdoll-resources.rs` is **not** included here — the existing `restore-clean-gpk-mappers` binary plus `.clean` backups handle rollback. |
| `teralaunch/src-tauri/tests/paperdoll_resource_round_trip.rs` | new | Integration: build a one-texture pack, install into a tmp game-root, parse it back via existing `gpk_package::parse_package`, assert dimensions / format / pixel bytes match the DDS source. |
| `tools/extract-paperdoll-swf.md` | new (docs only) | Step-by-step JPEXS instructions for Phase 7: decompile mod.gfx, locate the slot listener, patch AS2, recompile to mod.gfx-patched. **Manual, not automated.** |

---

## Task 0: Verify Hypothesis A (SWF API drift)

**Files:** none — observational only.

- [ ] **Step 1: User reproduces with controlled state**

Have the user take two screenshots back-to-back:
1. Open game with mod installed, take screenshot of paperdoll.
2. Without changing any equipment / costume / outfit set, run rollback:
```bash
cp "D:/Elinu/S1Game/CookedPC/CompositePackageMapper.clean" "D:/Elinu/S1Game/CookedPC/CompositePackageMapper.dat"
rm "D:/Elinu/S1Game/CookedPC/RestylePaperdoll.gpk"
```
3. Restart game, open paperdoll, take screenshot.

Same character, same equipment, same outfit tab. Differences are now purely SWF-driven, not state-driven.

- [ ] **Step 2: Compare screenshots**

If the modded screenshot has fewer item icons in non-equipped slots than vanilla → Hypothesis A confirmed (foglio's SWF can't process v100 slot-list payloads). Phase 7 work is needed.

If both have identical item visibility → Hypothesis A rejected. The visual delta is purely from missing shared resources (`S1UIRES_Skin`, `S1UIRES_Component`). Phase 7 is unnecessary.

- [ ] **Step 3: Record decision in this plan**

Edit Task 0 above and add `**Decision: A confirmed**` or `**Decision: A rejected**`. The Phase 7 tasks below are conditional on this.

---

## Task 1: DDS file parser

**Files:**
- Create: `teralaunch/src-tauri/src/services/mods/dds.rs`
- Modify: `teralaunch/src-tauri/src/services/mods/mod.rs`

DDS format reference: Microsoft DirectX SDK header `DDSURFACEDESC2` (`MAGIC "DDS " (0x20534444)` + 124-byte `DDS_HEADER` + optional 20-byte `DDS_HEADER_DXT10` + raw pixel data).

- [ ] **Step 1: Write the failing tests**

Append to `dds.rs` (will be created in step 3):

```rust
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
        // pixel format
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
```

- [ ] **Step 2: Run, verify RED**

```bash
cd teralaunch/src-tauri && cargo test --bin tera-europe-classicplus-launcher dds::tests
```
Expected: compile error — `parse_dds`, `DdsPixelFormat`, etc. not defined.

- [ ] **Step 3: Implement the parser**

```rust
//! Minimal DDS reader for DXT1 / DXT5 / uncompressed RGBA. Used by
//! texture_encoder to ingest foglio's loose DDS art assets.

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
    /// One Vec per mip level. We always include at least one (the primary).
    pub mips: Vec<Vec<u8>>,
}

const HEADER_SIZE: usize = 124;
const PF_OFFSET: usize = 4 + 76; // magic + 76 header bytes before the pixel format struct

pub fn parse_dds(bytes: &[u8]) -> Result<DdsImage, String> {
    if bytes.len() < 4 + HEADER_SIZE {
        return Err(format!("DDS file too small ({} bytes)", bytes.len()));
    }
    if &bytes[..4] != b"DDS " {
        return Err(format!("DDS magic missing (got {:?})", &bytes[..4]));
    }

    let read_u32 = |off: usize| -> u32 {
        u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
    };

    let height = read_u32(4 + 8);
    let width = read_u32(4 + 12);
    let mip_count = read_u32(4 + 24).max(1);

    let pf_flags = read_u32(PF_OFFSET + 4);
    let pf_fourcc = &bytes[PF_OFFSET + 8..PF_OFFSET + 12];

    const DDPF_FOURCC: u32 = 0x0000_0004;
    if pf_flags & DDPF_FOURCC == 0 {
        return Err("DDS pixel format flags do not include FOURCC; uncompressed not yet supported".into());
    }

    let format = match pf_fourcc {
        b"DXT1" => DdsPixelFormat::Dxt1,
        b"DXT3" => DdsPixelFormat::Dxt3,
        b"DXT5" => DdsPixelFormat::Dxt5,
        other => return Err(format!("unsupported DDS FOURCC '{}'", String::from_utf8_lossy(other))),
    };

    // DXT block sizes: DXT1 = 8 bytes / 4x4 block; DXT3/DXT5 = 16 bytes / 4x4 block.
    let block_bytes = match format {
        DdsPixelFormat::Dxt1 => 8,
        DdsPixelFormat::Dxt3 | DdsPixelFormat::Dxt5 => 16,
    };

    let mut mips = Vec::with_capacity(mip_count as usize);
    let mut cursor = 4 + HEADER_SIZE;
    let mut w = width.max(1);
    let mut h = height.max(1);
    for _ in 0..mip_count {
        let blocks_x = (w + 3) / 4;
        let blocks_y = (h + 3) / 4;
        let mip_size = (blocks_x as usize) * (blocks_y as usize) * block_bytes;
        if cursor + mip_size > bytes.len() {
            return Err(format!(
                "DDS mip data ends past EOF (need {} bytes from offset {}, file is {})",
                mip_size, cursor, bytes.len()
            ));
        }
        mips.push(bytes[cursor..cursor + mip_size].to_vec());
        cursor += mip_size;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }

    Ok(DdsImage { width, height, format, mips })
}
```

Add `pub mod dds;` to `services/mods/mod.rs`.

- [ ] **Step 4: Run, verify GREEN**

```bash
cd teralaunch/src-tauri && cargo test --bin tera-europe-classicplus-launcher dds::tests
```
Expected: 4 tests pass.

- [ ] **Step 5: Smoke-parse a real foglio DDS**

```bash
cd teralaunch/src-tauri && cargo run --bin smoke-dds -- "C:/Users/Lukas/AppData/Local/Temp/tera-restyle-clone/RES_Skin/BG_PaperDoll_AM.dds"
```

(Add `bin/smoke-dds.rs` calling `parse_dds` and printing dims/format/mip-count. Keep it — it's a useful diagnostic for any future foglio drop.)

Expected: prints reasonable dims (likely 512x1024 or similar) and DXT1 or DXT5.

- [ ] **Step 6: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/dds.rs teralaunch/src-tauri/src/services/mods/mod.rs
git commit -m "feat(mods): minimal DDS parser for DXT1/3/5 textures"
```

---

## Task 2: x64 Texture2D encoder

**Files:**
- Create: `teralaunch/src-tauri/src/services/mods/texture_encoder.rs`
- Modify: `teralaunch/src-tauri/src/services/mods/mod.rs`

**Reference shape:** decoded vanilla v100 `Texture2D` exports follow this layout (verified
by reading `gpk_resource_inspector.rs` lines 525–629 — the inverse of `read_bulk_metadata` /
`locate_first_mip_payload` / `read_mip_array_metadata`):

```
[property block: properties terminated by an i64 "None" name index]
[bulk: source-art FByteBulkData header (16 bytes: flags, element_count, size_on_disk, offset_in_file)]
[bulk: source-art payload (embedded bytes if flags & 0x1 == 0)]
[FString: SourceFilePath (i32 length + ANSI bytes)]
[i32: mip_count]
  for each mip:
    [bulk: FByteBulkData header (16 bytes)]
    [bulk: payload bytes]
    [i32: size_x]
    [i32: size_y]
[x64 only: 16 bytes of "unknown" cached-mip header — match a vanilla example]
[i32: cached_mip_count]
  for each cached mip: same shape as above
[i32: max_cached_resolution]
```

Properties for a typical paperdoll Texture2D (read from vanilla `PaperDoll_I147_dup` via
`inspect-gpk-resources` — `properties=13`):

```
NameProperty:    None marker (terminator)
StructProperty:  TextureFileCacheGuid = Guid (16 bytes zero)
NameProperty:    TextureFileCacheName = "None"
IntProperty:     UnpackMin[0..3] = 0.0 (each as Float, but exposed as Int)
ByteProperty:    Format = PF_DXT1 or PF_DXT5
IntProperty:     SizeX = 512
IntProperty:     SizeY = 1024
IntProperty:     OriginalSizeX = 512
IntProperty:     OriginalSizeY = 1024
IntProperty:     MipTailBaseIdx = 0
StructProperty:  TextureFileCacheGuid (Guid)
NameProperty:    LODGroup = TEXTUREGROUP_UI
ByteProperty:    PixelFormat
```

Exact list and order: model on a vanilla `PaperDoll_I147` export by dumping it via
`extract-vanilla-gpk` then parsing properties. **Do not invent property orderings** — use a
working vanilla as the canonical template.

- [ ] **Step 1: Extract a vanilla template export**

```bash
cd teralaunch/src-tauri && ./target/debug/extract-vanilla-gpk \
  --game-root "D:/Elinu" \
  --object-path "ffe86d35_e90341cb_1ddaf.PaperDoll_I147_dup" \
  --out "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/.gpk-rebuild/vanilla/PaperDoll_I147_dup.template.gpk"
```

Expected: produces a standalone GPK we'll use as the byte-template for new textures.

- [ ] **Step 2: Write `encode_texture2d_body` failing test**

```rust
// in texture_encoder.rs (#[cfg(test)] mod tests block)

#[test]
fn encode_round_trips_through_resource_inspector() {
    use super::super::{dds::DdsPixelFormat, gpk_resource_inspector};
    let pixels = vec![0xC3u8; 32]; // 8x8 DXT1
    let dds = super::super::dds::DdsImage {
        width: 8, height: 8, format: DdsPixelFormat::Dxt1, mips: vec![pixels.clone()],
    };
    let mut name_table = NameTableBuilder::new();
    let body = encode_texture2d_body(&dds, "PaperDoll_HighElf_F", &mut name_table)
        .expect("encode body");

    // Simulate a complete export by wrapping body in a synthetic GPK package.
    let pkg = wrap_body_in_minimum_gpk(body, name_table.into_entries(), "PaperDoll_HighElf_F");
    let parsed = super::super::gpk_package::parse_package(&pkg).expect("re-parse");

    let texture = parsed.exports.iter()
        .find(|e| e.object_path == "PaperDoll_HighElf_F").unwrap();
    let mip = gpk_resource_inspector::first_mip_bulk_location(
        texture, &parsed.names, true).unwrap();
    let mip_bytes = &texture.payload[mip.payload_offset..mip.payload_offset + mip.payload_len];
    assert_eq!(mip_bytes, pixels.as_slice());
}
```

This test is intentionally **integration-shaped**: encode → wrap → parse → locate first mip → bytes match. If any layout detail is wrong, the inspector chokes and the test fails.

- [ ] **Step 3: Run, verify RED**

```bash
cd teralaunch/src-tauri && cargo test --bin tera-europe-classicplus-launcher texture_encoder
```
Expected: fails at compile (not yet implemented).

- [ ] **Step 4: Implement `NameTableBuilder` and `encode_texture2d_body`**

```rust
//! Encode a v100-shaped Texture2D export body from a parsed DDS image.
//!
//! Layout reference: services::mods::gpk_resource_inspector locate_*_mip_payload /
//! read_bulk_metadata logic in reverse. Verified against vanilla
//! S1UI_PaperDoll.PaperDoll_I147_dup.gpk.

use super::dds::{DdsImage, DdsPixelFormat};
use super::gpk_package::{GpkNameEntry};

const NAME_FLAGS_DEFAULT: u64 = 1970393556451328;  // observed in vanilla x64 packages

#[derive(Debug, Default)]
pub struct NameTableBuilder {
    names: Vec<String>,
}

impl NameTableBuilder {
    pub fn new() -> Self { Self { names: Vec::new() } }

    /// Insert if not present; return its index.
    pub fn intern(&mut self, name: &str) -> u64 {
        if let Some(i) = self.names.iter().position(|n| n == name) {
            return i as u64;
        }
        self.names.push(name.to_string());
        (self.names.len() - 1) as u64
    }

    pub fn into_entries(self) -> Vec<GpkNameEntry> {
        self.names.into_iter().map(|n| GpkNameEntry {
            name: n,
            flags: NAME_FLAGS_DEFAULT,
        }).collect()
    }
}

/// Encode the property block + mip array. Does NOT include export-table or
/// surrounding GPK header — that's `composite_author`'s job.
pub fn encode_texture2d_body(
    dds: &DdsImage,
    object_name: &str,
    names: &mut NameTableBuilder,
) -> Result<Vec<u8>, String> {
    let format_name = match dds.format {
        DdsPixelFormat::Dxt1 => "PF_DXT1",
        DdsPixelFormat::Dxt3 => "PF_DXT3",
        DdsPixelFormat::Dxt5 => "PF_DXT5",
    };

    // Pre-intern every name we'll use so the property block can write indices.
    let none_idx = names.intern("None");
    let format_idx = names.intern("Format");
    let byteprop_idx = names.intern("ByteProperty");
    let pixelformat_idx = names.intern("EPixelFormat");
    let format_value_idx = names.intern(format_name);
    let sizex_idx = names.intern("SizeX");
    let sizey_idx = names.intern("SizeY");
    let intprop_idx = names.intern("IntProperty");
    let original_sizex_idx = names.intern("OriginalSizeX");
    let original_sizey_idx = names.intern("OriginalSizeY");
    let miptailbase_idx = names.intern("MipTailBaseIdx");
    let _object_idx = names.intern(object_name);

    let mut body = Vec::new();
    let _ = (sizey_idx, sizex_idx, intprop_idx, original_sizex_idx, original_sizey_idx,
            miptailbase_idx, none_idx, format_idx, byteprop_idx, pixelformat_idx,
            format_value_idx);

    // === Property block ===
    // Format: ByteProperty (1 byte enum value on x32, 8-byte enum-name index + 1 byte on x64)
    // Header: nameIdx(i64) typeIdx(i64) size(i32) arrayIndex(i32) [+ x64 enumType i64] [+ value]
    push_property_byteprop(&mut body, format_idx, byteprop_idx, pixelformat_idx, format_value_idx);
    push_property_intprop(&mut body, sizex_idx, intprop_idx, dds.width as i32);
    push_property_intprop(&mut body, sizey_idx, intprop_idx, dds.height as i32);
    push_property_intprop(&mut body, original_sizex_idx, intprop_idx, dds.width as i32);
    push_property_intprop(&mut body, original_sizey_idx, intprop_idx, dds.height as i32);
    push_property_intprop(&mut body, miptailbase_idx, intprop_idx, 0);
    // Property-block terminator: i64 None name index.
    body.extend_from_slice(&none_idx.to_le_bytes());

    // === Native section ===
    // source-art FByteBulkData (16 bytes: flags, element_count, size_on_disk, offset_in_file)
    body.extend_from_slice(&0u32.to_le_bytes());                  // flags = 0
    body.extend_from_slice(&0i32.to_le_bytes());                  // element_count = 0
    body.extend_from_slice(&0i32.to_le_bytes());                  // size_on_disk = 0
    let _src_offset_field_pos = body.len();
    body.extend_from_slice(&0i32.to_le_bytes());                  // offset_in_file (patched at install)
    // empty source-art payload (size_on_disk == 0)
    // SourceFilePath FString — empty
    body.extend_from_slice(&0i32.to_le_bytes());

    // === Mip array (uncompressed inline) ===
    let mip_count = dds.mips.len() as i32;
    body.extend_from_slice(&mip_count.to_le_bytes());
    let mut w = dds.width;
    let mut h = dds.height;
    for mip in &dds.mips {
        // bulk header: flags=0, element_count=mip.len, size_on_disk=mip.len, offset placeholder
        body.extend_from_slice(&0u32.to_le_bytes());
        body.extend_from_slice(&(mip.len() as i32).to_le_bytes());
        body.extend_from_slice(&(mip.len() as i32).to_le_bytes());
        let _mip_offset_field_pos = body.len();
        body.extend_from_slice(&0i32.to_le_bytes());
        body.extend_from_slice(mip);
        body.extend_from_slice(&(w as i32).to_le_bytes());
        body.extend_from_slice(&(h as i32).to_le_bytes());
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }

    // === x64 cached-mip array ===
    body.extend_from_slice(&[0u8; 16]); // 16 bytes of cached-mip preamble (observed in vanilla — copy exactly from template)
    body.extend_from_slice(&0i32.to_le_bytes()); // cached_mip_count = 0
    body.extend_from_slice(&0i32.to_le_bytes()); // max_cached_resolution = 0

    Ok(body)
}

fn push_property_intprop(buf: &mut Vec<u8>, name_idx: u64, type_idx: u64, value: i32) {
    buf.extend_from_slice(&name_idx.to_le_bytes());   // i64 name
    buf.extend_from_slice(&type_idx.to_le_bytes());   // i64 type
    buf.extend_from_slice(&4i32.to_le_bytes());       // i32 size = 4
    buf.extend_from_slice(&0i32.to_le_bytes());       // i32 arrayIndex = 0
    buf.extend_from_slice(&value.to_le_bytes());
}

fn push_property_byteprop(buf: &mut Vec<u8>, name_idx: u64, type_idx: u64, enum_type_idx: u64, value_name_idx: u64) {
    // x64 ByteProperty: header + i64 enumType + i64 enum-value name index
    buf.extend_from_slice(&name_idx.to_le_bytes());
    buf.extend_from_slice(&type_idx.to_le_bytes());
    buf.extend_from_slice(&8i32.to_le_bytes());       // size = 8 (the i64 enum-value)
    buf.extend_from_slice(&0i32.to_le_bytes());       // arrayIndex
    buf.extend_from_slice(&enum_type_idx.to_le_bytes());
    buf.extend_from_slice(&value_name_idx.to_le_bytes());
}
```

This is a **first cut**. The "16 bytes of cached-mip preamble" requires reading a vanilla
template. Step 4b adds that.

- [ ] **Step 4b: Read 16-byte cached-mip preamble from vanilla template**

```bash
cd teralaunch/src-tauri && ./target/debug/inspect-gpk-resources \
  "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/.gpk-rebuild/vanilla/PaperDoll_I147_dup.template.gpk" \
  --dump-export "PaperDoll_I147_dup" --hex-window-around-cached-mips
```

(May need to add `--dump-export` / `--hex-window-around-cached-mips` flags to that binary; they're small additions if missing.)

Inspect the 16 bytes between `mip-array-end` and `cached_mip_count`. Hard-code those bytes in `encode_texture2d_body` instead of `[0u8; 16]`.

- [ ] **Step 5: Run, verify GREEN**

```bash
cd teralaunch/src-tauri && cargo test --bin tera-europe-classicplus-launcher texture_encoder
```
Expected: round-trip test passes.

- [ ] **Step 6: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/texture_encoder.rs teralaunch/src-tauri/src/services/mods/mod.rs
git commit -m "feat(mods): x64 Texture2D encoder from DDS source"
```

---

## Task 3: Composite-slice authoring

**Files:**
- Create: `teralaunch/src-tauri/src/services/mods/composite_author.rs`
- Modify: `teralaunch/src-tauri/src/services/mods/mod.rs`

**Reference template:** `S1UI_PaperDoll.PaperDoll_I147_dup.gpk`. 35 names, 5 imports, 3 exports
(1 ObjectReferencer, 1 Texture2D, 1 Package). FileVersion 897. 527,270 bytes.

We'll copy this structure: same name table layout, same imports (Texture2D + ObjectReferencer
+ Package + ParentPackage class), same export shape — only the texture's name and pixel data
change.

- [ ] **Step 1: Failing test**

```rust
#[test]
fn authors_a_single_texture_composite_slice() {
    use super::super::dds::DdsPixelFormat;
    let pixels = vec![0xD7u8; 64];
    let dds = super::super::dds::DdsImage {
        width: 8, height: 8, format: DdsPixelFormat::Dxt5, mips: vec![pixels.clone()],
    };
    let bytes = author_composite_slice(
        &dds,
        "PaperDoll_HighElf_F",          // texture name
        "S1UIRES_Skin",                 // parent package name
        "modres_a1b2c3d4_1.PaperDoll_HighElf_F_dup", // composite UID + ".name_dup"
    ).expect("author slice");

    let pkg = super::super::gpk_package::parse_package(&bytes).expect("re-parse");
    assert_eq!(pkg.summary.file_version, 897);
    assert!(pkg.summary.package_name.starts_with("MOD:"));
    assert_eq!(pkg.exports.len(), 3);
    let tex = pkg.exports.iter()
        .find(|e| e.object_path.ends_with(".PaperDoll_HighElf_F"))
        .or_else(|| pkg.exports.iter().find(|e| e.object_path == "PaperDoll_HighElf_F"))
        .expect("texture export");
    assert_eq!(tex.class_name.as_deref(), Some("Core.Engine.Texture2D"));
    let mip = super::super::gpk_resource_inspector::first_mip_bulk_location(
        tex, &pkg.names, true).expect("locate mip");
    let mip_bytes = &tex.payload[mip.payload_offset..mip.payload_offset + mip.payload_len];
    assert_eq!(mip_bytes, pixels.as_slice());
}
```

- [ ] **Step 2: Run, verify RED**

```bash
cd teralaunch/src-tauri && cargo test --bin tera-europe-classicplus-launcher composite_author
```
Expected: compile error.

- [ ] **Step 3: Implement `author_composite_slice`**

This task is large. Break it into sub-functions:
- `build_name_table(...)`: pre-populates names from the vanilla template + adds the new texture name
- `build_import_table(...)`: 5 imports matching the template (Core/Package, Core/Texture2D, Engine/Package, Engine, S1UIRES_Skin)
- `build_exports(...)`: 3 exports — Package (no payload), ObjectReferencer (1 ref to Texture2D), Texture2D (uses `texture_encoder::encode_texture2d_body`)
- `assemble_gpk(...)`: writes header + name table + import table + export table + depends + bodies, computes offsets in two passes (first pass = sizes, second pass = patch offsets)

Implementation sketch (full — see `texture_encoder.rs` in Task 2 for the body encoder):

```rust
//! Build a complete single-texture standalone GPK matching v100's composite
//! slice layout. Modeled on vanilla S1UI_PaperDoll.PaperDoll_I147_dup.

use super::dds::DdsImage;
use super::gpk_package::{GpkExportEntry, GpkImportEntry, GpkNameEntry,
                          GpkPackage, GpkPackageSummary};
use super::texture_encoder::{NameTableBuilder, encode_texture2d_body};

pub fn author_composite_slice(
    dds: &DdsImage,
    texture_name: &str,
    parent_package_name: &str,
    composite_full_path: &str,   // e.g. "modres_a1b2c3d4_1.PaperDoll_HighElf_F_dup"
) -> Result<Vec<u8>, String> {
    let mut names = NameTableBuilder::new();

    // Encode texture body first so the name table contains everything it needs.
    let texture_body = encode_texture2d_body(dds, texture_name, &mut names)?;

    // Build name table additions for imports/exports.
    let _ = names.intern("Core");
    let _ = names.intern("Engine");
    let _ = names.intern("Package");
    let _ = names.intern("Texture2D");
    let _ = names.intern("ObjectReferencer");
    let _ = names.intern("None");
    let _ = names.intern(parent_package_name);

    // (Full implementation: build imports, exports, header, serialize.)
    // See accompanying notes file `composite_author_layout.md` for the exact
    // byte-by-byte serialization. The two-pass offset patching:
    //   pass 1: compute name_size, import_size, export_size
    //   pass 2: write header with computed offsets, then serialize sections in order
    
    todo!("full serializer — see composite_author_layout.md for detailed byte layout")
}
```

The `todo!()` is intentional in this plan-document — Task 3 Step 3 is the major
implementation chunk. **Before writing it**, the agent should:

1. Run `inspect-gpk-envelope` and `inspect-gpk-resources` on the template
   `PaperDoll_I147_dup.template.gpk` to get exact name/import/export counts and offsets.
2. Write a hex-dump comparison of the template against a fresh `author_composite_slice`
   output to find layout drift.
3. Iterate until `cargo test composite_author` passes.

Estimated implementation: ~400-600 lines of careful byte serialization.

- [ ] **Step 4: Run, verify GREEN**

Iterate Step 3 until:
```bash
cd teralaunch/src-tauri && cargo test --bin tera-europe-classicplus-launcher composite_author
```
passes.

- [ ] **Step 5: Smoke-author one real silhouette**

```bash
cd teralaunch/src-tauri && cargo run --bin smoke-author -- \
  --dds "C:/Users/Lukas/AppData/Local/Temp/tera-restyle-clone/RES_Skin/BG_PaperDoll_AM.dds" \
  --texture-name "PaperDoll_AM" \
  --parent-package "S1UIRES_Skin" \
  --composite "modres_paperdoll_skin_001.PaperDoll_AM_dup" \
  --out "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/.gpk-rebuild/publish-fresh/modres_paperdoll_skin_001.gpk"
```

(Add a small `bin/smoke-author.rs` that wires CLI to `author_composite_slice`.)

Then:
```bash
cd teralaunch/src-tauri && ./target/debug/inspect-gpk-resources \
  "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/.gpk-rebuild/publish-fresh/modres_paperdoll_skin_001.gpk"
```

Expected: parses cleanly, shows 1 Texture2D named `PaperDoll_AM`, dimensions matching the DDS.

- [ ] **Step 6: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/composite_author.rs \
         teralaunch/src-tauri/src/services/mods/mod.rs \
         teralaunch/src-tauri/src/bin/smoke-author.rs
git commit -m "feat(mods): author single-texture composite slices for v100"
```

---

## Task 4: Mapper extender

**Files:**
- Create: `teralaunch/src-tauri/src/services/mods/mapper_extend.rs`
- Modify: `teralaunch/src-tauri/src/services/mods/mod.rs`

- [ ] **Step 1: Failing test**

```rust
#[test]
fn extends_pkg_and_composite_mappers_atomically() {
    use super::super::gpk;
    let tmp = tempfile::TempDir::new().unwrap();
    let cooked = tmp.path().join("S1Game/CookedPC");
    std::fs::create_dir_all(&cooked).unwrap();

    // Seed minimal mapper files.
    let pkg_text = "S1UI_X.X,modres_baseline_0.X_dup|";
    let comp_text = "modres_baseline_0?modres_baseline_0.X_dup,modres_baseline_0,0,100,|!";
    std::fs::write(cooked.join("PkgMapper.dat"),
        gpk::encrypt_mapper(pkg_text.as_bytes())).unwrap();
    std::fs::write(cooked.join("PkgMapper.clean"),
        gpk::encrypt_mapper(pkg_text.as_bytes())).unwrap();
    std::fs::write(cooked.join("CompositePackageMapper.dat"),
        gpk::encrypt_mapper(comp_text.as_bytes())).unwrap();
    std::fs::write(cooked.join("CompositePackageMapper.clean"),
        gpk::encrypt_mapper(comp_text.as_bytes())).unwrap();

    let new_rows = vec![
        MapperAddition {
            logical_path: "S1UIRES_Skin.PaperDoll_AM".into(),
            composite_uid: "modres_skin_001".into(),
            composite_object_path: "modres_skin_001.PaperDoll_AM_dup".into(),
            composite_filename: "modres_paperdoll_skin_001".into(),
            composite_offset: 0,
            composite_size: 524441,
        },
    ];

    extend_mappers(tmp.path(), &new_rows).unwrap();

    // Verify PkgMapper has the new row.
    let pm = std::fs::read(cooked.join("PkgMapper.dat")).unwrap();
    let pm_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&pm)).to_string();
    assert!(pm_text.contains("S1UIRES_Skin.PaperDoll_AM,modres_skin_001.PaperDoll_AM_dup"));

    // CompositePackageMapper has the new row keyed by composite_uid.
    let cm = std::fs::read(cooked.join("CompositePackageMapper.dat")).unwrap();
    let cm_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&cm)).to_string();
    assert!(cm_text.contains("modres_paperdoll_skin_001?modres_skin_001.PaperDoll_AM_dup,modres_skin_001,0,524441"));

    // .clean files unchanged (preserved as vanilla baseline for rollback).
    let pmc = std::fs::read(cooked.join("PkgMapper.clean")).unwrap();
    let pmc_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&pmc)).to_string();
    assert!(!pmc_text.contains("PaperDoll_AM"));
}
```

- [ ] **Step 2: Run, verify RED**

```bash
cd teralaunch/src-tauri && cargo test --bin tera-europe-classicplus-launcher mapper_extend
```

- [ ] **Step 3: Implement**

```rust
//! Add new logical→composite and composite→file rows to the live mapper
//! files. Preserves .clean files as vanilla baseline for rollback.

use std::path::Path;
use super::gpk;

#[derive(Debug, Clone)]
pub struct MapperAddition {
    pub logical_path: String,                // S1UIRES_Skin.PaperDoll_AM
    pub composite_uid: String,                // modres_skin_001
    pub composite_object_path: String,        // modres_skin_001.PaperDoll_AM_dup
    pub composite_filename: String,           // modres_paperdoll_skin_001 (no .gpk extension)
    pub composite_offset: i64,
    pub composite_size: i64,
}

pub fn extend_mappers(game_root: &Path, additions: &[MapperAddition]) -> Result<(), String> {
    let cooked = game_root.join(gpk::COOKED_PC_DIR);
    if !cooked.exists() {
        return Err(format!("CookedPC missing: {}", cooked.display()));
    }

    // PkgMapper: append "<logical>,<composite_object_path>|" to the plaintext.
    let pm_path = cooked.join(gpk::PKG_MAPPER_FILE);
    let pm_enc = std::fs::read(&pm_path).map_err(|e| format!("read PkgMapper: {e}"))?;
    let mut pm_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&pm_enc)).to_string();
    for add in additions {
        let row = format!("{},{}|", add.logical_path, add.composite_object_path);
        if !pm_text.contains(&row) {
            pm_text.push_str(&row);
        }
    }
    let pm_new = gpk::encrypt_mapper(pm_text.as_bytes());
    gpk::write_atomic_file(&pm_path, &pm_new)?;

    // CompositePackageMapper: append "<filename>?<obj>,<uid>,<off>,<size>,|!" group.
    let cm_path = cooked.join(gpk::MAPPER_FILE);
    let cm_enc = std::fs::read(&cm_path).map_err(|e| format!("read CompositeMapper: {e}"))?;
    let mut cm_text = String::from_utf8_lossy(&gpk::decrypt_mapper(&cm_enc)).to_string();
    for add in additions {
        let row = format!(
            "{}?{},{},{},{},|!",
            add.composite_filename, add.composite_object_path,
            add.composite_uid, add.composite_offset, add.composite_size
        );
        if !cm_text.contains(&row) {
            cm_text.push_str(&row);
        }
    }
    let cm_new = gpk::encrypt_mapper(cm_text.as_bytes());
    gpk::write_atomic_file(&cm_path, &cm_new)?;
    Ok(())
}
```

- [ ] **Step 4: Run, verify GREEN**

```bash
cd teralaunch/src-tauri && cargo test --bin tera-europe-classicplus-launcher mapper_extend
```

- [ ] **Step 5: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/mapper_extend.rs teralaunch/src-tauri/src/services/mods/mod.rs
git commit -m "feat(mods): mapper extender for adding new resource rows"
```

---

## Task 5: PaperDoll-resource build CLI

**Files:**
- Create: `teralaunch/src-tauri/src/bin/build-paperdoll-resources.rs`

- [ ] **Step 1: Implement**

```rust
//! Walk tera-restyle/RES_Skin/ + RES_Component/ for *.dds files matching
//! foglio's PaperDoll silhouette + component textures. For each one:
//!   1. Parse the DDS (dds::parse_dds).
//!   2. Author a single-texture composite slice GPK
//!      (composite_author::author_composite_slice).
//!   3. Write the GPK to a staging directory.
//!   4. Append a MapperAddition entry to the install manifest.
//! Outputs:
//!   - <staging>/<container_filename>.gpk for each resource
//!   - <staging>/install-manifest.json with all MapperAddition rows

use std::env;
use std::fs;
use std::path::PathBuf;

#[allow(dead_code)] #[path = "../services/mods/dds.rs"] mod dds;
#[allow(dead_code)] #[path = "../services/mods/gpk_package.rs"] mod gpk_package;
#[allow(dead_code)] #[path = "../services/mods/gpk_resource_inspector.rs"] mod gpk_resource_inspector;
#[allow(dead_code)] #[path = "../services/mods/texture_encoder.rs"] mod texture_encoder;
#[allow(dead_code)] #[path = "../services/mods/composite_author.rs"] mod composite_author;
#[allow(dead_code)] #[path = "../services/mods/gpk.rs"] mod gpk;
#[allow(dead_code)] #[path = "../services/mods/mapper_extend.rs"] mod mapper_extend;

fn usage() -> &'static str {
    "build-paperdoll-resources --foglio-root <path> --staging <dir>"
}

fn main() {
    if let Err(e) = run() { eprintln!("FAIL: {e}"); std::process::exit(1); }
}

fn run() -> Result<(), String> {
    let mut foglio_root: Option<PathBuf> = None;
    let mut staging: Option<PathBuf> = None;
    let mut iter = env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--foglio-root" => foglio_root = iter.next().map(PathBuf::from),
            "--staging" => staging = iter.next().map(PathBuf::from),
            other => return Err(format!("unknown arg '{other}'\n{}", usage())),
        }
    }
    let foglio_root = foglio_root.ok_or("--foglio-root required")?;
    let staging = staging.ok_or("--staging required")?;
    fs::create_dir_all(&staging).map_err(|e| format!("create staging: {e}"))?;

    let mut additions: Vec<mapper_extend::MapperAddition> = Vec::new();
    let mut idx = 0u32;

    // RES_Skin: foglio's race silhouettes.
    let skin_dir = foglio_root.join("RES_Skin");
    if skin_dir.is_dir() {
        for entry in fs::read_dir(&skin_dir).map_err(|e| format!("read RES_Skin: {e}"))? {
            let entry = entry.map_err(|e| format!("dir entry: {e}"))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("dds") { continue; }
            let stem = path.file_stem().and_then(|s| s.to_str())
                .ok_or_else(|| format!("bad stem for {}", path.display()))?;
            // Convert "BG_PaperDoll_AM" → "PaperDoll_AM" (drop "BG_" prefix to match SWF's URI).
            let texture_name = stem.strip_prefix("BG_").unwrap_or(stem).to_string();
            let bytes = fs::read(&path).map_err(|e| format!("read DDS: {e}"))?;
            let dds = dds::parse_dds(&bytes)?;
            idx += 1;
            let composite_uid = format!("modres_skin_{idx:04x}");
            let composite_filename = format!("modres_paperdoll_skin_{idx:04x}");
            let composite_object_path = format!("{composite_uid}.{texture_name}_dup");
            let gpk = composite_author::author_composite_slice(
                &dds, &texture_name, "S1UIRES_Skin", &composite_object_path)?;
            let out_path = staging.join(format!("{composite_filename}.gpk"));
            fs::write(&out_path, &gpk).map_err(|e| format!("write {}: {e}", out_path.display()))?;
            additions.push(mapper_extend::MapperAddition {
                logical_path: format!("S1UIRES_Skin.{texture_name}"),
                composite_uid,
                composite_object_path,
                composite_filename,
                composite_offset: 0,
                composite_size: gpk.len() as i64,
            });
        }
    }

    // RES_Component: foglio's button/scroll/frame component textures.
    let component_dir = foglio_root.join("RES_Component");
    if component_dir.is_dir() {
        for entry in fs::read_dir(&component_dir).map_err(|e| format!("read RES_Component: {e}"))? {
            let entry = entry.map_err(|e| format!("dir entry: {e}"))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("dds") { continue; }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            let bytes = fs::read(&path).map_err(|e| format!("read DDS: {e}"))?;
            let dds = dds::parse_dds(&bytes)?;
            idx += 1;
            let composite_uid = format!("modres_comp_{idx:04x}");
            let composite_filename = format!("modres_paperdoll_comp_{idx:04x}");
            let composite_object_path = format!("{composite_uid}.{stem}_dup");
            let gpk = composite_author::author_composite_slice(
                &dds, &stem, "S1UIRES_Component", &composite_object_path)?;
            let out_path = staging.join(format!("{composite_filename}.gpk"));
            fs::write(&out_path, &gpk).map_err(|e| format!("write {}: {e}", out_path.display()))?;
            additions.push(mapper_extend::MapperAddition {
                logical_path: format!("S1UIRES_Component.{stem}"),
                composite_uid,
                composite_object_path,
                composite_filename,
                composite_offset: 0,
                composite_size: gpk.len() as i64,
            });
        }
    }

    // Write the manifest.
    let manifest_path = staging.join("install-manifest.json");
    let json = serde_json::to_string_pretty(&additions.iter().map(|a| serde_json::json!({
        "logical_path": a.logical_path,
        "composite_uid": a.composite_uid,
        "composite_object_path": a.composite_object_path,
        "composite_filename": a.composite_filename,
        "composite_offset": a.composite_offset,
        "composite_size": a.composite_size,
    })).collect::<Vec<_>>()).map_err(|e| format!("json: {e}"))?;
    fs::write(&manifest_path, json).map_err(|e| format!("write manifest: {e}"))?;
    println!("wrote {} resource GPKs + manifest to {}", additions.len(), staging.display());
    Ok(())
}
```

- [ ] **Step 2: Build**

```bash
cd teralaunch/src-tauri && cargo build --bin build-paperdoll-resources
```

- [ ] **Step 3: Run against foglio source**

```bash
cd teralaunch/src-tauri && ./target/debug/build-paperdoll-resources \
  --foglio-root "C:/Users/Lukas/AppData/Local/Temp/tera-restyle-clone" \
  --staging "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/.gpk-rebuild/publish-fresh/paperdoll-resources"
```

Expected: ~16-32 silhouette GPKs + ~N component GPKs + `install-manifest.json`.

- [ ] **Step 4: Spot-check one output**

```bash
cd teralaunch/src-tauri && ./target/debug/inspect-gpk-resources \
  "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/.gpk-rebuild/publish-fresh/paperdoll-resources/modres_paperdoll_skin_0001.gpk"
```
Expected: parses, 1 Texture2D, dimensions match the source DDS.

- [ ] **Step 5: Commit**

```bash
git add teralaunch/src-tauri/src/bin/build-paperdoll-resources.rs
git commit -m "feat(mods): CLI to build paperdoll resource GPKs from foglio DDS"
```

---

## Task 6: Install + smoke test

**Files:**
- Create: `teralaunch/src-tauri/src/bin/install-paperdoll-resources.rs`

- [ ] **Step 1: Implement**

Reads the manifest, copies GPKs into CookedPC, calls `mapper_extend::extend_mappers`. Atomic FS writes. Pre-checks state matches `.clean`.

```rust
use std::env;
use std::fs;
use std::path::PathBuf;

#[allow(dead_code)] #[path = "../services/mods/gpk.rs"] mod gpk;
#[allow(dead_code)] #[path = "../services/mods/mapper_extend.rs"] mod mapper_extend;

fn main() {
    if let Err(e) = run() { eprintln!("FAIL: {e}"); std::process::exit(1); }
}

fn run() -> Result<(), String> {
    let mut iter = env::args().skip(1);
    let mut game_root: Option<PathBuf> = None;
    let mut staging: Option<PathBuf> = None;
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--game-root" => game_root = iter.next().map(PathBuf::from),
            "--staging" => staging = iter.next().map(PathBuf::from),
            other => return Err(format!("unknown arg '{other}'")),
        }
    }
    let game_root = game_root.ok_or("--game-root required")?;
    let staging = staging.ok_or("--staging required")?;

    let manifest_path = staging.join("install-manifest.json");
    let json = fs::read_to_string(&manifest_path).map_err(|e| format!("read manifest: {e}"))?;
    let entries: Vec<serde_json::Value> = serde_json::from_str(&json)
        .map_err(|e| format!("parse manifest: {e}"))?;

    // Copy each GPK into CookedPC.
    let cooked = game_root.join(gpk::COOKED_PC_DIR);
    let mut additions = Vec::new();
    for e in &entries {
        let cf = e["composite_filename"].as_str().ok_or("missing composite_filename")?;
        let src = staging.join(format!("{cf}.gpk"));
        let dst = cooked.join(format!("{cf}.gpk"));
        gpk::copy_atomic(&src, &dst)?;
        additions.push(mapper_extend::MapperAddition {
            logical_path: e["logical_path"].as_str().ok_or("logical_path")?.into(),
            composite_uid: e["composite_uid"].as_str().ok_or("composite_uid")?.into(),
            composite_object_path: e["composite_object_path"].as_str().ok_or("composite_object_path")?.into(),
            composite_filename: cf.to_string(),
            composite_offset: e["composite_offset"].as_i64().unwrap_or(0),
            composite_size: e["composite_size"].as_i64().unwrap_or(0),
        });
    }

    mapper_extend::extend_mappers(&game_root, &additions)?;
    println!("installed {} resources", additions.len());
    Ok(())
}
```

- [ ] **Step 2: Run install**

```bash
# Pre-check: roll back any prior mod state
cp "D:/Elinu/S1Game/CookedPC/CompositePackageMapper.clean" "D:/Elinu/S1Game/CookedPC/CompositePackageMapper.dat"
cp "D:/Elinu/S1Game/CookedPC/PkgMapper.clean" "D:/Elinu/S1Game/CookedPC/PkgMapper.dat"
rm -f "D:/Elinu/S1Game/CookedPC/RestylePaperdoll.gpk" "D:/Elinu/S1Game/CookedPC/modres_paperdoll_*.gpk"

# Reinstall the SWF mod
cd teralaunch/src-tauri && ./target/debug/install-paperdoll-fresh \
  --game-root "D:/Elinu" \
  --mod-gpk "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/.gpk-rebuild/publish-fresh/RestylePaperdoll.gpk" \
  --container-name "RestylePaperdoll" \
  --object-path "c7a706fb_268926b3_1ddcb.PaperDoll_dup"

# Install resources
./target/debug/install-paperdoll-resources \
  --game-root "D:/Elinu" \
  --staging "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/.gpk-rebuild/publish-fresh/paperdoll-resources"
```

- [ ] **Step 3: Verify state**

```bash
cd teralaunch/src-tauri && ./target/debug/find-current-gpk-mapper "D:/Elinu" "PaperDoll_AM"
```
Expected: row `S1UIRES_Skin.PaperDoll_AM,modres_skin_*.PaperDoll_AM_dup` in PkgMapper, and row `modres_paperdoll_skin_*?modres_skin_*.PaperDoll_AM_dup,...` in CompositePackageMapper.

- [ ] **Step 4: User in-game smoke test**

User launches game, opens equipment window, screenshots. Compare to vanilla and to the SWF-only result. Report visual delta.

- [ ] **Step 5: Iterate if needed**

If visual is still wrong, gather a new screenshot and apply systematic-debugging:
- Are the new GPKs being read by the engine? (check the launcher log if any)
- Are the textures in the right format? (DDS DXT1 vs vanilla expected DXT5 mismatch?)
- Is the SWF actually loading via `img://__S1UIRES_Skin.<name>`?

---

## Task 7 (CONDITIONAL): SWF AS2 patch

Only if Task 0 confirms Hypothesis A.

- [ ] **Step 1: Decompile mod.gfx with JPEXS** (manual)

Open `tera-restyle-clone/PaperDoll/p95/mod.gfx` in JPEXS. Locate frame_1 → `DoAction.as`. Find the `OnGameEventUpdatePaperDollSlotList` listener. Inspect its parameters and how they're used. Compare to v100 expectations (which we infer from observed behavior — number of items, slot count).

- [ ] **Step 2: Patch the AS2** (manual)

Modify the listener to handle v100's data shape. Specific change depends on the discovered drift — likely an extra field in the array element or a new event name we need to register.

- [ ] **Step 3: Recompile to mod-patched.gfx** (manual)

JPEXS → "Save AS" → re-export.

- [ ] **Step 4: Re-author the paperdoll GPK with the patched SWF**

```bash
cd teralaunch/src-tauri && ./target/debug/port-paperdoll-fresh \
  --vanilla-x64 "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/.gpk-rebuild/vanilla/S1UI_PaperDoll.PaperDoll_dup.gpk" \
  --mod-swf "<path-to-mod-patched.gfx>" \
  --target-export "PaperDoll_dup" \
  --output "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/.gpk-rebuild/publish-fresh/RestylePaperdoll.gpk"
```

- [ ] **Step 5: Reinstall and smoke-test**

(Same commands as Task 6 Step 2.)

---

## Self-Review

**Spec coverage:**
- "Modded race silhouettes" → Task 5 builds them from RES_Skin DDS files via Tasks 1+2+3+4.
- "Modded UI component atlases" → Task 5 also builds them from RES_Component.
- "Possible AS2 fixes" → Task 0 verifies hypothesis A; Task 7 fixes if confirmed.
- "Verify hypothesis A" → Task 0.
- "Install + smoke" → Task 6.

**Placeholder scan:**
- Task 3 Step 3 has `todo!()` for the full serializer. **This is intentional** — that step's implementation is too large for inline plan code; the plan instead specifies the inputs/outputs/template-source and tells the agent to iterate against `cargo test`. Add a `composite_author_layout.md` notes file before starting Task 3 if you need a hex-level reference.
- Task 2 Step 4b assumes a `--dump-export --hex-window-around-cached-mips` flag on `inspect-gpk-resources`; if that flag doesn't exist, add it as part of Step 4b (it's a small addition).

**Type consistency:**
- `MapperAddition` defined in Task 4, used in Tasks 5 and 6.
- `DdsImage` defined in Task 1, used in Tasks 2 and 5.
- `NameTableBuilder` defined in Task 2, used in Task 3.
- `author_composite_slice` defined in Task 3, used in Task 5.
- `extend_mappers` defined in Task 4, used in Task 6.

**Risk callouts:**
- The 16-byte cached-mip preamble in `texture_encoder.rs` (Task 2 Step 4b) MUST be copied byte-for-byte from a vanilla template. Getting this wrong silently produces a malformed mip array that may parse OK but render garbage.
- We're authoring **new mapper rows** without modifying `.clean`. This means our `gpk::restore_clean_*` rollback path will leave residual rows in `.dat`. Decide: either also reflect new rows into `.clean` (so rollback removes them) or document that resource-pack uninstall requires a separate pass. **Recommendation:** add a `--write-clean` flag to `extend_mappers` and use it during install, so rollback restores cleanly.
- Texture format mismatch (DDS DXT5 → vanilla expects DXT1 or vice versa) will produce wrong colors / alpha. Add a format check in Task 5: if foglio DDS format ≠ what TERA expects for that texture name, log a warning. We can find expected format by scanning vanilla `S1UIRES_Skin` → composite `BG_*_dup` → Texture2D Format property, but for *new* textures we don't have a vanilla reference, so we trust foglio's authored format.
- Foglio's RES_Skin filenames use `BG_PaperDoll_<race><sex>` (e.g. `BG_PaperDoll_AM`) but the SWF references `__S1UIRES_Skin.PaperDoll_<race>_<sex>` (e.g. `PaperDoll_AM`). Task 5 strips the `BG_` prefix during file→texture-name conversion. Verify against an actual foglio DDS filename listing before relying on this rule — there may be edge cases (e.g. `BG_PaperDoll_HighElf_F`).
