# Unit Audit — `launcher/state/mods_state.rs`

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

- `mutate<F, R>(f: F) -> Result<R, String>` where `F: FnOnce(&mut Registry) -> Result<R, String>` — the ONLY sanctioned path to modify the in-memory registry. Acquires the `Mutex<Registry>` at module scope, calls `f`, auto-saves via `Registry::save()` on success.
- `list() -> Vec<ModEntry>` — read-only snapshot; acquires + drops the mutex.
- `entry(id) -> Option<ModEntry>` — single-row lookup.

The mutex is a `parking_lot::Mutex` (non-poisoning) wrapping the same `Registry` instance used by `services/mods/registry.rs`.

**Tauri commands registered:** none directly. Commands in `commands/mods.rs` call these.

**Settings files written:** `<app_data>/mods/registry.json` (indirectly, via `mutate`'s post-mutation `Registry::save()`).

**Ports / processes / network:** none.

**Game memory locations patched / DLLs injected:** none.

## Risks

- **Critical-section discipline** — the whole point of this module is to serialise registry mutations. A caller that bypasses `mutate()` and directly locks the mutex or pokes at the `Registry` could defeat `try_claim_installing`'s serialisation invariant (PRD §3.2.7). Mitigation: `Registry` is exposed only through this module; the mutex is `pub(crate)` at best, not re-exported.
- **Save-on-mutate atomicity** — if `Registry::save()` fails, `mutate()` returns `Err` but the in-memory registry has already been mutated. Next caller's `list()` sees the new state; next `save()` will write it. The tempfile-rename save path makes a partial-write corruption nearly impossible.
- **Lock hold time** — `mutate()` holds the mutex across the FnOnce body. If a caller performs IO (download, network) inside the closure, every other `mutate()` blocks. Convention: closures are short (flag-flip + upsert); long operations happen outside, with `try_claim_installing` gating entry.
- **No outbound network** — state module is purely local.

## Tests

Launcher-side tests:

- `src/state/mods_state.rs::tests::mutate_then_list_returns_entry` — end-to-end round-trip of upsert → list.
- `tests/parallel_install.rs` — symbolic integration of the serialisation invariant (PRD §3.2.7).
- `tests/crash_recovery.rs` — end-to-end crash + recovery.
- Ad-hoc: every `commands/mods.rs` command function exercises `mutate()` as the write boundary.

Manual verification steps:

- Invoke `install_mod` + `enable_mod` concurrently from two separate JS calls on the same id; confirm registry.json shows the final state as consistent (no half-flipped flags).
- Kill the process between `mutate`'s closure return and `Registry::save` completion; on restart, `Registry::load` sees the pre-mutate state (atomic tempfile-rename contract).

## Verification

- [x] Tests pass in CI
- [x] Sandbox predicates pass (mutex-guarded critical section)
- [ ] Sha256 matches catalog.json (n/a — internal module)
- [x] License declared
- [x] Risks triaged and mitigated

## Sign-off

| Date | Verifier | Notes |
|------|----------|-------|
| 2026-04-20 | `loop/iter-234` | Initial draft. Awaiting human sign-off. |
