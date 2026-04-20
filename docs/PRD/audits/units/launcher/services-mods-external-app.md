# Unit Audit — `launcher/services/mods/external_app.rs`

**Category:** launcher
**Status:** draft
**Last verified:** 2026-04-20 by `loop/iter-229`
**Iter:** `229`

## Provenance

- **Upstream source:** in-house.
- **License:** project licence.
- **Obfuscated binaries?** no.
- **Version pinned:** launcher crate version.
- **Download URL:** n/a.
- **SHA-256:** covered by launcher self-integrity guard.

## Public surface

- `download_and_extract(catalog_entry, progress_cb) -> Result<(), String>` — HTTPS GET → SHA-256 verify against `catalog.sha256` → extract zip into `<app_data>/mods/external/<id>/`. Aborts before any FS write on SHA mismatch.
- `download_file(url, dest, progress_cb)` — GET → SHA verify → `fs::write`. Used by GPK install path; reverts partial writes on failure.
- `spawn_app(slot) -> Result<ProcessHandle, String>` — launches the mod's executable from its slot dir. Uses `Command::new` with inherited working directory.
- `check_spawn_decision(slot, running_tera_count) -> SpawnDecision` — attach-once vs spawn-new vs skip, based on whether the mod is already running and how many TERA.exe instances are open.
- `is_process_running(name)` — name-based PID scan (Windows).
- `revert_partial_install_dir` / `revert_partial_install_file` — best-effort cleanup on error.

**Tauri commands registered:** none directly; callers in `commands/mods.rs`.

**Settings files written:** `<app_data>/mods/external/<id>/**` (whatever the zip contained).

**Ports / processes / network:**
- Outbound HTTPS GET against each catalog entry's `download_url`.
- Processes spawned: mod executables (e.g. `ShinraMeter.exe`, `TCC.exe`).

**Game memory locations patched / DLLs injected:** none. Mods read decrypted packets from the launcher's 127.0.0.1:7803 mirror socket — this module doesn't touch that socket itself, only spawns the processes that do.

## Risks

- **Arbitrary executable launch** — external mods are trusted up to their catalog SHA. Mitigations:
  - SHA-256 mandatory — mismatch aborts before write (`sha_mismatch_aborts_before_write` in-module test).
  - Zip-slip prevention — `extract_zip` validates every entry path against the destination dir (`extract_zip_rejects_zip_slip` test).
  - Exec path derived from `executable_relpath` in the catalog entry; validated against `<slot_dir>` via `utils::path::validate_path_within_base`.
- **Outbound network** — every GET is against a URL read from the signed catalog. The catalog fetch itself is on the allow-list; download URLs are ratified by the catalog author (TERA-Europe-Classic org).
- **Disk-full / partial writes** — `revert_partial_install_dir` / `revert_partial_install_file` run on every error path (`revert_on_enospc` test). Users can retry cleanly.
- **Process spawn** — inherits the launcher's environment but explicit `current_dir` is the slot dir. No env-var filtering (mod executables inherit `PATH`, etc.) — documented limitation.

## Tests

Launcher-side tests:

- `src/services/mods/external_app.rs::tests` — SHA verify happy path + mismatch abort (both external + GPK), zip-slip rejection, extract idempotency, revert helpers, spawn-decision predicate, at-least-10 Hz progress callback.
- `tests/disk_full.rs` — integration-style revert coverage.
- `tests/multi_client.rs` — multiple TERA.exe + multiple mod instances.
- `tests/http_allowlist.rs`, `tests/http_redirect_offlist.rs` — outbound-network sandboxing.

Manual verification steps:

- Install Shinra + TCC; launch game; confirm both spawn exactly once and attach to the one running TERA.exe.
- Fill the mod-install target disk; trigger install; confirm partial-dir revert leaves no residue.
- Modify an installed mod's executable on disk; catalog update should not silently reinstall.

## Verification

- [x] Tests pass in CI
- [x] Sandbox predicates pass (SHA verify, zip-slip reject, path-within-base, allow-list)
- [ ] Sha256 matches catalog.json (n/a — internal module)
- [x] License declared
- [x] Risks triaged and mitigated

## Sign-off

| Date | Verifier | Notes |
|------|----------|-------|
| 2026-04-20 | `loop/iter-229` | Initial draft. Awaiting human sign-off. |
