# Mod Manager Architecture

How the launcher's mod manager is wired, subsystem by subsystem. Read this
after `CLAUDE.md`'s `## Mod Manager` section for code layout; this doc goes
deeper on data flow and extension points.

Every section lists the owning file, the public surface it exposes, the
invariants it relies on, and known gaps/follow-ups.

---

## 1. Types (shared vocabulary)

**File:** `teralaunch/src-tauri/src/services/mods/types.rs`

**Exports.** `ModKind` (`External | Gpk`), `ModStatus`
(`NotInstalled | Disabled | Running | Starting | Enabled | UpdateAvailable
| Error | Installing`), `ModEntry`, `Catalog`, `CatalogEntry`, plus two
constructors: `ModEntry::from_catalog(&CatalogEntry)` and
`ModEntry::from_local_gpk(sha_hex, &ModFile)`.

**Invariants.**
- `ModEntry` is the Tauri-boundary shape — serde snake_case representation
  flows to `mods.js` for row rendering; any rename breaks the frontend.
- `ModKind` and `ModStatus` use `#[serde(rename_all = "snake_case")]`.
  JSON test fixtures in `tests/crash_recovery.rs` pin the tag spellings.
- `id` is a stable key. Catalog entries reuse the catalog id. User imports
  use `local.<sha12>` (iter 34) — deterministic on bytes so re-imports are
  idempotent.

**Follow-ups.** None currently.

---

## 2. Catalog

**File:** `services/mods/catalog.rs`

**Flow.**
1. `get_mods_catalog(force_refresh)` Tauri command is called by the
   frontend on the Browse tab.
2. Reads `<app_data>/mods/catalog-cache.json`. Serves fresh (< 24h) cache
   directly.
3. Stale or missing: HTTP-GETs
   `https://raw.githubusercontent.com/TERA-Europe-Classic/external-mod-catalog/main/catalog.json`,
   deserialises, writes the cache atomically (`*.tmp` + rename).

**Invariants.**
- Catalog URL is a `pub const` at the top of the file and appears in
  `tauri.conf.json::tauri.allowlist.http.scope` (pinned by
  `tests/http_allowlist.rs::every_mod_url_on_allowlist`). Adding a new
  source without updating the scope fails CI.
- Cache is per-user (`dirs_next::config_dir()`), not global.

**Follow-ups.** Catalog schema is validated at consume-time (see
`external-mod-catalog/scripts/validate-catalog.mjs`); additional client-side
validation could tighten what reaches the UI, tracked as P2 `catalog.json-schema`.

---

## 3. Registry

**File:** `services/mods/registry.rs`

**State.** A single JSON file at
`<app_data>/Crazy-eSports-ClassicPlus/mods/registry.json`. Version 1 shape:
`{ version: u32, mods: Vec<ModEntry> }`.

**Load path.**
1. File missing → empty default registry, no error.
2. File present → deserialise, then run `recover_stuck_installs()` (PRD
   3.2.2) — flips any `ModStatus::Installing` row to `Error` with a
   `last_error = "Install was interrupted..."` note and clears stale
   `progress`. This makes a SIGKILL mid-install self-healing on next boot.
3. Deserialise failure → `"...corrupted..."` error, not silently empty —
   we want the user to see it.

**Save path.** `save()` writes `<path>.tmp` then `fs::rename` — atomic on
Windows + POSIX; a concurrent reader never sees a partial write.

**Invariants.**
- `recover_stuck_installs` is idempotent (second call = 0 touched).
- `load()` is the only recovery entry point; don't call the recovery fn
  directly from callers or you risk forgetting it somewhere.

**Follow-ups.** Registry is single-process — we rely on OS file locking for
concurrent-launcher safety. The second launcher instance would see a
locked mapper (via the game), so this hasn't surfaced in practice.

---

## 4. External-app download + extract + spawn

**File:** `services/mods/external_app.rs`

**Download flow.**
1. `download_file(url, expected_sha256, dest, on_progress)` streams the
   HTTP body into memory, on_progress-called per chunk.
2. `Sha256::digest(&bytes)` compared hex-lowercase against the catalog's
   `sha256`. Mismatch → `Err("Download hash mismatch: ...")` before any
   `fs::write` / `fs::create_dir_all` runs. Fail-closed by construction;
   pinned by `sha_mismatch_aborts_before_write` (iter 19).
3. `download_and_extract` adds a `zip::ZipArchive` step with
   `enclosed_name()` guard against zip-slip (iter 21 — 4 vector test).
4. Writes to disk only after hash + structure validation.

**Spawn flow.**
- `spawn_app(exe_path, args)`: Windows uses `ShellExecuteExW` with
  `SEE_MASK_NOCLOSEPROCESS` (handles UAC for installers). Non-Windows
  falls back to `std::process::Command`.
- `is_process_running(exe_name)` queries sysinfo (case-insensitive name
  match).

**Attach-once predicate** (PRD 3.2.11 — iter 29).
`decide_spawn(already_running: bool) -> SpawnDecision { Attach | Spawn }`.
Both call sites (`launch_external_app_impl` + `spawn_auto_launch_external_apps`)
route through `check_spawn_decision(exe_name)`. If the 2nd `TERA.exe`
launches and Shinra is already up, the launcher attaches rather than
double-spawning.

**Overlay-lifecycle predicate** (PRD 3.2.12/3.2.13 — iter 31).
`decide_overlay_action(remaining_clients: usize) -> OverlayLifecycleAction
{ KeepRunning | Terminate }`. Partial close (≥ 1 remaining) keeps the
overlay alive; last close tears it down. Wiring to the teralib
game-count watch channel is P1 `fix.overlay-lifecycle-wiring`.

**Safe-filename predicate** (PRD 3.1.4 — iter 24). `executable_path`
rejects absolute paths and `..` components before `install_dir.join(...)`.

**Invariants.**
- Any URL literal in this file must appear in the tauri allowlist scope
  (pinned by `tests/http_allowlist.rs`).
- Download + extract is fail-closed: zero side-effects on hash mismatch
  or zip-slip rejection.
- `stop_process_by_name` is best-effort — some Windows processes deny
  termination, caller must handle the `killed` count.

---

## 5. TMM mapper + GPK install

**File:** `services/mods/tmm.rs`

**Model.** TERA's `CompositePackageMapper.dat` is an encrypted key-value
file mapping `composite_name → (filename, object_path, offset, size)`.
Multiple UPackages are grouped under one filename (grouped-serialisation
shape: `<filename>?<object_path>,<composite_name>,<offset>,<size>,|...|!`).

**Encryption.** 3-pass cipher mirroring VenoMKO/TMM's `CompositeMapper.cpp`:
GeneratePackageMapper XOR + middle-outward swap + Key1 shuffle. Round-trip
tested by `encrypt_then_decrypt_is_identity`.

**Install flow** (`install_gpk(game_root, source_gpk)`).
1. Read + parse source GPK bytes (`parse_mod_file`). Extracts the TMM
   footer: mod_name, mod_author, container, packages (per-object offsets
   + sizes), region_lock flag.
2. Container-filename sandbox (`is_safe_gpk_container_filename`, PRD
   3.1.4 — iter 24). Rejects separators, `..`, drive-letters, null
   bytes, dot-only. **Runs before any fs write**, so a rejected install
   leaves `.clean` untouched.
3. `ensure_backup(game_root)` — copies vanilla `CompositePackageMapper.dat`
   to `.clean` if not already backed up.
4. `fs::copy(source_gpk, game_root/CookedPC/<container>)`.
5. Read + decrypt current mapper, patch via `apply_mod_patches(&mut map,
   &modfile)` (PRD 3.3.2 — iter 33), insert the `TMM_MARKER` so other
   TMM tools recognise our mods.
6. Serialise + encrypt + write back atomically.

**Conflict detection** (`detect_conflicts(vanilla_map, current_map,
incoming)`, PRD 3.3.3 — iter 32). Returns `Vec<ModConflict { composite_name,
object_path, previous_filename }>` for slots owned by a *different* mod.
Not yet wired into a Tauri command; `fix.conflict-modal-wiring` is P1.

**Uninstall flow** (`uninstall_gpk(game_root, container, object_paths)`).
1. Safe-filename guard on `container` (same predicate as install).
2. Read `.clean`, restore vanilla entries for each object_path.
3. Remove the GPK from CookedPC.

**Invariants.**
- `container` is attacker-controlled (from the GPK footer) — sandbox
  guard is non-negotiable.
- `parse_mapper` keys by `composite_name`; per-mapper-entry unique
  assumption is a TMM format invariant (each UPackage has one
  composite_name).
- `apply_mod_patches` errors on unknown object_path (game-version skew);
  no partial patching.

**Follow-ups.** Property-based round-trip tests for the cipher are queued
as `pin.tmm.cipher` (P1).

---

## 6. Self-integrity

**File:** `services/self_integrity.rs` (not under `mods/` but part of
mod-manager-adjacent launcher boot).

**Flow.** At `main()` before Tauri initialises, `run_self_integrity_check`:
1. Reads `<exe_dir>/self_hash.sha256` sidecar (signed by release pipeline).
2. Hashes `current_exe()` and compares.
3. Mismatch → native Windows `MessageBoxW` with user-safe reinstall
   prompt (no raw hashes — social-engineering hygiene), then
   `process::exit(2)`.
4. Sidecar absent → WARN log, continue (dev builds).

**Follow-ups.** Baseline embedding via `build.rs` (forces tampering at
2 locations) is P1 `sec.self-integrity-baseline-embed`.

---

## 7. Tauri command boundary

**File:** `commands/mods.rs`

**Commands** (all registered in `main.rs::invoke_handler`).

| Command | Shape | Notes |
|---|---|---|
| `list_installed_mods()` | `-> Vec<ModEntry>` | Read-only registry snapshot. |
| `get_mods_catalog(force_refresh)` | `-> Catalog` | 24h disk cache. |
| `install_mod(entry, window)` | `-> ModEntry` | Downloads + deploys a catalog entry. Emits `mod_download_progress` events. |
| `add_mod_from_file(path)` | `-> ModEntry` | User-imported GPK. PRD 3.3.4. |
| `uninstall_mod(id, delete_settings)` | `-> ()` | Stops process, removes files, removes from registry. Prompts for settings dir via frontend. |
| `enable_mod(id)` / `disable_mod(id)` | `-> ModEntry` | Intent only — doesn't spawn/kill. Actual run state is the sysinfo query. |
| `launch_external_app(id)` / `stop_external_app(id)` | `-> ModEntry` | Explicit spawn / terminate. |
| `open_mods_folder()` | `-> ()` | Opens OS file-explorer at `mods/` root. |

**Boundary invariants.**
- Every field in `ModEntry` / `Catalog` is serde snake_case — frontend
  expects it (pinned by Playwright specs in `tests/e2e/`).
- Commands are `#[cfg(not(tarpaulin_include))]` to keep them out of
  coverage metrics since they depend on live Tauri state.

---

## 8. Frontend (mods.js)

**File:** `teralaunch/src/mods.js` (ES module, jsdom-friendly for Vitest).

**Tabs.** Browse + Installed. Each renders rows driven by
`list_installed_mods` + `get_mods_catalog`.

**Import button.** Picks `.gpk` via `@tauri-apps/api/dialog.open`, invokes
`add_mod_from_file`, refreshes the installed list. Wired in iter 34.

**Tests.** Vitest (417 tests) for unit-level DOM manipulation; Playwright
(76 tests in 16 files) for end-to-end flows. The Playwright specs
require a warm Tauri dev server — cold runs time out on webServer startup
(documented in the per-spec comment).

**Invariants.**
- `mods.js` is the only file that calls `@tauri-apps/api/tauri.invoke` for
  mod-manager commands; adding a new call site requires updating
  `tauri.conf.json` permission list (tracked implicitly via the Tauri v2
  capability migration — `sec.tauri-v1-eol-plan`).

---

## 9. Cross-subsystem guarantees

- **Fail-closed download/extract:** iter 19/21/23. No bytes on disk on
  hash mismatch or zip-slip.
- **Deploy sandbox:** iter 24. No filesystem write outside
  `<game>/S1Game/CookedPC/` under any crafted container name.
- **Crash recovery:** iter 28. SIGKILL mid-install self-heals on next boot.
- **Attach-once + overlay lifecycle:** iter 29/31. Second TERA.exe client
  attaches to live overlays instead of double-spawning.
- **Self-integrity:** iter 27. Tampered launcher shows a native dialog
  and refuses to proceed.
- **Deploy scope:** iter 25. CI gate refuses deploys outside
  `/classicplus/` on kasserver.
- **Secret scan:** iter 16. CI workflow in 4 repos (launcher, TCC, Shinra,
  external-mod-catalog) fails the job on any new secret leaked.

---

## 10. Known gaps

See `docs/PRD/fix-plan.md` for the current backlog. Top P1 follow-ups as
of iter 37:

- `fix.overlay-lifecycle-wiring` — wire the game-count watch channel
  (iter 31 left the predicate in but the channel listener is TODO).
- `fix.conflict-modal-wiring` — Tauri command + frontend modal for
  (composite, object) conflicts (iter 32 left the predicate in).
- `sec.self-integrity-baseline-embed` — move the self-integrity baseline
  from sidecar to `build.rs`-injected constant (iter 27 left it on
  sidecar only).
- `sec.tauri-v1-eol-plan` — audit is drafted at
  `docs/PRD/audits/security/tauri-v2-migration.md`, awaiting human
  sign-off on 4 decision gates before the migration milestones are
  turned into fix-plan items (gates 3.1.8, 3.1.9, 3.1.12).
