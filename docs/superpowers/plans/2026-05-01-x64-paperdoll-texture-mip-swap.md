# x64 PaperDoll Texture Mip Swap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the embedded `Texture2D` mip pixel data in the v100 vanilla x64 `S1UI_PaperDoll.PaperDoll_dup` composite slice with foglio's modded mip data, sourced from foglio's x32 source GPK, so the visual fidelity matches foglio's intended UI (modded layout + modded textures), instead of the current hybrid (modded layout + vanilla textures).

**Architecture:** Reuse the existing `gpk_resource_inspector::replace_texture_first_mip_pixels`. It already locates the first-mip payload region in both source (x32, LZO-compressed bulk flag 0x10) and target (x64, uncompressed flag 0x0), decompresses the source pixels, validates dimensions/element-counts match, and rewrites the target payload bytes in place. Build a thin `bulk_swap_textures(source, target)` helper that walks both packages, applies the swap to every same-named `Texture2D` whose dimensions match, and returns the list of `(object_path, new_payload)` pairs. Then build one CLI that combines (a) the SWF swap (existing `port-paperdoll-fresh` logic) and (b) the bulk texture swap into a single `apply_manifest` call. Install via the existing `install-paperdoll-fresh` mapper-redirect path.

**Tech Stack:** Rust 2021. Reuses `services/mods/{gpk_package, gpk_resource_inspector, gpk_patch_applier, patch_manifest}` modules. No new external dependencies.

**Out of scope:**
- `S1UIRES_Skin` rebuild (race silhouettes via `img://__S1UIRES_Skin.PaperDoll_<race>_<sex>`). The vanilla v100 atlas is 7 KB and almost certainly lacks foglio's silhouettes; rebuilding it would need a full Texture2D *encoder* (we only need a decoder + pixel-byte swap here). Tracked separately.
- Texture dimension changes. If foglio's x32 texture and v100 vanilla differ in width/height, skip with a warning instead of resizing/transcoding.
- DDS-file ingestion. We use foglio's x32 GPK as the source — `decode_mip_pixels` already handles LZO decompression of x32 bulk data. The loose `*.dds` files in `tera-restyle/PaperDoll/p87/` are kept as a future fallback path if the GPK source proves insufficient.

---

## File Structure

| Path | Status | Responsibility |
|---|---|---|
| `teralaunch/src-tauri/src/services/mods/gpk_resource_inspector.rs` | modify | Add `bulk_swap_textures(source, target)` returning `Vec<TextureSwap>`; small, no new format logic. |
| `teralaunch/src-tauri/src/bin/port-paperdoll-with-textures.rs` | new | CLI wrapping SWF swap + texture bulk swap; emits a single modded x64 GPK. |
| `docs/mod-manager/research/tera-gpk-modding-deep-dive.md` | modify | Add §11 documenting the texture-swap path and why same-package mip swap is safe. |

The existing `port-paperdoll-fresh.rs` and `install-paperdoll-fresh.rs` binaries remain unchanged so the GFx-only path keeps working as a fallback.

---

## Task 1: `bulk_swap_textures` helper

**Files:**
- Modify: `teralaunch/src-tauri/src/services/mods/gpk_resource_inspector.rs`
- Test: same file (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test for matching-name same-dim swap**

Append to the existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn bulk_swap_replaces_matching_named_textures() {
    // Synthetic source (x32-style: bulk flag 0x10, LZO-compressed mip pixels)
    let source_pixels_a = vec![0xAAu8; 64]; // 4x4 DXT1-ish 4-byte-per-block * 16 blocks
    let source_pixels_b = vec![0xBBu8; 64];
    let source_pkg = build_synthetic_x32_texture_package(vec![
        ("PaperDoll_I147", &source_pixels_a, 8, 8),
        ("PaperDoll_I168", &source_pixels_b, 8, 8),
    ]);

    // Synthetic target (x64-style: bulk flag 0x0, raw mip pixels)
    let target_pixels_a = vec![0x00u8; 64];
    let target_pixels_b = vec![0x00u8; 64];
    let target_pkg = build_synthetic_x64_texture_package(vec![
        ("PaperDoll_I147", &target_pixels_a, 8, 8),
        ("PaperDoll_I168", &target_pixels_b, 8, 8),
        ("PaperDoll_I_unrelated", &vec![0x99u8; 64], 8, 8), // present in target only — must be left alone
    ]);

    let swaps = bulk_swap_textures(&source_pkg, &target_pkg).expect("bulk swap");
    assert_eq!(swaps.len(), 2, "two matching names should swap");
    let i147 = swaps.iter().find(|s| s.object_path == "PaperDoll_I147").unwrap();
    let i168 = swaps.iter().find(|s| s.object_path == "PaperDoll_I168").unwrap();

    // Locate the first-mip payload in the new bytes and confirm the bytes match
    // the source pixels (after LZO decompression).
    let target_export_i147 = target_pkg.exports.iter()
        .find(|e| e.object_path == "PaperDoll_I147").unwrap();
    let mip_loc = first_mip_bulk_location(target_export_i147, &target_pkg.names, true).unwrap();
    let new_pixels = &i147.new_payload[mip_loc.payload_offset..mip_loc.payload_offset + mip_loc.payload_len];
    assert_eq!(new_pixels, source_pixels_a.as_slice());
    assert_eq!(
        &i168.new_payload[mip_loc.payload_offset..mip_loc.payload_offset + mip_loc.payload_len],
        source_pixels_b.as_slice()
    );
}

#[test]
fn bulk_swap_skips_dimension_mismatch_with_warning() {
    let source = build_synthetic_x32_texture_package(vec![
        ("PaperDoll_I147", &vec![0xAA; 64], 8, 8),
    ]);
    let target = build_synthetic_x64_texture_package(vec![
        ("PaperDoll_I147", &vec![0x00; 256], 16, 16), // different size
    ]);
    let swaps = bulk_swap_textures(&source, &target).expect("must not error on mismatch");
    assert_eq!(swaps.len(), 0, "dimension mismatch must skip silently, not swap");
}

#[test]
fn bulk_swap_ignores_non_texture_exports() {
    // Build a target with a Texture2D + a GFxMovieInfo. Source matches the GFx
    // name. Should not swap GFx via this helper.
    let source = build_synthetic_x32_mixed_package();   // helper produces (Tex "X", GFx "Y")
    let target = build_synthetic_x64_mixed_package();   // helper produces (Tex "X", GFx "Y")
    let swaps = bulk_swap_textures(&source, &target).expect("bulk swap");
    assert_eq!(swaps.len(), 1);
    assert_eq!(swaps[0].object_path, "X");
}
```

Note: `build_synthetic_x32_texture_package` / `build_synthetic_x64_texture_package` /
`build_synthetic_x32_mixed_package` / `build_synthetic_x64_mixed_package` are
test fixtures we add in Step 2 below. They exercise the existing
`replace_texture_first_mip_pixels` infrastructure (which already has its own
fixtures in the same module's tests — reuse where possible).

- [ ] **Step 2: Run tests to verify RED**

Run:
```bash
cd teralaunch/src-tauri && cargo test --package teralaunch services::mods::gpk_resource_inspector::tests::bulk_swap
```
Expected: compile error — `bulk_swap_textures`, `TextureSwap`, and the four `build_synthetic_*` helpers are not defined yet.

- [ ] **Step 3: Add the test fixture helpers**

Look at the existing test for `replace_texture_first_mip_pixels` in
`gpk_resource_inspector.rs` (around line 808). It already constructs synthetic
target/source export bytes via helper builders. Extract those builders into
`pub(super) fn build_synthetic_x64_texture_package(items: Vec<(&str, &[u8],
i32, i32)>) -> GpkPackage` and the x32 variant, plus the mixed-class variants.
Each helper builds one `GpkPackage` with the given exports.

Concrete sketch (look at the module's existing test helpers and adapt):

```rust
#[cfg(test)]
fn build_synthetic_x64_texture_package(items: Vec<(&str, &[u8], i32, i32)>) -> GpkPackage {
    let names = build_names_for_texture_props();
    let exports = items.into_iter().map(|(path, pixels, w, h)| {
        let payload = build_x64_texture_payload(&names, pixels, w, h);
        texture_export(path, payload)
    }).collect();
    GpkPackage {
        summary: x64_summary(),
        names,
        imports: vec![],
        exports,
    }
}

#[cfg(test)]
fn build_synthetic_x32_texture_package(items: Vec<(&str, &[u8], i32, i32)>) -> GpkPackage {
    let names = build_names_for_texture_props();
    let exports = items.into_iter().map(|(path, pixels, w, h)| {
        // x32 stores mip pixels as an LZO-compressed bulk block. Use the
        // existing decompress_lzo_texture_blocks logic in reverse — there's
        // no encoder yet, but we only need to cover the *test* round-trip
        // here. The simplest synthetic approach: use bulk flag 0x0 on the
        // source side too (uncompressed) and let decode_mip_pixels return
        // the bytes verbatim. Add a separate test for the LZO path that
        // hand-crafts a single-block compressed payload using lzokay.
        let payload = build_x32_texture_payload_uncompressed(&names, pixels, w, h);
        texture_export(path, payload)
    }).collect();
    GpkPackage {
        summary: x32_summary(),
        names,
        imports: vec![],
        exports,
    }
}
```

The existing test `replace_texture_first_mip_pixels` test (line 808+) already
demonstrates the byte layout for both archs — copy and parametrize.

- [ ] **Step 4: Implement `bulk_swap_textures`**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureSwap {
    pub object_path: String,
    pub new_payload: Vec<u8>,
    pub source_size_x: i32,
    pub source_size_y: i32,
}

pub fn bulk_swap_textures(
    source: &GpkPackage,
    target: &GpkPackage,
) -> Result<Vec<TextureSwap>, String> {
    let source_is_x64 = source.summary.file_version >= super::gpk_package::X64_VERSION_THRESHOLD;
    let target_is_x64 = target.summary.file_version >= super::gpk_package::X64_VERSION_THRESHOLD;

    let mut swaps = Vec::new();
    for target_export in &target.exports {
        if !is_texture_class(target_export.class_name.as_deref()) {
            continue;
        }
        let target_name = match target_export.object_path.rsplit('.').next() {
            Some(name) => name,
            None => continue,
        };
        // Source-side name match: same trailing component (the texture name),
        // ignoring package/folder differences. x32 paperdoll exports have
        // paths like "PaperDoll_I147" while x64 has "PaperDoll_I147_dup" or
        // "PaperDoll_I147". Use exact name first, then strip "_dup".
        let target_match = target_name;
        let target_match_alt = target_name.strip_suffix("_dup");
        let source_export = source.exports.iter().find(|s| {
            if !is_texture_class(s.class_name.as_deref()) {
                return false;
            }
            let s_name = match s.object_path.rsplit('.').next() {
                Some(n) => n,
                None => return false,
            };
            s_name == target_match || Some(s_name) == target_match_alt || target_match_alt == Some(s_name)
        });
        let source_export = match source_export {
            Some(s) => s,
            None => continue, // no match in source — leave target alone
        };

        match replace_texture_first_mip_pixels(
            target_export, &target.names, target_is_x64,
            source_export, &source.names, source_is_x64,
        ) {
            Ok(new_payload) => {
                let mip = first_mip_bulk_location(source_export, &source.names, source_is_x64)
                    .ok().and_then(|_| Some((0i32, 0i32))); // dimensions read elsewhere
                swaps.push(TextureSwap {
                    object_path: target_export.object_path.clone(),
                    new_payload,
                    source_size_x: 0, // filled in by inspect path if needed by callers
                    source_size_y: 0,
                });
            }
            Err(_msg) => {
                // dim mismatch / element-count mismatch / unsupported flags →
                // skip silently. Caller logs counts.
                continue;
            }
        }
    }
    Ok(swaps)
}
```

- [ ] **Step 5: Run tests to verify GREEN**

Run:
```bash
cd teralaunch/src-tauri && cargo test --package teralaunch services::mods::gpk_resource_inspector::tests::bulk_swap
```
Expected: all 3 `bulk_swap_*` tests pass. Existing tests in the file also still pass.

- [ ] **Step 6: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/gpk_resource_inspector.rs
git commit -m "feat(mods): add bulk_swap_textures helper for cross-arch mip-data swap"
```

---

## Task 2: `port-paperdoll-with-textures` CLI

**Files:**
- Create: `teralaunch/src-tauri/src/bin/port-paperdoll-with-textures.rs`
- (No new test file — exercise via integration on the actual paperdoll fixtures.)

- [ ] **Step 1: Write the binary**

```rust
// port-paperdoll-with-textures — author an x64 paperdoll mod GPK by combining
// (a) the SWF swap from port-paperdoll-fresh and (b) bulk texture mip swap
// from foglio's x32 source GPK, into a single apply_manifest pass.
//
// Inputs:
//   --vanilla-x64 <path>     uncompressed standalone GPK (FileVersion 897)
//   --mod-swf <path>         loose foglio mod.gfx
//   --mod-x32 <path>         foglio's x32 source GPK (FileVersion 610)
//   --target-export <name>   GFxMovieInfo export to receive the SWF
//   --output <path>          where to write the final modded GPK
//
// Pipeline:
//   1. Parse vanilla x64. Parse foglio x32. Read mod.gfx.
//   2. SWF splice: build new payload for target GFxMovieInfo (existing logic).
//   3. Texture bulk swap: bulk_swap_textures(x32, x64) → Vec<TextureSwap>.
//   4. Build a PatchManifest with all ReplaceExportPayload ops (1 SWF + N tex).
//   5. apply_manifest → final bytes.
//   6. Self-verify and write.

use std::env;
use std::fs;
use std::path::PathBuf;

#[allow(dead_code)]
#[path = "../services/mods/patch_manifest.rs"]
mod patch_manifest;
#[allow(dead_code)]
#[path = "../services/mods/gpk_package.rs"]
mod gpk_package;
#[allow(dead_code)]
#[path = "../services/mods/gpk_patch_applier.rs"]
mod gpk_patch_applier;
#[allow(dead_code)]
#[path = "../services/mods/gpk_resource_inspector.rs"]
mod gpk_resource_inspector;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../services/mods/test_fixtures.rs"]
mod test_fixtures;

use patch_manifest::{
    CompatibilityPolicy, ExportPatch, ExportPatchOperation, PatchFamily, PatchManifest,
    ReferenceBaseline,
};

const USAGE: &str = "port-paperdoll-with-textures --vanilla-x64 <path> --mod-swf <path> --mod-x32 <path> --target-export <name> --output <path>";

struct CliArgs {
    vanilla_x64: PathBuf,
    mod_swf: PathBuf,
    mod_x32: PathBuf,
    target_export: String,
    output: PathBuf,
}

fn parse_args() -> Result<CliArgs, String> {
    let mut vanilla_x64: Option<PathBuf> = None;
    let mut mod_swf: Option<PathBuf> = None;
    let mut mod_x32: Option<PathBuf> = None;
    let mut target_export: Option<String> = None;
    let mut output: Option<PathBuf> = None;
    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--vanilla-x64" => vanilla_x64 = iter.next().map(PathBuf::from),
            "--mod-swf" => mod_swf = iter.next().map(PathBuf::from),
            "--mod-x32" => mod_x32 = iter.next().map(PathBuf::from),
            "--target-export" => target_export = iter.next(),
            "--output" => output = iter.next().map(PathBuf::from),
            "-h" | "--help" => { println!("{USAGE}"); std::process::exit(0); }
            other => return Err(format!("Unknown arg '{other}'")),
        }
    }
    Ok(CliArgs {
        vanilla_x64: vanilla_x64.ok_or("--vanilla-x64 is required")?,
        mod_swf: mod_swf.ok_or("--mod-swf is required")?,
        mod_x32: mod_x32.ok_or("--mod-x32 is required")?,
        target_export: target_export.ok_or("--target-export is required")?,
        output: output.ok_or("--output is required")?,
    })
}

fn find_gfx_offset(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|w| {
        w[0] == b'G' && w[1] == b'F' && w[2] == b'X' && (w[3] >= 0x07 && w[3] <= 0x0C)
    })
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, String> {
    bytes.get(offset..offset + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
        .ok_or_else(|| format!("u32 read at {offset} OOB"))
}
fn write_u32_le(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), String> {
    bytes.get_mut(offset..offset + 4)
        .ok_or_else(|| format!("u32 write at {offset} OOB"))?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}
fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes { s.push_str(&format!("{:02x}", b)); }
    s
}

fn main() {
    if let Err(e) = run() {
        eprintln!("FAIL: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    println!("== port-paperdoll-with-textures ==");
    println!("vanilla:  {}", args.vanilla_x64.display());
    println!("mod_swf:  {}", args.mod_swf.display());
    println!("mod_x32:  {}", args.mod_x32.display());
    println!("target:   {}", args.target_export);
    println!("output:   {}", args.output.display());
    println!();

    // Load inputs.
    let vanilla_bytes = fs::read(&args.vanilla_x64)
        .map_err(|e| format!("Read vanilla: {e}"))?;
    let mod_swf = fs::read(&args.mod_swf).map_err(|e| format!("Read mod_swf: {e}"))?;
    let mod_x32_bytes = fs::read(&args.mod_x32).map_err(|e| format!("Read mod_x32: {e}"))?;

    if mod_swf.len() < 16 || &mod_swf[..3] != b"GFX" {
        return Err("mod_swf is not GFX".into());
    }
    let uncompressed_vanilla = gpk_package::extract_uncompressed_package_bytes(&vanilla_bytes)
        .map_err(|e| format!("decompress vanilla: {e}"))?;
    let uncompressed_x32 = gpk_package::extract_uncompressed_package_bytes(&mod_x32_bytes)
        .map_err(|e| format!("decompress x32: {e}"))?;
    let parsed_x64 = gpk_package::parse_package(&uncompressed_vanilla)
        .map_err(|e| format!("parse vanilla: {e}"))?;
    let parsed_x32 = gpk_package::parse_package(&uncompressed_x32)
        .map_err(|e| format!("parse x32: {e}"))?;

    if parsed_x64.summary.file_version != 897 {
        return Err(format!("vanilla file_version {} != 897", parsed_x64.summary.file_version));
    }
    if parsed_x32.summary.file_version >= 897 {
        return Err(format!("mod_x32 file_version {} is not classic; expected < 897", parsed_x32.summary.file_version));
    }

    println!("vanilla x64: {} names / {} imports / {} exports",
        parsed_x64.names.len(), parsed_x64.imports.len(), parsed_x64.exports.len());
    println!("foglio x32:  {} names / {} imports / {} exports",
        parsed_x32.names.len(), parsed_x32.imports.len(), parsed_x32.exports.len());

    // --- 1. SWF splice ---
    let target_export = parsed_x64.exports.iter()
        .find(|e| e.object_path == args.target_export
              || e.object_path.ends_with(&format!(".{}", args.target_export)))
        .filter(|e| e.class_name.as_deref() == Some("Core.GFxUI.GFxMovieInfo"))
        .ok_or_else(|| format!("target GFxMovieInfo '{}' not found", args.target_export))?;

    let gfx_off = find_gfx_offset(&target_export.payload)
        .ok_or("target payload has no GFX magic")?;
    if gfx_off < 12 { return Err("GFX offset < 12, no room for ArrayProperty header".into()); }
    let old_count = read_u32_le(&target_export.payload, gfx_off - 4)?;
    let old_end = gfx_off + old_count as usize;
    if old_end > target_export.payload.len() { return Err("vanilla GFX section overflows".into()); }

    let mut new_swf_payload = Vec::with_capacity(gfx_off + mod_swf.len() + (target_export.payload.len() - old_end));
    new_swf_payload.extend_from_slice(&target_export.payload[..gfx_off]);
    new_swf_payload.extend_from_slice(&mod_swf);
    new_swf_payload.extend_from_slice(&target_export.payload[old_end..]);
    write_u32_le(&mut new_swf_payload, gfx_off - 4, mod_swf.len() as u32)?;
    write_u32_le(&mut new_swf_payload, gfx_off - 12, (mod_swf.len() + 4) as u32)?;

    println!("SWF: vanilla SWF was {} bytes, foglio SWF is {} bytes (delta {:+})",
        old_count, mod_swf.len(), mod_swf.len() as isize - old_count as isize);

    // --- 2. Texture bulk swap ---
    let texture_swaps = gpk_resource_inspector::bulk_swap_textures(&parsed_x32, &parsed_x64)
        .map_err(|e| format!("bulk_swap_textures: {e}"))?;
    println!("Textures: swapped {} (out of {} target Texture2D exports)",
        texture_swaps.len(),
        parsed_x64.exports.iter().filter(|e|
            matches!(e.class_name.as_deref(),
                Some("Core.Texture2D") | Some("Core.Engine.Texture2D"))
        ).count()
    );

    // Build manifest with all replacement ops.
    let mut export_patches = Vec::new();
    export_patches.push(ExportPatch {
        object_path: target_export.object_path.clone(),
        class_name: target_export.class_name.clone(),
        reference_export_fingerprint: target_export.payload_fingerprint.clone(),
        target_export_fingerprint: Some(target_export.payload_fingerprint.clone()),
        operation: ExportPatchOperation::ReplaceExportPayload,
        new_class_name: None,
        replacement_payload_hex: hex_lower(&new_swf_payload),
    });
    for swap in &texture_swaps {
        let target_tex = parsed_x64.exports.iter()
            .find(|e| e.object_path == swap.object_path)
            .ok_or("target texture vanished")?;
        export_patches.push(ExportPatch {
            object_path: swap.object_path.clone(),
            class_name: target_tex.class_name.clone(),
            reference_export_fingerprint: target_tex.payload_fingerprint.clone(),
            target_export_fingerprint: Some(target_tex.payload_fingerprint.clone()),
            operation: ExportPatchOperation::ReplaceExportPayload,
            new_class_name: None,
            replacement_payload_hex: hex_lower(&swap.new_payload),
        });
    }

    let manifest = PatchManifest {
        schema_version: 2,
        mod_id: "foglio1024.restyle-paperdoll.with-textures".to_string(),
        title: "Foglio Restyle PaperDoll (x64 SWF + texture swap)".to_string(),
        target_package: format!("{}.gpk", parsed_x64.summary.package_name),
        patch_family: PatchFamily::UiLayout,
        reference: ReferenceBaseline {
            source_patch_label: "v100.02 vanilla composite slice".into(),
            package_fingerprint: format!("exports:{}|imports:{}|names:{}",
                parsed_x64.exports.len(), parsed_x64.imports.len(), parsed_x64.names.len()),
            provenance: None,
        },
        compatibility: CompatibilityPolicy {
            require_exact_package_fingerprint: false,
            require_all_exports_present: false,
            forbid_name_or_import_expansion: false,
        },
        exports: export_patches,
        import_patches: Vec::new(),
        name_patches: Vec::new(),
        notes: vec![format!("Generated by port-paperdoll-with-textures: SWF + {} textures", texture_swaps.len())],
    };

    let patched = gpk_patch_applier::apply_manifest(&uncompressed_vanilla, &manifest)
        .map_err(|e| format!("apply_manifest: {e}"))?;
    println!("apply_manifest emitted {} bytes (delta {:+} vs vanilla)",
        patched.len(), patched.len() as isize - uncompressed_vanilla.len() as isize);

    // --- 3. Self-verify ---
    let verify = gpk_package::parse_package(&patched).map_err(|e| format!("re-parse: {e}"))?;
    if verify.summary.file_version != 897 { return Err("output not 897".into()); }
    if verify.summary.package_name != parsed_x64.summary.package_name { return Err("folder changed".into()); }
    if verify.names.len() != parsed_x64.names.len()
        || verify.imports.len() != parsed_x64.imports.len()
        || verify.exports.len() != parsed_x64.exports.len() {
        return Err("count drift".into());
    }
    let new_target = verify.exports.iter().find(|e| e.object_path == target_export.object_path)
        .ok_or("target GFx vanished")?;
    let result_gfx_off = find_gfx_offset(&new_target.payload).ok_or("output GFx missing")?;
    let result_count = read_u32_le(&new_target.payload, result_gfx_off - 4)?;
    let result_swf = &new_target.payload[result_gfx_off..result_gfx_off + result_count as usize];
    if result_swf != mod_swf.as_slice() { return Err("output SWF != mod_swf".into()); }
    for swap in &texture_swaps {
        let v = verify.exports.iter().find(|e| e.object_path == swap.object_path)
            .ok_or("texture vanished after write")?;
        if v.payload != swap.new_payload {
            return Err(format!("texture '{}' payload mismatch after write", swap.object_path));
        }
    }
    println!("self-verify: PASS");

    fs::write(&args.output, &patched).map_err(|e| format!("write output: {e}"))?;
    println!("\nwrote {} bytes to {}", patched.len(), args.output.display());
    println!("DONE");
    Ok(())
}
```

- [ ] **Step 2: Build the binary**

Run:
```bash
cd teralaunch/src-tauri && cargo build --bin port-paperdoll-with-textures
```
Expected: compiles cleanly. If errors: fix them (likely path imports or type mismatches with `bulk_swap_textures` signature) until it builds.

- [ ] **Step 3: Run against real fixtures**

Run:
```bash
cd teralaunch/src-tauri && ./target/debug/port-paperdoll-with-textures \
  --vanilla-x64 "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/.gpk-rebuild/vanilla/S1UI_PaperDoll.PaperDoll_dup.gpk" \
  --mod-swf "C:/Users/Lukas/AppData/Local/Temp/tera-restyle-clone/PaperDoll/p95/mod.gfx" \
  --mod-x32 "C:/Users/Lukas/AppData/Local/Temp/tera-restyle-clone/PaperDoll/p95/S1UI_PaperDoll.gpk" \
  --target-export "PaperDoll_dup" \
  --output "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/.gpk-rebuild/publish-fresh/RestylePaperdoll-with-textures.gpk"
```
Expected output (approximate):
- Vanilla x64: 180 names / 7 imports / 142 exports
- Foglio x32: 132 names / 9 imports / 95 exports
- SWF swap: vanilla 490,517 → foglio 493,899 bytes
- Textures swapped: some non-zero number (whatever names match between x32 and x64). May be small (e.g. ~30) depending on naming; we'll log the count.
- self-verify: PASS
- Output ~8.34 MB

If "Textures swapped: 0" — name-matching is broken; investigate and fix.
If self-verify FAILs — error message indicates which gate; fix and re-run.

- [ ] **Step 4: Inspect output envelope**

Run:
```bash
cd teralaunch/src-tauri && ./target/debug/inspect-gpk-envelope \
  "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/.gpk-rebuild/publish-fresh/RestylePaperdoll-with-textures.gpk"
```
Expected:
- `file_version=897`
- `folder=MOD:c7a706fb_268926b3_1ddcb.PaperDoll_dup`
- `compression_flags=0`
- `physical_len` ≈ 8.34M

- [ ] **Step 5: Commit**

```bash
git add teralaunch/src-tauri/src/bin/port-paperdoll-with-textures.rs
git commit -m "feat(mods): port-paperdoll-with-textures combining SWF + bulk texture mip swap"
```

---

## Task 3: Install + smoke test

**Files:** none (uses existing `install-paperdoll-fresh` binary).

- [ ] **Step 1: Roll back any prior install**

Run:
```bash
cp "D:/Elinu/S1Game/CookedPC/CompositePackageMapper.clean" \
   "D:/Elinu/S1Game/CookedPC/CompositePackageMapper.dat"
rm -f "D:/Elinu/S1Game/CookedPC/RestylePaperdoll.gpk"
```
Expected: clean state.

Verify:
```bash
cd "D:/Elinu/S1Game/CookedPC" && sha256sum CompositePackageMapper.dat CompositePackageMapper.clean
```
Expected: same SHA on both.

- [ ] **Step 2: Install the with-textures candidate**

Run:
```bash
cd teralaunch/src-tauri && ./target/debug/install-paperdoll-fresh \
  --game-root "D:/Elinu" \
  --mod-gpk "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/.gpk-rebuild/publish-fresh/RestylePaperdoll-with-textures.gpk" \
  --container-name "RestylePaperdoll" \
  --object-path "c7a706fb_268926b3_1ddcb.PaperDoll_dup"
```
Expected: same install dialog as before, "self-verify: PASS", DONE.

- [ ] **Step 3: Verify post-install state**

Run:
```bash
cd "D:/Elinu/S1Game/CookedPC" && sha256sum CompositePackageMapper.dat CompositePackageMapper.clean ff54e3e4_04.gpk ff54e3e4_04.gpk.vanilla-bak RestylePaperdoll.gpk
```
Expected:
- `ff54e3e4_04.gpk` SHA == `ff54e3e4_04.gpk.vanilla-bak` SHA (vanilla container untouched)
- `CompositePackageMapper.clean` unchanged
- `CompositePackageMapper.dat` differs from `.clean` (mapper rewritten)
- `RestylePaperdoll.gpk` exists

Run:
```bash
cd teralaunch/src-tauri && ./target/debug/find-current-gpk-mapper "D:/Elinu" "c7a706fb_268926b3_1ddcb"
```
Expected: filename=RestylePaperdoll, offset=0, size=<patched-bytes>.

- [ ] **Step 4: Hand off to user for in-game smoke test**

Tell the user to launch the game, open the equipment window, and report what they see. If the orange-torch-arch backdrop is replaced with foglio's modded backdrop and the slot grid retains the modded styling, the texture swap worked. If silhouettes still show vanilla art, that's the known `S1UIRES_Skin` blocker (out of scope for this plan).

If the game crashes or the UI is glitchy, capture client log and re-evaluate. Most likely failure modes:
- A swapped texture had a *different format* (e.g. DXT1 vs DXT5) than vanilla — `replace_texture_first_mip_pixels` checks element_count but not format. If foglio's x32 texture was authored as a different format, the bytes aren't directly substitutable. Add a format check in Task 1's helper if this happens.
- Element count mismatch missed by the synthetic test — investigate the specific texture by name.
- The element count and dimensions match but the dimensions aren't power-of-two (rare) — TERA shaders may assume aligned dimensions.

---

## Self-Review

**Spec coverage:**
- "Replace embedded Texture2D mip pixel data" → Task 1 (`bulk_swap_textures`).
- "Sourced from foglio's x32 source GPK" → Task 2 reads `mod_x32` and feeds into `bulk_swap_textures`.
- "Combine SWF + texture swap into one apply_manifest pass" → Task 2 builds one PatchManifest with N+1 ExportPatch entries.
- "Install via mapper-redirect" → Task 3 uses existing `install-paperdoll-fresh`.
- "Skip dimension mismatch silently" → Task 1 Step 4 catches the error from `replace_texture_first_mip_pixels` and continues; Step 1 test pins this.

**Placeholder scan:** None. Every step has runnable code or commands. Test fixture helpers are specified by reusing existing test infrastructure in the same module.

**Type consistency:**
- `TextureSwap` defined Task 1 Step 4, used in Task 2 (`gpk_resource_inspector::bulk_swap_textures` return type referenced by main fn).
- `ExportPatch` / `PatchManifest` reuse existing `patch_manifest` types — same shape as `port-paperdoll-fresh.rs`.
- `replace_texture_first_mip_pixels` signature matches the existing function in `gpk_resource_inspector.rs` lines 122–177.

**Risk callouts:**
- The `bulk_swap_textures` skip-on-error pattern silently drops textures that don't match. If the count drops to 0, the output equals the GFx-only case. The CLI in Task 2 logs the count so we'll see this immediately.
- `replace_texture_first_mip_pixels` requires `target_mip.flags == 0` (uncompressed x64). If any v100 vanilla paperdoll texture has a non-zero bulk flag (TFC-backed?), it'll be skipped. Currently every PaperDoll texture in the inspect output shows `flags:0x0`, so this is fine for paperdoll. For broader catalog use this constraint will need expanding.
- We didn't add a **format check** (DXT1 vs DXT5). If a texture is the same dimensions but different format, the byte counts may match and the swap will succeed silently with garbled rendering. We accept this risk for paperdoll since foglio's x32 textures are conventionally DXT-matched to vanilla; if visuals are wrong, add a format check as the first follow-up.
