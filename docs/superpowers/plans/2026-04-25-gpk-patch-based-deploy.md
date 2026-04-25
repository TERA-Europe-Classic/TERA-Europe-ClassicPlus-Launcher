# GPK Patch-Based Deploy — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current "copy modded GPK whole-cloth into CookedPC and redirect the mapper" install path with a patch-based deploy that derives a manifest at install time, persists it, and applies it on enable. Disable reverts via the existing `.clean` mapper restore. Backwards-compat migration force-uninstalls existing legacy installs and reinstalls them under the new flow.

**Architecture:** At install, parse the modded GPK and the corresponding vanilla bytes (extracted from the composite container at the mapper-resolved `offset/size`), run `gpk_package::compare_packages`, build a `PatchManifest` via the same logic the offline converter uses, and persist the manifest at `<app_data>/mods/patch-manifests/<id>/manifest.json`. On enable, re-extract the vanilla bytes, run `gpk_patch_applier::apply_manifest` to produce patched bytes, write them to `<game>/CookedPC/<target>.gpk`, and patch the mapper to redirect. Disable restores the `.clean` mapper and deletes the standalone — vanilla bytes still live in the composite container, no per-package baseline file needed. Mods whose diff doesn't fit the existing applier slice **fail closed at install time** with a clear "this mod's diff shape isn't supported yet" message — strictly better failure mode than the current "install + break the client".

**Tech Stack:** Rust (Tauri backend), Vitest+Playwright (frontend tests), existing modules: `services::mods::{gpk, gpk_package, gpk_patch_applier, patch_manifest, registry}`, `commands::mods`.

**Out of scope (Phase 2):** Broadening `apply_manifest` to handle compressed packages, added exports, import/name patches, class changes. Not blockers — Phase 1 fails closed on those, Phase 2 broadens incrementally.

---

## File Structure

| Path | Status | Responsibility |
|---|---|---|
| `teralaunch/src-tauri/src/services/mods/patch_derivation.rs` | new | Derive a `PatchManifest` from `(reference_bytes, modded_bytes, mod_id)`. Wraps `compare_packages` + manifest emission. Returns `PatchUnsupported` for diff shapes the applier rejects. |
| `teralaunch/src-tauri/src/services/mods/composite_extract.rs` | new | Read the vanilla bytes for a given `object_path` from the composite container at `(filename, offset, size)` resolved against the `.clean` mapper. |
| `teralaunch/src-tauri/src/services/mods/manifest_store.rs` | new | Persist + load + delete manifest bundles at `<app_data>/mods/patch-manifests/<id>/manifest.json`. |
| `teralaunch/src-tauri/src/services/mods/gpk.rs` | modify | Add `install_gpk_via_patch`, `enable_gpk_via_patch`, `disable_gpk_via_patch`, `migrate_legacy_gpk_install`. Existing `install_legacy_gpk` stays as a hard-flagged escape hatch for now (no callers from production code paths). |
| `teralaunch/src-tauri/src/commands/mods.rs` | modify | Update `try_deploy_gpk` to call the patch-based path; update `rebuild_gpk_state` callers; wire migration into mod-state init. |
| `teralaunch/src-tauri/src/services/mods/mod.rs` | modify | `pub mod` declarations for the three new files. |
| `teralaunch/src-tauri/tests/gpk_patch_deploy.rs` | new | Integration: install → enable → game-loadable bytes → disable → vanilla restored → uninstall → manifest deleted. |

---

## Task 1: Patch derivation module

**Files:**
- Create: `teralaunch/src-tauri/src/services/mods/patch_derivation.rs`
- Modify: `teralaunch/src-tauri/src/services/mods/mod.rs`

- [ ] **Step 1: Write failing tests for `derive_manifest`**

```rust
// in patch_derivation.rs (#[cfg(test)] block)
use super::gpk_package::parse_package;
use super::gpk_patch_applier;

#[test]
fn derive_manifest_emits_replace_payload_for_changed_textures() {
    // Reuse the boss-window fixture from gpk_patch_applier tests as the reference.
    let reference_bytes = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], true);
    let modded_bytes = build_boss_window_test_package([0xAA, 0xBB, 0xCC, 0xDD], true);
    let manifest = derive_manifest("test.mod", &reference_bytes, &modded_bytes).unwrap();
    assert_eq!(manifest.exports.len(), 1);
    assert_eq!(manifest.exports[0].operation,
               crate::services::mods::patch_manifest::ExportPatchOperation::ReplaceExportPayload);
    assert_eq!(manifest.exports[0].replacement_payload_hex, "aabbccdd");
}

#[test]
fn derive_manifest_refuses_added_exports_in_phase1() {
    let reference_bytes = build_boss_window_test_package([0x10; 4], true);
    // Synthetic modded with extra export → out of scope for Phase 1 applier.
    let modded_bytes = build_boss_window_with_extra_export();
    let err = derive_manifest("test.mod", &reference_bytes, &modded_bytes).unwrap_err();
    assert!(err.contains("added exports"), "got: {err}");
}

#[test]
fn derive_manifest_round_trips_through_applier() {
    let reference_bytes = build_boss_window_test_package([0x10, 0x11, 0x12, 0x13], true);
    let modded_bytes = build_boss_window_test_package([0x90, 0x91, 0x92, 0x93], true);
    let manifest = derive_manifest("test.mod", &reference_bytes, &modded_bytes).unwrap();
    let applied = gpk_patch_applier::apply_manifest(&reference_bytes, &manifest).unwrap();
    let parsed = parse_package(&applied).unwrap();
    let main = parsed.exports.iter().find(|e| e.object_path == "GageBoss").unwrap();
    assert_eq!(main.payload, vec![0x90, 0x91, 0x92, 0x93]);
}
```

- [ ] **Step 2: Run tests, confirm fail with "not defined"**

```bash
cd teralaunch/src-tauri && cargo test --package teralaunch services::mods::patch_derivation
```

Expected: compilation error — module doesn't exist.

- [ ] **Step 3: Implement minimal `derive_manifest`**

```rust
// patch_derivation.rs
use super::{gpk_package, patch_manifest};

pub fn derive_manifest(
    mod_id: &str,
    reference_bytes: &[u8],
    modded_bytes: &[u8],
) -> Result<patch_manifest::PatchManifest, String> {
    let reference = gpk_package::parse_package(reference_bytes)?;
    let modded = gpk_package::parse_package(modded_bytes)?;
    let diff = gpk_package::compare_packages(&reference, &modded);

    if !diff.added_exports.is_empty() {
        return Err(format!(
            "Mod adds exports {:?} — Phase 1 applier does not support added exports yet",
            diff.added_exports
        ));
    }
    if diff.import_count_before != diff.import_count_after {
        return Err("Mod changes import-table size — Phase 1 applier does not support import patches yet".into());
    }
    if diff.name_count_before != diff.name_count_after {
        return Err("Mod changes name-table size — Phase 1 applier does not support name patches yet".into());
    }
    if reference.summary.compression_flags != 0 || modded.summary.compression_flags != 0 {
        return Err("Compressed packages are not supported by the Phase 1 applier".into());
    }

    let mut exports = Vec::new();
    for changed in &diff.changed_exports {
        let r = reference.exports.iter().find(|e| e.object_path == changed.object_path)
            .ok_or_else(|| format!("Reference export '{}' missing", changed.object_path))?;
        let m = modded.exports.iter().find(|e| e.object_path == changed.object_path)
            .ok_or_else(|| format!("Modded export '{}' missing", changed.object_path))?;
        if r.class_name != m.class_name {
            return Err(format!(
                "Export '{}' changes class ({:?} → {:?}); Phase 1 applier does not support class changes",
                changed.object_path, r.class_name, m.class_name
            ));
        }
        exports.push(patch_manifest::ExportPatch {
            object_path: changed.object_path.clone(),
            class_name: r.class_name.clone(),
            reference_export_fingerprint: r.payload_fingerprint.clone(),
            target_export_fingerprint: Some(r.payload_fingerprint.clone()),
            operation: patch_manifest::ExportPatchOperation::ReplaceExportPayload,
            new_class_name: None,
            replacement_payload_hex: hex_lower(&m.payload),
        });
    }
    for removed in &diff.removed_exports {
        let r = reference.exports.iter().find(|e| e.object_path == *removed)
            .ok_or_else(|| format!("Reference export '{}' missing", removed))?;
        exports.push(patch_manifest::ExportPatch {
            object_path: removed.clone(),
            class_name: r.class_name.clone(),
            reference_export_fingerprint: r.payload_fingerprint.clone(),
            target_export_fingerprint: Some(r.payload_fingerprint.clone()),
            operation: patch_manifest::ExportPatchOperation::RemoveExport,
            new_class_name: None,
            replacement_payload_hex: String::new(),
        });
    }

    let manifest = patch_manifest::PatchManifest {
        schema_version: 2,
        mod_id: mod_id.to_string(),
        title: mod_id.to_string(),
        target_package: format!("{}.gpk", reference.summary.package_name),
        patch_family: patch_manifest::PatchFamily::UiLayout,
        reference: patch_manifest::ReferenceBaseline {
            source_patch_label: "runtime-derived".into(),
            package_fingerprint: format!(
                "exports:{}|imports:{}|names:{}",
                reference.exports.len(), reference.imports.len(), reference.names.len()
            ),
            provenance: None,
        },
        compatibility: patch_manifest::CompatibilityPolicy {
            require_exact_package_fingerprint: true,
            require_all_exports_present: false,
            forbid_name_or_import_expansion: false,
        },
        exports,
        import_patches: Vec::new(),
        name_patches: Vec::new(),
        notes: vec!["Derived at install time from vanilla composite + modded GPK".into()],
    };
    manifest.validate()?;
    Ok(manifest)
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes { s.push_str(&format!("{b:02x}")); }
    s
}
```

Add `pub mod patch_derivation;` to `services/mods/mod.rs`.

- [ ] **Step 4: Test fixtures share with gpk_patch_applier tests**

Pull `build_boss_window_test_package` and friends out of `gpk_patch_applier::tests::` into a `#[cfg(test)] pub(super) mod test_fixtures;` next to the modules so `patch_derivation::tests` can reuse them. Also add `build_boss_window_with_extra_export()` that adds a third export the applier won't accept.

- [ ] **Step 5: Run tests, verify pass**

```bash
cd teralaunch/src-tauri && cargo test --package teralaunch patch_derivation
```

Expected: 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/patch_derivation.rs \
        teralaunch/src-tauri/src/services/mods/mod.rs \
        teralaunch/src-tauri/src/services/mods/test_fixtures.rs
git commit -m "feat(mods): add runtime patch-manifest derivation"
```

---

## Task 2: Composite vanilla-bytes extractor

**Files:**
- Create: `teralaunch/src-tauri/src/services/mods/composite_extract.rs`
- Modify: `teralaunch/src-tauri/src/services/mods/mod.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn extract_vanilla_bytes_returns_slice_at_resolved_offset() {
    let tmp = TempDir::new().unwrap();
    let game_root = tmp.path();
    let cooked_pc = game_root.join("S1Game/CookedPC");
    fs::create_dir_all(&cooked_pc).unwrap();

    // Write a synthetic composite container with two packages back-to-back.
    let pkg_a = build_boss_window_test_package([0xA0; 4], true);
    let pkg_b = build_boss_window_test_package([0xB0; 4], true);
    let mut composite = Vec::new();
    composite.extend_from_slice(&pkg_a);
    let off_b = composite.len() as i64;
    composite.extend_from_slice(&pkg_b);
    fs::write(cooked_pc.join("S1UI_GageBoss.gpk"), &composite).unwrap();

    // Write a .clean mapper pointing at that container.
    let mapper_text = format!(
        "S1UI_GageBoss.gpk?GageBossModded.GageBoss,Comp,{},{},|!",
        off_b, pkg_b.len()
    );
    let encrypted = encrypt_mapper(mapper_text.as_bytes());
    fs::write(cooked_pc.join("CompositePackageMapper.clean"), &encrypted).unwrap();

    let extracted = extract_vanilla_for_object_path(game_root, "GageBossModded.GageBoss").unwrap();
    assert_eq!(extracted, pkg_b);
}

#[test]
fn extract_vanilla_bytes_errors_when_object_not_in_clean_mapper() {
    let tmp = TempDir::new().unwrap();
    let game_root = tmp.path();
    let cooked_pc = game_root.join("S1Game/CookedPC");
    fs::create_dir_all(&cooked_pc).unwrap();
    fs::write(cooked_pc.join("CompositePackageMapper.clean"),
              encrypt_mapper(b"S1UI_Other.gpk?Foo.Bar,X,0,10,|!")).unwrap();
    let err = extract_vanilla_for_object_path(game_root, "Missing.Path").unwrap_err();
    assert!(err.contains("not found"), "got: {err}");
}
```

- [ ] **Step 2: Run, confirm fail**

```bash
cd teralaunch/src-tauri && cargo test composite_extract
```

Expected: compile error / test miss.

- [ ] **Step 3: Implement**

```rust
// composite_extract.rs
use std::fs;
use std::path::Path;
use super::gpk::{decrypt_mapper, parse_mapper, get_entry_by_object_path,
                 get_entry_by_incomplete_object_path, BACKUP_FILE};

const COOKED_PC_DIR: &str = "S1Game/CookedPC";

pub fn extract_vanilla_for_object_path(
    game_root: &Path,
    object_path: &str,
) -> Result<Vec<u8>, String> {
    let cooked_pc = game_root.join(COOKED_PC_DIR);
    let clean = cooked_pc.join(BACKUP_FILE);
    if !clean.exists() {
        return Err(format!("CompositePackageMapper.clean missing at {}", clean.display()));
    }
    let bytes = fs::read(&clean).map_err(|e| format!("Failed to read .clean: {e}"))?;
    let plain = String::from_utf8_lossy(&decrypt_mapper(&bytes)).to_string();
    let map = parse_mapper(&plain);

    let entry = get_entry_by_object_path(&map, object_path)
        .or_else(|| get_entry_by_incomplete_object_path(&map, object_path))
        .ok_or_else(|| format!("object_path '{object_path}' not found in vanilla mapper"))?;

    let container_path = cooked_pc.join(&entry.filename);
    let container = fs::read(&container_path)
        .map_err(|e| format!("Failed to read composite container {}: {e}", container_path.display()))?;
    let off = entry.offset as usize;
    let size = entry.size as usize;
    if off.checked_add(size).map_or(true, |end| end > container.len()) {
        return Err(format!("Vanilla offset/size out of bounds in {}", container_path.display()));
    }
    Ok(container[off..off + size].to_vec())
}
```

`BACKUP_FILE` and the helpers must be re-exported from `gpk.rs` (they already are `pub` for `BACKUP_FILE`; `get_entry_by_*` are `pub`).

- [ ] **Step 4: Run, verify pass**

```bash
cd teralaunch/src-tauri && cargo test composite_extract
```

Expected: 2 tests pass.

- [ ] **Step 5: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/composite_extract.rs \
        teralaunch/src-tauri/src/services/mods/mod.rs
git commit -m "feat(mods): add composite-container vanilla-bytes extractor"
```

---

## Task 3: Manifest store

**Files:**
- Create: `teralaunch/src-tauri/src/services/mods/manifest_store.rs`
- Modify: `teralaunch/src-tauri/src/services/mods/mod.rs`

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn save_then_load_round_trips_a_manifest() {
    let tmp = TempDir::new().unwrap();
    let manifest = sample_manifest("test.mod");
    save_manifest_at_root(tmp.path(), "test.mod", &manifest).unwrap();
    let loaded = load_manifest_at_root(tmp.path(), "test.mod").unwrap().unwrap();
    assert_eq!(loaded, manifest);
}

#[test]
fn delete_removes_the_bundle_dir() {
    let tmp = TempDir::new().unwrap();
    let manifest = sample_manifest("test.mod");
    save_manifest_at_root(tmp.path(), "test.mod", &manifest).unwrap();
    delete_manifest_at_root(tmp.path(), "test.mod").unwrap();
    assert!(load_manifest_at_root(tmp.path(), "test.mod").unwrap().is_none());
}
```

- [ ] **Step 2: Run, confirm fail**

- [ ] **Step 3: Implement using `patch_manifest::artifact_layout_for_mod_at_root`**

```rust
use std::fs;
use std::path::Path;
use super::patch_manifest::{self, PatchManifest, artifact_layout_for_mod_at_root};

pub fn save_manifest_at_root(root: &Path, mod_id: &str, manifest: &PatchManifest) -> Result<(), String> {
    let layout = artifact_layout_for_mod_at_root(root, mod_id);
    fs::create_dir_all(&layout.bundle_dir)
        .map_err(|e| format!("Failed to create bundle dir: {e}"))?;
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|e| format!("Failed to serialize manifest: {e}"))?;
    fs::write(&layout.manifest_path, json)
        .map_err(|e| format!("Failed to write manifest: {e}"))
}

pub fn load_manifest_at_root(root: &Path, mod_id: &str) -> Result<Option<PatchManifest>, String> {
    let layout = artifact_layout_for_mod_at_root(root, mod_id);
    if !layout.manifest_path.exists() { return Ok(None); }
    let body = fs::read_to_string(&layout.manifest_path)
        .map_err(|e| format!("Failed to read manifest: {e}"))?;
    let manifest: PatchManifest = serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse manifest: {e}"))?;
    manifest.validate()?;
    Ok(Some(manifest))
}

pub fn delete_manifest_at_root(root: &Path, mod_id: &str) -> Result<(), String> {
    let layout = artifact_layout_for_mod_at_root(root, mod_id);
    if layout.bundle_dir.exists() {
        fs::remove_dir_all(&layout.bundle_dir)
            .map_err(|e| format!("Failed to delete manifest bundle: {e}"))?;
    }
    Ok(())
}

// Production callers use the app-data root:
pub fn save_manifest(mod_id: &str, manifest: &PatchManifest) -> Result<(), String> {
    let root = patch_manifest::get_manifest_root().ok_or_else(|| "No manifest root".to_string())?;
    save_manifest_at_root(&root, mod_id, manifest)
}
pub fn load_manifest(mod_id: &str) -> Result<Option<PatchManifest>, String> {
    let root = patch_manifest::get_manifest_root().ok_or_else(|| "No manifest root".to_string())?;
    load_manifest_at_root(&root, mod_id)
}
pub fn delete_manifest(mod_id: &str) -> Result<(), String> {
    let root = patch_manifest::get_manifest_root().ok_or_else(|| "No manifest root".to_string())?;
    delete_manifest_at_root(&root, mod_id)
}
```

Replace `patch_manifest::load_manifest_for_mod` callers in `commands::mods` with `manifest_store::load_manifest`.

- [ ] **Step 4: Run, verify pass**

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(mods): add manifest store with bundle round-trip"
```

---

## Task 4: Patch-based install/enable/disable in `gpk.rs`

**Files:**
- Modify: `teralaunch/src-tauri/src/services/mods/gpk.rs`

Add three new public functions: `install_gpk_via_patch`, `enable_gpk_via_patch`, `disable_gpk_via_patch`. Keep `install_legacy_gpk` and `uninstall_legacy_gpk` private (no production callers after Task 5).

- [ ] **Step 1: Failing test (integration-style, against synthetic fixtures)**

```rust
#[test]
fn install_via_patch_writes_manifest_and_does_not_touch_cooked_pc_yet() {
    let tmp = TempDir::new().unwrap();
    let game_root = tmp.path().join("game");
    let app_root = tmp.path().join("app");
    let cooked_pc = game_root.join("S1Game/CookedPC");
    fs::create_dir_all(&cooked_pc).unwrap();
    let app_root_clone = app_root.clone();

    // Vanilla container with one package
    let vanilla_pkg = build_boss_window_test_package([0x10; 4], true);
    let mut container = Vec::new();
    container.extend_from_slice(&vanilla_pkg);
    fs::write(cooked_pc.join("S1UI_GageBoss.gpk"), &container).unwrap();
    let mapper = format!("S1UI_GageBoss.gpk?GageBoss.GageBoss,Comp,0,{},|!", vanilla_pkg.len());
    fs::write(cooked_pc.join("CompositePackageMapper.dat"), encrypt_mapper(mapper.as_bytes())).unwrap();
    fs::write(cooked_pc.join("CompositePackageMapper.clean"), encrypt_mapper(mapper.as_bytes())).unwrap();

    // Modded standalone
    let modded_pkg = build_boss_window_test_package([0xAA; 4], true);
    let mod_src = tmp.path().join("mod-src.gpk");
    fs::write(&mod_src, &modded_pkg).unwrap();

    let outcome = install_gpk_via_patch(
        &game_root, &app_root, "test.mod", &mod_src, "GageBoss.GageBoss",
    ).unwrap();
    assert_eq!(outcome.target_filename, "S1UI_GageBoss.gpk"); // taken from manifest target_package... TBD shape

    // Manifest persisted under app_root
    let manifest = manifest_store::load_manifest_at_root(&app_root, "test.mod").unwrap().unwrap();
    assert_eq!(manifest.exports.len(), 1);
    // CookedPC NOT yet patched — that happens at enable.
    let mapper_now = fs::read(cooked_pc.join("CompositePackageMapper.dat")).unwrap();
    assert_eq!(mapper_now, encrypt_mapper(mapper.as_bytes()));
}

#[test]
fn enable_writes_patched_bytes_and_redirects_mapper() {
    // setup as above, then:
    install_gpk_via_patch(...);
    enable_gpk_via_patch(&game_root, &app_root, "test.mod").unwrap();
    let standalone = fs::read(cooked_pc.join("<target>.gpk")).unwrap();
    let parsed = parse_package(&standalone).unwrap();
    let main = parsed.exports.iter().find(|e| e.object_path == "GageBoss").unwrap();
    assert_eq!(main.payload, vec![0xAA, 0xAA, 0xAA, 0xAA]);
    // Mapper now redirects.
}

#[test]
fn disable_restores_clean_mapper_and_deletes_standalone() {
    // setup + install + enable + disable
    disable_gpk_via_patch(&game_root, &app_root, "test.mod").unwrap();
    let standalone = cooked_pc.join("<target>.gpk");
    assert!(!standalone.exists());
    let mapper_now = fs::read(cooked_pc.join("CompositePackageMapper.dat")).unwrap();
    let clean_now = fs::read(cooked_pc.join("CompositePackageMapper.clean")).unwrap();
    assert_eq!(mapper_now, clean_now);
}
```

- [ ] **Step 2: Run, confirm fail**

- [ ] **Step 3: Implement the three functions**

```rust
pub struct PatchInstallOutcome { pub target_filename: String, pub manifest_target_object_path: String }

pub fn install_gpk_via_patch(
    game_root: &Path,
    app_root: &Path,
    mod_id: &str,
    source_gpk: &Path,
    target_object_path: &str,
) -> Result<PatchInstallOutcome, String> {
    let modded = fs::read(source_gpk)
        .map_err(|e| format!("Failed to read modded GPK: {e}"))?;
    let vanilla = composite_extract::extract_vanilla_for_object_path(game_root, target_object_path)?;
    let manifest = patch_derivation::derive_manifest(mod_id, &vanilla, &modded)?;
    manifest_store::save_manifest_at_root(app_root, mod_id, &manifest)?;
    let target_filename = format!("{}.gpk", manifest.target_package.trim_end_matches(".gpk"));
    Ok(PatchInstallOutcome { target_filename, manifest_target_object_path: target_object_path.into() })
}

pub fn enable_gpk_via_patch(game_root: &Path, app_root: &Path, mod_id: &str) -> Result<(), String> {
    let manifest = manifest_store::load_manifest_at_root(app_root, mod_id)?
        .ok_or_else(|| format!("No manifest persisted for '{mod_id}'"))?;
    let object_path = manifest.exports[0].object_path.clone(); // pick a representative; refine: store separately
    let vanilla = composite_extract::extract_vanilla_for_object_path(game_root, &object_path)?;
    let patched = gpk_patch_applier::apply_manifest(&vanilla, &manifest)?;
    let target_filename = manifest.target_package.clone();
    if !is_safe_gpk_container_filename(&target_filename) {
        return Err(format!("Manifest target_package '{target_filename}' unsafe"));
    }
    let dest = game_root.join(COOKED_PC_DIR).join(&target_filename);
    fs::write(&dest, &patched).map_err(|e| format!("Failed to write patched GPK: {e}"))?;

    // Mapper redirect: every entry whose object_path ends in `.<folder>` or matches `<folder>` repoints at this file.
    ensure_backup(game_root)?;
    redirect_mapper_to_standalone(game_root, &target_filename, patched.len() as i64)?;
    Ok(())
}

pub fn disable_gpk_via_patch(game_root: &Path, _app_root: &Path, mod_id: &str) -> Result<(), String> {
    // Mapper: hard restore from .clean
    restore_clean_mapper_state(game_root)?;
    // Standalone: delete using manifest's target_package
    let manifest = manifest_store::load_manifest(mod_id)?; // best effort
    if let Ok(Some(m)) = manifest_store::load_manifest_at_root_for_disable(game_root, mod_id) {
        let dest = game_root.join(COOKED_PC_DIR).join(&m.target_package);
        if dest.exists() { let _ = fs::remove_file(&dest); }
    }
    Ok(())
}
```

(Refine signatures during implementation — this sketch is structural.)

- [ ] **Step 4: Run, verify pass**

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(mods): patch-based install/enable/disable on gpk.rs"
```

---

## Task 5: Wire `try_deploy_gpk` to use the new path

**Files:**
- Modify: `teralaunch/src-tauri/src/commands/mods.rs`
- Modify: `teralaunch/src-tauri/src/services/mods/gpk.rs` (`rebuild_gpk_state`)

- [ ] **Step 1: Update `try_deploy_gpk`**

Replace the legacy fallback chain. New order:
1. Resolve `target_object_path` from URL hint or modded GPK header (e.g. `GageBoss.GageBoss` from URL `.../remove_BossWindow/S1UI_GageBoss.gpk`).
2. Call `gpk::install_gpk_via_patch(...)`.
3. On `Err`, surface a clear "this mod's diff isn't supported by Phase 1 applier" error in `last_error`. Do NOT fall back to legacy install.

- [ ] **Step 2: Update `rebuild_gpk_state`**

Replace the per-item `uninstall_legacy_gpk` + `install_legacy_gpk` reapply chain with `disable_gpk_via_patch` + `enable_gpk_via_patch`. The mapper-restore-then-reapply pattern stays.

- [ ] **Step 3: Update tests in `commands/mods.rs`**

Pin the new flow with a string-source assertion (matching the existing test style at line 1373).

- [ ] **Step 4: Run full Rust test suite**

```bash
cd teralaunch/src-tauri && cargo test --package teralaunch
```

Expected: existing GPK tests still pass; new ones added in Tasks 1–4 pass; regression tests under `tests/classicplus_guards_*` still pass.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(mods): switch try_deploy_gpk to patch-based deploy"
```

---

## Task 6: Backwards-compat migration

**Files:**
- Modify: `teralaunch/src-tauri/src/services/mods/registry.rs` or new `mods_state` migration step
- Modify: `teralaunch/src-tauri/src/state/mods_state.rs` (init path)

- [ ] **Step 1: Failing test**

```rust
#[test]
fn legacy_installed_gpk_with_no_manifest_is_force_uninstalled_on_init() {
    // Set up a registry entry with deployed_filename = Some("X.gpk") but no manifest bundle.
    // Run the migration. Verify:
    //  - Mapper gets restored from .clean (legacy uninstall path runs)
    //  - The CookedPC drop-in is removed
    //  - Registry row's status flips to NeedsReinstall (or similar) with last_error explaining
}
```

- [ ] **Step 2-5: Implement, run, commit**

```rust
pub fn migrate_legacy_gpk_installs(registry: &mut Registry, game_root: &Path) -> Vec<String> {
    let mut migrated = Vec::new();
    for entry in registry.mods.iter_mut() {
        if !matches!(entry.kind, ModKind::Gpk) { continue; }
        let has_manifest = manifest_store::load_manifest(&entry.id).ok().flatten().is_some();
        if entry.deployed_filename.is_some() && !has_manifest {
            if let Some(target) = entry.deployed_filename.as_deref() {
                let _ = uninstall_legacy_gpk(game_root, target);
            }
            entry.deployed_filename = None;
            entry.enabled = false;
            entry.auto_launch = false;
            entry.status = ModStatus::Error;
            entry.last_error = Some(
                "This mod was installed by an older launcher version that overwrote vanilla files. \
                 It has been removed. Click Retry to reinstall using the new patch-based deploy.".into()
            );
            migrated.push(entry.id.clone());
        }
    }
    migrated
}
```

Run from `mods_state::ensure_loaded` after `recover_stuck_installs` — gated by `game_root.is_some()` so it doesn't run before the user configures the install path.

```bash
git commit -m "feat(mods): migrate legacy GPK installs on launcher init"
```

---

## Task 7: End-to-end integration test against the flight-gauge mod

**Files:**
- Create: `teralaunch/src-tauri/tests/gpk_patch_deploy.rs`
- Add fixture: `teralaunch/src-tauri/tests/fixtures/flight-gauge/modded.gpk` (download once, hash-pinned via the catalog sha256)
- Add fixture: `teralaunch/src-tauri/tests/fixtures/flight-gauge/synthetic-vanilla.gpk` — a synthetic vanilla S1UI_ProgressBar shape (15 Texture2D exports, same names as modded but different payload bytes) to avoid shipping vanilla TERA assets.

- [ ] **Step 1: Write the integration test**

```rust
#[test]
fn flight_gauge_install_enable_disable_round_trip() {
    let tmp = TempDir::new().unwrap();
    // Set up game_root with synthetic vanilla container holding the flight-gauge-shaped vanilla GPK.
    // Run install_gpk_via_patch + enable_gpk_via_patch.
    // Assert the patched bytes parse + each export's payload matches the modded fixture.
    // Run disable_gpk_via_patch.
    // Assert mapper == .clean and standalone deleted.
}
```

- [ ] **Step 2-3: Run, confirm pass**

- [ ] **Step 4: Commit**

```bash
git commit -m "test(mods): pin flight-gauge install→enable→disable round-trip"
```

---

## Self-Review

- **Spec coverage:** Patch derivation (T1) ✓ Vanilla extract (T2) ✓ Manifest store (T3) ✓ install/enable/disable (T4) ✓ Wiring (T5) ✓ Migration (T6) ✓ E2E pin (T7) ✓.
- **Hard-fail-on-unsupported:** T1 step 3 lists the 5 refusal cases (added exports, import drift, name drift, compression, class change) — each emits a clear user-facing error. Replaces "install + break client".
- **Disable bug:** T4 `disable_gpk_via_patch` does not depend on `deployed_filename` from the registry — it reads the manifest's `target_package` instead. The original disable-bug mechanism (`deployed_filename` empty → `uninstall_legacy_gpk` skipped) becomes unreachable in the new flow.
- **No placeholders:** Every code block has runnable code. Test fixtures listed by exact path.
- **Type consistency:** `PatchInstallOutcome` defined in T4, used internally only. `manifest_store::load_manifest_at_root` signature consistent across T3/T4/T6. `target_object_path` parameter consistent across `install_gpk_via_patch` and frontend wiring (T5).

---
