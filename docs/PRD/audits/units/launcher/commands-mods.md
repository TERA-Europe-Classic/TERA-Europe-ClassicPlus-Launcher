# Unit Audit — `launcher/commands/mods.rs`

**Category:** launcher
**Status:** draft
**Last verified:** 2026-04-20 by `loop/iter-234`
**Iter:** `234`

## Provenance

- **Upstream source:** in-house.
- **License:** project licence.
- **Obfuscated binaries?** no.
- **Version pinned:** launcher crate version.
- **Download URL:** n/a.
- **SHA-256:** covered by launcher self-integrity guard.

## Public surface

Tauri commands exposed to the frontend via `#[tauri::command]`:

- `list_mods() -> Vec<ModEntry>` — read-only list snapshot.
- `install_mod(id: String) -> Result<ModEntry, String>` — catalog-driven install. Dispatches on `ModKind::External | Gpk` to `install_external_mod` / `install_gpk_mod`.
- `uninstall_mod(id) -> Result<(), String>` — remove slot dir (external) or restore mapper + delete file (GPK legacy or TMM).
- `enable_mod(id)` / `disable_mod(id) -> Result<ModEntry, String>` — flag-only intent changes via `apply_enable_intent` / `apply_disable_intent` pure helpers. No process spawn or kill.
- `launch_mod(id) -> Result<ModEntry, String>` — spawn an external mod's executable (respects attach-once predicate for multi-TERA-client scenarios).
- `stop_mod(id) -> Result<(), String>` — terminate the spawned process.
- `add_mod_from_file(path: String) -> Result<ModEntry, String>` — pick a local `.gpk`, compute SHA, parse UE3 header, validate safe-container-filename, deploy via legacy or TMM path.
- `open_mods_folder()` — shell-open the mods app_data dir.
- `refresh_catalog()` — force-fetch the remote catalog (bypasses 24h cache).

**Settings files written:** via the services layer (registry.json, external_app slot dirs, CookedPC mapper/gpk files).

**Ports / processes / network:** per dispatched service (catalog fetch, external-app download, spawn).

**Game memory locations patched / DLLs injected:** none directly; GPK installs patch `CompositePackageMapper.dat` via `services::mods::tmm`.

## Risks

- **Tauri command boundary** — every fn here is reachable from the frontend via IPC. Capabilities at `capabilities/migrated.json` whitelist the exact command set; a new pub fn without a capability entry is unreachable (defensive default).
- **Input validation** — all `String` params crossing the boundary MUST be validated before touching the filesystem. `add_mod_from_file` validates the path against `utils::path::validate_path_within_base` and the UE3-derived filename against `is_safe_gpk_container_filename`. A refactor that skips either step opens CookedPC/ to path-traversal writes.
- **Error-path preservation** — every command returns `Result<_, String>`; the frontend surfaces the string via toast. Errors must be user-actionable ("Mod file has no readable UE3 package header — not a TERA-compatible .gpk.") not framework noise ("io::Error::UnexpectedEof").
- **State machine invariants** — every install path ends by calling `finalize_installed_slot` (shared helper), so the defaults contract (enabled=true, auto_launch=true, status=Enabled) can't diverge between external and GPK code paths. Pinned by `fresh_install_defaults_enabled` + related tests.
- **Intent-only toggle** — `enable_mod` / `disable_mod` must NOT spawn or kill processes (PRD §3.3.15 "toggle-intent-only"). Enforced by the pure `apply_enable_intent` / `apply_disable_intent` helper signature (`&mut ModEntry` — no process access).

## Tests

Launcher-side tests:

- `src/commands/mods.rs::tests` — fresh-install defaults (3 variants), toggle-intent-only (4 variants, incl. source-inspection guard that rejects `spawn_app` / `stop_process_by_name` inside enable/disable bodies), `install_mod` dispatch coverage.
- `tests/multi_client.rs` — external spawn + attach-once predicate under multiple TERA instances.
- `tests/disk_full.rs` — revert-on-error end-to-end.
- `tests/tampered_catalog.rs` — corrupted catalog entries surface as `ModStatus::Error` rows, not crashes.
- `tests/parallel_install.rs` — serialisation of concurrent install attempts.

Manual verification steps:

- Install the Shinra Meter external mod; confirm it spawns exactly once even with two TERA.exe instances.
- Install a legacy (non-TMM) GPK via `add_mod_from_file`; confirm the file lands in `CookedPC/<PackageName>.gpk` AND the mapper entry for any `.<PackageName>`-suffixed object_path rewrites to it.
- Install a mod; invoke `disable_mod` while it's running; confirm the process stays alive and only the status label flips.

## Verification

- [x] Tests pass in CI
- [x] Sandbox predicates pass (path-within-base + is_safe_gpk_container_filename at every FS-write command)
- [ ] Sha256 matches catalog.json (n/a — internal module)
- [x] License declared
- [x] Risks triaged and mitigated

## Sign-off

| Date | Verifier | Notes |
|------|----------|-------|
| 2026-04-20 | `loop/iter-234` | Initial draft. Awaiting human sign-off. |
