//! PRD §3 measurement-path drift guard (iter 97).
//!
//! The PRD's §3 "Measurement path" column is ship-documentation: when
//! a future contributor scans the PRD to verify a criterion is pinned,
//! those paths must point at actually-existing tests. Over iters 61-95
//! several tests moved from `tests/foo.rs` to inline `src/services/
//! mods/foo.rs::tests` (bin-crate tests can't import library types,
//! so many "integration" tests migrated inline). The PRD was not
//! updated at the time and drifted.
//!
//! Iter 97 fixed 4 drifted paths (3.1.2, 3.1.4, 3.2.2, 3.2.4). This
//! guard pins those fixes so they stay in sync with the source tree:
//! for each (PRD row, expected source file, expected test fn name)
//! tuple, we assert both the PRD cites the expected path AND the file
//! actually contains a `#[test] fn <name>` item.
//!
//! Scope note: this covers the four criteria whose tests migrated
//! inline. Many other §3 rows cite tests that don't exist YET (unshipped
//! functionality) — those are PRD-as-spec, not drift. We deliberately
//! don't try to parse the full table; a curated list of known-shipped
//! invariants is enough to catch future rename-without-update drift.

use std::fs;

const PRD_PATH: &str = "../../docs/PRD/mod-manager-perfection.md";

struct Pin {
    /// Criterion label like "3.1.2" — must appear verbatim in the PRD row.
    criterion: &'static str,
    /// Path to the source file (relative to `src-tauri/`) that the PRD
    /// cell must mention and that must exist on disk.
    source_path: &'static str,
    /// Name of the `#[test]` function that must be grep-findable in the
    /// source file. The PRD cell must also name it.
    test_name: &'static str,
}

const PINS: &[Pin] = &[
    // --- Security (§3.1) ------------------------------------------------
    Pin {
        criterion: "3.1.1",
        source_path: "src/services/mods/external_app.rs",
        test_name: "sha_mismatch_aborts_before_write",
    },
    Pin {
        criterion: "3.1.2",
        source_path: "src/services/mods/external_app.rs",
        test_name: "sha_mismatch_aborts_before_write_gpk",
    },
    Pin {
        criterion: "3.1.3",
        source_path: "src/services/mods/external_app.rs",
        test_name: "extract_zip_rejects_zip_slip",
    },
    Pin {
        criterion: "3.1.4",
        source_path: "src/services/mods/tmm.rs",
        test_name: "deploy_path_clamped_inside_game_root",
    },
    Pin {
        criterion: "3.1.5",
        source_path: "tests/http_allowlist.rs",
        test_name: "every_mod_url_on_allowlist",
    },
    Pin {
        criterion: "3.1.5",
        source_path: "tests/http_redirect_offlist.rs",
        test_name: "external_app_download_client_disables_redirects",
    },
    Pin {
        criterion: "3.1.7",
        source_path: "tests/zeroize_audit.rs",
        test_name: "zeroize_derives_compose_with_skip_attribute",
    },
    Pin {
        criterion: "3.1.9",
        source_path: "tests/updater_downgrade.rs",
        test_name: "refuses_older_latest_json",
    },
    Pin {
        criterion: "3.1.11",
        source_path: "tests/self_integrity.rs",
        test_name: "detects_tampered_exe",
    },
    Pin {
        criterion: "3.1.12",
        source_path: "tests/csp_audit.rs",
        test_name: "csp_denies_inline_scripts",
    },
    // --- Reliability (§3.2) ---------------------------------------------
    Pin {
        criterion: "3.2.2",
        source_path: "src/services/mods/registry.rs",
        test_name: "mid_install_sigkill_recovers_to_error",
    },
    Pin {
        criterion: "3.2.2",
        source_path: "src/services/mods/registry.rs",
        test_name: "recover_stuck_installs_flips_installing_to_error",
    },
    Pin {
        criterion: "3.2.6",
        source_path: "src/services/mods/catalog.rs",
        test_name: "malformed_entries_filtered",
    },
    Pin {
        criterion: "3.2.3",
        source_path: "src/services/mods/tmm.rs",
        test_name: "clean_backup_not_overwritten_on_second_install",
    },
    Pin {
        criterion: "3.2.3",
        source_path: "src/services/mods/tmm.rs",
        test_name: "golden_cipher_encrypt_zeros_16",
    },
    Pin {
        criterion: "3.1.3",
        source_path: "src/services/mods/external_app.rs",
        test_name: "golden_extract_multi_entry_tree",
    },
    Pin {
        criterion: "3.2.10",
        source_path: "src/services/mods/tmm.rs",
        test_name: "golden_v1_fixture_parses_to_expected_modfile",
    },
    Pin {
        criterion: "3.2.10",
        source_path: "src/services/mods/tmm.rs",
        test_name: "parse_mod_file_rejects_non_tmm_gpks",
    },
    Pin {
        criterion: "3.2.10",
        source_path: "tests/bogus_gpk_footer.rs",
        test_name: "parse_mod_file_retains_magic_check_fallback",
    },
    Pin {
        criterion: "3.2.4",
        source_path: "src/services/mods/tmm.rs",
        test_name: "uninstall_all_restores_vanilla_bytes",
    },
    Pin {
        criterion: "3.2.7",
        source_path: "tests/parallel_install.rs",
        test_name: "same_id_serialised",
    },
    Pin {
        criterion: "3.2.7",
        source_path: "src/services/mods/registry.rs",
        test_name: "same_id_serialised_second_claim_refused",
    },
    Pin {
        criterion: "3.2.8",
        source_path: "tests/disk_full.rs",
        test_name: "revert_on_enospc",
    },
    Pin {
        criterion: "3.2.9",
        source_path: "src/services/mods/tmm.rs",
        test_name: "clean_recovery_logic_creates_backup_from_vanilla_current",
    },
    Pin {
        criterion: "3.2.9",
        source_path: "src/services/mods/tmm.rs",
        test_name: "clean_recovery_logic_refuses_when_current_is_modded",
    },
    Pin {
        criterion: "3.2.9",
        source_path: "tests/clean_recovery.rs",
        test_name: "recover_clean_mapper_is_a_tauri_command_and_delegates_to_tmm",
    },
    Pin {
        criterion: "3.2.11",
        source_path: "tests/multi_client.rs",
        test_name: "second_client_no_duplicate_spawn",
    },
    Pin {
        criterion: "3.2.12",
        source_path: "tests/multi_client.rs",
        test_name: "partial_close_keeps_overlays",
    },
    Pin {
        criterion: "3.2.13",
        source_path: "tests/multi_client.rs",
        test_name: "last_close_terminates_overlays",
    },
    // --- Functionality (§3.3) -------------------------------------------
    Pin {
        criterion: "3.3.2",
        source_path: "src/services/mods/tmm.rs",
        test_name: "per_object_merge_both_apply",
    },
    Pin {
        criterion: "3.3.2",
        source_path: "src/services/mods/tmm.rs",
        test_name: "golden_merger_commutes_on_disjoint_slots",
    },
    Pin {
        criterion: "3.3.2",
        source_path: "src/services/mods/tmm.rs",
        test_name: "golden_merger_three_disjoint_mods_all_orders_agree",
    },
    Pin {
        criterion: "3.3.3",
        source_path: "src/services/mods/tmm.rs",
        test_name: "detect_conflicts_flags_other_mod_owning_slot",
    },
    Pin {
        criterion: "3.3.3",
        source_path: "src/services/mods/tmm.rs",
        test_name: "golden_merger_last_install_wins_on_overlap",
    },
    Pin {
        criterion: "3.3.3",
        source_path: "tests/conflict_modal.rs",
        test_name: "preview_mod_install_conflicts_is_a_tauri_command_and_delegates_to_tmm",
    },
    Pin {
        criterion: "3.3.12",
        source_path: "src/commands/mods.rs",
        test_name: "fresh_install_defaults_enabled",
    },
    Pin {
        criterion: "3.3.15",
        source_path: "src/commands/mods.rs",
        test_name: "toggle_intent_only",
    },
    Pin {
        criterion: "3.3.15",
        source_path: "src/commands/mods.rs",
        test_name: "toggle_disable_intent_only",
    },
    Pin {
        criterion: "3.3.15",
        source_path: "src/commands/mods.rs",
        test_name: "toggle_command_bodies_do_not_spawn_or_kill",
    },
];

/// JS-side pins (iter 132): PRD §3 also cites Vitest tests for
/// several criteria where the measurement genuinely lives in JS
/// (i18n / perf — running those in Rust would need a JS engine).
/// Iters 124-131 added Rust structural guards that pin the JS
/// scanners' INVARIANTS, but the primary measurement cited in the
/// PRD is still the JS test. This table pins the PRD-to-JS mapping
/// so a rename of either side trips this guard.
struct JsPin {
    criterion: &'static str,
    /// src-tauri/-relative path — typically `../tests/foo.test.js`.
    js_path: &'static str,
    /// The `it('<name>', ...)` identifier. Must appear in the JS file
    /// AND the PRD cell must cite `teralaunch/tests/<file>::<name>`.
    test_name: &'static str,
}

const JS_PINS: &[JsPin] = &[
    JsPin {
        criterion: "3.4.7",
        js_path: "../tests/i18n-jargon.test.js",
        test_name: "no_jargon_in_translations",
    },
    JsPin {
        criterion: "3.6.4",
        js_path: "../tests/search-perf.test.js",
        test_name: "under_one_frame",
    },
    JsPin {
        criterion: "3.7.1",
        js_path: "../tests/i18n-parity.test.js",
        test_name: "keys_equal_across_locales",
    },
    JsPin {
        // 3.7.4's `it()` name is a full English sentence (Vitest
        // idiom tolerates it). The drift-guard pin + PRD cell
        // format matches exactly.
        criterion: "3.7.4",
        js_path: "../tests/i18n-no-hardcoded.test.js",
        test_name: "no new hardcoded English outside the allowlist",
    },
    JsPin {
        // 3.3.4's citation points at a Playwright spec (not Vitest).
        // The needle check below matches both `it(` and `test(` so
        // both idioms are covered. Among the PRD's many e2e spec
        // citations, this is the only file that currently exists —
        // the rest are PRD-as-spec forward declarations.
        criterion: "3.3.4",
        js_path: "../tests/e2e/mod-import-file.spec.js",
        test_name: "user_imported_gpk_deploys",
    },
];

fn js_cell_for(pin: &JsPin) -> String {
    // src-tauri/-relative `../tests/foo.test.js` maps to the PRD's
    // repo-root form `teralaunch/tests/foo.test.js`.
    let suffix = pin.js_path.strip_prefix("../").unwrap_or(pin.js_path);
    format!("teralaunch/{suffix}::{}", pin.test_name)
}

/// The PRD cell naming convention uses `::` as a module path separator.
/// Inline-in-src pins are `file.rs::tests::name` (because the `#[test]`
/// fns live inside a `#[cfg(test)] mod tests { ... }`). Integration-test
/// pins are `tests/file.rs::name` — bin-crate integration tests put the
/// `#[test]` fns at file root, so there's no `tests::` module in the path.
/// Callers that visually scan for these must not have the shape drift.
fn cell_for(pin: &Pin) -> String {
    let normalised = pin.source_path.replace('\\', "/");
    let module_prefix = if normalised.starts_with("tests/") {
        // bin-crate integration test — fn at file root
        ""
    } else {
        // inline `#[cfg(test)] mod tests { ... }`
        "::tests"
    };
    format!(
        "teralaunch/src-tauri/{normalised}{module_prefix}::{}",
        pin.test_name
    )
}

fn prd_body() -> String {
    fs::read_to_string(PRD_PATH).expect("mod-manager-perfection.md must be readable from src-tauri/")
}

fn source_body(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|_| panic!("{path} must exist"))
}

/// Each pin's source file must exist and contain `fn <test_name>`
/// inside a `#[test]`-annotated item.
#[test]
fn every_pin_source_file_has_named_test() {
    for pin in PINS {
        let body = source_body(pin.source_path);
        let needle = format!("fn {}", pin.test_name);
        assert!(
            body.contains(&needle),
            "{} says §{} is pinned by {}::{}, but `fn {}` was not \
             found in {}. Either the test was renamed without updating \
             the PRD, or the PRD cell was edited without the underlying \
             test being created.",
            PRD_PATH,
            pin.criterion,
            pin.source_path,
            pin.test_name,
            pin.test_name,
            pin.source_path,
        );
    }
}

/// For each pin, the PRD row for that criterion must mention the
/// source path and test name. If the PRD drifts again (e.g. someone
/// edits the cell to cite a different file), this catches it.
#[test]
fn every_pin_is_cited_in_prd_row() {
    let prd = prd_body();
    for pin in PINS {
        // Find the PRD row: the criterion label appears at start-of-cell
        // (after the leading "| "). We search for the row by the exact
        // "| {criterion} |" prefix.
        let row_marker = format!("| {} |", pin.criterion);
        let row_start = prd.find(&row_marker).unwrap_or_else(|| {
            panic!(
                "PRD row for criterion {} not found — PRD table shape \
                 may have changed",
                pin.criterion,
            )
        });
        // Row ends at the next newline.
        let row_end = prd[row_start..]
            .find('\n')
            .map(|i| row_start + i)
            .unwrap_or(prd.len());
        let row = &prd[row_start..row_end];

        let cell = cell_for(pin);
        assert!(
            row.contains(&cell),
            "PRD row for §{} must cite the measurement path \"{}\", but \
             its actual text is:\n  {}\n",
            pin.criterion,
            cell,
            row,
        );
    }
}

/// Regression guard: the four iter-97-fixed paths must NOT revert to
/// the old (non-existent) paths.
#[test]
fn iter_97_fixed_paths_do_not_regress() {
    let prd = prd_body();
    let stale_paths = [
        "teralaunch/src-tauri/tests/gpk_install_hash.rs",
        "teralaunch/src-tauri/tests/gpk_deploy_sandbox.rs",
        "teralaunch/src-tauri/tests/full_cycle.rs",
        // crash_recovery.rs DOES exist (JSON contract + filesystem
        // retry pins); it was the `::tests::` suffix after the
        // integration-test filename that was nonsensical. The fixed
        // cell is the behavioural inline-in-registry form. If anyone
        // re-adds `crash_recovery.rs::tests::` (note: Rust bin-crate
        // integration tests put fns at file root, not in `tests::`)
        // that's the stale shape.
        "crash_recovery.rs::tests::mid_install_sigkill_recovers_to_error",
        // iter 103 — §3.2.6 cited a non-existent frontend test file;
        // actual test lives inline in src/services/mods/catalog.rs.
        "teralaunch/tests/catalog-parse.test.js",
    ];
    for stale in &stale_paths {
        assert!(
            !prd.contains(stale),
            "PRD §3 contains stale path \"{stale}\" that iter 97 \
             fixed — do not regress. The actual test location lives \
             inline in a `src/services/mods/*.rs::tests::*` module."
        );
    }
}

/// Each JS pin's source file must exist and contain an
/// `it('<test_name>', ...)` (single- or double-quoted). Parallel to
/// `every_pin_source_file_has_named_test` but adapted for the
/// Vitest `it()` idiom.
#[test]
fn every_js_pin_source_file_has_named_test() {
    for pin in JS_PINS {
        let body = source_body(pin.js_path);
        // Accept both Vitest idiom (`it('name',`) and Playwright
        // idiom (`test('name',`) — both single- and double-quoted.
        let needles = [
            format!("it('{}',", pin.test_name),
            format!("it(\"{}\",", pin.test_name),
            format!("test('{}',", pin.test_name),
            format!("test(\"{}\",", pin.test_name),
        ];
        assert!(
            needles.iter().any(|n| body.contains(n)),
            "{} says §{} is pinned by {}::{}, but no matching \
             `it('...'` or `test('...'` declaration was found in {}. \
             Either the test was renamed without updating the PRD, \
             or the PRD cell was edited without the underlying test \
             being created.",
            PRD_PATH,
            pin.criterion,
            pin.js_path,
            pin.test_name,
            pin.js_path,
        );
    }
}

/// For each JS pin, the PRD row for that criterion must cite the
/// `teralaunch/tests/<file>::<name>` form of the measurement path.
#[test]
fn every_js_pin_is_cited_in_prd_row() {
    let prd = prd_body();
    for pin in JS_PINS {
        let row_marker = format!("| {} |", pin.criterion);
        let row_start = prd.find(&row_marker).unwrap_or_else(|| {
            panic!(
                "PRD row for criterion {} not found — PRD table shape \
                 may have changed",
                pin.criterion,
            )
        });
        let row_end = prd[row_start..]
            .find('\n')
            .map(|i| row_start + i)
            .unwrap_or(prd.len());
        let row = &prd[row_start..row_end];

        let cell = js_cell_for(pin);
        assert!(
            row.contains(&cell),
            "PRD row for §{} must cite the JS measurement path \"{}\", \
             but its actual text is:\n  {}\n",
            pin.criterion,
            cell,
            row,
        );
    }
}

// --------------------------------------------------------------------
// Iter 178 structural pins — table-shape invariants on PINS + JS_PINS
// + PRD file + cross-section coverage.
// --------------------------------------------------------------------
//
// Iter 97+132-134 pinned per-entry drift: every PINS/JS_PINS row maps
// to a real test + real PRD cell. Iter 178 widens to TABLE-LEVEL
// invariants that no per-entry test catches: duplicate rows, pin-count
// floors, explicit file-existence, PRD file health, and cross-section
// coverage guaranteeing the table hasn't drifted into one section.

/// No two PINS entries may share the EXACT same (criterion,
/// source_path, test_name) triple. Multiple entries pointing the
/// same test at the same criterion is redundant noise — if the
/// duplicate was intentional (e.g. a rebase merge), the author
/// should collapse it. The existing per-entry tests would pass on
/// duplicates but silently bloat the table.
#[test]
fn pins_table_has_no_duplicate_triples() {
    let mut seen: std::collections::HashSet<(&str, &str, &str)> =
        std::collections::HashSet::new();
    let mut duplicates: Vec<(&str, &str, &str)> = Vec::new();
    for pin in PINS {
        let key = (pin.criterion, pin.source_path, pin.test_name);
        if !seen.insert(key) {
            duplicates.push(key);
        }
    }
    assert!(
        duplicates.is_empty(),
        "PRD §3 drift-guard: PINS contains duplicate (criterion, \
         source_path, test_name) triple(s): {duplicates:?}. Each \
         triple should appear at most once — duplicates suggest a \
         rebase merge or copy-paste that should have been collapsed."
    );
    // Same for JS_PINS.
    let mut js_seen: std::collections::HashSet<(&str, &str, &str)> =
        std::collections::HashSet::new();
    let mut js_duplicates: Vec<(&str, &str, &str)> = Vec::new();
    for pin in JS_PINS {
        let key = (pin.criterion, pin.js_path, pin.test_name);
        if !js_seen.insert(key) {
            js_duplicates.push(key);
        }
    }
    assert!(
        js_duplicates.is_empty(),
        "PRD §3 drift-guard: JS_PINS contains duplicate triple(s): \
         {js_duplicates:?}. Same collapsing rule applies."
    );
}

/// Pin-count floors catch a bulk-delete regression. Current state:
/// 39 Rust pins + 5 JS pins (iter 177 snapshot). A floor at 30 / 3
/// gives margin for ad-hoc refactors without letting a rebase
/// accidentally drop half the table.
#[test]
fn pin_count_meets_minimum_floor() {
    const MIN_RUST_PINS: usize = 30;
    const MIN_JS_PINS: usize = 3;
    assert!(
        PINS.len() >= MIN_RUST_PINS,
        "PRD §3 drift-guard: PINS table has only {} entries \
         (floor: {MIN_RUST_PINS}). A bulk-delete or bad rebase may \
         have dropped rows — the PRD's measurement column would \
         then fail to route to tests.",
        PINS.len()
    );
    assert!(
        JS_PINS.len() >= MIN_JS_PINS,
        "PRD §3 drift-guard: JS_PINS table has only {} entries \
         (floor: {MIN_JS_PINS}). JS-side measurement coverage \
         must not drop below the floor.",
        JS_PINS.len()
    );
}

/// Every PIN's source_path must point to a file that actually
/// exists on disk. The per-entry `every_pin_source_file_has_named_
/// test` catches this via a panic inside `source_body`, but the
/// failure message is opaque ("file not readable") rather than
/// naming the drift. This pin surfaces missing files with a clear
/// message that names which entry moved/was deleted.
#[test]
fn every_pin_source_path_exists_on_disk() {
    let mut missing: Vec<(&str, &str)> = Vec::new();
    for pin in PINS {
        if !std::path::Path::new(pin.source_path).exists() {
            missing.push((pin.criterion, pin.source_path));
        }
    }
    assert!(
        missing.is_empty(),
        "PRD §3 drift-guard: {} PINS entry/entries point to source \
         paths that don't exist:\n  {}\nA file rename or deletion \
         without updating PINS leaves the PRD's measurement column \
         routing to nowhere.",
        missing.len(),
        missing
            .iter()
            .map(|(c, p)| format!("§{c} → {p}"))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    // JS side.
    let mut js_missing: Vec<(&str, &str)> = Vec::new();
    for pin in JS_PINS {
        if !std::path::Path::new(pin.js_path).exists() {
            js_missing.push((pin.criterion, pin.js_path));
        }
    }
    assert!(
        js_missing.is_empty(),
        "PRD §3 drift-guard: {} JS_PINS entry/entries point to \
         missing files:\n  {}",
        js_missing.len(),
        js_missing
            .iter()
            .map(|(c, p)| format!("§{c} → {p}"))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// The PRD file itself must meet a minimum line count. A truncation
/// past threshold is either a bad rebase or a partial commit — all
/// of our per-entry pins would panic with a row-not-found message
/// but the root cause ("the whole PRD got truncated") is faster to
/// diagnose with a file-level size check.
#[test]
fn prd_file_meets_minimum_line_count() {
    const MIN_PRD_LINES: usize = 300;
    let body = prd_body();
    let lines = body.lines().count();
    assert!(
        lines >= MIN_PRD_LINES,
        "PRD §3 drift-guard: {PRD_PATH} has only {lines} lines \
         (floor: {MIN_PRD_LINES}). Truncation below the floor \
         suggests a bad rebase or partial commit — the per-entry \
         pins would all fail with opaque row-not-found errors."
    );
}

/// The combined set of distinct PRD sections referenced by
/// PINS + JS_PINS must span at least §3.1, §3.2, §3.3, plus at
/// least one of §3.4-§3.8 (via JS pins). The guard's value is in
/// cross-section coverage — if all pins collapsed into one section,
/// the drift-guard would only catch drift inside that section while
/// silently passing for every other §3 row.
#[test]
fn pins_span_multiple_prd_sections() {
    let mut sections: std::collections::BTreeSet<&str> =
        std::collections::BTreeSet::new();
    for pin in PINS {
        // Criterion format is "X.Y.Z"; section is "X.Y".
        if let Some(section) = pin.criterion.rsplit_once('.').map(|(s, _)| s) {
            sections.insert(section);
        }
    }
    for pin in JS_PINS {
        if let Some(section) = pin.criterion.rsplit_once('.').map(|(s, _)| s) {
            sections.insert(section);
        }
    }
    // Must cover the security/reliability/functionality triplet.
    for required in ["3.1", "3.2", "3.3"] {
        assert!(
            sections.contains(required),
            "PRD §3 drift-guard: pins must cover §{required} (no \
             entry references this section). All coverage \
             collapsed into other sections would mean §{required} \
             drift goes undetected.\nSections present: {sections:?}"
        );
    }
    // And at least one JS-side section (3.4-3.8 range).
    let has_js_section = sections
        .iter()
        .any(|s| matches!(*s, "3.4" | "3.5" | "3.6" | "3.7" | "3.8"));
    assert!(
        has_js_section,
        "PRD §3 drift-guard: pins must cover at least one JS-side \
         section (§3.4-§3.8). Without any, i18n/perf/UX drift on \
         the frontend escapes the PRD-citation check.\nSections \
         present: {sections:?}"
    );
}

// --------------------------------------------------------------------
// Iter 212 structural pins — meta-guard header + criterion shape +
// PRD_PATH constant + cell_for formatter self-test + section-3 bound.
// --------------------------------------------------------------------
//
// The eleven pins above cover per-entry drift + table-level invariants
// (dups, count floor, disk-existence, PRD line count, section span).
// They do NOT pin: (a) the guard's own header cites PRD §3 drift —
// meta-guard contract; (b) every criterion matches the canonical
// `X.Y.Z` format — a typo like `3..2.7` or `3.1` (missing subsection)
// would pass every per-entry pin but break PRD-row location; (c) the
// `PRD_PATH` constant equals the canonical `mod-manager-perfection.md`
// filename — a rename without updating the constant would break every
// pin with an opaque "file not readable" panic; (d) `cell_for`
// formatter's inline-vs-integration split behaves correctly on both
// inputs — a regression that drops `::tests::` for src/ paths would
// make every PRD cite drift silently; (e) no pin criterion lives
// outside section 3 — `pins_span_multiple_prd_sections` checks
// inclusion (§3.1/§3.2/§3.3 present), not exclusion (some pin at §4
// or §2 would add a section that has no PRD row format).

/// The guard's own module header must cite PRD §3 drift so a reader
/// chasing a PRD-cell regression lands here via section-grep.
#[test]
fn guard_file_header_cites_prd_section_3_drift() {
    let body = fs::read_to_string("tests/prd_path_drift_guard.rs")
        .expect("tests/prd_path_drift_guard.rs must exist");
    let header = &body[..body.len().min(1500)];
    assert!(
        header.contains("PRD §3") || header.contains("§3 "),
        "meta-guard contract: tests/prd_path_drift_guard.rs header \
         must cite `PRD §3` or `§3 ` (measurement-path drift is a \
         §3-wide concern). Without it, a reader chasing a PRD-row \
         regression won't land here via section-grep.\nHeader:\n{header}"
    );
    assert!(
        header.contains("drift"),
        "meta-guard contract: header must use the word `drift` to \
         frame what the guard catches — readers scanning for \
         `drift guard` should find this file."
    );
}

/// Every `PINS` + `JS_PINS` criterion must match the canonical
/// `N.M.P` format (digit-dot-digit-dot-digit, each segment ≥ 1 char).
/// A typo like `3..2.7` or `3.1` (missing subsection depth) would
/// pass every per-entry pin because the row-marker `| 3.1 |` happens
/// to match EVERY §3.1.x row — so the PRD-row location returns the
/// first §3.1.x hit, hiding the real cell drift.
#[test]
fn every_pin_criterion_matches_x_y_z_format() {
    fn is_xyz(s: &str) -> bool {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return false;
        }
        parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    }

    let mut offenders: Vec<&str> = Vec::new();
    for pin in PINS {
        if !is_xyz(pin.criterion) {
            offenders.push(pin.criterion);
        }
    }
    for pin in JS_PINS {
        if !is_xyz(pin.criterion) {
            offenders.push(pin.criterion);
        }
    }
    assert!(
        offenders.is_empty(),
        "PRD §3 drift-guard (iter 212): all pin criteria must match \
         `N.M.P` format (three digit segments, dot-separated). \
         Offenders: {offenders:?}. A typo like `3.1` (missing \
         subsection) resolves the PRD row marker `| 3.1 |` to the \
         FIRST §3.1.x row found, silently matching whatever cell \
         happens to start the PRD's §3.1 block."
    );
}

/// `PRD_PATH` must equal `../../docs/PRD/mod-manager-perfection.md`
/// verbatim. A rename without updating this constant would break
/// every test with an opaque "file not readable" panic that doesn't
/// tell the reader which doc was renamed or to where.
#[test]
fn prd_path_constant_points_to_perfection_md() {
    let guard_body = fs::read_to_string("tests/prd_path_drift_guard.rs")
        .expect("guard source must be readable");
    assert!(
        guard_body
            .contains("const PRD_PATH: &str = \"../../docs/PRD/mod-manager-perfection.md\";"),
        "PRD §3 drift-guard (iter 212): \
         tests/prd_path_drift_guard.rs must retain \
         `const PRD_PATH: &str = \"../../docs/PRD/mod-manager-perfection.md\";` \
         verbatim. A rename of either the file or the constant must \
         be atomic — otherwise every pin fails with an opaque \
         `file not readable` panic."
    );
}

/// `cell_for` must produce the correct format for both path shapes:
/// an `src/...` source_path gets a `::tests::` module prefix (inline
/// `#[cfg(test)] mod tests`), and a `tests/...` integration-test
/// source_path gets NO module prefix (bin-crate integration tests
/// have `#[test]` fns at file root). If this formatter regresses —
/// e.g. always emits `::tests::` — half the PRD rows' cite strings
/// would drift silently and `every_pin_is_cited_in_prd_row` would
/// fire with confusing messages.
#[test]
fn cell_for_formatter_tracks_inline_vs_integration_split() {
    // Positive case 1: inline test in src/ — must include `::tests::`.
    let inline_pin = Pin {
        criterion: "9.9.9",
        source_path: "src/services/mods/tmm.rs",
        test_name: "my_inline_test",
    };
    let inline_cell = cell_for(&inline_pin);
    assert!(
        inline_cell
            .contains("teralaunch/src-tauri/src/services/mods/tmm.rs::tests::my_inline_test"),
        "PRD §3 drift-guard (iter 212): cell_for must format an \
         `src/...` source_path with a `::tests::` module prefix. \
         Got `{inline_cell}`."
    );

    // Positive case 2: integration test in tests/ — must NOT include
    // `::tests::` (bin-crate integration tests put fns at file root).
    let integration_pin = Pin {
        criterion: "9.9.9",
        source_path: "tests/parallel_install.rs",
        test_name: "my_integration_test",
    };
    let integration_cell = cell_for(&integration_pin);
    assert!(
        integration_cell
            .contains("teralaunch/src-tauri/tests/parallel_install.rs::my_integration_test"),
        "PRD §3 drift-guard (iter 212): cell_for must format a \
         `tests/...` source_path WITHOUT the `::tests::` module \
         prefix. Got `{integration_cell}`."
    );
    assert!(
        !integration_cell.contains("::tests::"),
        "PRD §3 drift-guard (iter 212): cell_for must NOT emit \
         `::tests::` for a `tests/...` path — bin-crate integration \
         tests have `#[test]` fns at file root."
    );
}

/// No pin criterion may reference a section outside §3. The PRD row
/// marker format `| {criterion} |` relies on §3.x.y shape for its
/// location lookup; a `4.1.1` pin would fail the row-find with an
/// opaque panic, OR (worse) match a coincidental `| 4.1.1 |` cell
/// somewhere in the doc unrelated to measurement-path drift.
#[test]
fn no_pin_criterion_outside_section_3() {
    for pin in PINS {
        let section = pin.criterion.split('.').next().unwrap_or("");
        assert_eq!(
            section, "3",
            "PRD §3 drift-guard (iter 212): PINS entry criterion \
             `{}` must start with `3.` (measurement-path drift is \
             §3-scoped). Section `{section}` is outside the drift-\
             guard's scope.",
            pin.criterion
        );
    }
    for pin in JS_PINS {
        let section = pin.criterion.split('.').next().unwrap_or("");
        assert_eq!(
            section, "3",
            "PRD §3 drift-guard (iter 212): JS_PINS entry criterion \
             `{}` must start with `3.` (measurement-path drift is \
             §3-scoped). Section `{section}` is outside the drift-\
             guard's scope.",
            pin.criterion
        );
    }
}

/// Self-test: prove the detector bites if a pin's source path goes
/// missing or the named test is absent.
#[test]
fn drift_detector_self_test() {
    // Synthetic bad pin: file does not exist → source_body panics (not
    // an assert). We can't easily test that without catching panics,
    // but we CAN test the positive-case substring check.
    let real_pin = &PINS[0];
    let body = source_body(real_pin.source_path);
    assert!(body.contains(&format!("fn {}", real_pin.test_name)));

    // Synthetic missing-fn: same file, wrong fn name.
    let fake_name = "this_fn_must_never_exist_in_the_repo_xyzzy";
    assert!(
        !body.contains(&format!("fn {fake_name}")),
        "self-test: the fake fn name leaked into a real source file — \
         the detector would silently pass on a missing pin"
    );
}
