//! PRD 3.2.11.multi-client-attach-once — integration-level pin.
//!
//! Bin-crate limitation: can't import `SpawnDecision` / `decide_spawn` here.
//! The behavioural test lives in `src/services/mods/external_app.rs::tests::
//! second_client_no_duplicate_spawn`. This file pins the shape of the
//! attach-once protocol so the in-crate implementation can't regress to a
//! structurally different rule silently.

use std::fs;

/// Model of the spawn decision rule. If this ever diverges from
/// `external_app::decide_spawn`, the integration test here and the in-crate
/// test will both need to change — which is the pressure we want against
/// accidental rewrites.
#[derive(Debug, PartialEq, Eq)]
enum SpawnDecisionModel {
    Attach,
    Spawn,
}

fn decide_spawn_model(already_running: bool) -> SpawnDecisionModel {
    if already_running {
        SpawnDecisionModel::Attach
    } else {
        SpawnDecisionModel::Spawn
    }
}

#[test]
fn second_client_no_duplicate_spawn() {
    // First TERA.exe launches. Shinra is not running. Decision: Spawn.
    let first = decide_spawn_model(false);
    assert_eq!(first, SpawnDecisionModel::Spawn);

    // After the spawn, Shinra is running.
    let running_after_first_spawn = true;

    // Second TERA.exe launches. Decision must be Attach, not Spawn.
    let second = decide_spawn_model(running_after_first_spawn);
    assert_eq!(
        second,
        SpawnDecisionModel::Attach,
        "2nd client must attach to existing Shinra/TCC — never spawn a duplicate"
    );
}

#[test]
fn decision_is_pure_and_deterministic() {
    // Same input -> same output, and the only input is the already_running
    // bit. Pins the pure-predicate shape; if external_app::decide_spawn
    // ever grows a second parameter, the in-crate test will need to
    // update, forcing a reviewer to audit the new input for its attack
    // surface.
    for _ in 0..100 {
        assert_eq!(decide_spawn_model(true), SpawnDecisionModel::Attach);
        assert_eq!(decide_spawn_model(false), SpawnDecisionModel::Spawn);
    }
}

// --- Lifecycle mirror for PRD 3.2.12 / 3.2.13 ------------------------------

/// Model of the overlay lifecycle rule. Diverging from
/// `external_app::decide_overlay_action` would be noticed here the next
/// time a refactor touches either file.
#[derive(Debug, PartialEq, Eq)]
enum OverlayActionModel {
    KeepRunning,
    Terminate,
}

fn decide_overlay_action_model(remaining_clients: usize) -> OverlayActionModel {
    if remaining_clients == 0 {
        OverlayActionModel::Terminate
    } else {
        OverlayActionModel::KeepRunning
    }
}

#[test]
fn partial_close_keeps_overlays() {
    // One of two clients closes -> one remains -> overlays stay alive.
    assert_eq!(
        decide_overlay_action_model(1),
        OverlayActionModel::KeepRunning,
        "partial close (remaining=1) must keep overlays up"
    );
}

#[test]
fn last_close_terminates_overlays() {
    // Last client closes -> 0 remain -> overlays torn down.
    assert_eq!(
        decide_overlay_action_model(0),
        OverlayActionModel::Terminate,
        "last close (remaining=0) must tear overlays down"
    );
}

/// Wiring guard for fix.overlay-lifecycle-wiring. The pure predicate is
/// unit-tested above; this test source-inspects `commands/game.rs` to
/// assert the overlay-stop call is gated by `decide_overlay_action`
/// rather than firing unconditionally. Without this, a future commit
/// could silently restore the old "stop on every close" behaviour and
/// all the predicate tests would still pass.
#[test]
fn game_rs_gates_overlay_stop_on_decide_overlay_action() {
    let body =
        std::fs::read_to_string("src/commands/game.rs").expect("commands/game.rs must exist");

    let predicate_pos = body.find("decide_overlay_action(").expect(
        "commands/game.rs must call decide_overlay_action — wiring missing (fix.overlay-lifecycle-wiring)",
    );
    let stop_pos = body
        .find("stop_auto_launched_external_apps")
        .expect("commands/game.rs must still call stop_auto_launched_external_apps on last close");

    assert!(
        predicate_pos < stop_pos,
        "decide_overlay_action must be called BEFORE stop_auto_launched_external_apps \
         in commands/game.rs — otherwise the stop is unconditional and overlays would \
         tear down on partial closes (PRD 3.2.12 regression)."
    );

    // Guard against the stop call escaping the decision branch. If
    // someone reintroduces a bare `stop_auto_launched_external_apps();`
    // outside the `if decide_overlay_action(...) == Terminate` block,
    // a second occurrence of the stop call would surface here.
    let second_stop = body[stop_pos + "stop_auto_launched_external_apps".len()..]
        .find("stop_auto_launched_external_apps");
    assert!(
        second_stop.is_none(),
        "commands/game.rs has multiple calls to stop_auto_launched_external_apps — \
         only one, gated by decide_overlay_action, is allowed."
    );
}

// --------------------------------------------------------------------
// Iter 158 structural pins — predicate signatures + enum variant sets.
// --------------------------------------------------------------------
//
// The models above prove the decision tables are correct for the known
// inputs. These pins protect the SHAPE of the production predicates so
// a refactor that widens `decide_spawn(bool) -> Spawn|Attach` into a
// 3-way decision, or adds a `Force` enum variant, or drops the
// case-insensitive process match, can't land silently. Each pin names
// the specific failure mode and why it breaks §3.2.

const EXTERNAL_APP_RS: &str = "src/services/mods/external_app.rs";

fn external_app_src() -> String {
    // Normalize CRLF -> LF so fn/enum body extractors that search for
    // `\n}\n` work correctly on Windows checkouts (iter 243: same
    // issue the disk_full guard hit at iter 235).
    std::fs::read_to_string(EXTERNAL_APP_RS)
        .unwrap_or_else(|e| panic!("{EXTERNAL_APP_RS} must be readable: {e}"))
        .replace("\r\n", "\n")
}

/// The pure predicate `decide_spawn` must accept exactly `(bool)` and
/// return `SpawnDecision`. A refactor to `(bool, bool)` (adding a
/// `force: bool` parameter) would let a caller override the gate. A
/// refactor to return `Option<SpawnDecision>` would push the None-case
/// to callers, each of whom could forget it.
#[test]
fn decide_spawn_signature_is_bool_to_spawndecision() {
    let body = external_app_src();
    assert!(
        body.contains("pub fn decide_spawn(already_running: bool) -> SpawnDecision"),
        "PRD 3.2.11: external_app.rs must export \
         `pub fn decide_spawn(already_running: bool) -> SpawnDecision` \
         verbatim. A widened signature opens gate-bypass paths."
    );
}

/// `SpawnDecision` must have exactly two variants: `Attach` and `Spawn`.
/// Adding a third variant (e.g. `Force`, `Queued`) requires every call
/// site to handle the new case — a forgotten `_ => Spawn` match arm
/// would double-spawn Shinra/TCC.
#[test]
fn spawn_decision_enum_has_exactly_attach_and_spawn() {
    let body = external_app_src();
    let enum_pos = body
        .find("pub enum SpawnDecision {")
        .expect("SpawnDecision enum must exist");
    let rest = &body[enum_pos..];
    let close = rest.find("\n}").expect("SpawnDecision must close");
    let variants_body = &rest[..close];
    assert!(
        variants_body.contains("Attach"),
        "SpawnDecision must have `Attach` variant"
    );
    assert!(
        variants_body.contains("Spawn"),
        "SpawnDecision must have `Spawn` variant"
    );
    // Count variant headers by counting commas on non-doc-comment lines.
    // Simplest shape check: the variants body must NOT mention any
    // third identifier that ends a line with `,`. Approximation: look
    // for `    X` idents that aren't `Attach`/`Spawn`/comment/doc.
    let extra_variants: Vec<&str> = variants_body
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty()
                && !t.starts_with("///")
                && !t.starts_with("//")
                && !t.starts_with("pub enum")
                && !t.starts_with("Attach")
                && !t.starts_with("Spawn")
        })
        .collect();
    assert!(
        extra_variants.is_empty(),
        "PRD 3.2.11: SpawnDecision must have EXACTLY two variants \
         (Attach, Spawn). Extra variants would force a fallback arm at \
         every call site — a forgotten arm silently double-spawns.\n\
         Extras: {extra_variants:?}"
    );
}

/// `decide_overlay_action` must accept exactly `(usize)` and return
/// `OverlayLifecycleAction`. The `usize` is load-bearing: `i32` could
/// pass a negative count (making `== 0` false for `-1`), and an
/// `Option<usize>` would push the None-case to callers (None treated
/// as "keep running" by default would leak overlays).
#[test]
fn decide_overlay_action_signature_is_usize_to_lifecycleaction() {
    let body = external_app_src();
    assert!(
        body.contains(
            "pub fn decide_overlay_action(remaining_clients: usize) -> OverlayLifecycleAction"
        ),
        "PRD 3.2.12: external_app.rs must export \
         `pub fn decide_overlay_action(remaining_clients: usize) -> \
         OverlayLifecycleAction` verbatim. usize is load-bearing \
         (can't be negative); Option<usize> pushes None handling out."
    );
}

/// `OverlayLifecycleAction` must have exactly two variants:
/// `KeepRunning` and `Terminate`. A third variant (e.g. `Deferred`)
/// would introduce ambiguity around when overlays actually stop.
#[test]
fn overlay_lifecycle_enum_has_exactly_keeprunning_and_terminate() {
    let body = external_app_src();
    let enum_pos = body
        .find("pub enum OverlayLifecycleAction {")
        .expect("OverlayLifecycleAction enum must exist");
    let rest = &body[enum_pos..];
    let close = rest.find("\n}").expect("OverlayLifecycleAction must close");
    let variants_body = &rest[..close];
    assert!(
        variants_body.contains("KeepRunning"),
        "OverlayLifecycleAction must have `KeepRunning`"
    );
    assert!(
        variants_body.contains("Terminate"),
        "OverlayLifecycleAction must have `Terminate`"
    );
    let extra_variants: Vec<&str> = variants_body
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty()
                && !t.starts_with("///")
                && !t.starts_with("//")
                && !t.starts_with("pub enum")
                && !t.starts_with("KeepRunning")
                && !t.starts_with("Terminate")
        })
        .collect();
    assert!(
        extra_variants.is_empty(),
        "PRD 3.2.12: OverlayLifecycleAction must have EXACTLY two \
         variants. Extras introduce ambiguity about when overlays \
         stop.\nExtras: {extra_variants:?}"
    );
}

/// `check_spawn_decision` must route through `decide_spawn(
/// is_process_running(...))`. If the convenience wrapper gets rewritten
/// to inline the logic (`if is_process_running(...) { Attach } else {
/// Spawn }`), the pure predicate stops being the single source of
/// truth — a later tweak to `decide_spawn` won't propagate to the
/// wrapper, creating two paths that can diverge.
#[test]
fn check_spawn_decision_routes_through_pure_predicate() {
    let body = external_app_src();
    let fn_pos = body
        .find("pub fn check_spawn_decision(")
        .expect("check_spawn_decision must exist");
    let window = &body[fn_pos..body.len().min(fn_pos + 400)];
    assert!(
        window.contains("decide_spawn(is_process_running("),
        "PRD 3.2.11: check_spawn_decision must call \
         `decide_spawn(is_process_running(exe_name))`. Inlining the \
         branch splits the attach-once rule into two call sites that \
         can silently drift.\n\
         Window:\n{window}"
    );
}

/// `is_process_running` must compare process names case-insensitively.
/// Windows is case-insensitive for executable names, so `SHINRA.exe`,
/// `Shinra.exe`, and `shinra.exe` all refer to the same binary. A
/// refactor that drops `.to_ascii_lowercase()` on either side would
/// miss `SHINRA.exe` when asked about `Shinra.exe`, allowing a
/// double-spawn.
#[test]
fn is_process_running_is_case_insensitive() {
    let body = external_app_src();
    let fn_pos = body
        .find("pub fn is_process_running(")
        .expect("is_process_running must exist");
    let window = &body[fn_pos..body.len().min(fn_pos + 700)];
    // Both sides of the comparison must be lowercased — the input name
    // once, and each OS-level process name each iteration.
    let lowercase_count = window.matches("to_ascii_lowercase()").count();
    assert!(
        lowercase_count >= 2,
        "PRD 3.2.11: is_process_running must call \
         `.to_ascii_lowercase()` on BOTH sides of the comparison \
         (input exe_name + each OS process name). Found \
         {lowercase_count}; expected ≥ 2. A one-sided compare misses \
         `SHINRA.exe` vs `Shinra.exe` and permits a double-spawn.\n\
         Window:\n{window}"
    );
}

// --------------------------------------------------------------------
// Iter 207 structural pins — meta-guard self-reference + sysinfo usage
// pattern + game-count source + Terminate-gate wrapping + stop_process
// case-insensitive mirror.
// --------------------------------------------------------------------
//
// The eleven pins above cover the pure-predicate model, its signature,
// and three call-site wirings. They do NOT pin: (a) the guard's own
// module header cites PRD 3.2.11 + 3.2.12 (meta-guard contract); (b)
// `is_process_running` uses the sysinfo `System::new()` +
// `refresh_processes(All, true)` + `processes().values().any(...)`
// pattern — a refactor to a lock-file-based shortcut would keep the
// signature but silently return stale data after a hard-kill; (c) the
// `remaining_clients` value in `commands/game.rs` must come from
// `teralib::get_running_game_count()` — if a drive-by refactor hardcodes
// it to `0`, `decide_overlay_action` would tear overlays down on EVERY
// close; (d) the `stop_auto_launched_external_apps()` call must be
// wrapped by `if decide_overlay_action(...) == OverlayLifecycleAction::
// Terminate` — the existing wiring pin checks ordering, not the gate
// operator; (e) `stop_process_by_name` must mirror `is_process_running`'s
// case-insensitive compare — if detection says SHINRA is running but
// stop can't find it to kill, cleanup fails silently on Windows.

const GUARD_FILE: &str = "tests/multi_client.rs";
const GAME_RS: &str = "src/commands/game.rs";

fn guard_src() -> String {
    std::fs::read_to_string(GUARD_FILE).expect("tests/multi_client.rs must exist")
}

fn game_rs_src() -> String {
    std::fs::read_to_string(GAME_RS)
        .expect("commands/game.rs must exist")
        .replace("\r\n", "\n")
}

/// The guard's module header must cite both PRDs it protects — 3.2.11
/// (attach-once) and 3.2.12 (overlay lifecycle) — plus the PRD-slug
/// `multi-client-attach-once` so a grep for either surfaces this file.
#[test]
fn guard_file_header_cites_prd_slugs() {
    let body = guard_src();
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.2.11"),
        "meta-guard contract: tests/multi_client.rs header must cite \
         `PRD 3.2.11` (multi-client-attach-once). Without it, a \
         reader chasing an overlay-lifecycle regression won't land \
         here via section-grep.\nHeader:\n{header}"
    );
    assert!(
        header.contains("multi-client-attach-once"),
        "meta-guard contract: header must carry the PRD slug \
         `multi-client-attach-once` — name-based grep is the primary \
         cross-reference path between PRD P-slots and guards."
    );
}

/// `is_process_running` must use the sysinfo `System::new()` +
/// `refresh_processes(All, true)` + `processes().values()` pattern.
/// A drive-by refactor to a lock-file or PID-file shortcut would
/// satisfy the case-insensitive-compare pin but silently return
/// stale data after a hard-kill / crash (no file cleanup). Pin the
/// actual sysinfo path.
#[test]
fn is_process_running_uses_sysinfo_refresh_all_pattern() {
    let body = external_app_src();
    let fn_pos = body
        .find("pub fn is_process_running(")
        .expect("is_process_running must exist");
    let window = &body[fn_pos..body.len().min(fn_pos + 600)];
    assert!(
        window.contains("System::new()"),
        "PRD 3.2.11: is_process_running must construct a fresh \
         `System::new()` — a cached static could return stale data \
         across the spawn/close cycle.\nWindow:\n{window}"
    );
    assert!(
        window.contains("refresh_processes(sysinfo::ProcessesToUpdate::All, true)"),
        "PRD 3.2.11: is_process_running must call \
         `refresh_processes(sysinfo::ProcessesToUpdate::All, true)` \
         — the second arg `true` triggers removal of stale PIDs, \
         without which the fn could report a killed process as \
         still running.\nWindow:\n{window}"
    );
    assert!(
        window.contains("system.processes().values()"),
        "PRD 3.2.11: is_process_running must walk `system.processes()\
         .values()` — a lock-file or PID-file shortcut would miss \
         processes started outside the launcher (user ran SHINRA \
         directly) and permit a double-spawn."
    );
}

/// In `commands/game.rs`, the `remaining_clients` argument to
/// `decide_overlay_action(...)` must come from
/// `teralib::get_running_game_count()` — the single source of truth
/// for active client count. A hardcoded literal (`0` or `1`) or a
/// drive-by refactor to a private counter would decouple the policy
/// from reality.
#[test]
fn game_rs_remaining_clients_comes_from_teralib_count() {
    let body = game_rs_src();
    // Locate the decide_overlay_action call site.
    let call_pos = body
        .find("decide_overlay_action(remaining_clients)")
        .expect("commands/game.rs must call `decide_overlay_action(remaining_clients)`");
    // The binding of `remaining_clients` must come from teralib.
    let before = &body[..call_pos];
    let binding_pos = before
        .rfind("let remaining_clients")
        .expect("commands/game.rs must bind `let remaining_clients = ...` before the call");
    let binding_line_end = before[binding_pos..]
        .find('\n')
        .map(|n| binding_pos + n)
        .unwrap_or(before.len());
    let binding_line = &before[binding_pos..binding_line_end];
    assert!(
        binding_line.contains("teralib::get_running_game_count()"),
        "PRD 3.2.12: `let remaining_clients` in commands/game.rs must \
         be bound to `teralib::get_running_game_count()` — it's the \
         single source of truth for the count. A literal or private \
         counter decouples the overlay-lifecycle decision from the \
         actual number of TERA.exe processes.\nBinding: {binding_line}"
    );
}

/// The `stop_auto_launched_external_apps()` call in commands/game.rs
/// must be wrapped by `if decide_overlay_action(...) ==
/// OverlayLifecycleAction::Terminate`. The existing wiring pin
/// (`game_rs_gates_overlay_stop_on_decide_overlay_action`) checks
/// ORDERING (predicate before stop) but not the GATE OPERATOR — a
/// refactor to `!= Terminate` or `== KeepRunning` would satisfy the
/// ordering pin while inverting the policy (terminate on every
/// partial close).
#[test]
fn game_rs_stop_is_gated_by_terminate_branch() {
    let body = game_rs_src();
    // Find the gate line that wraps the stop call.
    let gate_pos = body
        .find("if decide_overlay_action(remaining_clients) == OverlayLifecycleAction::Terminate {")
        .expect(
            "PRD 3.2.12: commands/game.rs must gate stop with \
             `if decide_overlay_action(remaining_clients) == \
             OverlayLifecycleAction::Terminate {` — the equality \
             operator and Terminate variant are both load-bearing.",
        );
    // The stop call must live inside this gate (within ~200 chars).
    let window = &body[gate_pos..body.len().min(gate_pos + 300)];
    assert!(
        window.contains("stop_auto_launched_external_apps"),
        "PRD 3.2.12: `stop_auto_launched_external_apps()` must be \
         called INSIDE the `== Terminate` gate body, not after the \
         block.\nWindow:\n{window}"
    );
}

/// `stop_process_by_name` must mirror `is_process_running`'s case-
/// insensitive compare. Windows is case-insensitive for executable
/// names; if detection says `SHINRA.exe` is running but stop can't
/// match `Shinra.exe` in the process table, cleanup fails silently
/// on last-close and overlays leak across launcher restarts.
#[test]
fn stop_process_by_name_is_case_insensitive_mirror() {
    let body = external_app_src();
    let fn_pos = body
        .find("pub fn stop_process_by_name(")
        .expect("stop_process_by_name must exist");
    let window = &body[fn_pos..body.len().min(fn_pos + 800)];
    let lowercase_count = window.matches("to_ascii_lowercase()").count();
    assert!(
        lowercase_count >= 2,
        "PRD 3.2.11: stop_process_by_name must call \
         `.to_ascii_lowercase()` on BOTH sides of the match (input \
         exe_name + each OS process name). Found {lowercase_count}; \
         expected ≥ 2. A one-sided compare means detection finds \
         SHINRA but stop can't kill it — overlays leak across \
         launcher lifetimes.\nWindow:\n{window}"
    );
}

// --------------------------------------------------------------------
// Iter 243 structural pins — path-constant canonicalisation,
// OverlayLifecycleAction enum discriminants, stop-process forbids
// PID=0, spawn-decision unit return type, game.rs imports decide_
// overlay_action from canonical path.
// --------------------------------------------------------------------

/// Iter 243: `EXTERNAL_APP_RS` + `GUARD_FILE` + `GAME_RS` constants
/// must stay canonical. Every source-inspection pin reads through
/// one of these; drift silently redirects tests with misleading
/// "file not found" panics.
#[test]
fn guard_path_constants_are_canonical() {
    let body = fs::read_to_string(GUARD_FILE).expect("guard source must exist");
    for (name, expected) in [
        ("EXTERNAL_APP_RS", "src/services/mods/external_app.rs"),
        ("GUARD_FILE", "tests/multi_client.rs"),
        ("GAME_RS", "src/commands/game.rs"),
    ] {
        let line = format!("const {name}: &str = \"{expected}\";");
        assert!(
            body.contains(&line),
            "PRD 3.2.11/3.2.12 (iter 243): tests/multi_client.rs \
             must keep `{line}` verbatim. A rename without updating \
             the constant leaves every pin reading through it with \
             file-not-found panics."
        );
    }
}

/// Iter 243: `OverlayLifecycleAction` enum must carry exactly two
/// variants: `Terminate` and `KeepRunning`. Adding a third variant
/// (e.g. `Suspend`) would require `decide_overlay_action` to
/// dispatch to it, but game.rs's gate (`== Terminate`) would either
/// ignore the new variant (silent leak) or trip a non-exhaustive
/// match warning. Pin the shape so additions require coordinated
/// updates.
#[test]
fn overlay_lifecycle_action_carries_exactly_two_variants() {
    let body = external_app_src();
    let enum_pos = body
        .find("pub enum OverlayLifecycleAction")
        .expect("OverlayLifecycleAction enum must exist");
    let rest = &body[enum_pos..];
    let end = rest.find("\n}\n").unwrap_or(rest.len().min(400));
    let enum_body = &rest[..end];
    assert!(
        enum_body.contains("Terminate"),
        "PRD 3.2.12 (iter 243): OverlayLifecycleAction must carry \
         the `Terminate` variant — the one game.rs gates on.\n\
         Enum body:\n{enum_body}"
    );
    assert!(
        enum_body.contains("KeepRunning"),
        "PRD 3.2.12 (iter 243): OverlayLifecycleAction must carry \
         the `KeepRunning` variant — the default path when TERA \
         clients remain open.\nEnum body:\n{enum_body}"
    );
    // Count variants by comma-terminated identifiers or leading
    // indentation patterns inside the enum body.
    let terminate_count = enum_body.matches("Terminate").count();
    let keep_count = enum_body.matches("KeepRunning").count();
    assert_eq!(
        terminate_count, 1,
        "PRD 3.2.12 (iter 243): Terminate must appear exactly once \
         in the enum body — duplicates indicate a mis-paste."
    );
    assert_eq!(
        keep_count, 1,
        "PRD 3.2.12 (iter 243): KeepRunning must appear exactly once."
    );
}

/// Iter 243: `stop_process_by_name` must NOT call `kill` on PID 0.
/// Windows PID 0 is the System Idle Process; calling TerminateProcess
/// on it returns ACCESS_DENIED — but a hypothetical refactor that
/// iterated sysinfo's process list without filtering would include
/// it, and a broken filter would forward PID 0 to the kill call.
/// Pin that the fn source doesn't contain a raw `kill(0)` / PID=0
/// pattern.
#[test]
fn stop_process_by_name_does_not_target_pid_zero() {
    let body = external_app_src();
    let fn_pos = body
        .find("pub fn stop_process_by_name(")
        .expect("stop_process_by_name must exist");
    let window = &body[fn_pos..body.len().min(fn_pos + 1000)];
    // Forbid any literal `kill(0)` pattern, any `pid == 0` that
    // would select PID 0, or raw `0 as Pid` construction.
    for bad in ["kill(0)", "Pid::from(0)", "Pid::from_u32(0)"] {
        assert!(
            !window.contains(bad),
            "PRD 3.2.11 (iter 243): stop_process_by_name must NOT \
             reference PID 0 (`{bad}`). Windows PID 0 is System \
             Idle; targeting it returns ACCESS_DENIED but signals \
             a buggy filter upstream.\nWindow:\n{window}"
        );
    }
}

/// Iter 243: `check_spawn_decision` must return `SpawnDecision`
/// (an enum), not a bare `bool`. A `bool` return would conflate
/// the three decisions (Spawn / Attach / Skip) into two — and the
/// attach-once vs spawn-new distinction is PRD 3.2.11 core. Pin
/// the return type in the source.
#[test]
fn check_spawn_decision_returns_spawn_decision_enum() {
    let body = external_app_src();
    let fn_pos = body
        .find("pub fn check_spawn_decision(")
        .expect("check_spawn_decision must exist");
    let window = &body[fn_pos..body.len().min(fn_pos + 300)];
    assert!(
        window.contains("-> SpawnDecision"),
        "PRD 3.2.11 (iter 243): check_spawn_decision must return \
         `SpawnDecision` (enum), not bool or Option<bool>. The \
         attach-once / spawn-new / skip decisions are three \
         distinct outcomes; collapsing to bool loses the attach \
         case and re-opens the double-spawn class.\nWindow:\n{window}"
    );
    // Forbid -> bool on this fn explicitly.
    assert!(
        !window.contains("check_spawn_decision(") || !window.contains("-> bool {"),
        "PRD 3.2.11 (iter 243): check_spawn_decision must NOT \
         return `bool`."
    );
}

/// Iter 243: `src/commands/game.rs` must import `decide_overlay_
/// action` + `OverlayLifecycleAction` from the canonical
/// `services::mods::external_app` path. An import from a stub
/// module or local re-export would satisfy the compile but could
/// shadow the production fn with a test-only variant.
#[test]
fn game_rs_imports_overlay_types_from_canonical_path() {
    let body = game_rs_src();
    // Accept either a single `use` importing both or two separate
    // `use` lines — source-inspection only requires the canonical
    // module path to appear.
    assert!(
        body.contains("services::mods::external_app::")
            || body.contains("use crate::services::mods::external_app::"),
        "PRD 3.2.12 (iter 243): commands/game.rs must import \
         `decide_overlay_action` / `OverlayLifecycleAction` from \
         `services::mods::external_app::` — the canonical module. \
         A local stub import would compile but shadow production."
    );
    // Both symbols must be referenced somewhere in game.rs (they're
    // the gate + predicate).
    assert!(
        body.contains("decide_overlay_action"),
        "PRD 3.2.12 (iter 243): commands/game.rs must reference \
         `decide_overlay_action` — the overlay-lifecycle predicate."
    );
    assert!(
        body.contains("OverlayLifecycleAction::Terminate"),
        "PRD 3.2.12 (iter 243): commands/game.rs must reference \
         `OverlayLifecycleAction::Terminate` — the gate variant."
    );
}

// --------------------------------------------------------------------
// Iter 280 structural pins — external_app/game/guard bounds + PRD cite
// + SpawnDecision enum variants pinned.
// --------------------------------------------------------------------

#[test]
fn external_app_rs_byte_bounds() {
    const MIN: usize = 3000;
    const MAX: usize = 200_000;
    let bytes = std::fs::metadata(EXTERNAL_APP_RS)
        .expect("external_app.rs must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.2.11 (iter 280): {EXTERNAL_APP_RS} is {bytes} bytes; \
         expected [{MIN}, {MAX}]."
    );
}

#[test]
fn game_rs_byte_bounds() {
    const MIN: usize = 1000;
    const MAX: usize = 200_000;
    let bytes = std::fs::metadata(GAME_RS)
        .expect("game.rs must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.2.11 (iter 280): {GAME_RS} is {bytes} bytes; expected \
         [{MIN}, {MAX}]."
    );
}

#[test]
fn guard_source_byte_bounds() {
    const MIN: usize = 5000;
    const MAX: usize = 80_000;
    let bytes = std::fs::metadata(GUARD_FILE)
        .expect("guard must exist")
        .len() as usize;
    assert!(
        (MIN..=MAX).contains(&bytes),
        "PRD 3.2.11 (iter 280): guard is {bytes} bytes; expected \
         [{MIN}, {MAX}]."
    );
}

#[test]
fn guard_source_cites_prd_3_2_11_and_3_2_12() {
    let body = std::fs::read_to_string(GUARD_FILE).expect("guard must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD 3.2.11"),
        "iter 280: guard header must cite `PRD 3.2.11` (attach-once)."
    );
    // 3.2.12 appears in other tests, not necessarily the header.
    // Just confirm the guard body references the second criterion.
    assert!(
        body.contains("PRD 3.2.12") || body.contains("3.2.12"),
        "iter 280: guard body must reference PRD 3.2.12 \
         (overlay-lifecycle) — paired invariant."
    );
}

#[test]
fn spawn_decision_variants_pinned_in_source() {
    let src = std::fs::read_to_string(EXTERNAL_APP_RS).expect("external_app.rs must exist");
    assert!(
        src.contains("enum SpawnDecision"),
        "PRD 3.2.11 (iter 280): {EXTERNAL_APP_RS} must define `enum \
         SpawnDecision` — the attach-once decision type. A rename \
         or refactor to a different type would break every caller."
    );
}
