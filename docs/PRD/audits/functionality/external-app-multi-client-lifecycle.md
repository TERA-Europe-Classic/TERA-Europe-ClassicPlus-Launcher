# External app multi-client lifecycle audit

Scope: PRD `3.2.11` to `3.2.13` for external apps (`classicplus.shinra`,
`classicplus.tcc`).

Audit date: 2026-04-22

---

## Objective

Verify that launcher-side external app lifecycle logic matches the intended
attach-once / stop-on-last-close contract:

1. second game client does **not** spawn a second overlay process,
2. partial close keeps overlays alive,
3. last close terminates overlays.

---

## Evidence reviewed

### Production code

- `teralaunch/src-tauri/src/commands/mods.rs`
  - `launch_external_app_impl()`
  - `spawn_auto_launch_external_apps()`
  - `stop_auto_launched_external_apps()`
- `teralaunch/src-tauri/src/services/mods/external_app.rs`
  - spawn decision / running-process detection seams referenced by tests

### Test coverage

- `teralaunch/src-tauri/tests/multi_client.rs`
  - `second_client_no_duplicate_spawn`
  - `partial_close_keeps_overlays`
  - `last_close_terminates_overlays`
  - structural wiring guards against unconditional stop / gate bypass

---

## Findings

### 1. Attach-once on second client launch — PASS

Evidence:

- `commands/mods.rs:821-830` checks `external_app::check_spawn_decision(&exe_name)`
  before spawning in the ad-hoc launch path.
- `commands/mods.rs:978-982` does the same in the game-launch auto-launch path.
- `tests/multi_client.rs` contains both behavioural and structural guards for
  the attach/spawn predicate.

Assessment:

- Current launcher code is aligned with PRD `3.2.11`.

### 2. Partial close keeps overlays alive — PASS

Evidence:

- `tests/multi_client.rs:79-97` explicitly models and pins the
  `remaining_clients > 0 => KeepRunning` rule.
- `tests/multi_client.rs:105-134` source-inspects `commands/game.rs` to ensure
  overlay-stop wiring is gated by the lifecycle decision rather than firing
  unconditionally.

Assessment:

- Current launcher code is aligned with PRD `3.2.12`.

### 3. Last close terminates overlays — PASS

Evidence:

- `commands/mods.rs:1018-1062` iterates installed external entries, skips apps
  that are already dead, and terminates still-running external processes via
  `stop_process_by_name`.
- `tests/multi_client.rs` pins the `remaining_clients == 0 => Terminate` rule
  and checks the call path remains present.

Assessment:

- Current launcher code is aligned with PRD `3.2.13`.

---

## Caveats / remaining verification work

This audit confirms the **launcher-side decision logic and wiring**. It does not
replace runtime visual/manual verification of the external apps themselves.

Remaining manual/runtime work still needed elsewhere:

- actual AppData TCC tray-icon verification,
- end-to-end live-client confirmation that the launcher's game-close hook fires
  as expected for the real game process count on this machine,
- release-path verification after catalog updates.

---

## Verdict

Launcher-side multi-client external app lifecycle logic is **present and
adequately pinned in code/tests** for PRD `3.2.11`–`3.2.13`.

Status: **PASS (logic + test coverage)**
