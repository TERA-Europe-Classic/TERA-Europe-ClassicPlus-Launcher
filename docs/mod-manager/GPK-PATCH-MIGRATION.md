# GPK Patch Migration Program

This document is the execution backbone for making **every catalog GPK mod**
work on live Classic+ clients without relying on brittle whole-file
replacement.

## Goal

Move the launcher from two legacy install shapes:

1. **TMM/footer-based composite mapper patching**
2. **legacy whole-file/package-name replacement**

to a third, safer path:

3. **curated export-level patch manifests applied to the user's current vanilla package**

The launcher should ultimately install vetted patch manifests, not raw old
modded GPK files.

---

## Current catalog inventory

`external-mod-catalog/catalog.json` currently contains **99 GPK mods**:

- **60** UI
- **27** cosmetic
- **9** performance
- **2** effects
- **1** fun

### Family grouping

#### Wave A — highest-value UI families

| Family | Count | Strategy | Notes |
|---|---:|---|---|
| `restyle` | 26 | `export-replacement` | foglio1024 + Taorelia packages. Strong candidate for shared converter flow. |
| `ui-remover` | 9 | `export-replacement` | High player demand. Good first family for a drift-safe pipeline. |
| `ui-layout` | 8 | `export-replacement` | message/boss/party/target/character window style changes. |
| `ui-recolor` | 8 | `texture-or-export` | likely texture/material-only or simple export replacement. |
| `misc` | 10 | `manual-review` | mixed bag; includes one-offs like `S1UI_Chat2`, `OverlayMap2`, `badGUI Loader`, etc. |

#### Wave B — effect/performance families

| Family | Count | Strategy | Notes |
|---|---:|---|---|
| `fx-cleanup` | 9 | `effect-replacement` | Owyn FPS packs; likely package/export-level effect replacements. |
| `effects` | 2 | `effect-replacement` | small but distinct family. |

#### Wave C — highest-risk families

| Family | Count | Strategy | Notes |
|---|---:|---|---|
| `cosmetic-model` | 27 | `high-risk-manual` | costume/model/mount/pet/accessory packages; likely most drift-sensitive. |

---

## Patch-manifest path

The launcher now has a schema for curated patch artifacts at:

- `teralaunch/src-tauri/src/services/mods/patch_manifest.rs`

Current launcher-side artifact contract:

- manifest root:
  - `<config>/Crazy-eSports-ClassicPlus/mods/patch-manifests/`
- per-mod bundle:
  - `<root>/<mod-id-sanitized>/manifest.json`
  - `<root>/<mod-id-sanitized>/payloads/`

Notes:

- mod ids are sanitized with `/` and `\` replaced by `_`
- `payloads/` is reserved now so the offline converter has a stable target,
  even though v1 launcher validation still keeps replacement payload bytes
  inline in the manifest
- launcher install preflight already consumes this bundle contract and fails
  closed when a curated artifact exists

### Proposed catalog / distribution contract

Recommended shipping shape for curated bundles:

1. One downloadable archive per converted mod id
2. Archive root contains exactly one sanitized bundle directory
3. Bundle directory contains:
   - `manifest.json`
   - `payloads/`
4. Catalog entry points at the archive URL, not at raw `manifest.json`
5. Launcher installs/unpacks the archive into:
   - `<config>/Crazy-eSports-ClassicPlus/mods/patch-manifests/<mod-id-sanitized>/`

Why this shape:

- matches the launcher's local bundle contract exactly
- lets the catalog stay one-entry-per-mod instead of inventing a second
  manifest-index registry
- keeps payload growth isolated from the JSON manifest
- preserves hash-verifiable delivery at the archive level
- leaves room for future sidecar files (review notes, extracted screenshots,
  provenance) without changing the install root contract

Archive invariants to enforce later:

- exactly one top-level bundle directory
- no zip-slip / parent traversal paths
- `manifest.json` must exist at bundle root
- extracted bundle dir name must match sanitized `mod_id`
- payload files may exist only under `payloads/`

The intended install model is:

1. Offline converter compares **reference vanilla** + **old modded** package.
2. Maintainer reviews the candidate patch.
3. Converter emits a `PatchManifest` + payloads.
4. Launcher applies the manifest onto the user's **current** vanilla package.

### Why we are not using binary deltas

- old vanilla vs current vanilla introduces unrelated patch drift
- export/import table indices may shift
- bulk data / TFC refs may drift
- package offsets are not stable enough to replay raw byte chunks safely

So we intentionally treat the patch as **structured export replacement**, not a
whole-file diff.

---

## Rollout plan

### Phase 1 — schema and converter skeleton

Deliverables:

- [x] patch-manifest schema in launcher repo
- [x] patch artifact storage layout
- [x] fail-closed validator for manifest compatibility
- [x] offline converter CLI skeleton

### Phase 2 — Wave A conversion

Target all:

- `ui-remover`
- `restyle`
- `ui-layout`
- `ui-recolor`

Each mod gets one of:

- `converted`
- `blocked-needs-reference-vanilla`
- `blocked-unsupported-export-shape`
- `manual-review-required`

### Phase 3 — Wave B conversion

Target:

- `fx-cleanup`
- `effects`

### Phase 4 — Wave C manual program

Target:

- `cosmetic-model`

These likely need bespoke handling and stricter compatibility checks.

---

## First implementation target

Start with:

1. **UI Remover: Flight Gauge**
2. **UI Remover: Boss Window**
3. **Message window — cleaned**
4. **Restyle: Community Window**

These are representative, high-value, and likely simpler than cosmetic/model mods.

### Wave A conversion table — initial seed

| Mod | Family | Target package | Current status | Notes |
|---|---|---|---|---|
| UI Remover: Flight Gauge | `ui-remover` | `S1UI_ProgressBar.gpk` | `blocked-unsupported-export-shape` | Reviewed against current v100 vanilla. The mod swaps 14 `ObjectRedirector` exports into real `Texture2D` exports, so current v1 payload-only manifests cannot express it safely. See `docs/mod-manager/reviews/flight-gauge-v100-review.md`. |
| UI Remover: Boss Window | `ui-remover` | `S1UI_GageBoss.gpk` | `blocked-unsupported-export-shape` | Reviewed against current v100 vanilla. The mod removes the `GageBoss_I1C` export entirely, which current v1 manifests cannot express safely. See `docs/mod-manager/reviews/boss-window-v100-review.md`. |
| Message window — cleaned | `ui-layout` | `S1UI_Message.gpk` | `blocked-unsupported-export-shape` | Reviewed against current v100 vanilla. The mod collapses the package from 181 exports to 1 export and materially changes imports, so current v1 manifests cannot express it safely. See `docs/mod-manager/reviews/message-window-cleaned-v100-review.md`. |
| Restyle: Community Window | `restyle` | `S1UI_CommunityWindow.gpk` | `blocked-unsupported-export-shape` | Reviewed against current v100 vanilla. The mod changes both export and import table shape and converts many redirectors into real `Texture2D` exports. See `docs/mod-manager/reviews/community-window-v100-review.md`. |

---

## Rules for “every single mod”

We only count a mod as “done” when:

1. its family strategy is assigned,
2. its patch artifact exists or a documented blocker is recorded,
3. the launcher can validate/apply it safely,
4. a maintainer reviewed the extracted changes.

No silent fallback to legacy whole-file replacement for converted families.

---

## Immediate next steps

- start producing the first reviewed candidate bundle with `gpk-patch-converter`
- extend conversion support beyond payload-only replacement for reviewed texture/redirector class-shape changes
- design the next manifest/applier revision around reviewed structural operations (class change, export add/remove, import/name edits)
- turn the Wave A seed rows into concrete review items with confirmed export/object targets
- wire the proposed archive distribution contract into catalog metadata once the
  first converted mod is ready to ship
- teach the launcher to surface curated-manifest state more explicitly than a
  generic install error once the patch applier exists

---

## V2 manifest/applier design

### Problem

All four reviewed Wave A mods need structural operations that v1 patch manifests
cannot express:

| Mod | Class change | Export delete | Export create | Import mutation |
|---|---|---|---|---|
| Flight Gauge | 14× `ObjectRedirector` → `Texture2D` | — | — | 32 → 4 imports |
| Boss Window | — | 1× `GageBoss_I1C` redirector removed | — | same count |
| Message window | — | 180× exports removed (collapse to 1) | — | 366 → 414 imports |
| Community Window | many× `ObjectRedirector` → `Texture2D` | — | +1 export | 132 → 6 imports |

V1 only supports `ReplaceExportPayload` — a single payload-swap on an existing
export whose class and table position are unchanged. That works for pure
texture/content swaps but not for the structural reshaping above.

### V2 schema additions

The v2 schema extends `ExportPatchOperation` with two new variants and adds two
new top-level patch lists to `PatchManifest`:

**Export operations (v2):**

| Operation | Meaning | Payload required |
|---|---|---|
| `replace_export_payload` (v1) | Swap export body bytes in-place | Yes |
| `replace_export_class_and_payload` (v2) | Change the export's class (e.g. `ObjectRedirector` → `Texture2D`) AND replace its body | Yes |
| `remove_export` (v2) | Delete the export from the export table entirely | No |
| `patch_properties` (reserved) | Structured property-level patching | Future |

**Import patches (v2):** New `import_patches: Vec<ImportPatch>` on `PatchManifest`.

| Operation | Meaning |
|---|---|
| `remove_import` | Remove an import entry by path |
| `add_import` | Add a new import entry (requires `class_name`, optional `class_package`) |

**Name patches (v2):** New `name_patches: Vec<NamePatch>` on `PatchManifest`.

| Operation | Meaning |
|---|---|
| `ensure_name` | Add a name to the name table if absent (idempotent) |
| `remove_name` | Remove a name (only safe if no export/import references it) |

**New field on `ExportPatch`:**

- `new_class_name: Option<String>` — the target class for
  `replace_export_class_and_payload` (e.g. `Core.Texture2D`).

### Applier execution order

The patch applier processes patches in this fixed order:

1. **Validate** — check package fingerprint, verify all referenced exports exist,
   verify no overlapping export patches.
2. **Name table** — apply `name_patches` (`ensure_name` first, `remove_name` last).
3. **Export patches** — process exports in manifest order:
   - `replace_export_payload`: swap body bytes, verify fingerprint.
   - `replace_export_class_and_payload`: update export class index, swap body bytes.
   - `remove_export`: remove export from export table, adjust `ExportCount`.
4. **Import patches** — apply `import_patches` (`add_import` first, `remove_import` second).
5. **Reindex** — rewrite the package header (`NameCount`, `ExportCount`,
   `ImportCount`, `HeaderSize`), update generation table, recompute offsets.
6. **Verify** — re-read the patched package, confirm all expected exports are
   present with correct classes, confirm no dangling import references.

### Compatibility

- V2 manifests set `schema_version: 2`. V1 remains `schema_version: 1`.
- The launcher's existing fail-closed preflight continues to reject manifests
  with `schema_version > max_supported` until the applier is implemented.
- `import_patches` and `name_patches` are optional (default empty) so v1
  manifests deserialize cleanly under the v2 schema.

### Wave A coverage under v2

| Mod | V2 operations needed | Unblocked? |
|---|---|---|
| Flight Gauge | 14× `replace_export_class_and_payload` + `remove_import` | ✅ |
| Boss Window | 1× `replace_export_payload` + 1× `remove_export` | ✅ |
| Message window | 1× `replace_export_payload` + 180× `remove_export` + import patches | ✅ |
| Community Window | `replace_export_class_and_payload` + `remove_import` + `ensure_name` | ✅ |
