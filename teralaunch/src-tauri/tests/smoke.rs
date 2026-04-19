//! Smoke test harness for `teralaunch/src-tauri`.
//!
//! Proves the integration-test directory compiles and runs. Every P0/P1
//! integration test authored under `docs/PRD/fix-plan.md` lives alongside this
//! file. Shared fixtures go in `tests/common/mod.rs`.

mod common;

#[test]
fn smoke_runs() {
    assert_eq!(common::two_plus_two(), 4);
}

#[test]
fn tempdir_fixture_works() {
    let dir = common::scratch_dir();
    assert!(dir.path().exists());
}

// --------------------------------------------------------------------
// Iter 166 structural pins — test-harness contract.
// --------------------------------------------------------------------
//
// smoke.rs was a thin "compile-runs" marker. These pins defend the
// harness itself: the integration-test file-count floor, the single
// `common/` submodule, the common fixture exports, the Cargo.toml
// bin-crate shape, and the tempfile dev-dep that common/mod.rs
// depends on. A refactor that deletes integration tests wholesale,
// adds stray test submodules, flips the crate from bin to lib, or
// drops the tempfile dep would pass every individual integration
// test while silently eroding the harness.

use std::fs;
use std::path::PathBuf;

const TESTS_DIR: &str = "tests";
const COMMON_MOD_RS: &str = "tests/common/mod.rs";
const CARGO_TOML: &str = "Cargo.toml";

/// The integration-test directory must carry at least this many
/// top-level `*.rs` files. Set to 30 as of iter 166 (we have 36);
/// dropping below the floor catches an accidental wholesale deletion
/// of integration tests (e.g. a merge that drops the `tests/` dir
/// into an older state).
const INTEGRATION_TESTS_FLOOR: usize = 30;

fn integration_test_files() -> Vec<PathBuf> {
    let dir = PathBuf::from(TESTS_DIR);
    assert!(
        dir.is_dir(),
        "tests/ must exist at {TESTS_DIR} relative to src-tauri/"
    );
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir).expect("read tests/") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// Floor the integration-test count. A silent regression below the
/// floor (from a bad rebase, a squash-merge of the wrong branch, or
/// a sweeping delete) would leave the harness running, this smoke
/// test passing, and large swaths of coverage just… gone.
#[test]
fn integration_tests_dir_meets_minimum_file_count() {
    let files = integration_test_files();
    assert!(
        files.len() >= INTEGRATION_TESTS_FLOOR,
        "tests/ must carry at least {INTEGRATION_TESTS_FLOOR} \
         top-level `.rs` files; got {count}. If this fires, a bulk \
         deletion or rebase error likely removed integration tests \
         — check `git log --diff-filter=D -- tests/`.",
        count = files.len()
    );
}

/// The only subdirectory under `tests/` must be `common/`. Cargo's
/// integration-test discovery treats every `tests/*.rs` as its own
/// binary, but subdirectories are included as modules from sibling
/// tests via `mod <name>;`. An unexpected subdir suggests someone
/// tried to share state between tests in a way that doesn't compile
/// on every binary.
#[test]
fn tests_dir_has_only_the_common_submodule() {
    let dir = PathBuf::from(TESTS_DIR);
    let mut subdirs: Vec<String> = Vec::new();
    for entry in fs::read_dir(&dir).expect("read tests/") {
        let entry = entry.expect("dir entry");
        if entry
            .file_type()
            .expect("file type")
            .is_dir()
        {
            subdirs.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    subdirs.sort();
    // `common/` — shared test fixture module (imported via `mod common;`).
    // `fixtures/` — read-only static data read by fs::read_to_string in tests
    //   (catalog-snapshot.json landed iter 229 for §3.3.1).
    let allowed = ["common".to_string(), "fixtures".to_string()];
    for subdir in &subdirs {
        assert!(
            allowed.contains(subdir),
            "tests/ must contain only allowed subdirectories \
             ({allowed:?}). Extra subdirs suggest an attempt to share \
             state that won't compile across every integration-test \
             binary. Got unexpected: `{subdir}`."
        );
    }
}

/// `tests/common/mod.rs` must export both fixture helpers. Every
/// integration test that uses `mod common;` imports against these
/// symbols; silently dropping either would break compilation (which
/// the test suite catches), but a rename that preserves the symbol
/// count could slip past without a smoke-level pin.
#[test]
fn common_module_exports_expected_fixtures() {
    let body = fs::read_to_string(COMMON_MOD_RS).expect("tests/common/mod.rs must exist");
    assert!(
        body.contains("pub fn two_plus_two() -> i32"),
        "tests/common/mod.rs must export \
         `pub fn two_plus_two() -> i32` — smoke.rs's baseline \
         compile-runs check depends on it."
    );
    assert!(
        body.contains("pub fn scratch_dir() -> TempDir"),
        "tests/common/mod.rs must export \
         `pub fn scratch_dir() -> TempDir` — every test that needs \
         a transient directory uses this fixture."
    );
}

/// `Cargo.toml` must keep the crate as a `bin` (name preserved),
/// NOT a `lib`. Integration tests under `tests/` are only
/// auto-discovered for bin crates; switching to a library changes
/// the whole discovery model and every `#[test]` in `tests/` would
/// silently stop running.
#[test]
fn cargo_toml_declares_expected_bin_crate() {
    let toml = fs::read_to_string(CARGO_TOML).expect("Cargo.toml must exist");
    assert!(
        toml.contains(r#"name = "tera-europe-classicplus-launcher""#),
        "Cargo.toml must keep `name = \"tera-europe-classicplus-launcher\"` \
         — the installer, updater signatures, and release pipeline all \
         depend on this name."
    );
    // No top-level `[lib]` stanza (would toggle the crate type and
    // invalidate the integration-test layout). A [lib.name] inside
    // a [[bench]] or similar won't collide because we anchor on the
    // bare `[lib]` header.
    assert!(
        !toml.contains("\n[lib]\n") && !toml.starts_with("[lib]\n"),
        "Cargo.toml must NOT declare a `[lib]` stanza — the crate \
         is a bin and the integration-test discovery model depends \
         on that."
    );
}

/// `tempfile` must stay in `[dev-dependencies]` — it's what
/// `tests/common/mod.rs::scratch_dir` wraps, and several other
/// integration tests (crash_recovery, disk_full, self_integrity)
/// call `TempDir::new()` directly. Dropping the dev-dep would
/// break compilation of the entire integration-test suite.
#[test]
fn tempfile_is_declared_in_dev_dependencies() {
    let toml = fs::read_to_string(CARGO_TOML)
        .expect("Cargo.toml must exist")
        .replace("\r\n", "\n");
    // Locate `[dev-dependencies]` and verify `tempfile` appears after
    // it (before the next `[` section header).
    let dev_pos = toml
        .find("\n[dev-dependencies]\n")
        .expect("Cargo.toml must carry a [dev-dependencies] section");
    let rest = &toml[dev_pos + 1..];
    // Bound the section to the next top-level `[` header (or end-of-file).
    let section_end = rest[20..]
        .find("\n[")
        .map(|p| 20 + p)
        .unwrap_or(rest.len());
    let section = &rest[..section_end];
    assert!(
        section.contains("tempfile"),
        "Cargo.toml [dev-dependencies] must declare `tempfile` — \
         integration tests under tests/ (common fixture, \
         crash_recovery, disk_full, self_integrity) depend on it.\n\
         Section:\n{section}"
    );
}

// --------------------------------------------------------------------
// Iter 192 structural pins — deeper test-harness contract.
// --------------------------------------------------------------------

/// Floor on the `*_guard.rs` subset of integration tests. This is the
/// structural-pin backbone of the perfection loop; falling below this
/// floor means multiple drift-guards were deleted wholesale.
const GUARD_FILES_FLOOR: usize = 15;

/// Iter 192: the structural-guard subset (files ending `_guard.rs`)
/// must meet its own floor, separate from the broader integration-
/// test count. A regression that deletes all `*_guard.rs` while
/// leaving behavioural tests intact would pass the iter-166 floor
/// but silence every doc-layer / config-layer drift detector.
#[test]
fn integration_tests_carry_expected_guard_file_subset_count() {
    let files = integration_test_files();
    let guards: Vec<_> = files
        .iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with("_guard.rs"))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        guards.len() >= GUARD_FILES_FLOOR,
        "tests/ must carry at least {GUARD_FILES_FLOOR} `*_guard.rs` \
         files; got {count}. This subset is the structural-pin \
         backbone; below the floor means drift-guards were wiped. \
         Current guards: {guards:?}",
        count = guards.len()
    );
}

/// Iter 192: `tests/common/mod.rs` must NOT reference `crate::` — in
/// an integration-test binary, `crate` is the test binary itself, not
/// the Tauri bin-crate's `lib`. A `use crate::services::...` would
/// fail to compile on every test binary that `mod common;`-s it. The
/// accidental inclusion is the classic "I wrote a helper that pulls
/// from the lib, it works in src/, why does CI fail?" story.
#[test]
fn common_mod_rs_does_not_use_crate_sources() {
    let body = fs::read_to_string(COMMON_MOD_RS).expect("tests/common/mod.rs must exist");
    assert!(
        !body.contains("use crate::") && !body.contains("crate::"),
        "tests/common/mod.rs must not reference `crate::` symbols. \
         In an integration-test binary, `crate` is the binary itself, \
         not the Tauri bin-crate's lib. Pulling from the lib requires \
         naming the crate by its package name \
         (`tera_europe_classicplus_launcher::...`) — or better, \
         keep common fixtures dependency-free."
    );
}

/// Iter 192: no stray `Cargo.toml` or `target/` under `tests/` — such
/// leftovers break Cargo's integration-test discovery silently
/// (either because a nested package confuses the resolver or because
/// a stray build artefact shadows a real test). A one-off copy of
/// some example crate into `tests/` could trigger this.
#[test]
fn tests_dir_has_no_stray_cargo_toml_or_target_dir() {
    let dir = PathBuf::from(TESTS_DIR);
    for entry in fs::read_dir(&dir).expect("read tests/") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "Cargo.toml" || name == "Cargo.lock" || name == "target" {
            panic!(
                "tests/ contains a stray `{name}` which would break \
                 Cargo's integration-test discovery. Remove it."
            );
        }
    }
}

/// Iter 192: every `tests/*.rs` must carry at least one `#[test]`
/// or `#[tokio::test]` attribute. A file that defines no tests is a
/// silent regression — it still compiles and counts toward the
/// file-count floor (iter 166) but contributes zero assertions to
/// the suite. Most likely cause: an accidental whole-file comment
/// during a merge.
#[test]
fn every_integration_test_file_carries_test_functions() {
    let files = integration_test_files();
    let mut empties = Vec::new();
    for path in &files {
        let body = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let has_test = body.contains("#[test]")
            || body.contains("#[tokio::test]")
            || body.contains("#[rstest]");
        if !has_test {
            empties.push(path.display().to_string());
        }
    }
    assert!(
        empties.is_empty(),
        "tests/ contains integration-test file(s) with zero \
         `#[test]` / `#[tokio::test]` / `#[rstest]` attributes — \
         they compile but contribute no assertions. Files: \
         {empties:?}"
    );
}

/// Iter 192: smoke.rs itself must self-identify as the harness
/// contract in its module header. Future maintainers debugging a
/// test failure in this file need to understand that it's not a
/// behavioural test — it pins the test infrastructure that every
/// other test depends on. Without the citation, a reviewer might
/// "simplify" this file and remove the harness pins.
#[test]
fn smoke_guard_file_self_identifies_as_harness_contract() {
    let body = fs::read_to_string("tests/smoke.rs").expect("smoke.rs must exist");
    // Header must cite the harness concept.
    let header = &body[..body.len().min(2000)];
    assert!(
        header.contains("harness"),
        "smoke.rs header must cite `harness` so future readers \
         understand this file pins the test infrastructure, not \
         behaviour."
    );
    // And must reference the iter-166 landmark where the pins were
    // introduced.
    assert!(
        body.contains("Iter 166") || body.contains("iter 166"),
        "smoke.rs must reference `iter 166` (the iteration that \
         introduced the structural-pin contract) so the history is \
         discoverable via grep."
    );
}

// --------------------------------------------------------------------
// Iter 221 structural pins — both-landmark header + path/threshold
// constants verbatim + common/mod.rs tempfile import + Cargo no-bin-
// stanza + integration_test_files sort call.
// --------------------------------------------------------------------
//
// The twelve pins above cover file-count floor + sole common/ subdir
// + fixture exports + bin-crate shape + tempfile dev-dep + guard-
// file-subset floor + crate-reference absence + no stray Cargo/target
// + per-file test-fn presence + self-identification. They do NOT pin:
// (a) the header cites BOTH `iter 166` AND `iter 192` landmarks — the
// iter-192 pin accepts either/or, letting one citation go stale; (b)
// every path constant + threshold constant equals its canonical
// verbatim value — a silent lowering of INTEGRATION_TESTS_FLOOR or
// GUARD_FILES_FLOOR would vacate those pins; (c) `tests/common/mod.rs`
// imports `tempfile::TempDir` (the positive pin complements the dev-
// dep check — dropping the `use` would break compilation, but a
// switch to a custom wrapper of `std::env::temp_dir()` would still
// compile while losing the auto-cleanup invariant); (d) `Cargo.toml`
// has NO explicit `[[bin]]` stanza — the default `src/main.rs` is
// the canonical entry point; an explicit `[[bin]]` adds another
// name that could silently diverge; (e) `integration_test_files`
// helper calls `.sort()` on its output — without sort, failure
// messages across runs would list files in filesystem order (non-
// deterministic), frustrating debugging.

/// The header must cite BOTH `iter 166` AND `iter 192` landmarks.
/// The iter-192 pin accepts either/or, but the history spans two
/// iterations and grep should find either landmark equally easily.
#[test]
fn header_cites_both_iter_166_and_iter_192_landmarks() {
    let body = fs::read_to_string("tests/smoke.rs").expect("smoke.rs must exist");
    assert!(
        body.contains("Iter 166") || body.contains("iter 166"),
        "harness (iter 221): smoke.rs must cite `iter 166` (harness-\
         contract introduction)."
    );
    assert!(
        body.contains("Iter 192") || body.contains("iter 192"),
        "harness (iter 221): smoke.rs must cite `iter 192` (deeper \
         harness-contract extension). The iter-192 `smoke_guard_file_\
         self_identifies_as_harness_contract` pin accepts iter 166 \
         OR iter 192; this pin requires both so neither landmark can \
         go stale."
    );
}

/// All three path constants + both threshold constants must equal
/// their canonical verbatim values. A silent lowering of either
/// floor to 0 would vacate the count pins; a rename of any path
/// constant would cause opaque "file not readable" panics.
#[test]
fn path_and_threshold_constants_are_canonical() {
    let body = fs::read_to_string("tests/smoke.rs").expect("smoke.rs must exist");
    for literal in [
        "const TESTS_DIR: &str = \"tests\";",
        "const COMMON_MOD_RS: &str = \"tests/common/mod.rs\";",
        "const CARGO_TOML: &str = \"Cargo.toml\";",
        "const INTEGRATION_TESTS_FLOOR: usize = 30;",
        "const GUARD_FILES_FLOOR: usize = 15;",
    ] {
        assert!(
            body.contains(literal),
            "harness (iter 221): tests/smoke.rs must retain \
             `{literal}` verbatim. A rename of any path constant or \
             a silent lowering of either floor (to 0) would vacate \
             the corresponding count pin."
        );
    }
}

/// `tests/common/mod.rs` must import `tempfile::TempDir` directly.
/// The iter-166 dev-dep pin verifies `tempfile` is in
/// `[dev-dependencies]`, but a refactor that swaps the import for a
/// custom wrapper of `std::env::temp_dir()` would still compile
/// (since tempfile is still a dev-dep from other tests using it)
/// while losing the auto-cleanup invariant that TempDir provides.
#[test]
fn common_mod_rs_uses_tempfile_crate_directly() {
    let body = fs::read_to_string(COMMON_MOD_RS)
        .expect("tests/common/mod.rs must exist");
    assert!(
        body.contains("use tempfile::TempDir;"),
        "harness (iter 221): tests/common/mod.rs must import \
         `use tempfile::TempDir;`. A switch to `std::env::temp_dir()` \
         or a custom wrapper would compile (tempfile stays a dev-dep \
         from other tests) but lose TempDir's auto-cleanup-on-drop \
         invariant that many tests rely on.\n\
         Body:\n{body}"
    );
    assert!(
        body.contains("tempfile::tempdir()"),
        "harness (iter 221): tests/common/mod.rs must call \
         `tempfile::tempdir()` in the scratch_dir fixture — a \
         refactor that constructs TempDir some other way (e.g. \
         `TempDir::new_in(...)` with a fixed base) would change \
         the isolation invariant every test depends on."
    );
}

/// `Cargo.toml` must NOT declare an explicit `[[bin]]` stanza. The
/// crate uses Cargo's default `src/main.rs`-as-bin convention. An
/// explicit `[[bin]]` with a different name / path would add a
/// second binary target that could silently diverge from the one
/// the installer + updater sign.
#[test]
fn cargo_toml_has_no_explicit_bin_stanza() {
    let toml = fs::read_to_string(CARGO_TOML).expect("Cargo.toml must exist");
    assert!(
        !toml.contains("\n[[bin]]\n"),
        "harness (iter 221): Cargo.toml must NOT declare an explicit \
         `[[bin]]` stanza. The crate uses the default `src/main.rs` \
         convention; an explicit stanza could add a second binary \
         with a diverging name that the installer / updater \
         signatures don't cover."
    );
    assert!(
        !toml.starts_with("[[bin]]\n"),
        "harness (iter 221): Cargo.toml must NOT start with `[[bin]]` \
         either — same rationale."
    );
}

/// The `integration_test_files` helper must call `.sort()` on its
/// output. Without a stable sort, failure messages would list files
/// in filesystem-dir-entry order (non-deterministic on some
/// platforms), making it hard to compare results across runs.
#[test]
fn integration_test_files_helper_sorts_output() {
    let body = fs::read_to_string("tests/smoke.rs").expect("smoke.rs must exist");
    // Locate the helper fn body.
    let fn_pos = body
        .find("fn integration_test_files() -> Vec<PathBuf>")
        .expect("integration_test_files helper must exist");
    // Window covers the body.
    let window = &body[fn_pos..body.len().min(fn_pos + 600)];
    assert!(
        window.contains("out.sort();"),
        "harness (iter 221): `integration_test_files` must call \
         `out.sort();` before returning. Without a stable sort, \
         failure messages across runs would list files in \
         filesystem-dir-entry order (non-deterministic), making \
         regressions hard to reproduce.\nWindow:\n{window}"
    );
}
