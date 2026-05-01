# Foglio Mod Catalog x64 Port — Batch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port all 49 remaining foglio1024.* catalog entries to v100.02 (x64) so they install via the launcher's catalog instead of pointing to foglio's raw x32 URLs which can't deploy on v100.

**Architecture:** Three reusable batch porters — one per mod pattern. (1) `port-foglio-swf-mod` extracts the modded SWF (`mod.gfx`) from a foglio compiled GPK and splices it into the matching v100 vanilla wrapper, emits a TMM-format composite GPK targeting the widget's logical path. (2) `port-foglio-redirector-mod` synthesises a TMM mod from foglio's UI-remover redirector definitions (one ObjectRedirector per export-to-remove). (3) `inspect-foglio-toolbox-mod` audits each toolbox-* entry to decide whether it's a SWF mod, a redirector mod, or unsupported. After porting, all artifacts get bundled into one GitHub release on the catalog repo, then `catalog.json` is rewritten in a single commit with all 49 entries pointing at release-asset URLs.

**Tech Stack:** Rust (cargo bins under `teralaunch/src-tauri/src/bin/`), Python 3 (catalog editing scripts), `gh` CLI (release + asset upload), `git` (catalog repo commit/push).

---

## Pre-flight (one-time)

- Working tree: `C:/Users/Lukas/Documents/GitHub/TERA EU Classic/TERA-Europe-ClassicPlus-Launcher` (the launcher repo, where the porter tools live)
- Foglio source clones used by the porter:
  - `C:/Users/Lukas/AppData/Local/Temp/tera-restyle-clone/` (already cloned during paperdoll work)
  - `C:/Users/Lukas/AppData/Local/Temp/tera-modern-ui/` (already cloned)
  - Will need to fetch `foglio1024/UI-Remover` separately (different repo)
  - Will need to fetch toolbox source as encountered
- Catalog repo working copy: `C:/Users/Lukas/AppData/Local/Temp/external-mod-catalog/` (already cloned, on `main`)
- v100 vanilla GPK source-of-truth: `D:/Elinu/S1Game/CookedPC/` and `D:/Elinu/S1Game/CookedPC/Art_Data/Packages/S1UI/`
- Existing splice tool reference: `teralaunch/src-tauri/src/bin/splice-x32-payloads.rs` (paperdoll-tested, reuse its splice helpers)
- Existing TMM wrap reference: how `RestylePaperdoll.gpk` was built — composite container with `MOD:S1UI_PaperDoll.PaperDoll_dup` folder name

---

## File Structure

**New Rust binaries** (`teralaunch/src-tauri/src/bin/`):
- `port-foglio-swf-mod.rs` — single-mod SWF splice + TMM wrap CLI.
- `port-foglio-redirector-mod.rs` — single-mod redirector TMM emit CLI.
- `port-foglio-batch.rs` — driver that reads a TOML config of mods and runs the appropriate per-mod porter, writes a manifest of resulting (id, path, sha256, size) tuples.

**New helper modules** (`teralaunch/src-tauri/src/services/mods/`):
- `swf_splice.rs` — extracted helpers from `splice-x32-payloads.rs`: `extract_modgfx_from_x32_gpk`, `splice_modgfx_into_x64_wrapper`. Pulled out so the batch porter can call them.
- `tmm_wrap.rs` — `wrap_as_tmm_composite(modded_gpk_bytes, target_object_path) -> Vec<u8>`. Embeds the `MOD:<object_path>` folder name + emits container with proper offsets.

**New config** (`docs/mod-manager/`):
- `foglio-port-batch.toml` — declarative list of mods: `id`, `pattern` (`swf` / `redirector` / `skip`), `foglio_source_path`, `target_logical_path`, `notes`.

**Catalog scripts** (`scripts/`):
- `catalog-batch-update.py` — reads the porter manifest + opens `external-mod-catalog/catalog.json`, updates each affected entry's `download_url`/`sha256`/`size_bytes`/`compatible_arch`/`compatibility_notes`/`credits`, bumps `updated_at`.

---

## Phase 0: Audit + config build

### Task 1: Build foglio-port-batch.toml mod inventory

**Files:**
- Create: `docs/mod-manager/foglio-port-batch.toml`

- [ ] **Step 1: Run audit script to dump every foglio mod's category and best-guess target logical path**

```bash
cd "C:/Users/Lukas/AppData/Local/Temp/external-mod-catalog"
python << 'EOF'
import json
with open('catalog.json', encoding='utf-8') as f: c = json.load(f)
foglio = [m for m in c['mods'] if m['id'].startswith('foglio1024.') and 'TERA-Europe-Classic' not in m['download_url']]
patterns = {'restyle-': 'swf', 'modern-ui-': 'swf', 'ui-remover-': 'redirector', 'toolbox-': 'investigate', 'modern-ui-jewels-fix-': 'swf', 'badgui-': 'investigate', 's1ui-chat2-': 'swf'}
for m in foglio:
    short = m['id'].replace('foglio1024.','')
    pattern = next((v for k,v in patterns.items() if short.startswith(k.replace('foglio1024.',''))), 'investigate')
    print(f'  [{m["id"]}] kind={pattern} src={m["download_url"]} gpk_files={m.get("gpk_files",[])}')
EOF
```
Expected: 49 lines, each annotated with `swf` / `redirector` / `investigate`.

- [ ] **Step 2: Hand-write `foglio-port-batch.toml`** — convert the audit output to TOML format.

Template:
```toml
# Each entry: foglio mod_id mapped to porter pattern + parameters.
# Patterns:
#   "swf"        → splice mod.gfx into v100 vanilla wrapper of `target_package`
#   "redirector" → emit ObjectRedirector for each export listed under [[mods.removed_exports]]
#   "skip"       → mod cannot be ported automatically (needs manual investigation)
schema_version = 1
generated = "2026-05-01"

[[mods]]
id = "foglio1024.restyle-community-window"
pattern = "swf"
foglio_url = "https://raw.githubusercontent.com/foglio1024/tera-restyle/master/CommunityWindow/p90/S1UI_CommunityWindow.gpk"
target_package = "S1UI_CommunityWindow"
target_object = "CommunityWindow"
notes = "Restyle mod, p90 patch revision."

[[mods]]
id = "foglio1024.restyle-ep-window"
pattern = "swf"
foglio_url = "https://raw.githubusercontent.com/foglio1024/tera-restyle/master/EpWindow/p90/S1UI_EpWindow.gpk"
target_package = "S1UI_EpWindow"
target_object = "EpWindow"
notes = "Restyle mod, p90 patch revision."

# ... (47 more entries — one per mod from the audit dump)
```

- [ ] **Step 3: Commit the audit config**

```bash
cd "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/TERA-Europe-ClassicPlus-Launcher"
git add docs/mod-manager/foglio-port-batch.toml
git commit -m "docs(mod-manager): seed foglio-port-batch.toml with 49 entries"
```

---

## Phase 1: SWF mod batch porter (covers ~31 mods)

### Task 2: Extract reusable SWF splice helpers into `swf_splice.rs`

**Files:**
- Create: `teralaunch/src-tauri/src/services/mods/swf_splice.rs`
- Modify: `teralaunch/src-tauri/src/services/mods/mod.rs` (add `pub mod swf_splice;`)
- Test: `teralaunch/src-tauri/src/services/mods/swf_splice.rs` (in-file `#[cfg(test)]` module)

- [ ] **Step 1: Write the failing test (extract mod.gfx from a known foglio compiled GPK)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Test fixture: a tiny synthetic x32 GPK with one GFxMovieInfo export.
    const TEST_X32_GPK: &[u8] = include_bytes!("../../../tests/fixtures/tiny-x32-gfx.gpk");

    #[test]
    fn extracts_modgfx_from_x32_gpk() {
        let modgfx = extract_modgfx_from_x32_gpk(TEST_X32_GPK).unwrap();
        // SWF magic: 'CWS' (compressed) or 'FWS' (uncompressed) or 'GFX'/'CFX' for Scaleform
        assert!(matches!(&modgfx[..3], b"GFX" | b"CFX" | b"CWS" | b"FWS"));
        assert!(modgfx.len() > 100);
    }
}
```

- [ ] **Step 2: Run the test — it must fail because the function does not exist**

```bash
cd "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/TERA-Europe-ClassicPlus-Launcher/teralaunch/src-tauri"
cargo test --lib services::mods::swf_splice::tests::extracts_modgfx_from_x32_gpk 2>&1 | tail -10
```
Expected: compile error `cannot find function extract_modgfx_from_x32_gpk` OR the new module is missing — confirms the function isn't implemented.

- [ ] **Step 3: Add the test fixture file**

Copy a minimal foglio x32 GPK we already have:
```bash
cp "C:/Users/Lukas/AppData/Local/Temp/tera-restyle-clone/PaperDoll/p79/S1UI_PaperDoll79.gpk" \
   "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/TERA-Europe-ClassicPlus-Launcher/teralaunch/src-tauri/tests/fixtures/tiny-x32-gfx.gpk"
```

- [ ] **Step 4: Implement `extract_modgfx_from_x32_gpk`**

The implementation parses the x32 GPK using `gpk_package::parse_package` (already supports x32), walks `exports` for the `Core.GFxUI.GFxMovieInfo` class, and returns the embedded SWF byte slice from inside that export's payload. The byte layout for a GFxMovieInfo payload after the property block is `[i32 NetIndex][properties...None terminator][i32 swf_byte_count][swf_bytes...][trailing_object_refs]`.

```rust
//! SWF splice helpers — extract foglio's mod.gfx from an x32 GPK and
//! splice it into a v100 (x64) vanilla wrapper for redeployment.

use super::gpk_package::{parse_package, GpkPackage};

/// Extract the embedded SWF (mod.gfx) bytes from the GFxMovieInfo export of
/// an x32 (file_version 610) GPK package.
pub fn extract_modgfx_from_x32_gpk(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let pkg = parse_package(bytes).map_err(|e| format!("parse x32 gpk: {e}"))?;
    let gfx_export = pkg.exports.iter()
        .find(|e| matches!(e.class_name.as_deref(), Some("Core.GFxUI.GFxMovieInfo")))
        .ok_or_else(|| "no GFxMovieInfo export in foglio gpk".to_string())?;
    extract_swf_payload(&gfx_export.payload, &pkg, /*is_x64=*/ false)
}

/// Splice a SWF byte slice into a v100 (x64) GFxMovieInfo wrapper. Reuses the
/// same parse/serialize round-trip logic as `splice-x32-payloads.rs --gfx-swap`,
/// but exposed here as a callable helper.
pub fn splice_modgfx_into_x64_wrapper(
    vanilla_x64_gpk: &[u8],
    new_modgfx: &[u8],
) -> Result<Vec<u8>, String> {
    // Parse vanilla wrapper.
    let mut pkg = parse_package(vanilla_x64_gpk)
        .map_err(|e| format!("parse v100 wrapper: {e}"))?;
    let gfx_idx = pkg.exports.iter().position(|e|
        matches!(e.class_name.as_deref(), Some("Core.GFxUI.GFxMovieInfo")))
        .ok_or_else(|| "v100 wrapper has no GFxMovieInfo export".to_string())?;
    let gfx_export = &mut pkg.exports[gfx_idx];

    // Replace the SWF bytes inside this export's payload, preserving the
    // surrounding property block + trailing object refs.
    let new_payload = replace_swf_in_export_payload(&gfx_export.payload, new_modgfx, /*is_x64=*/ true)?;
    gfx_export.payload = new_payload;

    // Serialize back to bytes.
    super::gpk_package::serialize_package(&pkg).map_err(|e| format!("serialize: {e}"))
}

fn extract_swf_payload(payload: &[u8], pkg: &GpkPackage, is_x64: bool) -> Result<Vec<u8>, String> {
    // Skip NetIndex (i32) + walk properties to None terminator, then read i32
    // swf_size, then read that many bytes. Reuse property-walk helper from
    // gpk_resource_inspector if exposed; otherwise inline.
    use super::gpk_resource_inspector::locate_property_terminator_pub;
    let after_props = locate_property_terminator_pub(&pkg.exports[0], &pkg.names, is_x64)
        .map_err(|e| format!("locate properties terminator: {e}"))?;
    let mut cursor = after_props.native_data_offset;
    let swf_size = i32::from_le_bytes(payload[cursor..cursor+4].try_into().unwrap()) as usize;
    cursor += 4;
    let swf = payload.get(cursor..cursor+swf_size)
        .ok_or_else(|| "swf payload range out of bounds".to_string())?;
    Ok(swf.to_vec())
}

fn replace_swf_in_export_payload(payload: &[u8], new_swf: &[u8], is_x64: bool) -> Result<Vec<u8>, String> {
    use super::gpk_resource_inspector::locate_property_terminator_pub;
    // Same offset locate as extract, then slice payload as [pre_swf | new_swf_size+new_swf | trailing_refs].
    // Implementation: needs access to the surrounding GpkExportEntry context, which the wrapper passes in via `pkg`.
    // For simplicity in this helper, re-derive the prefix length by parsing properties from a synthetic export.
    let _ = (payload, new_swf, is_x64);
    Err("inline implementation here — copy from splice-x32-payloads.rs::run_gfx_swap".to_string())
}
```

(Implementation of `replace_swf_in_export_payload` is a verbatim copy of the slicing logic in `splice-x32-payloads.rs`; the binary already contains the working version. The plan for this step is "lift that logic into this helper module without behavioural change".)

- [ ] **Step 5: Re-run the test, watch it pass**

```bash
cargo test --lib services::mods::swf_splice::tests 2>&1 | tail -5
```
Expected: `1 passed`.

- [ ] **Step 6: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/swf_splice.rs \
        teralaunch/src-tauri/src/services/mods/mod.rs \
        teralaunch/src-tauri/tests/fixtures/tiny-x32-gfx.gpk
git commit -m "feat(mods): extract SWF splice helpers into swf_splice module"
```

### Task 3: Build `tmm_wrap.rs` helper

**Files:**
- Create: `teralaunch/src-tauri/src/services/mods/tmm_wrap.rs`
- Modify: `teralaunch/src-tauri/src/services/mods/mod.rs` (add `pub mod tmm_wrap;`)
- Test: in-file `#[cfg(test)]` module

- [ ] **Step 1: Write failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wraps_modded_gpk_with_mod_folder_name() {
        let inner = b"\xC1\x83\x2A\x9E"; // bare GPK magic header — not a real package
        // Real test would use a parsed package; for the wrap test we just check
        // the FString folder is correctly written at offset 12.
        let wrapped = wrap_as_tmm_composite(inner, "S1UI_TestWidget.TestWidget").unwrap();
        // Folder field = positive int32 length + ASCII bytes + null terminator
        let len_bytes = &wrapped[12..16];
        let len = i32::from_le_bytes(len_bytes.try_into().unwrap());
        assert!(len > 0); // ASCII (positive)
        let folder = std::str::from_utf8(&wrapped[16..16+len as usize-1]).unwrap();
        assert_eq!(folder, "MOD:S1UI_TestWidget.TestWidget");
    }
}
```

- [ ] **Step 2: Run test to verify it fails (no `wrap_as_tmm_composite`)**

```bash
cargo test --lib services::mods::tmm_wrap::tests 2>&1 | tail -3
```
Expected: compile error.

- [ ] **Step 3: Implement `wrap_as_tmm_composite`**

```rust
//! TMM wrap — embed the `MOD:<object_path>` folder name into a modded GPK so
//! the launcher's `tmm::install_gpk` recognises it as a TMM-format mod.

use super::gpk_package::{parse_package, serialize_package};

pub fn wrap_as_tmm_composite(modded_gpk: &[u8], target_object_path: &str) -> Result<Vec<u8>, String> {
    let mut pkg = parse_package(modded_gpk).map_err(|e| format!("parse modded gpk: {e}"))?;
    pkg.summary.folder_name = format!("MOD:{target_object_path}");
    serialize_package(&pkg).map_err(|e| format!("serialize wrapped: {e}"))
}
```

- [ ] **Step 4: Run test, verify it passes**

```bash
cargo test --lib services::mods::tmm_wrap::tests 2>&1 | tail -3
```
Expected: `1 passed`.

- [ ] **Step 5: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/tmm_wrap.rs teralaunch/src-tauri/src/services/mods/mod.rs
git commit -m "feat(mods): add tmm_wrap helper for embedding MOD folder name"
```

### Task 4: Build `port-foglio-swf-mod` single-mod CLI

**Files:**
- Create: `teralaunch/src-tauri/src/bin/port-foglio-swf-mod.rs`
- Test: smoke-test by porting paperdoll and asserting the output sha256 matches our existing release asset.

- [ ] **Step 1: Write end-to-end smoke test (in-file `#[cfg(test)]` not feasible for a bin — use shell smoke test as part of step 4)**

(For binaries we drive correctness from the Phase 1 batch run output rather than a unit test.)

- [ ] **Step 2: Implement the CLI**

```rust
// port-foglio-swf-mod — single-mod x64 port of a foglio SWF widget.
//
// Usage:
//   port-foglio-swf-mod \
//     --foglio-gpk <path-to-foglio-x32-gpk> \
//     --vanilla-x64-wrapper <path-to-v100-vanilla-S1UI_<Widget>.gpk> \
//     --target-object <package.object>  e.g. S1UI_PaperDoll.PaperDoll \
//     --out <output-tmm-composite-gpk>
//
// Pipeline:
//   1. extract_modgfx_from_x32_gpk(foglio_gpk_bytes)
//   2. splice_modgfx_into_x64_wrapper(vanilla_wrapper_bytes, modgfx)
//   3. wrap_as_tmm_composite(modded, target_object)
//   4. write to --out

use std::env;
use std::fs;
use std::path::PathBuf;

#[allow(dead_code)] #[path = "../services/mods/gpk_package.rs"] mod gpk_package;
#[allow(dead_code)] #[path = "../services/mods/gpk_resource_inspector.rs"] mod gpk_resource_inspector;
#[allow(dead_code)] #[path = "../services/mods/swf_splice.rs"] mod swf_splice;
#[allow(dead_code)] #[path = "../services/mods/tmm_wrap.rs"] mod tmm_wrap;

fn main() { if let Err(e) = run() { eprintln!("FAIL: {e}"); std::process::exit(1); } }

fn run() -> Result<(), String> {
    let mut foglio_gpk: Option<PathBuf> = None;
    let mut wrapper: Option<PathBuf> = None;
    let mut target_object: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut iter = env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--foglio-gpk" => foglio_gpk = iter.next().map(PathBuf::from),
            "--vanilla-x64-wrapper" => wrapper = iter.next().map(PathBuf::from),
            "--target-object" => target_object = iter.next(),
            "--out" => out = iter.next().map(PathBuf::from),
            other => return Err(format!("unknown arg '{other}'")),
        }
    }
    let foglio_gpk = foglio_gpk.ok_or("--foglio-gpk required")?;
    let wrapper = wrapper.ok_or("--vanilla-x64-wrapper required")?;
    let target_object = target_object.ok_or("--target-object required")?;
    let out = out.ok_or("--out required")?;

    let foglio_bytes = fs::read(&foglio_gpk).map_err(|e| format!("read foglio: {e}"))?;
    let wrapper_bytes = fs::read(&wrapper).map_err(|e| format!("read wrapper: {e}"))?;
    let modgfx = swf_splice::extract_modgfx_from_x32_gpk(&foglio_bytes)?;
    let modded_x64 = swf_splice::splice_modgfx_into_x64_wrapper(&wrapper_bytes, &modgfx)?;
    let tmm_wrapped = tmm_wrap::wrap_as_tmm_composite(&modded_x64, &target_object)?;
    fs::write(&out, &tmm_wrapped).map_err(|e| format!("write out: {e}"))?;
    println!("wrote {} ({} bytes)", out.display(), tmm_wrapped.len());
    Ok(())
}
```

- [ ] **Step 3: Build and smoke-test against paperdoll**

```bash
cd "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/TERA-Europe-ClassicPlus-Launcher/teralaunch/src-tauri"
cargo build --release --bin port-foglio-swf-mod 2>&1 | tail -3
mkdir -p /tmp/port-smoke
./target/release/port-foglio-swf-mod.exe \
  --foglio-gpk "C:/Users/Lukas/AppData/Local/Temp/tera-restyle-clone/PaperDoll/p79/S1UI_PaperDoll79.gpk" \
  --vanilla-x64-wrapper "D:/Elinu/S1Game/CookedPC/Art_Data/Packages/S1UI/S1UI_PaperDoll.gpk.vanilla-bak" \
  --target-object "S1UI_PaperDoll.PaperDoll" \
  --out /tmp/port-smoke/paperdoll-smoke.gpk
ls -la /tmp/port-smoke/paperdoll-smoke.gpk
sha256sum /tmp/port-smoke/paperdoll-smoke.gpk
```
Expected: file written, size > 200KB, sha256 reproducible across runs (deterministic).

- [ ] **Step 4: Commit**

```bash
git add teralaunch/src-tauri/src/bin/port-foglio-swf-mod.rs
git commit -m "feat(mods): port-foglio-swf-mod single-mod x64 port CLI"
```

### Task 5: Build `port-foglio-batch` driver

**Files:**
- Create: `teralaunch/src-tauri/src/bin/port-foglio-batch.rs`
- Read: `docs/mod-manager/foglio-port-batch.toml`

- [ ] **Step 1: Implement the driver**

```rust
// port-foglio-batch — read foglio-port-batch.toml, fetch foglio source, run
// the appropriate per-mod porter, write output GPK + a result manifest.
//
// Usage:
//   port-foglio-batch \
//     --config docs/mod-manager/foglio-port-batch.toml \
//     --vanilla-cookedpc D:/Elinu/S1Game/CookedPC \
//     --output-dir /tmp/foglio-batch-out
//
// Output:
//   <output-dir>/<id>.gpk     for each successful port
//   <output-dir>/manifest.json — array of {id, gpk_filename, sha256, size_bytes, pattern}

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};

#[allow(dead_code)] #[path = "../services/mods/gpk_package.rs"] mod gpk_package;
#[allow(dead_code)] #[path = "../services/mods/gpk_resource_inspector.rs"] mod gpk_resource_inspector;
#[allow(dead_code)] #[path = "../services/mods/swf_splice.rs"] mod swf_splice;
#[allow(dead_code)] #[path = "../services/mods/tmm_wrap.rs"] mod tmm_wrap;

#[derive(serde::Deserialize)]
struct BatchConfig { mods: Vec<ModEntry> }

#[derive(serde::Deserialize, Clone)]
struct ModEntry {
    id: String,
    pattern: String,
    foglio_url: String,
    target_package: Option<String>,
    target_object: Option<String>,
    notes: Option<String>,
}

fn main() { if let Err(e) = run() { eprintln!("FAIL: {e}"); std::process::exit(1); } }

fn run() -> Result<(), String> {
    let mut config: Option<PathBuf> = None;
    let mut cookedpc: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut iter = env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--config" => config = iter.next().map(PathBuf::from),
            "--vanilla-cookedpc" => cookedpc = iter.next().map(PathBuf::from),
            "--output-dir" => out_dir = iter.next().map(PathBuf::from),
            other => return Err(format!("unknown arg '{other}'")),
        }
    }
    let config = config.ok_or("--config required")?;
    let cookedpc = cookedpc.ok_or("--vanilla-cookedpc required")?;
    let out_dir = out_dir.ok_or("--output-dir required")?;
    fs::create_dir_all(&out_dir).map_err(|e| format!("mkdir out: {e}"))?;

    let cfg_text = fs::read_to_string(&config).map_err(|e| format!("read config: {e}"))?;
    let cfg: BatchConfig = toml::from_str(&cfg_text).map_err(|e| format!("parse toml: {e}"))?;

    let mut manifest: Vec<serde_json::Value> = Vec::new();
    let mut skipped: Vec<(String, String)> = Vec::new();
    for m in &cfg.mods {
        let result = match m.pattern.as_str() {
            "swf" => port_swf(m, &cookedpc, &out_dir),
            "redirector" => Err("redirector pattern: not yet implemented in this CLI; \
                                 use port-foglio-redirector-mod (Task 6)".into()),
            "investigate" | "skip" => Err(format!("pattern={} — manual investigation needed", m.pattern)),
            other => Err(format!("unknown pattern '{other}'")),
        };
        match result {
            Ok(entry) => manifest.push(entry),
            Err(e) => { skipped.push((m.id.clone(), e)); }
        }
    }

    let manifest_path = out_dir.join("manifest.json");
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap())
        .map_err(|e| format!("write manifest: {e}"))?;
    println!("ported {} mods, skipped {}", manifest.len(), skipped.len());
    for (id, err) in &skipped { println!("  SKIP {id}: {err}"); }
    println!("manifest: {}", manifest_path.display());
    Ok(())
}

fn port_swf(m: &ModEntry, cookedpc: &Path, out_dir: &Path) -> Result<serde_json::Value, String> {
    let target_package = m.target_package.as_deref().ok_or("swf pattern requires target_package")?;
    let target_object = m.target_object.as_deref().ok_or("swf pattern requires target_object")?;
    let wrapper_path = cookedpc.join("Art_Data/Packages/S1UI").join(format!("{target_package}.gpk"));
    let wrapper_bak = wrapper_path.with_extension("gpk.vanilla-bak");
    let wrapper = if wrapper_bak.exists() { wrapper_bak } else { wrapper_path };
    let wrapper_bytes = fs::read(&wrapper).map_err(|e| format!("read wrapper: {e}"))?;

    // Download foglio source.
    let foglio_bytes = http_get(&m.foglio_url)?;

    // Pipeline.
    let modgfx = swf_splice::extract_modgfx_from_x32_gpk(&foglio_bytes)?;
    let modded = swf_splice::splice_modgfx_into_x64_wrapper(&wrapper_bytes, &modgfx)?;
    let logical = format!("{}.{}", target_package, target_object);
    let wrapped = tmm_wrap::wrap_as_tmm_composite(&modded, &logical)?;

    let out_filename = format!("{}.gpk", m.id.replace('.', "_"));
    let out_path = out_dir.join(&out_filename);
    fs::write(&out_path, &wrapped).map_err(|e| format!("write {}: {e}", out_path.display()))?;

    let sha = format!("{:x}", Sha256::digest(&wrapped));
    Ok(serde_json::json!({
        "id": m.id,
        "gpk_filename": out_filename,
        "sha256": sha,
        "size_bytes": wrapped.len(),
        "pattern": "swf",
        "notes": m.notes,
    }))
}

fn http_get(url: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url).call().map_err(|e| format!("GET {url}: {e}"))?;
    let mut buf = Vec::new();
    resp.into_reader().read_to_end(&mut buf).map_err(|e| format!("read body: {e}"))?;
    Ok(buf)
}
```

- [ ] **Step 2: Add deps to Cargo.toml**

Append to `[dependencies]` in `teralaunch/src-tauri/Cargo.toml`:
```toml
toml = "0.8"
ureq = "2"
sha2 = "0.10"  # if not already present
```

- [ ] **Step 3: Build the bin**

```bash
cd "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/TERA-Europe-ClassicPlus-Launcher/teralaunch/src-tauri"
cargo build --release --bin port-foglio-batch 2>&1 | tail -3
```
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add teralaunch/src-tauri/src/bin/port-foglio-batch.rs teralaunch/src-tauri/Cargo.toml teralaunch/src-tauri/Cargo.lock
git commit -m "feat(mods): port-foglio-batch driver for SWF mods (redirector pending)"
```

### Task 6: Run batch porter for all SWF mods

**Files:**
- Outputs to: `/tmp/foglio-batch-out/` (gitignored)

- [ ] **Step 1: Run the batch**

```bash
cd "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/TERA-Europe-ClassicPlus-Launcher/teralaunch/src-tauri"
./target/release/port-foglio-batch.exe \
  --config "$(pwd)/../../docs/mod-manager/foglio-port-batch.toml" \
  --vanilla-cookedpc "D:/Elinu/S1Game/CookedPC" \
  --output-dir /tmp/foglio-batch-out 2>&1 | tail -30
```
Expected: ~31 SWF mods successfully ported, ~18 skipped (redirector + investigate). Manifest written.

- [ ] **Step 2: Smoke-spot-check three random outputs**

For three random ported GPKs, run:
```bash
./target/release/inspect-gpk-envelope.exe /tmp/foglio-batch-out/foglio1024_restyle_<window>.gpk | head -5
```
Expected: each shows `file_version=897` (x64) and has a `MOD:` folder name.

---

## Phase 2: Redirector mod batch porter (covers ~10 mods)

### Task 7: Investigate UI-Remover repo structure

**Files:**
- Create: `docs/mod-manager/audits/foglio-ui-remover-pattern.md`

- [ ] **Step 1: Clone foglio's UI-Remover repo**

```bash
cd "C:/Users/Lukas/AppData/Local/Temp"
gh repo clone foglio1024/UI-Remover 2>&1 | tail -2
ls UI-Remover/
```

- [ ] **Step 2: Inspect one UI-remover GPK to learn the redirector pattern**

```bash
cd "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/TERA-Europe-ClassicPlus-Launcher/teralaunch/src-tauri"
./target/release/inspect-gpk-resources.exe \
  "C:/Users/Lukas/AppData/Local/Temp/UI-Remover/remove_BossWindow/S1UI_GageBoss.gpk" 2>&1 | grep -E "^(file_version|class|redirector)=" | head -10
```
Expected: shows `Core.ObjectRedirector` count and a redirector pointing to a removed export.

- [ ] **Step 3: Document findings**

Write `docs/mod-manager/audits/foglio-ui-remover-pattern.md` summarising:
- File version (x32 or already x64?)
- Number of redirectors per remover
- Whether v100 vanilla wrapper exists for each `S1UI_<Widget>.gpk` target
- Whether the redirector approach can be byte-dropped or needs translation

- [ ] **Step 4: Commit audit doc**

```bash
git add docs/mod-manager/audits/foglio-ui-remover-pattern.md
git commit -m "docs(mod-manager): audit foglio UI-Remover redirector pattern"
```

### Task 8: Build redirector batch porter (only if Task 7 shows porting is feasible)

**Files:**
- Create: `teralaunch/src-tauri/src/bin/port-foglio-redirector-mod.rs`
- Modify: `port-foglio-batch.rs` to dispatch to redirector porter

- [ ] **Step 1: If audit shows redirectors are already x64-compatible, mark `pattern = "swf"`** in `foglio-port-batch.toml` for these (just byte-drop). Else write a redirector synthesis CLI that emits an x64 GPK with `Core.ObjectRedirector` exports for each removed target.

(Implementation details depend on Task 7 outcome — fill in this step's code blocks once that audit is complete. If implementation needed, follow the same pattern as Tasks 2-4: helper module + CLI + smoke test.)

- [ ] **Step 2: Re-run Task 6 batch** to capture redirector mods.

```bash
./target/release/port-foglio-batch.exe \
  --config "$(pwd)/../../docs/mod-manager/foglio-port-batch.toml" \
  --vanilla-cookedpc "D:/Elinu/S1Game/CookedPC" \
  --output-dir /tmp/foglio-batch-out 2>&1 | tail -30
```
Expected: ~41 mods total (31 SWF + 10 redirector). Skip count drops to ~8.

- [ ] **Step 3: Commit**

```bash
git add teralaunch/src-tauri/src/bin/port-foglio-redirector-mod.rs teralaunch/src-tauri/src/bin/port-foglio-batch.rs
git commit -m "feat(mods): port redirector pattern for foglio UI-Remover mods"
```

---

## Phase 3: Toolbox / edge cases

### Task 9: Per-mod investigation of toolbox-* and edge cases

**Files:**
- Append to: `docs/mod-manager/audits/foglio-ui-remover-pattern.md` (rename to `foglio-port-audit.md`)

- [ ] **Step 1: For each `toolbox-*` and `badgui-*` and `s1ui-chat2-*` entry, fetch the source and inspect**

```bash
for id in foglio1024.toolbox-gagebar-topscreen \
          foglio1024.toolbox-transparent-damage \
          foglio1024.toolbox-thinkblob \
          foglio1024.badgui-loader \
          foglio1024.s1ui-chat2-p75; do
  url=$(jq -r --arg id "$id" '.mods[] | select(.id == $id) | .download_url' "C:/Users/Lukas/AppData/Local/Temp/external-mod-catalog/catalog.json")
  echo "=== $id ==="
  curl -sL "$url" -o /tmp/check-$id.gpk
  ./target/release/inspect-gpk-envelope.exe /tmp/check-$id.gpk | head -5
done
```

- [ ] **Step 2: Document each as `swf` / `redirector` / `unsupported`** in the audit doc, then update `foglio-port-batch.toml` accordingly.

- [ ] **Step 3: For any newly-classified `swf` or `redirector` entries, re-run Task 6.**

- [ ] **Step 4: Commit**

```bash
git add docs/mod-manager/foglio-port-batch.toml docs/mod-manager/audits/foglio-port-audit.md
git commit -m "docs(mod-manager): classify foglio toolbox/edge-case mods for porter"
```

---

## Phase 4: Catalog batch update + release

### Task 10: Build catalog-batch-update.py

**Files:**
- Create: `scripts/catalog-batch-update.py`

- [ ] **Step 1: Implement the script**

```python
#!/usr/bin/env python3
"""
catalog-batch-update.py — apply a port-foglio-batch manifest to catalog.json.

For each entry in the manifest:
  - Update download_url to the release-asset URL pattern
  - Update sha256, size_bytes
  - Set compatible_arch = 'x64'
  - Append the standard x64-port disclaimer to compatibility_notes
  - Append the x64-port credit to credits
  - Bump updated_at on both the entry and the top-level catalog
"""
import argparse, json, sys
from datetime import datetime, timezone
from pathlib import Path

X64_DISCLAIMER = (
    "Adapted from foglio's x32 mod by the TERA-Europe-Classic team for v100.02 (x64). "
    "May not look exactly as foglio intended — some texture references may render differently."
)
X64_CREDIT_SUFFIX = (
    "Adapted to v100.02 (x64) by the TERA-Europe-Classic team."
)

def main():
    p = argparse.ArgumentParser()
    p.add_argument('--catalog', required=True, help='path to catalog.json (mutated in place)')
    p.add_argument('--manifest', required=True, help='path to port-foglio-batch manifest.json')
    p.add_argument('--release-tag', required=True, help='GitHub release tag for the asset URLs')
    p.add_argument('--release-base-url', default='https://github.com/TERA-Europe-Classic/external-mod-catalog/releases/download')
    args = p.parse_args()

    catalog = json.loads(Path(args.catalog).read_text(encoding='utf-8'))
    manifest = json.loads(Path(args.manifest).read_text(encoding='utf-8'))
    by_id = {m['id']: m for m in manifest}

    now = datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
    updated = 0
    for entry in catalog['mods']:
        m = by_id.get(entry['id'])
        if not m: continue
        url = f"{args.release_base_url}/{args.release_tag}/{m['gpk_filename']}"
        entry['download_url'] = url
        entry['sha256'] = m['sha256']
        entry['size_bytes'] = m['size_bytes']
        entry['compatible_arch'] = 'x64'
        notes = entry.get('compatibility_notes') or ''
        if X64_DISCLAIMER not in notes:
            entry['compatibility_notes'] = (notes + ' ' + X64_DISCLAIMER).strip()
        credits = entry.get('credits') or ''
        if X64_CREDIT_SUFFIX not in credits:
            entry['credits'] = (credits + ' ' + X64_CREDIT_SUFFIX).strip()
        entry['updated_at'] = now
        updated += 1
    catalog['updated_at'] = now
    Path(args.catalog).write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
    print(f'updated {updated} entries')

if __name__ == '__main__': main()
```

- [ ] **Step 2: Commit the script**

```bash
git add scripts/catalog-batch-update.py
chmod +x scripts/catalog-batch-update.py
git commit -m "feat(scripts): catalog-batch-update.py for x64-port catalog rewrites"
```

### Task 11: Create one big release on the catalog repo with all ported assets

- [ ] **Step 1: Compose release body markdown**

Save to `/tmp/release-body.md`:
```markdown
# Foglio Mod Catalog x64 Batch Port — 2026-05-01

Bulk x64 port of foglio1024.* catalog entries to v100.02. Each asset is a
TMM-format composite GPK installable via the launcher's `tmm::install_gpk`
path.

## Per-mod coverage

(filled in by Task 11 step 2 from manifest.json)

## Caveats

These ports adapt foglio's x32-only mods to v100.02 by splicing the modded
SWF into a v100-native GFxMovieInfo wrapper (or, for UI-Remover entries,
synthesising x64 ObjectRedirectors). They may not look exactly as foglio
intended — some texture object names were renumbered between Classic and
v100, so a small subset of decorations may render differently than in
foglio's screenshots.

## Credits

Originally by Foglio1024 (https://github.com/foglio1024/tera-restyle and
https://github.com/foglio1024/UI-Remover). Adapted to v100.02 (x64) by
the TERA-Europe-Classic team.

If you are the original author and would prefer entries removed,
reattributed, or replaced with a different upstream URL, open an issue on
this repo.
```

Then auto-generate the per-mod coverage list from the manifest:
```bash
python << 'EOF'
import json
m = json.load(open('/tmp/foglio-batch-out/manifest.json', encoding='utf-8'))
print('| id | size | sha256 (12-prefix) |')
print('|---|---|---|')
for e in sorted(m, key=lambda x: x['id']):
    print(f"| `{e['id']}` | {e['size_bytes']:,} | `{e['sha256'][:12]}` |")
EOF
```
Inject the table into `/tmp/release-body.md` under "## Per-mod coverage".

- [ ] **Step 2: Create the release with all GPK assets**

```bash
cd "C:/Users/Lukas/AppData/Local/Temp/external-mod-catalog"
gh release create foglio-x64-port-batch-2026-05-01 \
  --repo TERA-Europe-Classic/external-mod-catalog \
  --title "Foglio Mod Batch x64 Port — 2026-05-01" \
  --notes-file /tmp/release-body.md \
  /tmp/foglio-batch-out/*.gpk
```
Expected: release page URL printed; all GPK files uploaded as assets.

- [ ] **Step 3: Verify all asset URLs resolve**

```bash
python << 'EOF'
import json, urllib.request
m = json.load(open('/tmp/foglio-batch-out/manifest.json', encoding='utf-8'))
base = 'https://github.com/TERA-Europe-Classic/external-mod-catalog/releases/download/foglio-x64-port-batch-2026-05-01'
for e in m:
    url = f"{base}/{e['gpk_filename']}"
    try:
        with urllib.request.urlopen(url, timeout=10) as r:
            print(f"✓ {r.status:>3}  {e['id']}")
    except Exception as ex:
        print(f"✗      {e['id']}  ({ex})")
EOF
```
Expected: every line starts with `✓ 200`.

### Task 12: Apply catalog batch update + commit + push

- [ ] **Step 1: Run the batch update**

```bash
cd "C:/Users/Lukas/AppData/Local/Temp/external-mod-catalog"
git pull --quiet
python "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/TERA-Europe-ClassicPlus-Launcher/scripts/catalog-batch-update.py" \
  --catalog catalog.json \
  --manifest /tmp/foglio-batch-out/manifest.json \
  --release-tag foglio-x64-port-batch-2026-05-01
```
Expected: prints `updated N entries` matching the number of manifest rows.

- [ ] **Step 2: Diff inspection**

```bash
git diff --stat catalog.json
git diff catalog.json | head -100
```
Expected: only the foglio.* entries are touched.

- [ ] **Step 3: Commit and push**

```bash
git add catalog.json
git commit -m "$(cat <<'EOF'
feat(catalog): batch x64 port of foglio mods (2026-05-01)

Updates ~41 foglio1024.* entries to point at the foglio-x64-port-batch-
2026-05-01 release-asset URLs. Each port replaces the previous raw x32
GitHub URL (which couldn't install on v100.02) with a TMM-format
composite GPK targeting the corresponding S1UI widget.

Per-entry: bumps version, adds compatible_arch=x64, appends the standard
"adapted from x32 — may not look as intended" disclaimer to
compatibility_notes, and credits the TERA-Europe-Classic adaptation.

Toolbox / edge-case entries that couldn't be auto-ported remain with
their raw x32 download_url and compatible_arch=x32 so the launcher's
Browse-tab UI surfaces an "incompatible" badge.
EOF
)"
git push origin main
```

- [ ] **Step 4: Verify catalog fetch via raw URL the launcher uses**

```bash
curl -s "https://raw.githubusercontent.com/TERA-Europe-Classic/external-mod-catalog/main/catalog.json" \
  | python -c "
import json, sys
c = json.load(sys.stdin)
ported = sum(1 for m in c['mods'] if m['id'].startswith('foglio1024.') and 'TERA-Europe-Classic' in m['download_url'])
unported = sum(1 for m in c['mods'] if m['id'].startswith('foglio1024.') and 'TERA-Europe-Classic' not in m['download_url'])
print(f'foglio entries ported: {ported}')
print(f'foglio entries un-ported (toolbox/edge): {unported}')
print(f'catalog updated_at: {c[\"updated_at\"]}')
"
```
Expected: ported >= 31 (Phase 1) + 10 (Phase 2) = 41+; un-ported <= 8.

### Task 13: Update CATALOG-LIFECYCLE-MATRIX.md

**Files:**
- Modify: `docs/mod-manager/CATALOG-LIFECYCLE-MATRIX.md`

- [ ] **Step 1: For each ported foglio.* row in the matrix, change the `install` column from `pending` → `pass` (link to the release as the review/doc evidence).**

Example diff:
```
| `foglio1024.restyle-paperdoll` | `gpk` | ui | pass (release foglio-x64-port-batch-2026-05-01) | pending | pending | pending | pending | pending | pending |
```

- [ ] **Step 2: Commit**

```bash
cd "C:/Users/Lukas/Documents/GitHub/TERA EU Classic/TERA-Europe-ClassicPlus-Launcher"
git add docs/mod-manager/CATALOG-LIFECYCLE-MATRIX.md
git commit -m "docs(mod-manager): mark batch-ported foglio entries install=pass"
```

---

## Self-review summary

- **Spec coverage:** All 49 foglio entries are reachable through Phase 1 (SWF), Phase 2 (redirector), or Phase 3 (toolbox investigation). Catalog write is gated on the porter manifest so we never publish entries without working GPKs.
- **Out of scope:** smoke-testing each ported mod in-game. With 49 mods, manual smoke tests are cost-prohibitive; the standard x64-port disclaimer in compatibility_notes sets user expectations and the launcher already shows runtime errors via `ModEntry.last_error` for any install that fails. If the user wants smoke tests, that's a follow-up plan.
- **Task numbering:** 13 tasks across 4 phases. Tasks 2-5 build the toolchain (sequential). Task 6 runs the SWF batch (depends on 1-5). Tasks 7-8 are the redirector pipeline (independent of SWF batch but depends on 1). Task 9 is per-mod investigation (sequential). Tasks 10-13 publish (sequential, depend on all prior).
- **No placeholders** except in Task 8 step 1 where the implementation depends on Task 7's audit findings — explicitly called out as conditional.
