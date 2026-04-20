# Unit Audit — `launcher/services/mods/registry.rs`

**Category:** launcher
**Status:** draft
**Last verified:** 2026-04-20 by `loop/iter-233`
**Iter:** `233`

## Provenance

- **Upstream source:** in-house.
- **License:** project licence.
- **Obfuscated binaries?** no.
- **Version pinned:** launcher crate version.
- **Download URL:** n/a (internal module).
- **SHA-256:** covered by launcher self-integrity guard.

## Public surface

- `Registry::load(path) -> Result<Registry, String>` — read `<app_data>/mods/registry.json` from disk. On parse error, returns empty registry and logs a WARN (tolerant read — a corrupted registry doesn't brick the mods page).
- `Registry::save(&self, path) -> Result<(), String>` — atomic JSON write (write to `.tmp` + rename). Used after every state mutation.
- `Registry::upsert(&mut self, entry: ModEntry)` — replace-or-insert by id.
- `Registry::try_claim_installing(&mut self, row: ModEntry) -> Result<(), String>` — atomic install-serialisation primitive (PRD §3.2.7). If an id is already in `Installing` state, refuses with a user-facing "already in progress" message; else upserts with `Installing` status and takes ownership.
- `Registry::recover_stuck_installs(&mut self)` — on every startup, flip any `Installing` slot to `Error` with an "interrupted" note. The process that owned the install died — no other process could have been writing that slot concurrently.
- `Registry::entry(&self, id) -> Option<&ModEntry>` / `entry_mut` / `remove` / `list` — standard CRUD surface.

**Tauri commands registered:** none directly. Callers in `commands/mods.rs` invoke these through the shared `Mutex<Registry>` at `state::mods_state::mutate`.

**Settings files written:** `<app_data>/mods/registry.json`.

**Ports / processes / network:** none. Pure CPU + local filesystem.

**Game memory locations patched / DLLs injected:** none.

## Risks

- **Crash-during-install hazard** — if the launcher dies between `try_claim_installing` (marks slot Installing) and the install-success upsert (marks slot Enabled), the slot is stranded as Installing forever. Mitigation: `recover_stuck_installs()` runs on every `Registry::load()` and rewrites Installing → Error with a note. Next start: user sees "installation interrupted" in the row; they can retry and `try_claim_installing` accepts the retry because the status is Error, not Installing (per `reclaim_after_error_succeeds` test).
- **Concurrent install race** — two `invoke('install_mod')` calls for the same id could both download into the same slot dir. Mitigation: the `Mutex<Registry>` at `state::mods_state::mutate` serialises; inside the critical section, `try_claim_installing` enforces the "first one wins" invariant — second caller sees `Installing` and gets a clean Err. Integration test: `same_id_serialised_second_claim_refused`.
- **Corrupted registry.json** — user opens `%APPDATA%/tera-europe-classicplus-launcher/mods/registry.json` in a text editor, saves malformed JSON. On next start, `Registry::load` returns the empty registry with a WARN log — the mods page renders empty, the user can re-install. No crash, no cascade into other launcher features.
- **Atomic writes** — `save()` uses the write-then-rename pattern. Partial writes (power loss mid-save) leave either the old file or no temp file, never a truncated registry.
- **No outbound network** — registry is purely local state.

## Tests

Launcher-side tests:

- `src/services/mods/registry.rs::tests` — CRUD primitives, `try_claim_installing` happy path + refusal + reclaim-after-error, `recover_stuck_installs` coverage, atomic-save round-trip.
- `src/state/mods_state.rs::tests` — Mutex-guarded mutate + list boundary.
- `tests/parallel_install.rs` — symbolic integration of the serialisation invariant (PRD §3.2.7).
- `tests/crash_recovery.rs` — end-to-end crash-mid-install simulation.

Manual verification steps:

- Kill the launcher process during a mod download; restart; confirm the row shows "Error: interrupted" not a stranded spinner.
- Fire two simultaneous installs for the same mod from separate invoke calls; confirm the second one surfaces the "already in progress" error.

## Verification

- [x] Tests pass in CI
- [x] Sandbox predicates pass (no FS escape, atomic writes)
- [ ] Sha256 matches catalog.json (n/a — internal module)
- [x] License declared
- [x] Risks triaged and mitigated

## Sign-off

| Date | Verifier | Notes |
|------|----------|-------|
| 2026-04-20 | `loop/iter-233` | Initial draft. Awaiting human sign-off. |
