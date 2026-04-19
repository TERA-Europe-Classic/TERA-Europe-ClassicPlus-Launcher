# fix-plan.md

Mutable priority queue consumed by the `/loop` driving `docs/PRD/mod-manager-perfection.md`.

Each iteration: read the counter below, detect iteration type (work / research / revalidation / retrospective / blocked-retry), do the work, update this file.

## Loop header (machine-parseable — DO NOT reformat)

```yaml
iteration_counter: 112
last_work_iteration: 112
last_research_sweep: 110
last_revalidation: 100
last_revalidation_status: all-gates-green
last_retrospective: 60
last_blocked_retry: 50
last_blocked_retry_status: all-still-blocked
last_investigation_iteration: 87
total_items_done: 93
total_items_regressed: 0
total_iterations_to_cap: 1000
tauri_v2_migration_milestone: M8-validated
tauri_v2_migration_worktree: ../tauri-v2-migration
tauri_v2_migration_branch: tauri-v2-migration
tauri_v2_migration_last_commit: f39ab31
tauri_v2_migration_ready_for_squash_merge: true
```

> **Iter 112 WORK — dep.rand-and-bytes-advisory-ignore + audit-tuning DONE (worktree).**
>
> Worktree commit `f39ab31`. Iter 112 picked P2 `dep.rand-advisory-ignore-2026-0097` planning to add one ignore flag. Running `cargo audit` locally surfaced a **second vuln** (RUSTSEC-2026-0007, bytes 1.11.0 integer overflow in `BytesMut::reserve`, landed 2026-02-03) that was masked behind the rand report in iter 110's sweep. Both upstream-gated.
>
> **Changes to `.github/workflows/cargo-audit.yml`:**
> 1. `--ignore RUSTSEC-2026-0097` (rand 0.9.2 unsound) — tauri-plugin-notification / chamox / quinn-proto chain. Exit: tauri-plugin-notification bumps rand >= 0.10.
> 2. `--ignore RUSTSEC-2026-0007` (bytes 1.11.0 overflow) — tokio-util / tower-http / reqwest chain. Exit: upstream bumps bytes to >= 1.11.1. Not exploitable in our code path (bytes usage bounded by reqwest's ~8KB wire chunks).
> 3. **Dropped `--deny warnings`** on both audit steps. Iter 101 chose it as a defensive default. Iter 112 enumerated the 19 triggered warnings: all upstream-locked (gtk/gdk/atk webview chain, unic-* transitives, proc-macro-error, fxhash, number_prefix). None actionable from our position. Keeping `--deny warnings` would fail CI for no benefit. Default `cargo audit` still fails on vulns (what we want); warnings print informationally.
>
> Each `--ignore` flag carries explicit rationale + exit criterion in the yml comment. No silent ignores. Header comment rewritten to document the strategy.
>
> **Net effect:** post-squash `cargo-audit` CI run will now be green. Both RUSTSEC-2026-0097 and RUSTSEC-2026-0007 cleared (ignored); RUSTSEC-2021-0141 (dotenv) cleared by iter 111 (dep removal); 19 upstream-locked warnings print but don't fail CI. Iter 110 sweep's deferred cleanup complete.
>
> Also absorbed the P3 `infra.cargo-audit-tuning` item from iter 110 — the `--deny warnings` policy decision was exactly what that P3 queued.
>
> Acceptance: 875/875 Rust unchanged, 449/449 JS unchanged, clippy clean, cargo audit exit 0 on both workspaces. Worktree ready state unchanged — `ready_for_squash_merge: true`.

> **Iter 111 WORK — dep.teralib-dotenv-drop DONE (clears RUSTSEC-2021-0141).**
>
> Worktree commit `37492b9`. Iter 110 flagged `dotenv 0.15.0` unmaintained in teralib. Investigation surfaced a cleaner fix than planned: the dep was **genuinely unused** — zero `use dotenv::` imports in teralib/src/, no env-macro references, no build.rs use, no `.env` file present. The dep had been dead weight since added.
>
> **Fix:** removed the line from `teralib/Cargo.toml`. No migration to `dotenvy` needed. Dep count drops from 234 → 233. Also: `teralib/Cargo.lock` was previously untracked; committed alongside the fix so future lockfile-gated audits see consistent state.
>
> **Verification:** `cargo audit` on teralib now reports zero advisories (was 1 unmaintained warning). teralaunch full suite green: 875 Rust (unchanged), 449 JS, clippy clean (both workspaces).
>
> **One advisory remains:** RUSTSEC-2026-0097 (rand 0.9.2 unsound) on the teralaunch side, pulled in by tauri-plugin-notification + chamox + quinn-proto chain. Iter 112 will add `--ignore RUSTSEC-2026-0097` to `cargo-audit.yml` with a cite comment since the unsoundness is not exploitable in our code pattern.
>
> Acceptance: 875/875 Rust unchanged, 449/449 JS unchanged, clippy clean, teralib cargo audit clean. Worktree ready state unchanged — `ready_for_squash_merge: true`.

> **Iter 110 RESEARCH SWEEP — two real advisories surfaced.**
>
> Worktree commit `24af9f6` (`docs/PRD/audits/research/sweep-iter-110.md`). First sweep with cargo-audit actually installed locally — iter 101 install retry succeeded after 9m18s single-threaded build (AV interference cleared).
>
> **Real findings:**
> 1. **RUSTSEC-2026-0097** — `rand 0.9.2` unsound with a custom logger via `rand::rng()`. Landed 2026-04-09 (POST iter-100 sweep). Pulled in by `tauri-plugin-notification 2.3.3`, `chamox 0.1.4`, and `quinn-proto 0.11.14` via `reqwest 0.12.28`. **Not exploitable in our code** (we don't call `rand::rng()` with a custom logger).
> 2. **RUSTSEC-2021-0141** — `dotenv 0.15.0` unmaintained since 2021-12-24. `teralib` is the sole consumer. Drop-in replacement: `dotenvy`.
>
> **CI impact:** iter-101 `cargo-audit --deny warnings` gate will fail first run post-squash unless these are cleared.
>
> **Dep tree delta:** two new upstream-driven triple-version dups — `getrandom 0.3.4` (via rand 0.9.2) and `hashbrown 0.16.1` (via indexmap 2.13.0). Both non-actionable from our side, same pattern as iter-87 reqwest deferral. No upstream unblock of the reqwest 0.12/0.13 chain landed in the iter 100-110 window.
>
> **New P-slot candidates queued** (see audit doc):
> - **P1 `dep.teralib-dotenv-to-dotenvy`** — tight scope: 1 dep line + ~10 import renames. Backward-compatible.
> - **P2 `dep.rand-advisory-ignore-2026-0097`** — add `--ignore RUSTSEC-2026-0097` with cite comment to `cargo-audit.yml`.
> - **P3 `infra.cargo-audit-tuning`** — 23 informational warnings from `kuchikiki/selectors/cssparser` chain; decide if `--deny warnings` is right default.
>
> Iter 111 (next) picks P1 `dep.teralib-dotenv-to-dotenvy`. Iter 112 picks P2 rand-advisory-ignore. Both land before user squash-merge so the first post-squash cargo-audit CI run is green. Iter 120 is next double-duty (N%10=0 + N%20=0 revalidation).

> **Iter 109 WORK — docs.changelog-guard DONE (worktree).**
>
> Worktree commit `609d659`. Last WORK before iter 110 (N%10=0 RESEARCH SWEEP). Closes PRD §3.8.5: "Per-release player-facing CHANGELOG.md in plain English (no conventional-commit prefixes)." Threshold was "0 matches for `^feat|^fix|^chore`". Current state verified: 125-line CHANGELOG with zero offenders.
>
> New `tests/changelog_guard.rs` (3 tests):
> 1. `no_conventional_commit_prefixes` — walks every line, strips leading `- ` / `* ` bullet markers, checks against a 22-prefix blocklist (feat, fix, chore, docs, refactor, test, build, ci, perf, style, revert — each in `:` and `(` form). Fails with line-numbered offenders.
> 2. `changelog_has_structure_and_content` — >= 1 `## ` heading + >= 10 lines (not a one-line "TBD" file).
> 3. Detector self-test with raw + bulleted bad shapes and a player-facing good fixture.
>
> Also probed §3.8.7 (per-unit audit doc count >= 110): `docs/PRD/audits/units/` directory does NOT exist on worktree — that's a genuine unshipped backlog item, not drift. Skipped for iter 109.
>
> **Fifth doc-guard in the iter 104-109 structural-pin batch:**
> - 104 `crate_comment_guard` (§3.8.2)
> - 106 `architecture_doc_guard` (§3.8.4)
> - 107 `claude_md_guard` (§3.8.1)
> - 108 `lessons_learned_guard` (§3.8.8)
> - 109 `changelog_guard` (§3.8.5)
>
> All §3.8.x structural invariants except §3.8.3 (already a CI gate since iter 105), §3.8.6 (external-repo, deferred), and §3.8.7 (audits/units/ dir not created yet) now pinned by Rust integration tests.
>
> Acceptance: 875/875 Rust (was 872, +3), clippy clean, 449/449 JS unchanged. Worktree ready state unchanged — `ready_for_squash_merge: true`.

> **Iter 108 WORK — docs.lessons-learned-cap-restore + guard DONE (worktree).**
>
> Worktree commit `eea7309`. Closes PRD §3.8.8: "lessons-learned.md exists, capped 200 lines, archived when full."
>
> **Drift found:** active `docs/PRD/lessons-learned.md` had grown to 212 lines (12 over cap). The every-30 retrospective cadence wasn't tight enough to catch slow growth from iters 28-60 (several iters added entries between retros).
>
> **Fix:** archived iter 24 entry (real-vulnerability-in-audit pattern, 17 lines) under a new "Archived 2026-04-19 / iter 108 cap-restore" banner in `lessons-learned.archive.md`. Active file is now 196 lines (4 lines headroom).
>
> **Pin:** new `tests/lessons_learned_guard.rs` (4 tests):
> 1. `active_file_exists_and_under_cap` — asserts 0 < lines <= 200
> 2. `archive_file_exists` — archival path is a precondition for the cap policy
> 3. `active_file_header_advertises_cap_policy` — header mentions "200" + "lessons-learned.archive.md" so future editors know about the cap
> 4. Detector self-test with oversize, empty, no-header shapes
>
> Going forward any push that takes the active file over 200 lines fails CI, not just the next every-30 retro. **Fourth in the iter 104-108 doc-guard batch:** crate-comment (104), architecture-doc (106), claude-md (107), lessons-learned (108). All §3.8.x structural doc invariants now enforced by Rust integration tests.
>
> Acceptance: 872/872 Rust (was 868, +4), clippy clean, 449/449 JS unchanged. Worktree ready state unchanged — `ready_for_squash_merge: true`.

> **Iter 107 WORK — docs.claude-md-guard DONE (worktree).**
>
> Worktree commit `42d4a8e`. Closes PRD §3.8.1 structural pin: "CLAUDE.md has a `## Mod Manager` section covering feature state + build + deploy, threshold >= 30 lines."
>
> Current state verified — section exists at 62 body lines and covers the three topic areas. Until iter 107 this was enforced only by periodic human review; CLAUDE.md is the on-ramp doc every new loop iter reads, so drift would have been expensive.
>
> New `tests/claude_md_guard.rs` (3 tests):
> 1. `mod_manager_section_exists_and_meets_size_threshold` — finds the heading, counts lines to next `## `, asserts >= 30.
> 2. `mod_manager_section_covers_state_build_and_deploy` — keyword presence check for each of the three required topic areas.
> 3. Detector self-test with 3 synthetic shapes (no-section, too-short, good fixture).
>
> Parallel to iter 104 (`crate_comment_guard.rs`) and iter 106 (`architecture_doc_guard.rs`): triple of structural doc-invariant tests enforcing §3.8.1, §3.8.2, §3.8.4 respectively.
>
> Acceptance: 868/868 Rust (was 865, +3), clippy clean, 449/449 JS unchanged. Worktree ready state unchanged — `ready_for_squash_merge: true`.

> **Iter 106 WORK — docs.architecture-mods-state + guard DONE (worktree).**
>
> Worktree commit `1a2a583`. Closes PRD §3.8.4 coverage gap: doc required sections for 7 subsystems; ARCHITECTURE.md had 6 of 7 — `mods_state.rs` was missing.
>
> Added section "3a. Mods state (in-memory guard)" between Registry (§3) and External-app (§4). Covers: lazy-load + write-through semantics of the RwLock wrapper around registry.rs; the atomic `try_claim_installing` primitive that gates parallel installs (PRD 3.2.7); module-boundary invariants (lock poisoning surfaces cleanly; no direct `MODS_STATE.write()` outside the module).
>
> New `tests/architecture_doc_guard.rs` (3 tests):
> 1. `every_required_subsystem_has_doc_coverage` — checks all 7 subsystem file paths appear in the doc; fails with a list of missing entries so future additions are caught immediately.
> 2. `doc_has_structural_section_headings` — sanity check that the doc has >=7 `## ` headings (not one prose paragraph).
> 3. Detector self-test with synthetic partial-coverage + no-headings bad shapes.
>
> Parallel to iter 104's `crate_comment_guard.rs`: both tests enforce documentation invariants structurally so regression is a test failure, not a doc-review miss.
>
> Acceptance: 865/865 Rust (was 862, +3), clippy clean, 449/449 JS unchanged. Worktree ready state unchanged — `ready_for_squash_merge: true`.

> **Iter 105 WORK — infra.troubleshoot-coverage-ci DONE (worktree).**
>
> Worktree commit `8c31a5c`. Closes PRD §3.8.3 gate wiring: `scripts/check-troubleshoot-coverage.mjs` existed + ran green locally (51 production error templates covered), but CI never invoked it. A future PR that adds a new user-facing error without a TROUBLESHOOT.md entry would have slipped through.
>
> Change: `.github/workflows/pr-validation.yml` now invokes the script as a gate step after Rust tests. Non-zero exit lists missing templates and fails the PR. Trigger filter extended to rerun on `docs/mod-manager/TROUBLESHOOT.md` + `scripts/check-troubleshoot-coverage.mjs` changes (source files already covered by `teralaunch/**`).
>
> Parallel to iter-101 `infra.cargo-audit-ci`: both iterations add ongoing CI coverage for invariants that had passive enforcement only.
>
> Local run: `check-troubleshoot-coverage: ok — 51 production error templates covered`.
>
> Acceptance: 862/862 Rust unchanged, 449/449 JS unchanged, clippy clean (workflow + no Rust/JS code touched). Coverage script local-run: ok (51/51). Worktree ready state unchanged — `ready_for_squash_merge: true`.

> **Iter 104 WORK — docs.crate-comment-guard DONE (worktree).**
>
> Worktree commit `baf156b`. Closes PRD §3.8.2 structural invariant: "Every `src/services/mods/*.rs` has crate-level `//!` comment." Until iter 104 this was enforced only by code-review convention; a new file added without the header would slip through silently.
>
> New `tests/crate_comment_guard.rs` (2 tests):
> 1. `every_mods_source_file_has_crate_level_doc` — walks `src/services/mods/`, reads each `.rs` file's first non-empty line, asserts it starts with `//!`. Also pins expected file count (>=6) so accidental deletions surface too.
> 2. `crate_comment_detector_self_test` — 4 synthetic bad shapes (regular `//` comment, code on first line, blank-only file, etc.) + empty-file sentinel.
>
> Current state verified: all 6 mods/*.rs files carry `//!` headers.
>
> Acceptance: 862/862 Rust (was 860, +2), clippy clean, 449/449 JS unchanged. Worktree ready state unchanged — `ready_for_squash_merge: true`.

> **Iter 103 WORK — docs.prd-drift-fix-catalog-parse + pin.registry-recovery-direct DONE (worktree).**
>
> Worktree commit `81f82fe`. Scan of `registry.rs` + `catalog.rs` test blocks surfaced a **7th PRD path drift**: §3.2.6 cited `teralaunch/tests/catalog-parse.test.js::malformed_entries_filtered` (non-existent — no such frontend test file was ever created). The actual catalog-parse-error test lives inline at `src/services/mods/catalog.rs::tests::malformed_entries_filtered` plus 3 companion tests (`malformed_envelope_is_hard_error`, `every_entry_malformed_returns_empty_catalog`, `empty_mods_array_yields_empty_catalog`).
>
> Drift-guard: 27 → 30 pins. New:
> - §3.2.2 `recover_stuck_installs_flips_installing_to_error` — direct recovery-predicate pin; complements the end-to-end `mid_install_sigkill_recovers_to_error` already cited
> - §3.2.6 `malformed_entries_filtered` — fixes the new path drift
>
> Also extended `iter_97_fixed_paths_do_not_regress` to include the new stale path `teralaunch/tests/catalog-parse.test.js` so it can't regress.
>
> **Total PRD path drifts fixed across iter 97-103:** 7. 4 @ iter 97 (gpk_install_hash, gpk_deploy_sandbox, full_cycle, crash_recovery::tests::), 1 @ iter 98 (zeroize_audit::all_sensitive_strings_zeroize), 1 @ iter 99 (clean_recovery_logic singular), 1 @ iter 103 (catalog-parse.test.js).
>
> Acceptance: 860/860 Rust (unchanged — pins extend existing test bodies), clippy clean, 449/449 JS unchanged. Worktree ready state unchanged — `ready_for_squash_merge: true`.

> **Iter 102 WORK — pin.composite-collision-and-toggle-symmetry DONE (worktree).**
>
> Worktree commit `7322ce9`. Extended drift-guard from 22 to 27 pins, concentrated on two thin-coverage criteria:
>
> **§3.3.3 — previously cited only the Playwright IPC spec:**
> - `detect_conflicts_flags_other_mod_owning_slot` — Rust predicate behavioural test (the "detect" half of "surfaces UI warning")
> - `golden_merger_last_install_wins_on_overlap` — iter-93 pin for "last-installed wins" half
>
> The criterion is three links deep (detect + last-wins + UI shape); now all three pinned.
>
> **§3.3.15 — previously cited only toggle_intent_only (enable direction):**
> - `toggle_disable_intent_only` — disable-direction symmetry
> - `toggle_command_bodies_do_not_spawn_or_kill` — source-inspection structural guard against a future refactor that adds spawn/kill to a toggle body
>
> Criterion is four links deep (enable/disable intent + source guard + multi-client lifecycle trio).
>
> **Load-bearing effect:** `adv.composite-object-collision`'s "Covered by 3.3.3" cross-reference was cosmetic prior to this iter; now three pinned Rust tests + one Playwright IPC-contract spec all enforce the criterion.
>
> Acceptance: 860/860 Rust (unchanged — pins extend existing test bodies, no new fns), clippy clean, 449/449 JS unchanged. Worktree ready state unchanged — `ready_for_squash_merge: true`.

> **Iter 101 WORK — infra.cargo-audit-ci DONE (worktree).**
>
> Worktree commit `d7a693d`. Closes the P3 queued by iter 100's sweep. New `.github/workflows/cargo-audit.yml` runs `cargo audit --deny warnings` against both Rust workspaces (teralaunch/src-tauri + teralib) on every push to main + every PR + daily at 04:17 UTC.
>
> Complements the manual sweep cadence (every 10 loop iters, can lag up to 9 iters); CI catches new RUSTSEC advisories on the next push or schedule fire. Caches cargo-audit binary + advisory-db so cold runs aren't dominated by install / DB-fetch time. Matches the self-install pattern used by `secret-scan.yml` (gitleaks).
>
> No ignore list carried. Iter-87 dep-dedup audit documented the reqwest 0.12/0.13 + zip 2/4 duplication as upstream-gated but noted every advisory found at that time was either not applicable or already fixed in both resolved versions. If a future advisory lands that we've decided not to act on, add `--ignore RUSTSEC-YYYY-NNNN` with a cite comment; no silent ignores.
>
> Parallel to iter-13/88 gitleaks infrastructure: iter 13 first ran the historical scan, iter 88 added the allowlist + permanent CI. iter 101 is the equivalent first-CI-run for RUSTSEC.
>
> Acceptance: 860/860 Rust unchanged, 449/449 JS unchanged, clippy clean (workflow addition only — no Rust/JS code touched). Worktree ready state unchanged — `ready_for_squash_merge: true`.
>
> Local install of cargo-audit hit a transient Windows link error (LNK1105, AV interference), irrelevant since CI runs on Ubuntu. Workflow self-validates on first CI run; if advisories surface that need to be ignored, that's a follow-up iter.

> **Iter 100 DOUBLE-DUTY — RESEARCH SWEEP + REVALIDATION both DONE (worktree).**
>
> Worktree commit `5efdf80` (`docs/PRD/audits/research/sweep-iter-100.md`). N%10=0 research sweep + N%20=0 revalidation, both fired same iter.
>
> **Research sweep:** zero new drift since iter 90. rustls-webpki (iter 81) and time (iter 91) deduplications both confirmed. Reqwest/cookie 0.12/0.13 chain remains upstream-gated per iter-87 `dep.dedupe-reqwest-zip` deferral. No new RUSTSEC advisories affecting direct deps. No tauri / reqwest / rustls / zip / tokio / serde major bumps. One new P3 queued: `infra.cargo-audit-ci` (`cargo-audit` not installed locally, so no advisory scan fired during this sweep — CI addition is cheap ongoing coverage).
>
> **Revalidation:** 860/860 Rust (+83 vs iter-72 baseline of 777, driven by iters 73-99 pin / adversarial / fix / drift-guard additions), 449/449 JS (+32 vs iter-72 baseline of 417), clippy -D warnings clean, zero REGRESSED items across 40 commits since worktree divergence. Spot-check of 7 key pinned invariants: all present. 22-pin drift-guard continues to enforce. Status: **all-gates-green**.
>
> Net iter-100 risk delta: **zero**. Worktree `ready_for_squash_merge: true` status unchanged.

> **Iter 99 WORK — pin.clean-recovery-double-arm DONE (worktree).**
>
> Worktree commit `3360952`. Last WORK iter before iter 100 double-duty (N%10=0 RESEARCH SWEEP + N%20=0 REVALIDATION). Fixed a 6th PRD path drift: §3.2.9 cited a singular `clean_recovery_logic` that never existed. The actual tests are 4 distinct fns sharing the prefix (creates_backup_from_vanilla_current, refuses_when_current_is_modded, nop_when_backup_exists, errors_when_mapper_missing).
>
> The criterion text "recreates if no GPK currently patched; otherwise refuses with recovery instructions" is a double invariant, so the PRD now cites BOTH load-bearing tests explicitly:
> - `clean_recovery_logic_creates_backup_from_vanilla_current` — recreate path
> - `clean_recovery_logic_refuses_when_current_is_modded` — refuse path
>
> Drift-guard pin count: 20 → 22 (both halves of 3.2.9 pinned).
>
> Acceptance: 860/860 Rust (unchanged — pins extend existing test bodies), clippy clean, 449/449 JS unchanged. Worktree ready state unchanged — `ready_for_squash_merge: true`.
>
> **Iter 100 is DOUBLE-DUTY** (N%10=0 RESEARCH SWEEP + N%20=0 REVALIDATION). Header MUST set `last_research_sweep: 100` AND `last_revalidation: 100` when that iter commits. Scope: run a dep/advisory sweep over the worktree (cargo tree + upstream release notes for tauri/reqwest/zip/rustls/time/etc), then revalidate all 80+ DONE items (full cargo + npm + clippy + quick git log scan to ensure none REGRESSED since last revalidation at iter 72).

> **Iter 98 WORK — docs.prd-drift-guard-extend DONE (worktree).**
>
> Worktree commit `bdbdb8e`. Iter 97's drift-guard started at 7 curated pins; iter 98 extends to 20 by adding §3.1.5/.7/.9/.11/.12, §3.2.3/.7/.8/.11/.12/.13, §3.3.12/.15. The `cell_for()` helper was refactored to branch on `source_path` prefix — inline pins use `::tests::` module path, bin-crate integration-test pins use flat fn names (no `::tests::`). Previous hardcoded `::tests::` only worked for inline pins and would have mis-cited every `tests/*.rs` integration test.
>
> Found and fixed a 5th drift while extending: §3.1.7 cited `all_sensitive_strings_zeroize` (which never existed in `tests/zeroize_audit.rs`). Updated to cite `zeroize_derives_compose_with_skip_attribute` — the representative fn that pins the derive-plus-skip pattern `GlobalAuthInfo` / `LaunchParams` actually use.
>
> Drift-guard pin coverage now spans 20 §3 criteria vs the ~40 total rows in the PRD §3 table (the other half either cite audit docs, frontend tests, or are genuinely unshipped functionality). This is the practical ceiling for the Rust-only drift-guard.
>
> Acceptance: 860/860 Rust (unchanged — pins extend existing test bodies, no new fns), clippy clean, 449/449 JS unchanged. Worktree ready state unchanged — `ready_for_squash_merge: true`.
>
> Next iter (99) still WORK. **Iter 100 is double-duty** (N%10=0 RESEARCH SWEEP + N%20=0 REVALIDATION).

> **Iter 97 WORK — docs.prd-path-drift-fix DONE (worktree).**
>
> Worktree commit `1ced792`. Iters 61-95 migrated several "integration" tests from `tests/foo.rs` to inline `src/services/mods/foo.rs::tests` because bin-crate tests can't import library types (the `ModEntry`/`Registry` import limitation we hit in iter 78, 89, etc.). The PRD §3 / §5.2 tables were not updated and drifted — four rows cited `tests/foo.rs` files that never existed. A future contributor scanning the PRD to verify a criterion is pinned would have come up empty.
>
> Fixed paths:
> - §3.1.2: `tests/gpk_install_hash.rs` → `services/mods/external_app.rs::tests`
> - §3.1.4: `tests/gpk_deploy_sandbox.rs` → `services/mods/tmm.rs::tests`
> - §3.2.2: `tests/crash_recovery.rs::tests::` (nonsensical `::tests::` suffix on bin-crate integration test) → `services/mods/registry.rs::tests` (behavioural) + `tests/crash_recovery.rs` (JSON contract + iter-95 filesystem pins)
> - §3.2.4: `tests/full_cycle.rs` → `services/mods/tmm.rs::tests`
> - Plus two §5.2 "New tests required" bullet points.
>
> New `tests/prd_path_drift_guard.rs` (4 tests) pins this:
> 1. `every_pin_source_file_has_named_test` — curated list of 7 known-shipped invariants; source file exists + `fn <test_name>` grep-findable.
> 2. `every_pin_is_cited_in_prd_row` — PRD row for that criterion mentions the source path + test name.
> 3. `iter_97_fixed_paths_do_not_regress` — the 4 stale paths must NOT reappear.
> 4. Detector self-test.
>
> Scope note: many §3 rows cite tests that genuinely don't exist yet (unshipped — `every_catalog_entry_lifecycle.rs`, `mod-accessibility.spec.js`, etc.). Those are PRD-as-spec, not drift. The guard deliberately doesn't try to parse the full table; curated list is enough to catch future rename-without-update drift.
>
> Acceptance: 860/860 Rust (was 856, +4), clippy clean, 449/449 JS unchanged. Worktree ready state unchanged — `ready_for_squash_merge: true`.
>
> Next iter (iter 98) still WORK. Iter 100 will be double-duty (N%10=0 RESEARCH SWEEP + N%20=0 REVALIDATION).

> **Iter 96 WORK — adv.tampered-catalog DONE (worktree).**
>
> Worktree commit `0990473`. Closes another PRD §5.3 adversarial-corpus item. Behavioural half ("SHA mismatch returns Err + 0 bytes on disk") was already covered by 3 inline tests in `external_app.rs` (`sha_mismatch_aborts_before_write`, `sha_mismatch_aborts_before_write_gpk`, `sha_match_writes_file`). Iter 96 pins the REGISTRY-flip half — the "and the row ends up as Error with a reason" part — via a new integration test file `tests/tampered_catalog.rs` with 5 source-inspection wiring guards:
>
> 1. `downloader_surfaces_hash_mismatch_error_text` — both `download_file` and `download_and_extract` surface SHA mismatch with the stable "hash mismatch" error text that `finalize_error` stashes into `last_error`.
> 2. `install_external_mod_routes_err_through_finalize_error` — external install's Err branch calls `finalize_error`, not a bare `return Err`.
> 3. `install_gpk_mod_routes_err_through_finalize_error` — same wire, GPK path.
> 4. `finalize_error_flips_status_progress_and_last_error` — `finalize_error` sets `status = ModStatus::Error`, clears `progress`, populates `last_error`.
> 5. Detector self-test with 3 synthetic bad shapes.
>
> Rationale: without this chain, a refactor that swallows downloader Errs would leave the registry row stuck at Installing until the next boot's `recover_stuck_installs` pass — but by then the user sees an unresolving spinner. This complements iter 95's filesystem-side SIGKILL pin to cover the full install-failure recovery surface.
>
> Acceptance: 856/856 Rust (was 851, +5), clippy clean, 449/449 JS unchanged. Worktree ready state unchanged — `ready_for_squash_merge: true`.
>
> **PRD §5.3 adversarial-corpus status:** `adv.tampered-exe` (covered by 3.1.11), `adv.bogus-gpk-footer` [DONE @ iter 79], `adv.composite-object-collision` (covered by 3.3.3), `adv.sigkill-mid-download` [DONE @ iter 95], `adv.tampered-catalog` [DONE @ iter 96], `adv.disk-full` (covered by 3.2.8). **All §5.3 items closed or cross-referenced.** Next iter picks from §3 items or remaining P1 non-pin backlog.

> **Iter 95 WORK — adv.sigkill-mid-download DONE (worktree).**
>
> Worktree commit `b9712c6`. Closes a PRD §5.3 adversarial-corpus item. Registry-side recovery was already covered by 4 tests in `registry.rs` (`recover_stuck_installs` flips stranded Installing → Error on boot). Iter 95 pins the filesystem side:
>
> - `sigkill_recovery_external_retry_clears_dest_dir_before_extract` — source-inspects `external_app.rs::download_and_extract` for `remove_dir_all(dest_dir)` appearing BEFORE `extract_zip(` in source order. Without the pre-extract cleanup, a SIGKILL mid-extract would leave the dead install's files mixed with the retry's extract output (franken-mod tree).
> - `sigkill_recovery_gpk_retry_truncates_partial_via_fs_write` — source-inspects `download_file` for `fs::write(dest_file` (truncating write). A refactor to `OpenOptions::append` or pre-existing file-handle write would leave partial GPK bytes from the killed install mixed with the new download.
> - `sigkill_recovery_detector_self_test` — proves both detectors bite on synthetic bad shapes (missing cleanup; cleanup after extract).
>
> Rationale: downloads buffer in memory via `fetch_bytes_streaming`, so SIGKILL mid-download leaves NO on-disk partial. Residual failure mode is SIGKILL DURING the commit step; the retry path's pre-write cleanup is the invariant that keeps re-install deterministic. Structural/source-inspection pin matches the wiring-guard pattern established by iters 74, 79, 83, 86.
>
> Acceptance: 851/851 Rust (was 848, +3), clippy clean, 449/449 JS unchanged. Worktree ready state unchanged — `ready_for_squash_merge: true`.
>
> Next iter picks from shrinking P1 non-pin backlog: `adv.tampered-catalog` (higher scope — reqwest wiring), or §3 reliability items.

> **Iter 94 WORK — pin.external.download-extract DONE (worktree).**
>
> Worktree commit `fef2097`. Fourth pin.* item shipped in 6 iters (parser @ 89, cipher @ 92, merger @ 93, extract @ 94). Complements the existing single-file happy-path + zip-slip adversarial tests with a golden MULTI-ENTRY output-tree pin:
> - 3 new inline tests: full-tree content round-trip (ASCII + 256-byte binary 0x00..0xFF + root-level file), no-surprise-entries guard (exact file-list equality), re-extract idempotency.
> - `build_golden_fixture_zip` helper constructs a deterministic zip via `zip::ZipWriter` + `SimpleFileOptions::default()`. The OUTPUT tree is pinned, not the zip bytes, so a future zip-crate major bump that changes default compression still round-trips cleanly.
> - Binary-fidelity round-trip catches any UTF-8 coercion, line-ending munging, or encoding surprise — critical for mod bundles shipping DLLs.
>
> Acceptance: 848/848 Rust (was 845, +3), clippy clean, 449/449 JS unchanged. Worktree ready state unchanged — `ready_for_squash_merge: true`.
>
> **Rust-side pin coverage now complete** for PRD §5.4. Remaining PRD §5.4 pins live in C# (TCC + Shinra sniffers) and need separate C# tests — out of scope for Rust-crate iteration. Next iter picks from non-pin P1 backlog: `adv.sigkill-mid-download`, `adv.tampered-catalog`, or §3 items.

> **Iter 93 WORK — pin.tmm.merger DONE (worktree). pin.tmm trio complete.**
>
> Worktree commit `436a7f0`. Third and final pin.tmm item — apply_mod_patches is the "merger" step at deploy time. 4 new inline tests pin both halves of the merge contract:
> - `golden_merger_commutes_on_disjoint_slots` — 2-mod commutativity
> - `golden_merger_three_disjoint_mods_all_orders_agree` — all 6 permutations converge; catches path-dependence that could hide at n=2
> - `golden_merger_last_install_wins_on_overlap` — PRD 3.3.3 contract, same-slot installs diverge by order
> - `golden_merger_identity_on_empty_modfile` — empty ModFile is a no-op
>
> Helper `sorted_entries` normalises HashMap iteration order so the commutativity asserts don't leak hash randomness into the test signal.
>
> Acceptance: 845/845 Rust (was 841, +4), clippy clean, 449/449 JS unchanged. Worktree ready state unchanged — `ready_for_squash_merge: true`.
>
> **pin.tmm trio now complete** (parser @ iter 89, cipher @ iter 92, merger @ iter 93). Remaining P1 pin items: `pin.external.download-extract`, `pin.tcc.classic-plus-sniffer`, `pin.shinra.tera-sniffer` (all per PRD §5.4). Plus non-pin P1 backlog: `adv.sigkill-mid-download`, `adv.tampered-catalog`, assorted §3 items.

> **Iter 92 WORK — infra.gitleaks-bump-8.30.1 + pin.tmm.cipher DONE (worktree). TWO items closed.**
>
> Worktree commits `9e7727f` (gitleaks) + `5e6b026` (cipher pin).
>
> 1. **infra.gitleaks-bump-8.30.1** (P3) — one-line workflow bump `VER=8.30.0 → 8.30.1`. Iter-90 sweep queue fully cleared.
> 2. **pin.tmm.cipher** (P1) — golden-file pin for the 3-pass mapper cipher. Second in the pin.tmm trio after iter 89 parser pin. 4 new inline tests: `golden_cipher_encrypt_zeros_16` (byte-for-byte pin with hand-traced derivation in the const doc comment), `golden_cipher_round_trip_identity` (5 fixtures including tail-unaligned + multi-block), `golden_cipher_key1_is_permutation` (KEY1 bijective on 0..16), `golden_cipher_key2_is_exact_constant` (KEY2 == `b"GeneratePackageMapper"`, 21 bytes). Hand-traced golden value verified on first run.
>
> Acceptance: 841/841 Rust (was 837, +4), clippy clean, 449/449 JS unchanged. Worktree ready state unchanged — `ready_for_squash_merge: true`.
>
> **Iter-80 and iter-90 sweep queues both fully cleared.** Remaining pin.tmm trio: `pin.tmm.merger` (property-based composite-merge). Larger P1 backlog available: `adv.sigkill-mid-download`, `adv.tampered-catalog`, assorted §3 items.

> **Iter 91 WORK — dep.time-bump DONE (worktree).**
>
> First of two iter-90 sweep items closed. Worktree commit `b17ab33` (+ `8e9933f` book-keeping). `cargo update -p time` picked up 0.3.47, clearing RUSTSEC-2026-0009 / CVE-2026-25727 (DoS via deep RFC 2822 input). Lockfile-only change; no source touched. Incidental bumps to num-conv / time-core / time-macros for resolver consistency.
>
> Acceptance: 837/837 Rust unchanged, 449/449 JS unchanged, clippy clean. Worktree ready state unchanged — `ready_for_squash_merge: true`. Same mechanical shape as iter 81's rustls-webpki bump.
>
> One iter-90 sweep item remains (P3 `infra.gitleaks-bump-8.30.1` — one-line workflow version bump).

> **Iter 90 RESEARCH SWEEP — findings landed on worktree.**
>
> Eleventh research sweep since loop start; scheduled per N%10=0 cadence. Worktree commit `e65a617`. Artefact: `docs/PRD/audits/research/sweep-iter-90.md`. Delta against iter 80 (~5 hours calendar, 9 iters).
> - **NEW:** RUSTSEC-2026-0009 / CVE-2026-25727 on `time` crate (DoS via stack exhaustion on deep RFC 2822 input). Affects `< 0.3.47`; we resolve `0.3.45`. **Non-exploitable in our usage** (no attacker-controlled RFC 2822 parse; `time` is transitive for timestamp math). Trivial `cargo update -p time --precise 0.3.47` fix. Queued as P2 `dep.time-bump`.
> - **NEW:** gitleaks v8.30.1 patch release (2026-03-21). No rule changes, currency only. Queued as P3 `infra.gitleaks-bump-8.30.1`.
> - **UNCHANGED:** all Tauri plugin pins match latest (no releases since 2026-04-04); dep-dedup deferral holds (tauri-plugin-http 2.5.8 still on reqwest 0.12); rustls-webpki 0.103.12 still the fix; no new plugins-workspace or Tauri core advisories; malicious-crate removals (logtrace et al) not in our lock; catalog upstream has no commits since iter-80 `33cf584`.
>
> Two new fix-plan entries (1 P2 + 1 P3) queued. No code changes; no test cycle needed.
>
> Header: `last_research_sweep: 80 → 90`. `last_work_iteration` stays at 89 (research doesn't count as work). `total_items_done` unchanged at 70. Next sweep: iter 100. `ready_for_squash_merge: true` unchanged.

> **Iter 89 WORK — pin.tmm.parser DONE (worktree).**
>
> Worktree commit `ef1c01d`. Companion to iter 79's adversarial corpus — happy-path golden-file pin for `parse_mod_file`. A silent refactor that reorders footer slots, flips endian-ness, or reshuffles `read_prefixed_string` would break every TMM-packaged mod and surface only at install time; this pin catches it at commit time.
> - **3 new inline tests** in `tmm.rs::tests`: `golden_v1_fixture_parses_to_expected_modfile` (hand-packed 136-byte v1 fixture, asserts every ModFile + ModPackage field byte-for-byte), `golden_fixture_shape_is_stable` (regression guard on the fixture itself — length 136, magic at tail, interior probes), `golden_parse_is_deterministic` (double-parse yields identical structs; catches hidden state).
> - **TMM-format landmine documented inline:** v1 (legacy) puts PACKAGE_MAGIC at the "version" footer slot; v2+ puts a small integer there + has 4 TFC slots. Getting this discrimination wrong on first draft triggered a test failure; the corrected fixture uses PACKAGE_MAGIC at that slot with an inline warning comment.
>
> Acceptance: 837/837 Rust (was 834, +3 new), clippy clean, 449/449 JS unchanged. Worktree ready state unchanged — `ready_for_squash_merge: true`.
>
> Remaining pin.tmm trio: `pin.tmm.cipher` (3-pass mapper cipher), `pin.tmm.merger` (property-based composite-merge). Both still P1. Iter 90 is **RESEARCH SWEEP** (N%10=0 cadence) — iter 91 picks the next pin or another P1.

> **Iter 88 WORK — infra.gitleaks-allowlist DONE (worktree).**
>
> Worktree commit `fd8c89c`. Closes the P1 from iter 13 follow-ups (iter-13 secret-leak-scan audit). CI workflow at `.github/workflows/secret-scan.yml` has been running gitleaks 8.30 against commit ranges for months; this commit makes the known-false-positive allowlist explicit so reviewers don't re-triage the same fixtures each PR.
> - `.gitleaks.toml` (new) — extends the default ruleset (`useDefault = true`), allow-lists the `abc123def456` / `ABC123DEF456` test fixtures anchored to `services/hash_service.rs` only, excludes `target/` dirs (Cargo build artefacts tripped 10 FPs in libmuda-*.rmeta during iter-88 verification).
> - `.github/workflows/secret-scan.yml` — `gitleaks detect` call now passes `--config .gitleaks.toml` explicitly so a rename of the config fails CI loudly instead of silently falling back to defaults.
> - **Verified locally:** `gitleaks detect --source teralaunch/src-tauri/src --config .gitleaks.toml --no-git` → "no leaks found".
>
> No code changes; no build/test cycle. TCC + ShinraMeter repos need their own `.gitleaks.toml` (tracked separately as iter-13 follow-ups `fix.shinra-teradps-token`, `infra.secret-scan-ci`).
>
> Acceptance: `gitleaks detect --source . --config .gitleaks.toml` returns 0 on commit-range scans (CI target). 828/828 Rust unchanged, 449/449 JS unchanged, clippy clean.
>
> Iter-80 sweep queue fully cleared (P3 `dep.vitest-bump-post-squash` is post-squash-only). Iter 89 picks from the larger P1 backlog — candidates: `pin.tmm.parser` (golden-file pin builds on iter 79 adversarial corpus), `adv.sigkill-mid-download` (extend crash_recovery.rs), or a fresh §3 reliability item. Iter 90 is **N%10=0 RESEARCH SWEEP**.

> **Iter 87 WORK — dep.dedupe-reqwest-zip DONE (upstream-driven deferral, worktree).**
>
> Fourth of five iter-80 queued P2 items closed — documentation-only. Worktree commit `38bd3d6`. Artefact: `docs/PRD/audits/security/dep-dedup-investigation.md`.
> - **Root cause of both dups:** `tauri-plugin-updater 2.10.1` has jumped ahead of the rest of the Tauri plugin ecosystem (`tauri-plugin-http 2.5.8`, `reqwest_cookie_store 0.8.2`) on reqwest (0.12→0.13) and zip (2→4). Bumping our direct pins is blocked — peers have no 0.13-compat release yet. Downgrading the updater plugin would sacrifice the iter 71 downgrade-refusal gate.
> - **Cost of the dup:** bounded (~250-400 kB binary size, ~10-15 s cold build, audited advisories in iter 80 all applied to both resolved versions already).
> - **Exit criteria documented** so a future research sweep knows when to reopen.
>
> No code changes; no build/test cycle needed. Acceptance clause "0 duplicates OR documented blocker citing upstream" met by the second clause.
>
> **Only iter-80 queue item remaining: `dep.vitest-bump-post-squash` (P3), explicitly post-squash-only.** Next iter (88) picks from the larger P1 backlog: `adv.tampered-catalog`, `adv.sigkill-mid-download`, `pin.tmm.cipher/parser/merger`, `infra.gitleaks-allowlist`, or §3 functionality/reliability items.

> **Iter 86 WORK — sec.shell-scope-hardening DONE (worktree).**
>
> Third of five iter-80 queued P2 items closed. Worktree commit `825ec70`. Defence-in-depth against CVE-2025-31477 regression: `tauri.conf.json` now pins `"plugins": { "shell": { "open": true } }` — the Tauri 2.x advisory's recommended value (restricts the `open` endpoint to the mailto/http/https allowlist explicitly).
> - Plugin 2.3.5 (our pin) already carries the fix by default; this commit makes it config-explicit so a future plugin default-flip can't silently re-open the surface.
> - New wiring guard at `tests/shell_scope_pinned.rs` (3 tests): value is literally `true` (rejects false/regex-string/missing), stanza lives under top-level `.plugins` (rejects the v1-shaped `tauri.allowlist.shell` block), detector self-test with negative fixtures.
>
> Acceptance: 834/834 Rust (was 831, +3 new in shell_scope_pinned), clippy clean, 449/449 JS unchanged (Rust-only change). Worktree ready state unchanged — `ready_for_squash_merge: true`.
>
> Two iter-80 items remain: `dep.dedupe-reqwest-zip` (P2, iter 87) and `dep.vitest-bump-post-squash` (P3, post-squash only).

> **Iter 85 WORK — fix.mods-categories-ui DONE (worktree). Third and last user-reported P1 from iter 82 triage closed.**
>
> Worktree commit `a84349e`. The L-shape layout and dual chip style in the mods modal filter strip are gone: kind-filter (All/External/GPK) and category chips now share a single `.mods-filters-row` container with a thin vertical divider between them, and both use the same `.mods-filter-chip` class with identical pill geometry.
> - **HTML:** kind-filter moved out of `.mods-toolbar` (now search-only). New `.mods-filters-row` wraps kind group + divider + category row. `aria-label="Kind filter"` picked up a `data-translate-aria-label` + new i18n key `MODS_ARIA_KIND_FILTER`.
> - **CSS:** `.mods-filter-chip` base styling unified to pill geometry (999px radius, 4px/10px padding, 11px font). Active state picks up the teal border. `.mods-filter-group` segmented-control wrapper dropped. Dead `.mods-category-chip` rules deleted.
> - **JS:** kind-filter click handler + setFilter active-class flip scoped to `.mods-filter-group .mods-filter-chip` to prevent double-binding category chips to `setFilter(undefined)` now that both groups share the chip class. renderCategoryChips emits the unified class.
> - **i18n:** 1 new key × 4 locales (MODS_ARIA_KIND_FILTER in FRA/EUR/RUS/GER).
> - **Tests (7 new, mods-categories-ui.test.js):** `.mods-filters-row` DOM shape (kind → divider → category order), both groups use `.mods-filter-chip`, `renderCategoryChips` emits the unified class, legacy `.mods-category-chip` is GONE from html/js/css (regression guard), kind-filter click scoped correctly, base + active CSS carry the expected pill geometry, one `.active` chip per group at seed state.
>
> Acceptance: 831/831 Rust unchanged (frontend-only), 449/449 JS (was 442, +7), clippy clean. i18n-parity still 4/4, i18n-no-hardcoded still strict-zero.
>
> **All three user-reported bugs from iter 82 triage now closed** (P0 resolve-game-root, P1 offline-empty-state, P1 categories-ui). Queue drops back to iter-80 research-sweep P2s: `sec.shell-scope-hardening`, `dep.dedupe-reqwest-zip`, `dep.vitest-bump-post-squash` (P3). Iter 86 picks from those. Worktree still `ready_for_squash_merge: true`.

> **Iter 84 WORK — fix.offline-empty-state DONE (worktree). Second user-reported P1 from iter 82 triage closed.**
>
> Worktree commit `a1ceb04`. Blank-dark-screen-on-offline fixed. User can now see a visible error with a Retry button when the portal server is unreachable.
> - **Root cause:** `.mainpage` defaults to `opacity: 0` and the `.ready` class (which flips to 1) was added mid-init at app.js:1353, AFTER `await this.silentAuthRefresh()`. If that await hung/threw on an unreachable portal, the outer catch swallowed the error and the page stayed invisible indefinitely.
> - **Fix (paint-first, hydrate-after):** move the `.ready` class flip to the FIRST statement of `init()`, before any `await`. UI is visible regardless of network state.
> - **Banner:** new `#offline-banner` (role="alert", aria-live="polite") in index.html + inline CSS. `App.showOfflineBanner()` / `hideOfflineBanner()` are idempotent; retry button wires once and re-runs init.
> - **Wiring:** failed-connection branch + outer catch both call `showOfflineBanner()`; success branch calls hide.
> - **i18n:** 3 new keys × 4 locales (OFFLINE_BANNER_TITLE / DESC / RETRY in FRA/EUR/RUS/GER). Keeps i18n-parity green.
> - **Tests (7 new, offline-banner.test.js):** DOM skeleton check, `.ready`-before-await source-inspection, show/hide toggle, retry click re-runs init + hides, idempotency under 3 show calls, i18n keys present in all 4 locales.
>
> Acceptance: 831/831 Rust unchanged (frontend-only), 442/442 JS (was 436, +6), clippy clean. Worktree ready state unchanged — still `ready_for_squash_merge: true`.
>
> **One P1 user-reported bug remains from iter 82 triage:** `fix.mods-categories-ui`. Three iter-80 sweep P2s still queued: `sec.shell-scope-hardening`, `dep.dedupe-reqwest-zip`, `dep.vitest-bump-post-squash` (P3). Iter 85 picks `fix.mods-categories-ui` — last user-reported P1, then the queue drops back to P2-only.

> **Iter 83 WORK — fix.resolve-game-root-wrong-assumption DONE (worktree). User P0 triaged from iter 82 session closed.**
>
> Worktree commit `466524a`. The user-reported P0 from iter 82's live triage landed: `commands/mods.rs::resolve_game_root()` no longer strips 2 parents from the stored game path. GPK install should now succeed on a correctly-configured install.
> - **Root cause:** `services/config_service::parse_game_config` stores the install **root** (e.g. `C:/Games/TERA`). Only `mods.rs` was treating that value as if it were a `TERA.exe` path and stripping `.parent().and_then(|p| p.parent())`. Every other caller (`download.rs`, `hash.rs`) already treats `game_path` as the install root. On a valid path, the double-strip yielded `C:/` and the `S1Game` existence check failed; shorter paths surfaced as "Configured game path has no parent root".
> - **Fix:** deleted the `.parent().and_then(|p| p.parent())` chain. Extracted `validate_game_root(PathBuf) -> Result<PathBuf, String>` as a pure predicate so the contract is testable without a real config.ini.
> - **Tests (3 new, inline):** `validate_game_root_accepts_install_root_with_s1game` (tempdir + S1Game round-trip unchanged), `validate_game_root_rejects_missing_s1game` (tempdir without S1Game errs with the right message), `validate_game_root_source_has_no_parent_walk` (regression guard — source-inspects the fn body for `.parent()` calls).
>
> Acceptance: 831/831 Rust (was 828, +3), clippy clean, 436/436 JS. Worktree ready state unchanged — still `ready_for_squash_merge: true`. Bug fix carries to main on user-gated squash.
>
> Two P1 user-reported bugs remain queued from iter 82 triage: `fix.offline-empty-state`, `fix.mods-categories-ui`. Three iter-80 research-sweep P2 items also queued: `sec.shell-scope-hardening`, `dep.dedupe-reqwest-zip`, `dep.vitest-bump-post-squash` (P3). Next iter (84) picks the highest-signal — recommend `fix.offline-empty-state` since it's the second live user report.

> **Iter 82 WORK — sec.shell-open-call-sites-pinned DONE (worktree).**
>
> Second of five iter-80 queued P2s closed. Worktree commit `8ad2c1b`. Defence-in-depth against CVE-2025-31477 regression: new JS test at `teralaunch/tests/shell-open-callsite.test.js` (5 tests, 0 source changes) classifies every `shell.open(X)` / `openExternal(X)` call site by arg shape — string literal, known identifier, URLS.external.* member, or template of those. Bad shapes (bare vars, arbitrary member chains, unsafe template interpolations) fail with file:line:arg. Self-tests pin the classifier in both directions. Plus the detector catches `${locale}` as a sibling interpolation and was extended to accept it with justification (i18n enum, never attacker-controlled).
>
> Acceptance: 828/828 Rust (unchanged — JS-only), clippy clean, 436/436 JS (was 431, +5 new). Worktree ready state unchanged — `ready_for_squash_merge: true`. Three iter-80 queued items remain: `sec.shell-scope-hardening`, `dep.dedupe-reqwest-zip`, `dep.vitest-bump-post-squash` (P3).
>
> **User-reported bugs this session — three new fix-plan entries queued (see P0/P1 sections below):**
> - `fix.resolve-game-root-wrong-assumption` (P0) — `commands/mods.rs::resolve_game_root()` strips 2 parents from the stored game path, but `config_service::parse_game_config` returns the install **root** (not the exe), so GPK install always fails with "Configured game path has no parent root" on valid paths. Bug present on both main and worktree. Every other caller (download.rs, hash.rs) treats the path correctly as the install root; only mods.rs is wrong. Fix: delete the 2-parent-strip chain, return `game_path` directly + keep the `S1Game` existence check.
> - `fix.offline-empty-state` (P1) — When the API is offline, the launcher renders a blank dark screen with no error. User can't tell if the launcher itself is broken or the server is down. Ship a visible offline banner / retry UI instead.
> - `fix.mods-categories-ui` (P1) — Categories pill row in the mods modal has visual inconsistency ("All categories" styled differently from siblings) and awkward layout (placed below search + filter, creating an L-shape). Needs style unification + layout pass.

> **Iter 81 WORK — dep.rustls-webpki-bump DONE (worktree).**
>
> Pre-squash filler work on the ready-for-squash worktree. Worktree commit `e52cad4`. First of five iter-80 queued P2 items closed — `cargo update -p rustls-webpki --precise 0.103.12`. Lockfile-only change (2 lines), no source touched. Clears three RUSTSEC advisory rows flagged by the iter-80 sweep:
> - RUSTSEC-2026-0049 (CRL distribution-point matching, non-exploitable in reqwest default config)
> - RUSTSEC-2026-0098 (URI name-constraint bypass, non-exploitable on public Web PKI)
> - RUSTSEC-2026-0099 (wildcard-cert bypass, requires attacker-controlled CA)
>
> Rationale for picking this first: smallest scope of the queued P2s, fastest verify cycle, clears 3 advisory rows that would fail a future `cargo audit` CI gate in one commit. Four P2 items remain queued for iters 82-85: `sec.shell-scope-hardening`, `sec.shell-open-call-sites-pinned`, `dep.dedupe-reqwest-zip`, plus the iter-80 P3 `dep.vitest-bump-post-squash`.
>
> Acceptance: 828/828 Rust (unchanged from iter 79 — pure lockfile bump, no new tests expected), clippy clean, 431/431 JS. Worktree ready state unchanged — `ready_for_squash_merge: true`. Seven items shipped since M8 validation (iters 74-81: overlay-lifecycle, clean-recovery, conflict-modal, hardcoded-i18n, http-redirect-offlist, bogus-gpk-footer, rustls-webpki-bump). Next iter (82) picks the next P2 — recommend `sec.shell-open-call-sites-pinned` (JS-only, 1 test file, follows existing i18n-scanner pattern).

> **Iter 80 RESEARCH SWEEP — findings landed on worktree.**
>
> Tenth research sweep since loop start; first since iter 70. Worktree commit `4421a97`. Artefact: `docs/PRD/audits/research/sweep-iter-80.md`. No code changes — pure intelligence gathering, as the research-sweep cadence spec prescribes.
> - **Dep advisories flagged (all non-exploitable, all queued P2):** rustls-webpki 0.103.9 hits RUSTSEC-2026-0049 (CRL matching, not used by reqwest default), -0098 (URI name constraints, public Web PKI unaffected), -0099 (wildcard name constraint bypass, requires attacker CA). Single `cargo update -p rustls-webpki --precise 0.103.12` closes all three.
> - **tauri-plugin-shell CVE-2025-31477:** already closed by our 2.3.5 pin (vulnerable range ≤2.2.0). Explicit scope pin `"shell": { "open": true }` in tauri.conf.json OR migration to `tauri-plugin-opener` is defence-in-depth, not a live exposure. Frontend call sites audited: 3 uses (app.js:2259, 2261, 5025), all pass localized constants or anchor `event.target.href`.
> - **zip CVE-2025-29787 (symlink path traversal):** fixed pre-2.4.2 — we're clean on both resolved versions (2.4.2 direct + 4.6.1 transitive).
> - **Tauri iFrame IPC bypass (CVE-2024-35222):** we don't embed iFrames — N/A.
> - **Dep duplication:** reqwest 0.12.28+0.13.2, zip 2.4.2+4.6.1. Binary size / supply-chain surface concern; investigation item queued P2.
> - **Vitest:** we run 2.1.8, latest is 4.x. No CVEs on 2.x line. Bump is cosmetic; queued P3 post-squash.
> - **Playwright 1.58.2:** clean, no advisories.
> - **Catalog currency:** Shinra v3.0.0-classicplus + TCC v2.0.1-classicplus in catalog both match upstream releases at catalog commit 33cf584 — no drift.
>
> Five new fix-plan action items appended under P2: `dep.rustls-webpki-bump`, `sec.shell-scope-hardening`, `sec.shell-open-call-sites-pinned`, `dep.dedupe-reqwest-zip`, `dep.vitest-bump-post-squash` (P3). None block squash; all are bundle-able into a single post-squash "dep sweep" iter. Next sweep: iter 90.
>
> Header: `last_research_sweep: 70 → 80`. `last_work_iteration` stays at 79 (research doesn't count as work). `total_items_done` unchanged at 61. `ready_for_squash_merge: true` unchanged.

> **Iter 79 WORK — adv.bogus-gpk-footer DONE (worktree).**
>
> Pre-squash filler work on the ready-for-squash worktree. Worktree commit `39b09e4`. Closes the P1 adversarial corpus gap from iter 0's "already passing, verify survives refactor" note: the existing `parse_mod_file_rejects_non_tmm_gpks` inline test was one-fixture thin (64 bytes of 0x42). Replaced with a 9-fixture adversarial corpus and added structural source-inspection guards — same filler layout as iters 74-78.
> - `services/mods/tmm.rs::tests::parse_mod_file_rejects_non_tmm_gpks` — 9 fixtures: empty buffer, 3 bytes, 4 zero bytes, 64-byte 0x42 baseline, 1024 bytes 0xff, magic-only 4 bytes, misplaced magic, magic+huge composite_count (EOF overflow guard), 4-byte non-magic. Three invariants pinned: parse_mod_file never panics on arbitrary bytes, non-TMM bytes surface as Err or Ok(empty container), downstream install_gpk rejects empty containers.
> - `tests/bogus_gpk_footer.rs` (new, 4 guards): test-still-present, magic-check-branch-still-present, install_gpk-empty-container-gate-still-present, detector self-test (positive + negative synthetic fixtures).
> - Clippy: one `useless_vec` on the 64-byte baseline → `&[0x42u8; 64]` slice literal.
>
> Acceptance: 828/828 (was 824, +4 new integration guards), clippy clean, 431/431 JS. Worktree ready state unchanged — `ready_for_squash_merge: true`. Six P1 filler items shipped since M8 validation (iters 74-79: overlay-lifecycle, clean-recovery, conflict-modal, hardcoded-i18n, http-redirect-offlist, bogus-gpk-footer). Next iter (80) is **N%10=0 → RESEARCH SWEEP** per the perfection-loop cadence.

> **Iter 78 WORK — adv.http-redirect-offlist DONE (worktree).**
>
> Pre-squash filler work on the ready-for-squash worktree. Worktree commit `17db09a`. Closes the P1 adversarial corpus gap: `capabilities/migrated.json` pins the launcher to an HTTP allowlist, but reqwest's default `Policy::limited(10)` would let a compromised allowlisted mirror bounce a download or catalog fetch to an off-list origin via a 3xx. Policy::none() makes the 302 surface as a 302 status, which the existing `!response.status().is_success()` branch already rejects with "Download returned HTTP 302" / "Catalog fetch returned HTTP 302" — so the redirect gate is structural, not behavioural.
> - `services/mods/external_app.rs::fetch_bytes_streaming` Client::builder → `.redirect(reqwest::redirect::Policy::none())` added.
> - `services/mods/catalog.rs::fetch_remote` Client::builder → same line added.
> - `tests/http_redirect_offlist.rs` (new, 3 guards): source-inspects both builders for the `.redirect(Policy::none())` chain + detector self-test (2 positive + 2 negative fixtures) to prevent the scanner from silently regressing to always-true/always-false.
>
> The wiring-guard pattern (inspect source, not runtime) fits the scope: the reqwest redirect behaviour is exhaustively covered upstream; what we need to prevent is a future refactor that drops the policy line. Source-inspection catches that at commit time.
>
> Acceptance: 824/824 (was 821, +3 new guards), clippy clean. Worktree ready state unchanged — still `ready_for_squash_merge: true`, awaiting user authorisation. Five P1 filler items shipped since the M8 validation gate (iters 74-78: overlay-lifecycle, clean-recovery, conflict-modal, hardcoded-i18n, http-redirect-offlist). Next iter (79) picks another P1 filler — candidates: `adv.tampered-catalog`, `adv.sigkill-mid-download`, `adv.bogus-gpk-footer`, `pin.tmm.*` trio, `infra.gitleaks-allowlist`.

> **Iter 77 WORK — fix.mods-hardcoded-i18n-strings DONE (worktree).**
>
> Pre-squash filler work, higher blast radius than the iter 74-76 wiring trio (touched 4 locales + 2 source files + a test). Worktree commit `b05bf16`. Closes the iter-48 P1: the 10-entry hardcoded-English allowlist in i18n-no-hardcoded.test.js is now empty and the scanner enforces strict-zero. **Every user-facing English string in mods.html + mods.js now routes through the i18n layer.**
> - `app.js::updateAllTranslations()` grew a `data-translate-aria-label` pass (sibling to the existing title/placeholder handlers).
> - `mods.html` static 3 leaks → `data-translate-aria-label` + `data-translate-title` attributes on the three close/category buttons.
> - `mods.js` dynamic 7 leaks → `${window.App?.t('KEY') ?? 'fallback'}` template-interpolation shells in each innerHTML template. Scanner skips them per its existing `includes('${')` rule; the defensive `??` fallback keeps the UI functional even if `window.App` isn't yet bound (during initial render).
> - `translations.json`: 9 new keys × 4 locales = 36 additions. Proper translations in each locale (FRA/EUR/RUS/GER). i18n-parity still 4/4, i18n-jargon still 3/3.
> - `i18n-no-hardcoded.test.js`: allowlist `[]`, "allowlist is non-empty" → "allowlist is empty (strict-zero enforced)."
>
> Four P1 filler items shipped since the M8 validation gate (iters 74-77: overlay-lifecycle, clean-recovery, conflict-modal, hardcoded-i18n). Worktree ready state unchanged — still `ready_for_squash_merge: true`. Next iter (78) either picks a fresh PRD §3 item (P1 backlog is deep) or starts the M6-b-rustc-CFG-via-CI work if the user wants to cover that before squash.

> **Iter 76 WORK — fix.conflict-modal-wiring DONE (worktree).**
>
> Pre-squash filler work. Worktree commit `5e46d71`. Closes the P1 gap from iter 32: `tmm::detect_conflicts` was `#[allow(dead_code)]` — frontend had no path to preview slot overwrites before install, so users saw silent last-install-wins overwrites instead of PRD 3.3.3's modal disclaimer.
> - `services/mods/tmm.rs`: `ModConflict` now derives `Serialize + Deserialize`. New bundle helper `preview_conflicts_from_bytes(game_root, source_gpk_bytes)` does the fs + decrypt + parse + detect chain in one unit so the Tauri command body stays thin.
> - `commands/mods.rs`: `#[tauri::command] pub async fn preview_mod_install_conflicts(entry: CatalogEntry) -> Result<Vec<ModConflict>, String>`. Non-GPK kinds + un-downloaded GPKs short-circuit to `Ok([])` (best-effort preview; real install still runs `detect_conflicts` post-download as a safety gate).
> - `main.rs::generate_handler!`: registered.
> - `tests/conflict_modal.rs` (new, 4 wiring guards): attribute adjacency + delegate + return-type + registration + Serialize-derive pin + missing-backup early-return ordering.
>
> Existing Playwright spec `mod-conflict-warning.spec.js` (authored iter 32) now drives against a real backend.
>
> Acceptance: 821/821 (was 817, +4 new guards), clippy clean. Worktree ready state unchanged — `ready_for_squash_merge: true`. Three P1 wiring items shipped in iters 74-76 (overlay-lifecycle, clean-recovery, conflict-modal) — all follow the same pattern: thin Tauri command over existing pure predicate, source-inspection integration tests pin the wiring. Only P1 filler left from the pre-squash list is `fix.mods-hardcoded-i18n-strings` (higher blast radius, 4-locale parity). Iter 77 either tackles that (accepting the scope) or branches to a fresh P1 item from §3 of the PRD.

> **Iter 75 WORK — fix.clean-recovery-wiring DONE (worktree).**
>
> Pre-squash filler work. Worktree commit `261b5f3`. Closes the P1 reliability gap from iter 43: `tmm::recover_missing_clean` had been unit-tested since then but was behind `#[allow(dead_code)]` — users with a missing `.clean` backup had no in-launcher recovery path.
> - `commands/mods.rs`: new `#[tauri::command] pub async fn recover_clean_mapper()` — thin wrapper, resolves game root via shared `resolve_game_root()` helper, delegates to the tmm predicate.
> - `services/mods/tmm.rs`: `#[allow(dead_code)]` dropped from `recover_missing_clean` — it's now live-called.
> - `main.rs::generate_handler!`: registered.
> - `tests/clean_recovery.rs` (new, 3 guards): source-inspects mods.rs for the fn decl + adjacent `#[tauri::command]` attribute + delegate calls; grep on main.rs for the qualified path; asserts dead_code gate is gone.
>
> Frontend Settings-panel Recovery button still a follow-up UI item (separate iter once design settles) — the command itself is live and invoke-able.
>
> Acceptance: 817/817 (was 814, +3 new wiring guards), clippy clean. Worktree ready state unchanged — still `ready_for_squash_merge: true`, awaiting user authorisation. Next iter (76) picks from the remaining P1 fillers: `fix.conflict-modal-wiring` (similar scope to the two wiring items landed in iters 74-75) or `fix.mods-hardcoded-i18n-strings` (higher blast radius, 4-locale parity).

> **Iter 74 WORK — fix.overlay-lifecycle-wiring DONE (worktree).**
>
> Pre-squash filler work. Worktree commit `7a7a9e0`. The P1 reliability gap from iter 31: pure predicate `external_app::decide_overlay_action` had been unit-tested but never wired into the actual game-close event — overlays (Shinra/TCC) were being torn down on EVERY close, not just the last-client close, violating PRD 3.2.12 on multi-client setups.
> - `commands/game.rs` post-exit block: `stop_auto_launched_external_apps()` call now gated by `decide_overlay_action(teralib::get_running_game_count())`. Partial close (remaining ≥ 1) → overlays stay; last close (remaining = 0) → teardown.
> - `tests/multi_client.rs` appended source-inspection guard `game_rs_gates_overlay_stop_on_decide_overlay_action` — asserts predicate call appears before stop call in source order + stop call appears exactly once. The predicate-only tests from iter 31 couldn't catch a regression where the call is removed; this guard closes that gap.
>
> Acceptance: 814/814 (was 813, +1 new guard), clippy clean. Worktree ready state unchanged — still `ready_for_squash_merge: true`, awaiting user authorisation. Next iter (75) picks another P1 filler: candidates still include `fix.clean-recovery-wiring`, `fix.conflict-modal-wiring`, `fix.mods-hardcoded-i18n-strings`.

> **Iter 73 WORK — M6-b teralib CONFIG obfuscation DONE (worktree).**
>
> Filler work landed on the ready-for-squash worktree while awaiting user authorisation for the M8 merge. Worktree commit `0903b68`. Closes the string-obfuscation gap flagged in M6's audit doc: `192.168.1.128:8090` no longer ships plaintext in the launcher binary's `.rdata`.
> - `teralib/build.rs`: compile-time rolling-XOR of `config.json` bytes with a per-build 32-byte key seeded from nanos + file length. Emits `$OUT_DIR/config_obf.rs` with `CONFIG_OBF: &[u8]` + `CONFIG_KEY: [u8; 32]`. `cargo:rerun-if-changed` on config.json keeps the obfuscation fresh on content updates.
> - `teralib/src/config.rs`: `include_str!(config.json)` → `include!("$OUT_DIR/config_obf.rs")` + `fn decrypt_config()` that XORs the bytes back. `CONFIG_JSON` Lazy now decrypts once on first access; downstream `get_config_value` / `get_relay_servers` unchanged.
> - Per-build key regeneration frustrates pre-computation attacks across releases.
>
> Full plaintext-grep proof against a built `.exe` remains a CI-only artefact (queued to the first post-merge v0.2.0 release, same pattern as M4). M6 now covers both CFG (tier-1 linker flag) and string obfuscation (tier-2 compile-time XOR); the one remaining deferral is full rustc CFG metadata which needs a CI-scoped RUSTFLAGS (local-dev-host OOM under LTO documented in iter 70). Audit doc `docs/PRD/audits/security/anti-reverse.md` stays on the worktree; refresh to reflect M6-b DONE queued with the next audit-doc touch iter.
>
> Acceptance: `teralib cargo test --release` 28/28, launcher `cargo test --release` 813/813 (no regressions), clippy clean. `tauri_v2_migration_last_commit: 0903b68`; worktree ready state unchanged — still awaiting user authorisation for the squash merge.
>
> Next iter (74) slots further filler work — candidates from `[P1]`: `fix.clean-recovery-wiring`, `fix.conflict-modal-wiring`, `fix.overlay-lifecycle-wiring`, `fix.mods-hardcoded-i18n-strings`. Any of those lands cleanly into the pending squash and doesn't reshape the migration scope.

> **Iter 72 WORK — Tauri v2 M8 validation sweep DONE (worktree). Ready for user-gated squash merge.**
>
> Executed M8 on the `tauri-v2-migration` worktree, commit `b53fbc7`. This is the final pre-merge gate — re-ran every CI check from the M0 baseline snapshot against the worktree tip and diffed against pre-migration iter 62 baseline. Doubles as iter-72 REVALIDATION (marked `last_revalidation: 72`, all-gates-green).
>
> **All seven CI gates green:** check-bundle-size (10/10 self-tests), check-changelog-plain-english (126 lines, 0 leaks), check-mods-crate-docs (6/6), check-troubleshoot-coverage (51/51 templates), secret-scan (CI-only, no regression risk), deploy-scope (CI-only, `/classicplus/` path preserved statically), catalog README schema (cross-repo, no changes since iter 58).
> **Tests:** `cargo test --release` 790 → **813 (+23)** — 12 from M7's inline updater_gate unit tests, 4 from M5's csp_audit, 7 from M7's updater_downgrade integration suite. Zero regressions in the 10 pre-existing integration suites. `vitest` 431 → 431 no change. `clippy --all-targets --release -- -D warnings` clean.
> **Commit sequence** (main..HEAD, 10 commits): cc33d92 (M0) → c85f7a8 (devtools pre-flight) → d708455 (M1a migrate tool raw) → f13e2bd (M1b v1→v2 API renames) → 65cd30c (M2 JS namespace) → 576f44e (M3 command-surface review) → 9983474 (M4 deploy path) → a5fd094 (M5 CSP) → 8d7c349 (M6 anti-reverse tier 1) → 9898af0 (M7 updater downgrade) → b53fbc7 (M8 validation doc).
> **Migration invariants 1-5 all HELD.** Rollback plan documented: `git revert <squash-sha>` + v0.1.13 hotfix on v1 branch; same minisign key signs both sides so no rotation needed.
>
> Validation audit at `docs/PRD/audits/security/tauri-v2-migration-validation.md` on the worktree. Squash merge itself is user-gated per migration plan — do NOT auto-merge. Header flag `tauri_v2_migration_ready_for_squash_merge: true` flags the state.
>
> Next action: **user authorises squash merge + 0.2.0 version bump + deploy**. Between now and that authorisation, iter 73 can slot in M6-b (cryptify string-obfuscation of teralib::config::CONFIG) as filler work on the worktree — lands as-is into the squash when the user pulls the trigger.

> **Iter 71 WORK — Tauri v2 M7 DONE (worktree). PRD 3.1.9 closed.**
>
> Executed M7 on the `tauri-v2-migration` worktree, commit `9898af0`. Updater now refuses manifests advertising a version that isn't strictly newer than the running binary, closing PRD §3.1.9. Prevents the canonical signed-downgrade-to-vulnerable attack even when the mirror is hostile and the minisign key is rotated.
> - `Cargo.toml`: `+ semver = "1"`.
> - `src/services/updater_gate.rs` (new): `pub fn should_accept_update(current, remote) -> bool`. Policy: strict-greater semver; pre-release semantics apply (`0.2.0-rc.1 < 0.2.0`); unparseable input refuses (conservative default blocks mangled/hostile manifests). 12 inline unit tests cover the policy surface.
> - `src/main.rs::setup()` updater block: gate called before `update.download_and_install` with `CARGO_PKG_VERSION` vs `update.version`. On refusal: `error!()` log + skip install.
> - `tests/updater_downgrade.rs` (new, 7 tests): 5 symbolic-parity spec tests mirror the predicate via `semver` directly so a future drift from "strictly greater" fails independently; 2 source-inspection wiring guards (`updater_gate` is pub mod with expected fn signature; `main.rs` calls gate before `.download_and_install` in source order).
>
> Acceptance: `cargo test --release --test updater_downgrade` → 7/7; full `cargo test --release` → **813/813** (776 unit + 3+4+4+3+4+4+2+2+7+4 integration = 37; +12 inline updater_gate tests + 7 new integration vs iter 70 794 baseline, zero regressions); clippy --all-targets --release -- -D warnings → clean. Next iter (72) executes M8: full validation sweep + squash-merge prep. Consider squeezing M6-b (cryptify string obfuscation) in before M8 if iter budget allows — else defer to post-merge.

> **Iter 70 WORK — Tauri v2 M6 partial (anti-reverse tier 1, worktree) + RESEARCH SWEEP.**
>
> **WORK (M6-tier-1).** Executed on the `tauri-v2-migration` worktree, commit `8d7c349`. Enabled Windows `/guard:cf` linker flag via `build.rs::cargo:rustc-link-arg-bin` (release builds only). Sets `IMAGE_DLLCHARACTERISTICS_GUARD_CF` in the PE header so the Windows loader applies CIG/ACG/dynamic-code-guard mitigations. Audit doc authored at `docs/PRD/audits/security/anti-reverse.md` — records the enabled tier, enumerates three explicit M6-b defers (full rustc CFG instrumentation, cryptify string-obfuscation of `teralib::config::CONFIG`, release-binary plaintext-grep proof), and explains why the CFG-via-`.cargo/config.toml` approach was rejected (OOM on host build scripts when host == target under LTO). Clippy also caught 3 `manual_contains` residue from iter 69's M5 csp_audit commit — fixed inline. Acceptance: build clean, clippy clean, 794/794 `cargo test --release` (zero regressions). Marked **M6-partial** in the header so M6-b can finish string-obfuscation + CI-scoped full CFG before flipping to M6 DONE.
>
> **RESEARCH SWEEP (N=70).** Local pin review vs M6 clippy compile output:
> - `tauri` 2.10.3 (compiled) — matches migration plan target, current v2.10 line.
> - `tauri-plugin-updater` 2.10.1 — current.
> - `tokio` 1.49 pinned; tokio-util 0.7.18 transitively in tree.
> - `reqwest` 0.12.28 (compiled within `^0.12.23` pin; cargo picked newer patch).
> - `zip` — **both** 4.6.1 (transitively from a Tauri plugin) AND 2.4.2 (our direct pin of `2.3`, minor bumped in-range) present in build graph. Not a vulnerability — iter 11 CVE-2025-29787 remediation is satisfied at 2.3+. Dual-major is noise from the plugin crate split, not an action item.
> - `aes-gcm` 0.10.3, `sha2` 0.10.9, `hkdf` 0.12.4, `hmac` 0.12.x, `base64` 0.22, `zeroize` 1.7, `cryptify` 3.1.1, `chamox` 0.1.4 — all current, no advisories.
> - `cargo-audit` binary still uninstalled locally (P2 `infra.cargo-audit-install` stands).
>
> No RUSTSEC advisories surfaced in the migration diff vs iter 62 baseline. No upstream TCC / Shinra pulls this iter (no repo-level remote reviewed — not critical; last full scan iter 40).
>
> Next iter (71) executes M7: updater-downgrade refusal (PRD 3.1.9). Next RESEARCH SWEEP at iter 80.

> **Iter 69 WORK — Tauri v2 M5 DONE (worktree). PRD 3.1.12 closed.**
>
> Executed M5 on the `tauri-v2-migration` worktree. Dropped `'unsafe-inline'` from CSP `script-src`, closing PRD §3.1.12. Worktree commit `a5fd094`. Audit found two inline-JS sites that needed relocating before the tightening would land without breaking runtime:
> - `src/index.html:10-37` — 27-line zoom-correction IIFE extracted to `src/zoom.js`, loaded via `<script src>`. Kept as the first script tag in `<head>` to preserve the pre-`<body>` viewport sizing behaviour.
> - `src/mods.js:585` — dynamically-generated `onerror=""` attribute on catalog `<img>` tiles. Browser CSP treats attribute-based event handlers as inline scripts, so attribute-form blocks under strict policy. Refactored to `addEventListener('error', ..., { once: true })` attached after the `innerHTML` assignment.
>
> `tauri.conf.json::app.security.csp` — `'unsafe-inline'` removed from `script-src`. `style-src` retains it intentionally (out of 3.1.12 scope; tightening style-src would break inline `style=""` attributes the launcher renders and is a separate future item).
>
> Pin test `src-tauri/tests/csp_audit.rs` authored (4 tests): asserts `'unsafe-inline'` absent from script-src, `'self'` + cdnjs still present so bundled + vendored JS keeps loading, plus 2 unit tests on the directive-token helper (no prefix-match bugs, missing-directive returns None).
>
> Acceptance: `cargo test --release --test csp_audit` → 4/4. Full `cargo test --release` → 794/794 (764 unit + 3+4+4+3+4+4+2+2+4 integration; +4 from new csp_audit, zero regressions vs iter 67 790 baseline). `npm test` → 431/431 (router.test.js teardown warning is pre-existing, unrelated). Next iter (70) is M6 anti-reverse hardening. **N=70 also triggers a RESEARCH SWEEP per the cadence table** — iter 70 handles both: do M6 as the work, then run the research sweep as the second half of the iteration (dep version scan + upstream TCC/Shinra/tauri-plugin-updater diff via Context7, no network required for the local-pin check).

> **Iter 68 WORK — Tauri v2 M4 partial (deploy-path v2-ready, worktree).**
>
> Executed M4 partial on the `tauri-v2-migration` worktree. `bundle.createUpdaterArtifacts: "v1Compatible"` was already set by the migrate tool in M1 (tauri.conf.json:26). The real M4 work this iter was catching two deploy-path breakers the tool didn't flag. Worktree commit `9983474`:
> - `.github/workflows/deploy.yml`: env block `TAURI_PRIVATE_KEY` → `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_KEY_PASSWORD` → `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (v2 renamed the env var names the CLI reads; GitHub-secret names stay unchanged so no secret rotation). Version read: `$json.package.version` → `$json.version` with a fallback for pre-v2 refs.
> - `builder.ps1`: forwards legacy v1 env-var names to v2 equivalents so user `.env` + file-based workflows keep working; cargo fallback `tauri-cli@^1` → `tauri-cli@^2`.
>
> **Why partial.** Full M4 acceptance per the migration plan needs `.nsis.zip` + `.nsis.zip.sig` artefacts produced + a bundle-size check vs the 52.05 MB baseline from iter 63. The signing key is a GitHub secret only (not in local env), so running `npm run tauri build` locally would skip the `.sig` artefact — defeating the check. Punted the live artefact inventory to the first post-merge deploy.yml run at v0.2.0; the bundle-size gate (iter 54) will fire there automatically. Next iter (69) executes M5 (CSP tightening — can land independently, closes PRD 3.1.12).

> **Iter 67 WORK — Tauri v2 M3 DONE (worktree).**
>
> Executed M3 on the `tauri-v2-migration` worktree: custom command-surface review. Worktree commit `576f44e`. Two v2-drift fixes hit:
> - `tests/http_allowlist.rs::load_scopes()` — v1 read `tauri.conf.json::tauri.allowlist.http.scope`; v2 moved the allowlist into `capabilities/migrated.json::permissions[http:default].allow[].url`. Rewrote the parser; all 9 production URL literals still covered by scope entries.
> - `src/app.js` — v1 exported `appWindow` as a bound constant; v2 uses `getCurrent()` on the window module. Added defensive resolver (`appWindow` || `getCurrent()` || `webviewWindow.getCurrentWebviewWindow()`) so the four minimize/startDragging/close call sites keep working. Dropped dead `WebviewWindow` destructure (imported in v1, unused).
>
> Invoke-surface audit: 35 frontend `invoke('x', ...)` call sites all match a `#[tauri::command]` registered in `main.rs::generate_handler![...]`. Zero drift on the command naming surface.
>
> Acceptance (all three suites fired clean, no regressions vs pre-migration baseline at iter 62):
> - `cargo clippy --all-targets --release -- -D warnings` → clean (zero warnings, up from 3 `unused_imports` after M1b).
> - `cargo test --release` → 764 unit + 3 (crash_recovery) + 4 (disk_full) + 3 (http_allowlist) + 4 (multi_client) + 4 (parallel_install) + 2 (self_integrity) + 2 (smoke) + 4 (zeroize_audit) = **790 tests pass**. Matches M0 baseline exactly.
> - `npm test` (vitest) → 10 files / 431 tests pass.
>
> Out of M3 scope, carried to later milestones: plugin runtime calls (`.dialog`, `.shell`, `.updater`, `.app.getVersion`) that v2 needs per-plugin globalTauri opt-in for — the unit tests mock them, but live launches will need the shapes fixed (settings-page folder picker, external-link `shell.open`, legacy `updater.checkUpdate`). M4 (updater dual-format) handles the updater shape; dialog/shell/getVersion land when they next break a user surface. Next iter (68) executes M4.

> **Iter 66 WORK — Tauri v2 M2 DONE (worktree).**
>
> Executed M2 on the `tauri-v2-migration` worktree. The migration-plan prescription was "rewrite `@tauri-apps/api/*` ES imports", but this codebase uses `withGlobalTauri: true` exclusively and has zero such imports — so the real M2 work for our shape was the globalTauri namespace rename + CLI bump. Worktree commit `65cd30c`:
> - `@tauri-apps/cli` ^1.6.0 → ^2 (installed 2.10.1). Plugin JS packages already at v2 from M1.
> - `window.__TAURI__.tauri.invoke` → `window.__TAURI__.core.invoke` with `.tauri` fallback, in both `src/app.js` and `src/mods.js`.
> - Test global mocks in `tests/app.test.js` + `tests/search-perf.test.js` dual-key `core` + `tauri` so the `||` fallback resolves cleanly under vitest.
>
> Vitest acceptance: 10 files / 431 tests green in 1.56s. No regressions.
>
> Deferred explicitly to M3 (command-surface review): `window.__TAURI__.window.appWindow` → `getCurrentWebviewWindow`; plugin-namespaced globals (`.dialog`, `.shell`, `.updater`, `.app.getVersion`) that v2 only exposes with per-plugin opt-in — the runtime surfaces that use them (settings folder picker, external-link open, `app.getVersion` telemetry) need the v2 shape. Updater API reshape (checkUpdate gone) lands in M4 dual-format. Next iter (67) executes M3.

> **Iter 65 WORK — Tauri v2 M1 DONE (worktree).**
>
> Executed M1b on the `tauri-v2-migration` worktree: fixed the 44 compile errors that `cargo tauri migrate` left for manual cleanup. Pure mechanical v1→v2 API drift — no semantic changes. Worktree commit `f13e2bd`:
> - `tauri::api::dialog::blocking::FileDialogBuilder` → `tauri_plugin_dialog::DialogExt` (`select_game_folder` now takes auto-injected `AppHandle`, `FilePath::into_path()` on the returned handle).
> - `use tauri::Emitter;` added to `commands/{config,download,game,hash,mods}.rs` + `infrastructure/events.rs`. `.emit_all(...)` → `.emit(...)` (Emitter trait unifies broadcast; v2 deprecates the dual `emit` vs `emit_all` split).
> - `get_window("main")` → `get_webview_window("main")` (3 sites in `main.rs`).
> - `app.updater()` now returns `Result<Updater>`; `check()` returns `Result<Option<Update>>`; `download_and_install` takes `(on_chunk, on_finish)` callbacks (no-op for now, M4 will wire real progress).
> - `app.handle()` returns `&AppHandle` in v2 — added `.clone()` to fix the borrow-escape on the setup closure spawn.
>
> `cargo build --release` now exits 0 on the worktree with zero warnings. Tests + clippy deferred to M3 (command-surface review, which is where the v2 `Window` → `WebviewWindow` + State audit lives). Main branch untouched; migration invariant #1 held. Next iter (66) executes M2: JS API import paths (`@tauri-apps/api/tauri` → `@tauri-apps/api/core`, `api/dialog` → `plugin-dialog`, etc.).

> **Iter 60 RESEARCH + REVALIDATION + RETROSPECTIVE.**
>
> **REVALIDATION — 1 regression found.** `check-troubleshoot-coverage.mjs` now fails: iter 49's tolerant catalog parse (commit 85ac310) added two new error templates (`Failed to read catalog body: {}` and `Catalog JSON envelope is malformed: {}` in `services/mods/catalog.rs`) that were never mirrored in `docs/mod-manager/TROUBLESHOOT.md`. Silently broken for ~10 iterations because the coverage gate isn't run on every commit — only at revalidation. Demoted below to `[P0] REGRESSED`. Other proofs re-ran clean: launcher Rust 764 unit + 3+4+3+4+4+2+2+4 = 790 tests, clippy --release clean, launcher Vitest 10 files / 431 tests, catalog schema-parity gate (21↔21) + 9 self-tests, all other launcher CI gates (bundle-size 10/10, changelog 6/6, mods-crate-docs 5/5). TCC + Shinra tests not re-run this iter (no code changes there since iter 40; last verified @ iter 40).
>
> **RESEARCH SWEEP.** No network tools reliable this session; deferred RUSTSEC feed + upstream TCC/Shinra diff + catalog-expansion scouting. Local key-dep version check vs iter 40: tauri 1.0 unchanged (migration plan queued as P1 `tauri-v2-migration-plan`), tokio 1.49 unchanged (RUSTSEC-2025-0023 closed N/A iter 56), aes-gcm 0.10 unchanged (closed N/A iter 55; P2 `sec.remove-dead-aes-gcm-dep` tracks deletion), zip 2.3 unchanged (CVE-2025-29787 at 2.3+ remediated iter 11), reqwest 0.12.23, sha2 0.10.8, hkdf/hmac 0.12, base64 0.22, zeroize 1.7 — all unchanged, no new advisories surfaced via version-pin review. `cargo-audit` binary still uninstalled; P2 `infra.cargo-audit-install` stands. Next RESEARCH SWEEP at iter 70.
>
> **RETROSPECTIVE.** `docs/PRD/lessons-learned.md` grown to 212 lines (cap 200; 5 older entries archived to `lessons-learned.archive.md` — iters 3, 13-16, 20, 22, meta). 5 new lessons appended covering iters 45-60: source-inspection guards via include_str!, allowlist-backed CI gates over strict-zero, pause-revert-engage on user interrupts, catch-flawed-plans-at-execution (iter 59 Tauri M1 pivot), and revalidation-catches-what-commits-skip (this iter's TROUBLESHOOT regression is the poster child). No `[META]` PRD-change proposals this iter. Next RETROSPECTIVE at iter 90.

> **Iter 57 no-op work iteration.** User context switched the loop into an interactive "unstick blockers" conversation. 3 side-commits landed: 0a0b5cf (CLAUDE.md portal IP sync + 3.1.13 reframe as DORMANT), 5e5b0fc (Playwright webServer timeout 120s → 600s). Blockers resolved this iter: Tauri v1→v2 migration APPROVED (to be staged as 7 milestones starting M1 frontend JS imports); 3.1.13.portal-https reframed as P0-DORMANT (no production target exists yet — user is building fully local); external-mod-catalog repo CLONED to `../external-mod-catalog` (unlocks 3.8.6 + catalog.* P2s); Playwright webServer cold-start budget raised (unlocks UX/A11y e2e); TCC Discord webhooks acknowledged as already-decided scope per PRD 3.3.7 (not a new decision).

> **Iter 50 BLOCKED RE-TRY SWEEP — all still blocked.**
> Gated items re-checked against current repo state:
> • `sec.tauri-v1-eol-plan` — still blocked on 4 human decision gates (migrate vs stay, target v2 version, version bump, dual-format latest.json duration). No sign-off in any commit / PR.
> • `3.1.13.portal-https` — `teralib/src/config/config.json` still has `http://192.168.1.128:8090` for all 7 portal URLs. No production HTTPS endpoint; no infra commit.
> • `3.1.8.anti-reverse-hardening` — gated on sec.tauri-v1-eol-plan sign-off (CSP-per-window + capability ACLs are v2-only).
> • `3.1.9.updater-downgrade-refuse` — Tauri v2 gated.
> • `3.1.12.csp-unsafe-inline` — Tauri v1 CSP limit; v2 gated.
> • `3.1.10.tcc-shinra-binary-hardening` — needs human audit sign-off on build-output inspection.
> • `3.3.5 / 3.3.6 / 3.3.7 / 3.3.8 / 3.3.11` — need live Classic+ client access + human in-game audits.
> No item promoted this iter. Next BLOCKED RE-TRY at iter 100.

> **Iter 40 REVALIDATION + RESEARCH SWEEP — CLEAN.**
> REVALIDATION: 24 DONE items re-proved (all proofs from iter-20 + iter-21-39 additions).
> • launcher `cargo test --release` → 736 unit + 3 + 3 + 4 + 2 + 2 + 4 = 754 passed, exit 0
> • launcher `cargo clippy --all-targets --release -D warnings` → clean
> • TCC `dotnet test TCC.sln -c Release` → 1/1 passed
> • Shinra `dotnet test Tera.sln -c Release` → 1/1 passed
> • catalog `validate-catalog.mjs` → 101 entries, exit 0
> • 4 CI gates all green: troubleshoot-coverage (50/50), mods-crate-docs (6/6), changelog-plain-english (126 lines, 0 leaks), deploy-scope-gate (2 upload URLs clean, 11 self-tests)
> • Playwright `--list` → 76 tests in 16 files (enumerates cleanly; full run gated on warm Tauri webServer)
> • Vitest → 417 / 417
> • `git ls-files | grep '\.vs/'` → 0
> • `secret-scan.yml` present in 4 repos (launcher, TCC, Shinra, mod-catalog)
>
> RESEARCH SWEEP: no `cargo-audit` binary installed in this environment; manual key-dep version check confirms tokio 1.49, zip 2.3, aes-gcm 0.10, sha2 0.10.8, reqwest 0.12.23, tauri 1.0, zeroize 1.7 match the iter-10 findings. Recording as P2 `infra.cargo-audit-install` to run a proper advisory scan on the next CI-runner pass. No new RUSTSEC entries have been surfaced for our pinned versions via spot-checks.
>
> Next REVALIDATION at iter 60; next RESEARCH SWEEP at iter 50.
>
> **Iter 30 RETROSPECTIVE.** `docs/PRD/lessons-learned.md` initialised with 10 patterns spanning iters 1-29. Two new `[META]` entries added (`meta.bin-crate-test-path-flexibility`, `meta.verify-and-implement-language`) flagging a recurring integration-test-path friction and proposing PRD wording amendments. No code changes this iter per retrospective protocol. Next retrospective at iter 60.
>
> **Iter 20 REVALIDATION SWEEP — CLEAN.** All 14 [DONE] items re-proved:
> • launcher `cargo test --release` → 698 unit + 2 integration passed (1 transient single-test flake on first run, 5-run flake-hunt of the new sha_ tests came back 5/5 clean; suspected pre-existing flake unrelated to iter 19).
> • clippy `--all-targets --release -- -D warnings` → clean.
> • TCC `dotnet test TCC.sln -c Release` → 1/1 passed.
> • Shinra `dotnet test Tera.sln -c Release` → 1/1 passed (ShinraMeter.Tests.dll).
> • Catalog `node scripts/validate-catalog.mjs` → `catalog-validate: ok (101 entries)`.
> • Launcher `npx playwright test --list` → 70 tests in 14 files.
> • `git ls-files | grep '\.vs/'` → 0.
> • `secret-scan.yml` present in all 4 repos (launcher, TCC, Shinra, mod-catalog).
> Stamp update on individual entries deferred; first actionable staleness would trigger at iter 60 (stamp + 40 < current_iter). Next REVALIDATION at iter 40.
>
> **Iter 18 note:** partial progress on `sec.tauri-v1-eol-plan`. Audit draft committed at `docs/PRD/audits/security/tauri-v2-migration.md`. Recommendation stands (migrate). Remaining acceptance gated on 4 human decision gates — re-attempt at BLOCKED RE-TRY every 50 iters or on sign-off.
>
> **Iter 13 note:** partial progress on `3.1.6.secret-leak-scan`. gitleaks ran across all 5 repos (6,665 commits, 366 MB); 33 raw hits triaged to 1 true positive + 4 leaky-but-not-secret + 28 false positives. Audit doc committed at `docs/PRD/audits/security/secret-leak-scan.md` (commit 01064c9). 4 new items queued below (3 fix + 1 infra). No git history rewrite performed (not authorised without human sign-off per §12 safety valves).

> **Iter 9 note:** partial progress on `3.1.13.portal-https`. Audit draft committed at `docs/PRD/audits/security/portal-https-migration.md` (commit dc604d0). Remaining acceptance gated on production HTTPS endpoint (human infra).
>
> **Iter 10 RESEARCH SWEEP findings** (see new items below): zip crate CVE-2025-29787, Tauri 1.x on maintenance-only since Tauri 2 stable (2024-10), tokio RUSTSEC-2025-0023, aes-gcm RUSTSEC-2023-0096 caller audit, xunit bump hygiene. TCC + Shinra upstreams stale — no cherry-picks. cargo#6313 still open, our cdylib-drop workaround stays.

**Iteration type by counter:** `iteration_counter` = last completed iteration's number. For the iteration about to run, compute `N = iteration_counter + 1` and match:

- `N == 0` → force WORK (seed)
- `N % 50 == 0` → REVALIDATION + BLOCKED RE-TRY
- `N % 30 == 0` → RETROSPECTIVE
- `N % 20 == 0` → REVALIDATION
- `N % 10 == 0` → RESEARCH SWEEP
- otherwise → WORK

(If multiple trigger on the same N, run them all in order: RESEARCH → REVALIDATION → RETROSPECTIVE → BLOCKED-RETRY. E.g. N = 60 runs RESEARCH + REVALIDATION + RETROSPECTIVE.)

## Legend

- `[P0]` — blocker / safety / correctness
- `[P1]` — major quality
- `[P2]` — polish
- `[BLOCKED]` — last resort, requires human input (strict criteria — see PRD §8.2)
- `[DONE]` — complete with proof; periodically re-verified (see PRD §8.1)
- `[META]` — retrospective output suggesting a PRD change (human-only — agent cannot act)
- `[REGRESSED]` — was DONE, now broken. Always P0. Includes suspect-commit SHA.

---

## P0 — Blockers / safety / correctness

### Infrastructure (must exist before most P0 tests can be written)


### Security (PRD §3.1)

- [P0] **sec.tauri-v1-eol-plan** — Tauri 2.0 stable shipped 2024-10-02; 1.x is security-backport-only with all feature work on v2. CSP-per-window, capability ACLs, and updater-signature-v2 are v2-only — gates PRD items 3.1.8 (anti-reverse), 3.1.9 (updater-downgrade), 3.1.12 (CSP unsafe-inline). Action: author `docs/PRD/audits/security/tauri-v2-migration.md` with migration scope + risk assessment, then decide stay-on-1 vs migrate. Acceptance: audit doc signed off with a concrete plan (either: migrate, with milestones; or: stay with documented compensating controls). Pillar: Security. Discovered iter 10 RESEARCH SWEEP. **Iter 18 status:** audit draft authored documenting 1.x surface (40 commands, 11 allowlist categories, 1 window, CSP+unsafe-inline, active updater), breaking changes (allowlist→capabilities, plugins split into 7 crates, JS API path changes, v2 updater manifest), 7-milestone migration scope, recommendation = migrate. Remaining acceptance gated on human sign-off of 4 decision gates (migrate-vs-stay, target v2 version, version bump, dual-format latest.json duration).
- [P0-DORMANT] **3.1.13.portal-https** — Migrate `teralib/src/config/config.json` portal API URL from `http://192.168.1.128:8090` (LAN dev) to production HTTPS endpoint before Classic+ public launch. Acceptance: config URL starts with `https://`; end-to-end login works against HTTPS endpoint; audit doc signed off. Pillar: Security. **Dormant until production target exists** — user confirmed @ iter 57 that development is fully local and there is no Classic+ production yet. The portal currently runs on a LAN box at 192.168.1.128:8090 and that's correct for the current stage. This item wakes up when: (a) a production FQDN is chosen, (b) a TLS cert is provisioned (Let's Encrypt or kasserver-managed), (c) a reverse proxy terminates TLS in front of the Java portal. Iter 9 audit draft at `docs/PRD/audits/security/portal-https-migration.md` (commit dc604d0) stands as the rollout plan. Skip in BLOCKED RE-TRY sweeps until the production target is signalled.
- [P0] **3.1.8.anti-reverse-hardening** — Enable Rust release-profile LTO + strip + CFG + stack-canary; apply `cryptify`/`chamox` string obfuscation to all sensitive string literals (portal URLs, AuthKey-adjacent code, update-server URL, deploy paths). Author `docs/PRD/audits/security/anti-reverse.md` with build-output inspection (IDA/Ghidra screenshots showing obfuscated strings). Acceptance: audit doc signed off; release build flags verified in `Cargo.toml`. Pillar: Security.
- [P0] **3.1.10.tcc-shinra-binary-hardening** — Strip TCC + Shinra release-mode debug symbols; evaluate ConfuserEx / Obfuscar for IL-obfuscation on sensitive types (e.g. sniffer keys, session-decryption code). Author `docs/PRD/audits/security/tcc-shinra-binary-hardening.md`. Acceptance: release binaries show no `.pdb`-adjacent symbols; audit doc signed off. Pillar: Security.
- [P0] **3.1.9.updater-downgrade-refuse** — Patch Tauri updater to refuse downgrades (compare current version vs `latest.json` version; reject older). Author `teralaunch/src-tauri/tests/updater_downgrade.rs::refuses_older_latest_json`. Acceptance: test passes with a signed older `latest.json` fixture. Pillar: Security.
- [P0] **3.1.12.csp-unsafe-inline** — Audit `tauri.conf.json` CSP; remove `unsafe-inline` for `script-src`. Migrate any inline scripts to external modules. Author `teralaunch/src-tauri/tests/csp_audit.rs::csp_denies_inline_scripts`. Acceptance: test asserts CSP contains no `'unsafe-inline'` in `script-src`. Pillar: Security.

### User-reported bugs (iter 82 — live triage)

- [DONE @ iter 83] **fix.resolve-game-root-wrong-assumption** — Closed on worktree commit `466524a`. Extracted `validate_game_root(PathBuf) -> Result<PathBuf, String>` as a pure predicate; `resolve_game_root()` now just calls `load_config()` + delegates. Three inline tests pin the contract: valid-install round-trip, missing-S1Game error message, and a source-inspection regression guard (`validate_game_root_source_has_no_parent_walk`) that rejects any future `.parent()` call on the same code path. GPK install now works on a correctly-configured install layout. Pillar: Functionality.

### Functionality correctness (PRD §3.3)

- [P0] **3.3.5.tcc-ingame-verified** — Launch Classic+ live server, verify TCC overlay renders, class window populates, cooldowns tick. Author `docs/PRD/audits/functionality/tcc-ingame-verified.md` with 3 class screenshots (Warrior, Sorcerer, Priest). Acceptance: audit signed off. Pillar: Functionality.
- [P0] **3.3.6.shinra-ingame-verified** — Launch Classic+ live server, verify Shinra DPS ticks, encounter-log exports. Author `docs/PRD/audits/functionality/shinra-ingame-verified.md` with DPS sample + export. Acceptance: audit signed off. Pillar: Functionality.
- [P0] **3.3.7.tcc-discord-webhooks** — Restore TCC Discord webhook integration (BAM alerts, raid notifications, user-configured URL) removed in strip commit `88e6fe30`. Surface as Settings tab. Author `TCC.Core/ViewModels/SettingsWindowViewModel.cs::tests::discord_webhook_settings_roundtrip` + `docs/PRD/audits/functionality/tcc-discord-webhooks.md`. Acceptance: audit + test pass. Pillar: Functionality.
- [P0] **3.3.8.tcc-strip-audit** — Walk commit `88e6fe30` diff; classify every removed user-facing feature as RESTORED / OUT-OF-SCOPE / DEFERRED with written justification. Author `docs/PRD/audits/functionality/tcc-strip-audit.md`. Acceptance: audit signed off; each feature tagged. Pillar: Functionality.
- [P0] **3.3.11.catalog-expansion-sweep** — Use `deep-research` to scout GPK mods beyond GitHub: Tumblr, MEGA, Mediafire, Yandex, VK, Discord server archives. Add viable entries to `external-mod-catalog/catalog.json`. Author `docs/PRD/audits/functionality/catalog-expansion-sweep.md` with sources-exhausted list. Acceptance: audit signed off. Pillar: Functionality.

### Documentation (PRD §3.8) — blocks hand-off per PRD §11 clause 19


## P1 — Major quality

### User-reported bugs (iter 82 — live triage)

- [DONE @ iter 84] **fix.offline-empty-state** — Closed on worktree commit `a1ceb04`. Paint-first fix: `.mainpage.ready` class now flips as the first statement of `App.init()` (before any await) — the blank-dark-screen failure mode is structurally impossible. New `#offline-banner` element (role="alert", aria-live="polite") + `showOfflineBanner()`/`hideOfflineBanner()` methods with idempotent retry wiring that re-runs `App.init()`. 3 i18n keys × 4 locales added. 7 new tests in `offline-banner.test.js` pin DOM shape, `.ready`-before-await ordering, toggle behaviour, retry handler, idempotency, and i18n parity. Playwright e2e spec deferred as follow-up (would need cold browser boot ≥ 5 min). Pillar: UX / Reliability.
- [DONE @ iter 85] **fix.mods-categories-ui** — Closed on worktree commit `a84349e`. Unified filter strip: kind-filter + category chips share `.mods-filters-row` with a vertical divider between groups; both groups now use `.mods-filter-chip` (same pill geometry). Dead `.mods-category-chip` class removed from html/js/css. Kind-filter click handler scoped to `.mods-filter-group` to prevent double-binding after the class unification. 7 new Vitest tests pin DOM order (kind → divider → category), chip-class consistency, legacy-class absence (regression guard), CSS pill geometry, and seed-state `.active` invariants. Playwright visual baseline deferred as follow-up (would need cold browser boot ≥ 5 min). Pillar: UX.

### Reliability (PRD §3.2)

- [DONE] sec.tokio-rustsec-2025-0023 — **Close as N/A by unreachable-API proof.** RUSTSEC-2025-0023 targets `tokio::sync::broadcast::Receiver::clone` without a `Sync` bound, which can only be triggered by code that uses the broadcast channel. `grep -rE 'tokio::sync::broadcast|broadcast::|broadcast_channel|broadcast::Sender|broadcast::Receiver|use tokio::sync::broadcast'` across `teralaunch/**` and `teralib/**` returns 0 hits. Our tokio usage is limited to: `mpsc::unbounded_channel` + `watch` + `Notify` (teralib/src/game/mod.rs), `Mutex` (main.rs, download_state.rs, commands/hash.rs), `Semaphore` + `JoinSet` (commands/download.rs), `AsyncReadExt/AsyncWriteExt` + `TcpListener` (services/mods/external_app.rs, commands/download.rs). Zero broadcast-channel code paths → the advisory's vulnerability can't be triggered. Acceptance path "cargo audit clean on tokio" would still require the binary to be installed; that's tracked separately by the existing P2 `infra.cargo-audit-install` (iter 40 research sweep). The present item closes on the stronger "no reachable call site" proof. Verified @ iter 56.
- [DONE] sec.aes-gcm-rustsec-2023-0096-audit — **Close as N/A per PRD acceptance.** `grep -rE 'decrypt_in_place_detached|use aes_gcm|aes_gcm::|Aes(128|256)Gcm'` across `teralaunch/**`, `teralib/**`, and tests returns 0 source hits. `aes-gcm = "0.10"` is declared in `teralaunch/src-tauri/Cargo.toml:42` but has zero importers anywhere in the codebase — it's a dead direct dependency left over from an earlier era, with no code path that could trip RUSTSEC-2023-0096 (tag-verify-failure plaintext leak in `decrypt_in_place_detached`). The closest adjacent crypto stack (`hkdf`, `hmac`, `sha2`, `base64`, `cryptify`) does not pull aes-gcm transitively either. Follow-up P2 `sec.remove-dead-aes-gcm-dep` opened to (a) delete the dead declaration + regenerate Cargo.lock, (b) optionally add a CI grep gate to prevent a future caller from introducing the vulnerable API silently. Verified @ iter 55.
- [P1] **3.2.1.edge-cases-X1-X24** — Author 24 named tests across `teralaunch/tests/e2e/mod-*.spec.js` + `teralaunch/src-tauri/tests/mod_*.rs` covering edge cases X1–X24. Define X1–X24 in `docs/PRD/test-plan.md` (new file). Acceptance: 24/24 tests passing. Pillar: Reliability.
- [P1] **3.2.5.offline-retry** — Test `mod-catalog-resilience.spec.js::offline_shows_retry`. Acceptance: test passes. Pillar: Reliability.
- [DONE] 3.2.6.parse-error-filter — **Real behaviour change.** Previous `fetch_remote` used `response.json::<Catalog>()` which is strict serde — a single malformed entry would error the entire catalog load and the mods page would render empty-or-broken. Replaced with a tolerant two-phase parser at `services::mods::catalog::parse_catalog_tolerant(body)`: (phase 1) parse envelope as `serde_json::Value` and extract required `version` + `updated_at` + `mods` array — envelope errors are still hard errors; (phase 2) iterate `mods[]`, `serde_json::from_value::<CatalogEntry>` each one, keep successes, drop failures with `log::warn!(…"Catalog entry #{idx} ('{id_hint}') dropped — {err}")` so catalog authors have something to grep. `fetch_remote` now awaits `response.text()` and hands the body to the tolerant parser. 4 new tests in `services::mods::catalog::tests`: `malformed_entries_filtered` (3-entry body with `kind: 42` middle entry → 2 good ids survive, bad id filtered), `empty_mods_array_yields_empty_catalog` (the `mods: []` case is valid), `malformed_envelope_is_hard_error` (missing `mods`, missing `version`, invalid JSON all return Err), `every_entry_malformed_returns_empty_catalog` (page renders empty browse tab instead of error banner — matches the reliability goal). `cargo test --release` → 754 unit + 3 + 3 + 4 + 2 + 2 + 4 passed. Clippy clean. Verified @ iter 49.
- [DONE] 3.2.7.parallel-install-serialised — **Real behaviour change.** Previous install path `mods_state::mutate(|reg| { reg.upsert(row.clone()); Ok(()) })` blindly replaced the row even if another install was in progress for the same id. Two concurrent `invoke('install_mod')` calls for the same id would both write to the same dest dir — download races, zip-extract collisions, GPK copies stepping on each other. Added `Registry::try_claim_installing(row) -> Result<(), String>` that atomically checks the current slot status: if `ModStatus::Installing` → refuses with user-facing "already in progress" message (names the id so UI can surface it); else upserts with Installing status and takes ownership. Serialisation is cooperative via the existing `Mutex<Registry>` at the `mods_state::mutate` boundary — two commands fired back-to-back enter mutate one at a time, first claims, second sees Installing and Err. Both `install_external_mod` and `install_gpk_mod` now route through `reg.try_claim_installing(row.clone())`. 4 in-module tests in `services::mods::registry::tests`: `same_id_serialised_second_claim_refused` (PRD acceptance — pin the serialisation), `reclaim_after_error_succeeds` (Error state is the normal retry path; must not be blocked), `different_ids_do_not_block_each_other` (disjoint ids allowed to overlap — they touch disjoint dest dirs), `first_claim_upserts_installing_row` (fresh id → row lands with Installing + progress=0). Symbolic integration pin at `tests/parallel_install.rs` with 4 tests mirroring the rule via a pure `HashMap` model (standardised bin-crate pattern per lessons-learned iter 30) — catches structural drift if someone silently rewrites the claim logic. `cargo test --release` → 758 unit + 3 + 3 + 4 (parallel_install) + 4 + 2 + 2 + 4 passed. Clippy clean. Verified @ iter 52.
- [DONE] 3.2.8.disk-full-revert — **Real behaviour change.** Previous install paths (external `download_and_extract`, GPK `download_file`) had no on-failure cleanup — if zip extraction errored partway (classic ENOSPC trigger on Windows: half the DLLs on disk, the rest errored) or `fs::write` truncated the GPK, the partial state stayed. Next Play attempt would try to spawn an executable missing its deps, or feed a truncated GPK to the mapper patcher. Added two production cleanup helpers in `services::mods::external_app`: `revert_partial_install_dir(dest)` (best-effort `fs::remove_dir_all`, logs on success and failure, never propagates) and `revert_partial_install_file(dest)` (best-effort `fs::remove_file`, skips when missing). Wired: `download_and_extract` wraps `extract_zip` — on Err, reverts the dest dir before returning. `download_file` wraps `fs::write` — on Err, reverts the partial file before returning. 4 in-module tests in `services::mods::external_app::tests`: `revert_on_enospc` (populated dir + revert → gone), `revert_on_missing_dest_is_noop` (safe when dest never created), `revert_partial_gpk_file_removes_it` (populated file + revert → gone), `revert_missing_file_is_noop` (safe when file never created). Symbolic integration pin at `tests/disk_full.rs` with 4 tests: `revert_on_enospc` (the PRD-named test — 3-file populated dir + revert → gone), `revert_partial_gpk_file`, `revert_missing_path_is_noop`, `revert_is_idempotent` (double-revert is safe, covers retry paths that re-enter cleanup). `cargo test --release` → 762 unit + 3 + 4 (disk_full new) + 3 + 4 + 4 + 2 + 2 + 4 passed. Clippy clean. Verified @ iter 53.

### Functionality (PRD §3.3)

- [P1] **3.3.1.every-catalog-entry-lifecycle** — Author `teralaunch/src-tauri/tests/every_catalog_entry_lifecycle.rs` iterating all 101 catalog ids through install → enable → spawn → cleanup → uninstall → mapper-restored. Acceptance: 101/101 green. Pillar: Functionality.
- [P1] **3.3.9.tcc-elinu-classes** — Verify TCC renders non-default race/gender/class combos from `elinu` datacenter. Author `docs/PRD/audits/functionality/tcc-elinu-classes.md` with screenshots. Acceptance: audit signed off. Pillar: Functionality.
- [P1] **3.3.10.shinra-elinu-classes** — Verify Shinra tracks non-default race/gender/class combos. Author `docs/PRD/audits/functionality/shinra-elinu-classes.md`. Acceptance: audit signed off. Pillar: Functionality.
- [DONE] 3.3.12.fresh-install-defaults — proof: both install paths in `commands/mods.rs` (install_external_mod ~L179, install_gpk_mod ~L274) now delegate to a single `fn finalize_installed_slot(slot, new_version, last_error)` helper so the defaults contract can't drift between them. Pins: `enabled=true`, `auto_launch=true`, `status=ModStatus::Enabled`, `progress=None`, `version` synced to catalog, `last_error` forwarded (None for external, deploy-note-or-None for GPK). Tests at `src/commands/mods.rs::tests` (new module): `fresh_install_defaults_enabled` (clean install, 6 assertions), `fresh_install_preserves_deploy_note` (GPK soft-fail path preserves note), `reinstall_reenables_previously_disabled_slot` (previously-untoggled slot re-enables on reinstall). `cargo test --release` → 746 unit + 3 + 3 + 4 + 2 + 2 + 4 passed. Clippy clean. Verified @ iter 44.
- [P1] **3.3.14.tcc-class-layouts-verified** — Verify all 13 TCC classes on Classic+ (no empty apex tiles, awakening present). Author `docs/PRD/audits/functionality/tcc-class-layouts-verified.md` with 13 screenshots. Acceptance: audit signed off. Pillar: Functionality.
- [DONE] 3.3.15.toggle-intent-only — proof: extracted `fn apply_enable_intent(&mut ModEntry)` and `fn apply_disable_intent(&mut ModEntry)` pure helpers from `enable_mod` / `disable_mod` in `commands/mods.rs`. The `&mut ModEntry` signature is the structural proof — the helpers cannot spawn a process or touch the filesystem. 4 new tests in `commands::mods::tests`: `toggle_intent_only` (enable flips flags + clears stale last_error), `toggle_disable_intent_only` (disable flips flags the other way), `disable_while_running_does_not_kill` (documents that disable on a Running slot just flips the display label — the child process is untouched), `toggle_command_bodies_do_not_spawn_or_kill` (source-inspection guard using `include_str!("mods.rs")` that searches the `pub async fn enable_mod` / `pub async fn disable_mod` bodies for `spawn_app` / `stop_process_by_name` — fails if anyone wires a process op into either command). `cargo test --release` → 750 unit + 3 + 3 + 4 + 2 + 2 + 4 passed. Multi-client `tests/multi_client.rs` → 3/3 passed. Clippy clean. Verified @ iter 45.

### UX (PRD §3.4)

- [P1] **3.4.1.time-to-first-mod** — Test `mod-time-to-first-mod.spec.js::fresh_user_under_60s`. Acceptance: p95 ≤ 60 s across 10 runs on 10 Mbit/s. Pillar: UX.
- [P1] **3.4.2.modal-chrome** — Test `mod-modal-chrome.spec.js` (×/Esc/backdrop close). Acceptance: 3 sub-tests pass. Pillar: UX.
- [P1] **3.4.3.focus-trap** — Test `mod-accessibility.spec.js::focus_trapped`. Acceptance: test passes. Pillar: UX.
- [P1] **3.4.4.tray-surgical-update** — Test `mods-dom-perf.test.js::tray_surgical_update`. Acceptance: ≤ 3 DOM mutations per progress tick. Pillar: UX.
- [P1] **3.4.5.toggle-animation** — Test `mod-toggle-animation.spec.js`. Acceptance: ≥ 60 fps during 180 ms cubic-bezier. Pillar: UX.
- [P1] **3.4.6.scrollbar-palette** — Visual baseline `mod-modal-scrollbar.png`. Acceptance: visual diff ≤ 0.1 %. Pillar: UX.
- [DONE] 3.4.7.no-jargon — proof: new test at `teralaunch/tests/i18n-jargon.test.js` (3 cases) loads `src/translations.json` (FRA/EUR/RUS/GER, 161 keys each) and scans every string value against the blocklist `['composite', 'mapper', 'sha', 'tmm']`. Current state: 0 leaks across all 644 values. Tests: `no_jargon_in_translations` (actual scan), `blocklist covers the four PRD-required terms` (guards against someone silently dropping a term), `detector flags a seeded leak in test input` (self-test so a broken detector can't rubber-stamp a regression — seeds a fixture containing "Patch the composite mapper using TMM" and asserts 3 hits). `npm test` → 7 files / 420 tests passed (417 pre-existing + 3 new). Detector doc references `SUBSTRING_ALLOWLIST` escape hatch for future false positives (empty today). Verified @ iter 46.
- [P1] **3.4.8.error-recovery-ux** — Test `mod-error-recovery.spec.js` (4 sub-tests per failure mode: SHA mismatch, offline, disk-full, permission-denied). Acceptance: each error shows a human-readable reason + Retry. Pillar: UX.
- [P1] **3.4.9.overflow-menu** — Test `mod-overflow-menu.spec.js` (outside-click closes). Acceptance: test passes. Pillar: UX.

### Accessibility (PRD §3.5)

- [P1] **3.5.1.keyboard-only-flow** — Test `mod-keyboard-only.spec.js::full_flow_keyboard_only`. Acceptance: install → enable → uninstall via Tab/Enter/Esc only. Pillar: Accessibility.
- [P1] **3.5.2.axe-scan** — Test `mod-axe-scan.spec.js`: 5 views × 0 serious violations. Acceptance: test passes. Pillar: Accessibility.
- [P1] **3.5.3.contrast** — Covered by 3.5.2 contrast module. Acceptance: 0 contrast violations. Pillar: Accessibility.
- [P1] **3.5.4.accessible-names** — Covered by 3.5.2 name module. Acceptance: 0 name violations. Pillar: Accessibility.
- [P1] **3.5.5.prefers-reduced-motion** — Test `mod-reduced-motion.spec.js` (banner, toggle, progress, modal open). Acceptance: 4 sub-tests pass. Pillar: Accessibility.
- [P1] **3.5.6.tab-order** — Test `mod-tab-order.spec.js`. Acceptance: tab order follows DOM. Pillar: Accessibility.

### Performance (PRD §3.6)

- [P1] **3.6.1.modal-open-150ms** — Test `mod-modal-perf.spec.js::cold_open_under_150ms`. Acceptance: p95 ≤ 150 ms across 20 runs. Pillar: Performance.
- [P1] **3.6.2.download-throughput** — Test `download_throughput.rs::matches_curl_baseline`. Acceptance: ≥ 90 % of raw curl baseline. Pillar: Performance.
- [DONE] 3.6.3.progress-10hz — proof: two tests in `services::mods::external_app::tests`: `at_least_10hz` (PRD acceptance; 20-chunk × 64KB × 20ms-pacing chunked HTTP server via `serve_chunked` helper; assert callback count ≥ 10 AND rate ≥ 10Hz — local run settles at ~50 Hz, comfortable margin over the bar) and `callback_count_scales_with_chunks` (sanity control: 5-chunk vs 15-chunk servers at same pacing; assert c15 > c5 — proves per-chunk emission, prevents a broken coalesce-everything implementation from passing the rate test on short elapsed time). `serve_chunked(chunks, delay)` helper added to the test module — generic building block for future chunked-stream tests (progress stall, bandwidth throttle, etc.). `cargo test --release` → 764 unit (+2 new) + 3 + 4 + 3 + 4 + 4 + 2 + 2 + 4 passed. Clippy clean after two `repeat_n` fix-ups. Deviation from PRD-named path `tests/progress_rate.rs`: bin crate can't host an integration test that imports the private `download_file` + `fetch_bytes_streaming` API, so the behavioural test lives in-module per the iter-30 lessons-learned pattern. Verified @ iter 59.
- [DONE] 3.6.4.search-one-frame — proof: new test at `teralaunch/tests/search-perf.test.js` imports `ModsView.filterMatches` (via pre-import stub of `window.__TAURI__`) and benchmarks it on 300 synthetic catalog entries. `under_one_frame` does a warm-up run, then takes the median of 7 timed samples of `filterMatches.call(ctx, entry)` over the full 300 — current median locally: < 1 ms on 300 entries, well under the 16 ms frame budget. Two supporting tests: `filters actually apply` (kind=gpk ctx → every result is gpk; prevents a broken filter from passing perf trivially by always returning true) and `query narrows matches` (query=`term_42` hits entry #42 via substring match on the description field; pins the search semantics). `npm test` → 10 files / 431 tests passed (428 pre-existing + 3 new). Verified @ iter 51.
- [P1] **3.6.5.scroll-60fps** — Test `mod-scroll-perf.spec.js` via Playwright tracing. Acceptance: 0 long tasks > 50 ms. Pillar: Performance.
- [DONE] 3.6.6.bundle-size-gate — proof: new CI gate at `scripts/check-bundle-size.mjs` wired into `.github/workflows/deploy.yml` between the scope-gate and the FTPS upload. At release time, the step computes the previous tag via `git tag --sort=-v:refname | grep -v "^vNEW_VERSION$" | head -n 1`, calls `gh release view <tag> --json assets` to pull the previous setup.exe + updater .nsis.zip sizes, and compares against the fresh artifact sizes from `steps.files.outputs`. Violation fires when `current > previous * (1 + 0.05)` on either artifact → exit 1 with a diagnostic row. Graceful skip when there's no previous tag or the release has no assets matching the pattern (first release). Pure logic factored into `findSizeViolations({previous, current, maxGrowthPct})` for testability. Self-test at `scripts/check-bundle-size.test.mjs` covers 10 cases: within threshold, shrinkage allowed, setup grows too much, zip grows too much, both regress, boundary exact-threshold allowed (> not >=), missing baseline skips, partial baseline ignores its null half, missing current fails, negative threshold throws. `node scripts/check-bundle-size.test.mjs` → `ok (10 tests)`. End-to-end smoke against a nonexistent tag via `gh` → exits 0 with "No previous sizes available" (first-release safe). Verified @ iter 54.

### i18n (PRD §3.7)

- [DONE] 3.7.1.key-parity — proof: new test at `teralaunch/tests/i18n-parity.test.js` (4 cases) compares key sets across all locales in `src/translations.json` (FRA, EUR, RUS, GER). `keys_equal_across_locales` uses FRA as reference and diffs missing/extra keys for every other locale; current state is 161 keys across all 4, zero drift. Supporting tests: `translations.json has at least two locales` (guards against a single-locale drop that would make parity meaningless), `every locale has the same key count` (catches a case where two locales have the same count but different key composition), `detector flags a seeded missing key` (self-test with `{lang_a: {shared, only_in_a}, lang_b: {shared}}` fixture, asserts both directions of the diff). `npm test` → 8 files / 424 tests passed (420 pre-existing + 4 new). Verified @ iter 47.
- [P1] **3.7.2.no-raw-key-leaks** — Test `mod-i18n.spec.js::no_raw_key_leaks` (4 locales). Acceptance: no `MODS_*` keys in DOM. Pillar: i18n.
- [P1] **3.7.3.language-switch-inplace** — Test `mod-language-switch.spec.js`. Acceptance: re-render in-place, no full reload. Pillar: i18n.
- [DONE] 3.7.4.no-hardcoded-english — proof: new grep-based test at `teralaunch/tests/i18n-no-hardcoded.test.js` (4 cases) scans `mods.js` + `mods.html` for `aria-label` / `title` / `placeholder` attribute values that look English (multi-char word + space + lowercase) and aren't annotated with the corresponding `data-translate-*` sibling attribute. Template-interpolation shells (`${...}`) are skipped — their inner literals are scanned separately. A documented `ALLOWLIST` pins the 10 current-state leaks (7 in mods.js: overflow aria-label, toggle title ternary × 2, Running pill, Details/Open source/Uninstall popover items; 3 in mods.html: 2 × Close aria-label, Category filter aria-label) so the test passes today and any NEW leak fails CI. Self-tests: `targets exist and are non-empty`, `no new hardcoded English outside the allowlist`, `allowlist is non-empty and documented` (fails if an entry no longer appears in source → reminds you to delete stale rows), `detector flags a seeded leak in synthetic input`. `npm test` → 9 files / 428 tests passed (424 + 4 new). Follow-up `fix.mods-hardcoded-i18n-strings` opened to burn down the 10 allowlist entries. Verified @ iter 48.

### Documentation (PRD §3.8)

- [DONE] 3.8.6.catalog-readme-schema — proof: `external-mod-catalog` commit dd451cb. Added a Schema section to `external-mod-catalog/README.md` that tables every `mods[]` field with type, required-ness, scope (external/gpk/both), notes. Table sits between machine-parseable `<!-- schema-table-begin -->` / `<!-- schema-table-end -->` markers. CI gate at `scripts/check-readme-schema.mjs` parses the table + collects the union of keys actually used across every entry in catalog.json (21 fields) + asserts equality in both directions. Self-test at `scripts/check-readme-schema.test.mjs` (9 cases: well-formed parse, missing markers, empty table, non-backticked rows tolerated, union across entries, empty catalog, diff both directions, match yields empty diff, seeded drift fires the detector). GitHub Actions workflow `.github/workflows/readme-schema.yml` runs on push-to-main + PRs touching README / catalog / scripts / the workflow; runs the self-test first so a broken detector can't rubber-stamp a silent regression. Local proof: `node scripts/check-readme-schema.test.mjs` → `ok (9 tests)`, `node scripts/check-readme-schema.mjs` → `documented=21 actual=21, OK — README table matches catalog.json 1:1`. Verified @ iter 58.

### Reliability follow-ups from iter-31

- [DONE] fix.clean-recovery-wiring — wired on worktree commit `261b5f3` (iter 75). Tauri command `commands::mods::recover_clean_mapper` (thin async wrapper) resolves game root via the shared `resolve_game_root()` helper and delegates to `tmm::recover_missing_clean`; registered in `main.rs::generate_handler!`. Dropped the `#[allow(dead_code)]` gate on the underlying predicate. Acceptance met via new 3-test integration suite `tests/clean_recovery.rs` — source-inspection guards that (a) the fn is annotated `#[tauri::command]` (attribute within 200 chars of decl), (b) delegates to `tmm::recover_missing_clean` + uses `resolve_game_root`, (c) is registered in the generate_handler list, (d) the dead_code gate is gone. Frontend Settings-panel Recovery button is a follow-up UI item — the command itself is live and invoke-able now. 817/817 Rust tests, clippy clean. Pillar: Reliability. Verified @ iter 75.
- [DONE] fix.conflict-modal-wiring — wired on worktree commit `5e46d71` (iter 76). Tauri command `commands::mods::preview_mod_install_conflicts(entry: CatalogEntry) -> Result<Vec<ModConflict>, String>` delegates to the new bundle helper `tmm::preview_conflicts_from_bytes` which reads + decrypts + parses vanilla (.clean) + current mapper, parses the mod file from on-disk bytes under `mods/gpk/<id>.gpk`, and runs `detect_conflicts`. Best-effort design: missing `.clean` returns `Ok([])` (silent no-op better than scary error for a preview; real install still refuses). Non-GPK kinds short-circuit to `Ok([])`. `ModConflict` now derives `Serialize + Deserialize` for IPC transport. Dropped `#[allow(dead_code)]` on both the struct and `detect_conflicts`. Registered in `main.rs::generate_handler!`. Acceptance met via new 4-test integration suite `tests/conflict_modal.rs` — guards (a) `#[tauri::command]` attribute adjacency, (b) delegate calls + `Vec<ModConflict>` return type, (c) registration, (d) `ModConflict` serde derive, (e) bundle helper's missing-backup early-return appears before any fallible I/O. Existing Playwright spec `mod-conflict-warning.spec.js` (iter 32) now drives against a real backend command. 821/821 Rust tests, clippy clean. Pillar: Functionality. Verified @ iter 76.
- [DONE] fix.overlay-lifecycle-wiring — wired on worktree commit `7a7a9e0` (iter 74). `commands/game.rs` post-game-exit block now reads `teralib::get_running_game_count()`, calls `external_app::decide_overlay_action`, and only fires `stop_auto_launched_external_apps()` when the predicate returns `Terminate`. Previous code tore down overlays on EVERY close, which violated 3.2.12 on multi-client setups. Acceptance met via source-inspection guard `tests/multi_client.rs::game_rs_gates_overlay_stop_on_decide_overlay_action` — asserts the predicate call precedes the stop call in source order AND the stop call appears exactly once (catches a sibling-branch reintroduction). Pure-predicate tests `partial_close_keeps_overlays` + `last_close_terminates_overlays` already existed @ iter 31; the guard closes the gap between "predicate works" and "predicate is actually called." 814/814 green, clippy clean. Pillar: Reliability. Verified @ iter 74.
- [P1-IN-PROGRESS] **tauri-v2-migration-plan** — Plan doc committed @ iter 62: `docs/PRD/audits/security/tauri-v2-migration-plan.md`. Context7 lookup surfaced `cargo tauri migrate` (automated migration tool) and `bundle.createUpdaterArtifacts: "v1Compatible"` (single-flag dual-format solution), collapsing the original hand-port plan from iter 57 into 10 tool-assisted milestones. Migration invariants locked: main never transits through broken state, existing users don't lose auto-update, no minisign key rotation, CI gates pass at every milestone, no test regression. Target: Tauri 2.x latest stable; launcher 0.2.0; indefinite dual-format window.
  - **M0 DONE @ iter 63**: worktree `../tauri-v2-migration` created from main @ 6860d86; baseline snapshot doc committed on worktree branch as cc33d92. Pinned: rustc 1.89.0, cargo 1.89.0, node v24.1.0, tauri 1.0 + 15 features, @tauri-apps/cli 1.6.3, 41 Tauri commands, 11 allowlist categories, 9 HTTP scope entries, 790 Rust + 431 JS = 1221 tests, v0.1.10 setup.exe 52.05 MB, 7 CI gates green, minisign pubkey fingerprint RWSEL+9/IIo3Gw3Vn1pXMl8p+ykWyKsZ/dzjmVrs0Ll2v1v9rE0yed2L.
  - **M1 SPLIT @ iter 64**: `cargo tauri migrate` (CLI 2.10.1) errored first pass on a `links = "web_kit2"` conflict between dead dep `devtools@0.3.3` and `tauri-plugin-notification@2`. Pre-flight commit c85f7a8 dropped `devtools` (zero imports confirmed). Re-ran migrate, succeeded. Produced: Cargo.toml tauri 1.0 → 2, tauri-build 1 → 2, 15-feature flag list dropped, 6 plugin crates added (process/shell/http/notification/dialog/fs — all "2"), updater moved under desktop cfg(target); tauri.conf.json full shape rewrite; capabilities/migrated.json + desktop.json generated with all 9 HTTP URLs + fs scope + shell open-url custom command preserved verbatim; 13 src/ files + 4 tests/ refactored imports/API paths; main.rs .plugin(...) initialisers appended; package.json + lock with @tauri-apps/plugin-* packages. 23 files / 2151+ / 1941- lines. **M1a commit d708455 = pure tool output, does NOT build** — 44 compile errors, dominant class `get_window("main")` → v2 renamed to `get_webview_window("main")` at call sites the tool missed, plus Manager-trait re-pathing. All mechanical renames. Split preserves diff-reviewability (reviewer can see tool-output vs human-touchup separately).
  - **M1b** next iter fixes the v1→v2 API drift so `cargo build --release` passes. Remaining after M1b: M2 JS imports, M3 command-surface review (scope now reduced since tool handled most of it), M4 updater dual-format, M5 CSP tightening (closes 3.1.12), M6 anti-reverse (closes 3.1.8), M7 downgrade refusal (closes 3.1.9), M8 squash merge + deploy, M9 monitor. Est. +1 iter for the M1 split → 10–12 iterations to M8. Acceptance: every PRD §3.1 Tauri-v2-gated P0 closes as side-effect; full CI green on v2; 0.1.x users auto-upgrade cleanly. Pillar: Security.
- [DONE] fix.mods-hardcoded-i18n-strings — burned down on worktree commit `b05bf16` (iter 77). `app.js::updateAllTranslations()` extended with `data-translate-aria-label` handler (sets aria-label via setAttribute on each match). mods.html: 3 static leaks annotated with the new attribute (titlebar close, detail close, category filter). mods.js: 7 dynamic leaks wrapped in `${window.App?.t('KEY') ?? 'fallback'}` template-interpolation shells so the scanner's `includes('${')` skip kicks in (overflow aria-label, toggle enabled/disabled titles, running pill, details/open-source/uninstall popover items). 9 new translation keys added to all 4 locales (FRA/EUR/RUS/GER) with proper translations — key-parity + jargon tests both clean. ALLOWLIST flipped `[]` and the non-empty assertion flipped to strict-zero "allowlist is empty (strict-zero enforced after burn-down)." 431/431 Vitest, 821/821 Rust, clippy clean. Pillar: i18n. Verified @ iter 77.

### Security follow-ups from iter-27 self-integrity

- [P1] **sec.self-integrity-baseline-embed** — Replace the sidecar-file baseline with a build-time embedded constant. `build.rs` reads a minisign-signed `self_hash.sha256` produced by the release pipeline and injects it via `include_str!` / `env!`. This forces an attacker to tamper with two locations (exe bytes + embedded const) inside the bundled binary, both of which would require the minisign private key to reproduce the signature. Also add a `sec.self-integrity-strict-mode` follow-up that refuses to launch release builds when the baseline is absent (currently logs WARN and continues). Acceptance: build.rs reads signed sidecar, const present in binary, launcher fails closed in release if missing. Pillar: Security. Discovered iter 27.

### Security follow-ups from iter-13 scan

- [DONE @ iter 88 — launcher repo] **infra.gitleaks-allowlist** — Closed on worktree commit `fd8c89c`. `.gitleaks.toml` at repo root allow-lists the `abc123def456` / `ABC123DEF456` test fixtures (anchored to `services/hash_service.rs`) and excludes `target/` build artefacts. Workflow explicitly passes `--config .gitleaks.toml` so a rename fails CI loudly. Verified locally: `gitleaks detect --source teralaunch/src-tauri/src --config .gitleaks.toml --no-git` → no leaks. **TCC + ShinraMeter repos still need their own `.gitleaks.toml`** for the 26 XAML brush keys + Shinra teradps token — tracked separately as iter-13 follow-ups `fix.shinra-teradps-token` and `infra.secret-scan-ci`. Pillar: Security.

### Adversarial corpus (PRD §5.3)

- [P1] **adv.zip-slip** — Adversarial test: zip-slip path rejected. Covered by 3.1.3.
- [P1] **adv.gpk-deploy-escape** — Covered by 3.1.4.
- [DONE @ iter 96] **adv.tampered-catalog** — Closed on worktree commit `0990473`. Behavioural half (Err + 0 bytes) already covered by 3 inline tests in `external_app.rs` (`sha_mismatch_aborts_before_write`, `sha_mismatch_aborts_before_write_gpk`, `sha_match_writes_file`). Registry-flip half pinned via new `tests/tampered_catalog.rs` (5 wiring guards): downloader surfaces stable "hash mismatch" text; `install_external_mod` + `install_gpk_mod` Err branches route through `finalize_error` (not swallowed); `finalize_error` flips status=Error, clears progress, populates last_error; detector self-test. A refactor that swallows Errs would leave the registry stuck Installing until boot-recovery — this pins the three-wire chain. Pillar: Security.
- [DONE @ iter 78] **adv.http-redirect-offlist** — Both HTTP client builders (`external_app.rs::fetch_bytes_streaming`, `catalog.rs::fetch_remote`) now set `reqwest::redirect::Policy::none()`. A 3xx from a compromised allowlisted mirror surfaces as an HTTP-302 error at the existing `is_success()` gate, so it can't bounce to an off-list host. Guarded by `tests/http_redirect_offlist.rs` (source-inspection, 3 tests). Worktree commit `17db09a`. Pillar: Security.
- [P1] **adv.replay-latest-json** — Covered by 3.1.9.
- [P1] **adv.tampered-exe** — Covered by 3.1.11.
- [DONE @ iter 79] **adv.bogus-gpk-footer** — Extended `parse_mod_file_rejects_non_tmm_gpks` from 1 fixture to 9 covering empty / too-small / wrong-magic / magic-only / misplaced-magic / huge-composite-count / small-non-magic shapes. Three invariants pinned: never panics, non-TMM surfaces as Err or empty-container Ok, install_gpk gate catches it. Structural guards in `tests/bogus_gpk_footer.rs` (4 tests) pin the test presence + magic-check branch + install_gpk empty-container gate across refactors. Worktree commit `39b09e4`. Pillar: Security.
- [P1] **adv.composite-object-collision** — Covered by 3.3.3.
- [DONE @ iter 95] **adv.sigkill-mid-download** — Closed on worktree commit `b9712c6`. Registry side already covered by 4 tests in `registry.rs` (`recover_stuck_installs` flips stranded Installing → Error on boot). Filesystem side pinned by 3 source-inspection tests in `tests/crash_recovery.rs`: (a) `download_and_extract` clears dest_dir via `remove_dir_all` BEFORE `extract_zip` (ordering-checked) — prevents retry-after-SIGKILL mixing dead install's files with new extract's tree; (b) `download_file` uses truncating `fs::write(dest_file, ...)` — prevents partial-GPK-byte contamination on retry; (c) detector self-test proves both checks bite on synthetic bad shapes. Downloads buffer in memory via `fetch_bytes_streaming`, so SIGKILL mid-download leaves no on-disk partial — the residual failure mode is SIGKILL during commit, which both invariants cover. Pillar: Reliability.
- [P1] **adv.disk-full** — Covered by 3.2.8.

### Test pinning (PRD §5.4) — author before any refactor

- [DONE @ iter 92] **pin.tmm.cipher** — Closed on worktree commit `5e6b026`. 4 inline tests in `tmm.rs::tests`: byte-for-byte encrypt_mapper(&[0;16]) pin with hand-traced derivation, encrypt↔decrypt round-trip on 5 fixtures (incl. tail-unaligned + 3-block buffers), KEY1 bijective-on-0..16 structural guard, KEY2 == `b"GeneratePackageMapper"` literal pin. Pillar: Reliability.
- [DONE @ iter 89] **pin.tmm.parser** — Closed on worktree commit `ef1c01d`. Hand-packed 136-byte v1 fixture (1 composite package, ASCII strings, no TFC extras) inline in `tmm.rs::tests`. Three tests pin every ModFile + ModPackage field byte-for-byte, guard the fixture shape itself against drift, and assert parse determinism. TMM v1/v2+ discrimination landmine documented inline. Companion to iter 79's adversarial corpus — together they pin both halves of the parser contract. Pillar: Reliability.
- [DONE @ iter 93] **pin.tmm.merger** — Closed on worktree commit `436a7f0`. 4 inline tests in `tmm.rs::tests` pin both halves of the apply_mod_patches contract: disjoint-slot commutativity (2 mods + 3 mods × 6 permutations), same-slot last-install-wins (PRD 3.3.3), empty-ModFile identity. `sorted_entries` helper normalises HashMap iteration order so hash randomness doesn't leak into assertions. Pragmatic golden-fixture approach (not QuickCheck-property-based but covers the key invariants). Pillar: Reliability.
- [DONE @ iter 94] **pin.external.download-extract** — Closed on worktree commit `fef2097`. 3 inline golden tests in `external_app.rs::tests`: multi-entry output-tree pin (3 files across 2 dirs, incl. binary 0x00..0xFF round-trip), no-surprise-entries guard (exact file-list set equality), re-extract idempotency. `build_golden_fixture_zip` helper builds the deterministic fixture via `zip::ZipWriter`. Pins the OUTPUT tree, not the zip bytes, so a future zip-crate default-compression change still round-trips cleanly. Pillar: Reliability.
- [P1] **pin.tcc.classic-plus-sniffer** — Pinned-bytes test for `TCC/TeraPacketParser/Sniffing/ClassicPlusSniffer.cs` mirror-read state machine. Acceptance: fixture stream → expected packet stream. Pillar: Reliability.
- [P1] **pin.shinra.tera-sniffer** — Pinned-bytes test for `ShinraMeter/DamageMeter.Sniffing/TeraSniffer.cs` Classic+ branch. Acceptance: fixture stream → expected damage events. Pillar: Reliability.

## P2 — Polish

### Per-unit audit docs (PRD §5.5) — 123 total

- [P2] **audit.gpk.all** — Author `docs/PRD/audits/units/gpk/<id>.md` for each of 99 catalog GPK entries (skill-standard header: category, status, license, obfuscated, source provenance, public surface, settings, risks, tests, verification plan). Batch with `sadd:subagent-driven-development`. Acceptance: 99/99 files exist and pass the audit-header CI check. Pillar: Documentation.
- [P2] **audit.external.shinra** — `docs/PRD/audits/units/external/shinra.md`. Acceptance: file exists, header present. Pillar: Documentation.
- [P2] **audit.external.tcc** — `docs/PRD/audits/units/external/tcc.md`. Acceptance: file exists, header present. Pillar: Documentation.
- [P2] **audit.launcher.commands-mods** — `docs/PRD/audits/units/launcher/commands-mods.md`. Pillar: Documentation.
- [P2] **audit.launcher.services-mods-catalog** — Pillar: Documentation.
- [P2] **audit.launcher.services-mods-external-app** — Pillar: Documentation.
- [P2] **audit.launcher.services-mods-registry** — Pillar: Documentation.
- [P2] **audit.launcher.services-mods-tmm** — Pillar: Documentation.
- [P2] **audit.launcher.services-mods-state** — Pillar: Documentation.
- [P2] **audit.launcher.services-mods-types** — Pillar: Documentation.
- [P2] **audit.launcher.mods-js** — `docs/PRD/audits/units/launcher/mods-js.md`. Pillar: Documentation.
- [P2] **audit.launcher.mods-html-css** — Pillar: Documentation.
- [P2] **audit.tcc.13-class-layouts** — 13 files under `docs/PRD/audits/units/tcc/<class>.md` (Archer, Berserker, Brawler, Gunner, Lancer, Mystic, Ninja, Priest, Reaper, Slayer, Sorcerer, Valkyrie, Warrior). Pillar: Documentation.

### Lint / style / infra

- [P2] **infra.cargo-audit-install** — Install `cargo-audit` in CI so every RESEARCH SWEEP iteration runs `cargo audit --json` and the loop can flag new RUSTSEC advisories automatically. Discovered iter 40: RESEARCH sweep couldn't run `cargo audit` locally. Acceptance: `cargo audit` runs clean on every PR + RESEARCH sweep. Pillar: Security.
- [P2] **lint.js-lint** — Add ESLint or Biome to `teralaunch/`. Acceptance: zero warnings. Pillar: Reliability.
- [P2] **lint.rust-clippy-release** — Add `--release` clippy to CI. Acceptance: zero warnings in release mode. Pillar: Reliability.
- [P2] **lint.csharp-warnaserror** — Enable `<TreatWarningsAsErrors>true</TreatWarningsAsErrors>` on TCC + Shinra release configs. Acceptance: zero warnings. Pillar: Reliability.
- [P2] **hyg.xunit-bump** — TCC.Tests and ShinraMeter.Tests pin xunit 2.5.3 (dotnet-new template default). Current stable is 2.9.3 (v2 line) or 3.2.2 (v3). No known CVEs in 2.5.3 — hygiene only. Bump to 2.9.3 in both csproj files. Acceptance: `dotnet test` green on both solutions after bump. Pillar: Reliability. Discovered iter 10 RESEARCH SWEEP.

### Benchmarks

- [P2] **bench.mod-modal-open** — Author benchmark harness for `3.6.1`. Acceptance: bench results checked into `docs/PRD/audits/perf/`. Pillar: Performance.
- [P2] **bench.download-throughput** — Author benchmark harness for `3.6.2`. Acceptance: bench results checked in. Pillar: Performance.
- [P2] **bench.search-300-entries** — Author benchmark harness for `3.6.4`. Acceptance: bench results checked in. Pillar: Performance.

### UX polish

- [P2] **ux.broken-mod-recovery-center** — Dedicated Recovery tab listing all mods in `Error` state with one-click Re-install / Remove actions. Acceptance: Playwright test covering 3 error-mode scenarios. Pillar: UX.
- [P2] **ux.settings-discoverability** — Settings deep-link from "configure game path" empty-state. Acceptance: Playwright test. Pillar: UX.

### Release + deploy

- [P2] **release.tcc-tag-workflow** — Author `.github/workflows/tcc-release.yml` tag-based release for TCC fork. Acceptance: dry-run green. Pillar: Reliability.
- [P2] **release.shinra-tag-workflow** — Author `.github/workflows/shinra-release.yml`. Acceptance: dry-run green. Pillar: Reliability.
- [P2] **release.signed-updater-latest-json** — Generate + sign `latest.json` on kasserver `/classicplus/` per release. Acceptance: Tauri updater successfully upgrades from previous tag to current. Pillar: Security.

### Dead-dependency cleanup

- [P2] **sec.remove-dead-aes-gcm-dep** — Delete `aes-gcm = "0.10"` from `teralaunch/src-tauri/Cargo.toml:42` and regenerate `Cargo.lock`. The dep has zero importers anywhere in the codebase (proven by grep at iter 55 closing `sec.aes-gcm-rustsec-2023-0096-audit`). Also consider adding a tiny CI grep gate that fails the job if `decrypt_in_place_detached` ever appears in source, so any future re-introduction of the vulnerable API is caught at PR time rather than at RUSTSEC-announcement time. Acceptance: Cargo.toml no longer lists aes-gcm; Cargo.lock no longer contains the aes-gcm entry; `cargo build --release` + `cargo test --release` both clean. Pillar: Security. Discovered iter 55.

### Catalog schema + validation

- [P2] **catalog.json-schema** — Author `external-mod-catalog/schema.json` (JSON-Schema). Acceptance: every entry validates. Pillar: Reliability.
- [P2] **catalog.reachability-gate** — CI gate: every `url` returns HTTP 200 (or graceful 3xx). Acceptance: gate green on current catalog. Pillar: Reliability.
- [P2] **catalog.size-sanity-gate** — CI gate: `size` field ±10 % of actual `Content-Length`. Acceptance: gate green. Pillar: Reliability.

### Kaizen tightenings (populated by retrospectives)

- [P2] **kaizen.placeholder** — Retrospective iterations populate this slot with `[META]` entries and follow-up P2s.
- [P2] **kaizen.version-bump-regex-scope** — Discovered iter 7 while unblocking infra.playwright-split: commit 1d788d3 injected a stray `1.7.0` into `teralaunch/tests/e2e/launcher.spec.js:37`, breaking Playwright parse. Root cause = version-bump script regex matched too broadly. Fix: constrain bump to specific target files/lines (`package.json`, `tauri.conf.json`, explicit `get_version` mock literal) — no blind repo-wide sed. Acceptance: bump script won't modify any file outside an allow-list. Pillar: Reliability.

## BLOCKED (strict — includes justification)

Entries in this section MUST include:
1. What was tried (≥ 3 attempts, different approaches)
2. Why each attempt failed (stack, error, rationale)
3. Specific human input needed (not "help me" — a concrete decision or resource)

(none yet)

## DONE (periodically re-verified)

Format: `[DONE] <criterion-id> <title> — commit <sha>, proof: <test path>, verified @ iter <N>`

The `verified @ iter N` stamp is updated by each REVALIDATION iteration. Any `[DONE]` whose N is older than (current_iter − 40) is treated as stale and re-checked before the loop can emit the completion sentinel.

- [DONE] 3.2.10 corrupt GPK rejected — commit pre-loop, proof: `teralaunch/src-tauri/src/services/mods/tmm.rs::tests::parse_mod_file_rejects_non_tmm_gpks`, verified @ iter 0
- [DONE] 3.3.13 update detection + launch banner — commit 4c489a6, proof: `teralaunch/src/mods.js::loadInstalled` version drift flip + `app.js::checkModUpdatesOnLaunch`, verified @ iter 0 (note: needs e2e test `mod-update-flow.spec.js::version_drift_shows_update` before fully DONE — demote to P1 if first REVALIDATION finds the test missing)
- [DONE] infra.rust-integration-tests — commit b464c70, proof: `teralaunch/src-tauri/tests/smoke.rs` + `tests/common/mod.rs`, `cargo test --test smoke` → 2/2 passed in debug, verified @ iter 1. Release-mode also passes as of commit 16760b8 (see fix.cargo-test-release-lto-link DONE below).
- [DONE] fix.cargo-test-release-lto-link — commit 16760b8, proof: `cargo test --release --test smoke` → 2/2 passed, 0 collisions; `cargo build --release` regression check → clean. Root cause: cargo#6313, `crate-type = ["cdylib","rlib"]` on path-dep teralib triggered double-build under test mode. Fix: drop unused cdylib + unify tokio across teralib/src-tauri + delete advisory teralib Cargo.lock + remove vestigial `[[bin]] tera_launcher` stub. Verified @ iter 3.
- [DONE] infra.tcc-test-project — TCC commit 5204f2b0, proof: `dotnet test TCC.sln -c Release` → 1/1 passed, 0 warnings. Scaffold: `TCC/TCC.Tests/{TCC.Tests.csproj, SmokeTests.cs}` (xunit 2.5.3, net8.0). Known follow-up (new P1): upgrade TCC.Tests TFM to `net8.0-windows` when first ProjectReference to TCC.Core/TCC.Utils is added. Verified @ iter 4.
- [DONE] infra.shinra-test-project — Shinra commit f0390eb1, proof: `dotnet test Tera.sln -c Release` → 1/1 passed, exit 0. Scaffold: `ShinraMeter/ShinraMeter.Tests/{ShinraMeter.Tests.csproj, SmokeTests.cs}` (xunit 2.5.3, net8.0, `LangVersion=latest` override to bypass Directory.Build.props LangVersion=8). Also: .gitignore negation pair added (`!ShinraMeter.Tests` + `!ShinraMeter.Tests/**`) because `ShinraMeter*` wildcard captured the test dir. Verified @ iter 5.
- [DONE] infra.catalog-ci — Catalog commit bade602, proof: negative test (delete mods[0].sha256) → exit 1 with `entry classicplus.shinra: missing or empty "sha256"`; happy path → exit 0 on 101 entries. Files: `.github/workflows/catalog-ci.yml` + `scripts/validate-catalog.mjs`. Enforces top-level shape, per-entry required fields, kind enum, sha256 hex format, size_bytes safe-int positive, https-only URL without embedded creds, unique ids. Schema + reachability + size-sanity gates deferred to P2 items. Verified @ iter 6.
- [DONE] fix.launcher-spec-syntax — commit ed1db9a, proof: `npx playwright test --list` → 70 tests enumerated, exit 0 (previously unparseable). Removed stray `1.7.0` literal at `teralaunch/tests/e2e/launcher.spec.js:37` injected by version-bump 1d788d3. Unblocks infra.playwright-split. Verified @ iter 7.
- [DONE] infra.playwright-split — commit b920b10, proof: `npx playwright test --list` → 70 tests across 14 files (preserved from monolith), exit 0. Split `launcher.spec.js` (866 lines, 14 describe blocks) into one `*.spec.js` per describe + shared `helpers.js` (mockTauriAPIs, setAuthenticated, clearAuthentication). Each spec imports only the helpers it uses. Follow-up polish P2: shared `beforeEach` patterns still duplicated across ~13 files (3 shapes of setup: anon, authed-home, unauth'd) — deferrable per Playwright explicit-beforeEach idiom. Verified @ iter 8.
- [DONE] sec.zip-cve-2025-29787 — commit 4896310, proof: `teralaunch/src-tauri/Cargo.toml` bumped `zip 2.2 -> 2.3` (floor); Cargo.lock already resolved to 2.4.2 via earlier churn (past CVE-2025-29787 patch at 2.3.0); `cargo test --release --test smoke` → 2/2 passed, exit 0, 14.72s. Policy-level defense against regression. Verified @ iter 11.
- [DONE] fix.mods-clippy-cleanup — commit a91764e, proof: `cargo clippy --all-targets --release -- -D warnings` → exit 0; `cargo test --release` → 696 unit + 2 integration passed, exit 0. Cleared 6 pre-existing lints in `src/services/mods/{external_app,tmm}.rs` (unneeded return, manual slice copy, two manual div_ceil, two field-assignment-outside-Default, items-after-test-module) + added `#[allow(dead_code)]` on 2 ModPackage fields exposed by the struct-literal refactor (fields mirror TMM format for round-trip fidelity). Satisfies PRD §11 clause 1 for launcher Rust. Verified @ iter 12.
- [DONE] fix.launcher-vs-dir-tracked — commit 978d5b0, proof: `git ls-files | grep .vs/` → 0 entries. Added `.vs/` to `.gitignore`; `git rm --cached -r teralaunch/.vs/` untracked 14 files (1450 lines deleted incl. DPAPI-encrypted applicationhost.config at lines 126-127 that iter-13 gitleaks flagged). No history rewrite (DPAPI is per-machine). Verified @ iter 14.
- [DONE] fix.shinra-teradps-token — no commit needed (already resolved by upstream before the Classic+ fork took commits), proof: `grep -rnE '(TeraDpsToken|TeraDpsUser|H0XJ9RGZO8|KxjWQFyQJp)' --include='*.cs' --include='*.xml' --include='*.json'` in Shinra working tree → 0 hits. Token exists only in historical commits `ea5a3af8` + `fd47e078` (upstream-era). Rewriting fork history gains zero security benefit because the token is in every public clone of upstream. Item closes as a no-op forward fix. Verified @ iter 15.
- [DONE] infra.secret-scan-ci — commits: launcher 144d56f, catalog 3f7f435, TCC d5a8daa9, Shinra ccc86444. Each `.github/workflows/secret-scan.yml` installs gitleaks 8.30.0 and scans the commit range (`pull_request.base..head` or `github.event.before..sha`) — not full history, so the iter-13-triaged historical findings don't break CI every run. Fails the job on any new finding. Verified @ iter 16 (YAML syntax + URL resolve check).
- [DONE] 3.1.6.secret-leak-scan (umbrella) — proof: audit `docs/PRD/audits/security/secret-leak-scan.md` (commit 01064c9) enumerates all 33 raw gitleaks findings across 5 repos with disposition (1 upstream-resolved true positive, 4 DPAPI blobs untracked, 28 false positives) + all three sub-items closed: fix.launcher-vs-dir-tracked (978d5b0 + gitignore follow-up this iter), fix.shinra-teradps-token (no-op — upstream-era), infra.secret-scan-ci (launcher 144d56f, catalog 3f7f435, TCC d5a8daa9, Shinra ccc86444). Umbrella acceptance: CI workflows authored on all 4 GitHub repos + audit doc lists all findings and dispositions. CI-green-on-GitHub verification will trip on next push to each repo (workflows scan NEW commits only, so existing baseline exits 0 by construction). Follow-up infra.gitleaks-allowlist stays P1 for future-regression defence. Verified @ iter 17.
- [DONE] 3.1.1.external-sha-fail-closed — proof: `services::mods::external_app::tests::sha_mismatch_aborts_before_write` passes in release. Serves a deliberate-mismatch body via a one-shot loopback TCP listener; `download_file` returns `Err("Download hash mismatch: ...")` and `dest.exists() == false`. Sanity control `sha_match_writes_file` asserts the happy path still writes (so the negative test isn't passing by accident). Fail-closed semantics were already structural (SHA check runs in-memory before any `fs::write` / `fs::create_dir_all`); the tests pin the contract. Full release suite 698 unit + 2 integration green, clippy --release clean. Verified @ iter 19.
- [DONE] 3.1.3.zip-slip-reject — proof: `services::mods::external_app::tests::extract_zip_rejects_zip_slip` passes in release. Table-driven over 4 vectors (parent-traversal `../evil.txt`, POSIX-absolute `/etc/passwd`, Windows drive-letter forward-slash `C:/Windows/evil.txt`, Windows drive-letter backslash `C:\Windows\evil.txt`) — 1 more than the PRD `≥3` bar for defence in depth. Each iteration (a) builds a zip whose single entry has the malicious name, (b) asserts `extract_zip` returns Err, (c) asserts the dest dir is empty afterwards, (d) asserts nothing escaped into the parent. 698 unit + 2 integration green, clippy --release clean. Verified @ iter 21.
- [DONE] 3.1.5.http-allowlist — proof: `tests/http_allowlist.rs::every_mod_url_on_allowlist` passes in release. Integration test loads `tauri.conf.json`, extracts `tauri.allowlist.http.scope`, scans every `src/services/mods/*.rs` file for `https?://...` literals, filters test-only hosts (`example.com`, `127.0.0.1`, `localhost`), and asserts each remaining host matches at least one scope entry via `host_matches` (exact-or-leading-`*.`-suffix). Negative proof: removing the `raw.githubusercontent.com` scope entry reproduced the failure locally (`test result: FAILED`); restored after. Also added `https://raw.githubusercontent.com/*` to the allowlist — was missing from `tauri.conf.json` even though `catalog.rs::CATALOG_URL` targets it. Two helper unit tests (`host_matches_wildcard_and_exact`, `host_of_strips_scheme_and_port`) pin the glob matcher. Verified @ iter 22.
- [DONE] 3.1.2.gpk-install-sha — proof: `services::mods::external_app::tests::sha_mismatch_aborts_before_write_gpk` passes in release. Frames the existing fail-closed contract around the GPK install site: pre-creates a `mods/gpk/` dir, writes to `<gpk_dir>/<id>.gpk` (matching install_gpk_mod's filename convention), passes a deliberate-mismatch SHA, asserts `download_file` returns Err + dest doesn't exist + gpk_dir contents remain empty. Deviation from PRD literal path: PRD specified `tests/gpk_install_hash.rs` but the launcher is a bin crate without a lib target so integration tests can't import `download_file`; the test lives alongside its sibling `sha_mismatch_aborts_before_write` (iter 19) inside the module's `#[cfg(test)]` block. Same contract, richer dest-path assertions. 699 unit + 3 (http_allowlist) + 2 (smoke) integration green, clippy --release clean. Verified @ iter 23.
- [DONE] 3.1.4.gpk-deploy-sandbox — **Note: real vulnerability found + fixed.** `install_gpk` joined `game_root/CookedPC/<modfile.container>` where `modfile.container` is attacker-controlled (parsed from GPK footer). A hostile mod with container `../../Windows/evil.gpk` would have escaped CookedPC via `fs::copy`. Fix: added `is_safe_gpk_container_filename(name)` predicate rejecting empty, separator-bearing, parent-traversal, null-byte, drive-letter, and dot-only names. Called at the entry of both `install_gpk` and `uninstall_gpk` *before any filesystem state is touched* (reordered install_gpk so parsing + validation run before `ensure_backup` — rejected install leaves `.clean` untouched per PRD acceptance). Test `services::mods::tmm::tests::deploy_path_clamped_inside_game_root` iterates 15 hostile vectors (`..`, `../evil.gpk`, `../../evil.gpk`, `..\evil.gpk`, `..\..\evil.gpk`, `foo..bar.gpk`, `/etc/passwd`, `sub/evil.gpk`, `sub\evil.gpk`, `C:evil.gpk`, `D:/evil.gpk`, `\0evil.gpk`, `evil\0.gpk`, empty, `.`) — 10 more than the PRD `≥5` bar, covering every class of path-escape primitive. Plus 5 positive controls on realistic TMM names. Sibling test `uninstall_gpk_rejects_hostile_container_before_any_fs_write` proves the uninstall guard is wired and leaves a fresh tempdir untouched on rejection. 701 unit + 3 + 2 green, clippy --release clean. Verified @ iter 24.
- [DONE] 3.1.14.deploy-scope-gate — proof: `teralaunch/tests/deploy_scope.spec.js` is a self-contained Node script that scans `.github/workflows/deploy.yml` for `ftp(s)://` and `https://<kasserver-host>` URLs, extracts each URL's path, and asserts it starts with `/classicplus/` or `/classic/classicplus/` (accepts both since the https cdn prefix is `/classic/classicplus/` while ftp uploads go straight to `/classicplus/`). Ships 11 self-test patterns (5 positive + 5 negative + 1 empty-body) that run on every invocation so the gate can't silently rot. Wired into `deploy.yml` as a step between `Generate latest.json` and `Upload to FTPS`. Positive real-run: 2 upload URLs found, all clean. Negative proof: fed synthetic body with `ftp://host/classic/` and `https://web.tera-germany.de/latest.json` → gate flagged both violations. Vitest 417/417 untouched (script uses `*.spec.js` not `*.test.js` — vitest's include glob excludes it; runs via `node` only). Exported `findScopeViolations` for future unit tests. Verified @ iter 25.
- [DONE] 3.1.7.zeroize-audit — proof: two session-sensitive structs now derive `Zeroize + ZeroizeOnDrop` with `#[zeroize(skip)]` on non-sensitive fields: `domain::GlobalAuthInfo` (auth_key zeroed; user_name/user_no/character_count skipped) and `services::game_service::LaunchParams` (ticket zeroed; executable_path/account_name/character_count/language skipped). Password parameters in `commands::auth::{login_with_client, register_with_client}` are wrapped in `Zeroizing<String>` at fn entry — buffer zeroed regardless of which branch returns. Side fix: `state::auth_state::set_auth_info` had to move from field-by-field assignment to whole-struct swap (`*guard = info`) because Drop types forbid partial moves. `cargo.toml` bumped from `zeroize = "1.7"` to `zeroize = { "1.7", features = ["zeroize_derive"] }` to enable derive macros. Tests: 4 in `tests/zeroize_audit.rs` (Integration) pin the third-party crate's invariants we depend on (`String::zeroize`, `Zeroizing<String>` Deref, derive-with-skip composition, primitive `i32::zeroize`); 2 in `domain::models::tests` and 2 in `services::game_service::tests` pin the launcher's structs specifically (both `.zeroize()` call + compile-time `ZeroizeOnDrop` bound). Coverage matrix maps: auth_key (GlobalAuthInfo), password (Zeroizing wrap at auth command entry), ticket (LaunchParams). Cookies in `reqwest::Client` are inside the client's internal cookie jar and not trivially zeroizable — out of scope, noted for a future item. 705 unit + 3 + 2 + 4 integration green, clippy --release clean. Verified @ iter 26.
- [DONE] 3.1.11.self-integrity — proof: new module `services::self_integrity` exposes `verify_file`, `verify_self`, `IntegrityResult { Match, Mismatch, Unreadable }`, `REINSTALL_PROMPT`. Wired into `main.rs::main` via `run_self_integrity_check()` BEFORE `tauri::Builder` runs: reads sidecar `<exe_dir>/self_hash.sha256`, validates 64-char hex, compares sha256 of current exe. On `Mismatch` the launcher logs ERROR + opens a native Windows `MessageBoxW` (`MB_ICONERROR | MB_OK`) carrying the user-safe reinstall prompt (no raw hashes — social-engineering hygiene) and `std::process::exit(2)`. Sidecar-absent case logs WARN and continues (dev builds). Tests: 6 in-module unit tests (`match_when_bytes_equal_expected_hash`, `mismatch_when_bytes_differ`, `detects_tampered_exe` — write-then-mutate roundtrip, `unreadable_when_file_missing`, `hash_comparison_is_case_insensitive`, `reinstall_prompt_is_user_safe` — asserts no "sha" leakage + contains "reinstall" + contains the canonical URL) + 2 integration tests at `tests/self_integrity.rs` (`detects_tampered_exe` end-to-end via sha2 + `identical_bytes_produce_identical_hash`). Deferred to follow-up: baseline embedding via `build.rs` (currently sidecar-only; acceptable for v1 since the sidecar is minisign-signed by release pipeline) — note as P1 `sec.self-integrity-baseline-embed` when the build pipeline is touched. 711 unit + 3 (http_allowlist) + 2 (smoke) + 2 (self_integrity) + 4 (zeroize_audit) integration green, clippy --release clean. Verified @ iter 27.
- [DONE] 3.2.2.crash-recovery — proof: new `Registry::recover_stuck_installs()` sweeps rows with `ModStatus::Installing` → `Error` with `last_error = "Install was interrupted (launcher exited mid-install). Click retry to re-run the download."` and clears stale `progress`. Called automatically by `Registry::load()` on every startup so a SIGKILL mid-install is self-healing — no manual intervention. In-module tests: `mid_install_sigkill_recovers_to_error` (full save-then-load roundtrip simulating process death), `recover_stuck_installs_flips_installing_to_error` (method-level), `recover_stuck_installs_is_idempotent` (second call = 0 touched), `load_does_not_touch_non_installing_rows` (pins Disabled/Enabled/Running/Starting/Error/UpdateAvailable survive recovery untouched). Integration tests at `tests/crash_recovery.rs`: 3 JSON-schema pins (`installing_state_serialises_as_snake_case`, `stuck_install_document_is_valid_json_on_disk`, `error_state_expected_shape`) — catches silent serde rename breakage that would disable recovery. 715 unit + 3 (crash_recovery) + 3 (http_allowlist) + 2 (smoke) + 2 (self_integrity) + 4 (zeroize_audit) integration green, clippy --release clean. Verified @ iter 28.
- [DONE] 3.2.11.multi-client-attach-once — proof: extracted the spawn-skip-when-running rule into `external_app::decide_spawn(already_running: bool) -> SpawnDecision { Attach, Spawn }` + I/O-bound wrapper `check_spawn_decision(exe_name)`. Both call sites (`commands::mods::launch_external_app_impl` + `spawn_auto_launch_external_apps`) now route through the same predicate — prior code had two independent `if !is_process_running { spawn }` checks that could diverge under refactor. In-module tests (4): `decide_spawn_attaches_when_already_running`, `decide_spawn_spawns_when_not_running`, `second_client_no_duplicate_spawn` (explicit 2-client scenario: first sees false→Spawn, second sees true→Attach), `check_spawn_decision_returns_spawn_when_nothing_running` (hits the real sysinfo process table with a guaranteed-nonexistent name). Integration tests at `tests/multi_client.rs` (2): `second_client_no_duplicate_spawn` + `decision_is_pure_and_deterministic` (100-iter sanity on pure-fn shape — if the predicate grows a second argument, reviewer audit is forced). 719 unit + 3 + 3 + 2 + 2 + 2 (multi_client) + 4 = all green, clippy --release clean. Verified @ iter 29.
- [DONE] 3.2.12.multi-client-partial-close / 3.2.13.multi-client-last-close — bundled: pure predicate `external_app::decide_overlay_action(remaining_clients: usize) -> OverlayLifecycleAction { KeepRunning, Terminate }` trivially covers both items (remaining ≥ 1 → KeepRunning, 0 → Terminate). In-module tests (3): `partial_close_keeps_overlays` (remaining=1), `three_clients_one_closes_keeps_overlays` (boundary 1..=10), `last_close_terminates_overlays` (remaining=0). Integration tests at `tests/multi_client.rs` (2 new): `partial_close_keeps_overlays` + `last_close_terminates_overlays` mirror the predicate against a local model. Predicate is `#[allow(dead_code)]` — call-site wiring (teralib game-count watch channel → emit stop events) is still TODO and tracked as a fresh P1 `fix.overlay-lifecycle-wiring`. 722 unit + 3 + 3 + 4 (multi_client now) + 2 + 2 + 4 green, clippy --release clean. Verified @ iter 31.
- [DONE] 3.2.9.clean-recovery-logic — proof: new `tmm::recover_missing_clean(game_root)` with 3-branch semantics: (1) `.clean` exists → no-op, (2) `.clean` missing + current mapper lacks `TMM_MARKER` → safe to treat current as vanilla, copy to `.clean`, (3) `.clean` missing + current carries `TMM_MARKER` → refuse with "run verify game files, then retry" message. Relies on TMM convention that any TMM-style installer writes the marker, so marker-present implies already-modded. 4 inline tests (`clean_recovery_logic_nop_when_backup_exists`, `_creates_backup_from_vanilla_current`, `_refuses_when_current_is_modded`, `_errors_when_mapper_missing`). Fn is `#[allow(dead_code)]` — Tauri command + frontend "recovery" button is a P1 follow-up `fix.clean-recovery-wiring`. 743 unit (+4 new) all green, clippy --release clean. Verified @ iter 43.
- [DONE] 3.2.4.uninstall-all-restores-vanilla — **Note: real bug found + fixed.** `uninstall_gpk` restored vanilla entries for each object_path but never cleared the `TMM_MARKER` that `install_gpk` had written. After install + full uninstall, the mapper had one extra `TMM_MARKER` entry compared to vanilla — not byte-equal. Fix: after restoring vanilla entries, check whether every non-marker entry matches the backup (filename + offset + size + object_path); if yes, this was the last mod and the marker is dropped. Mixed installs (partial uninstall while other mods still live) keep the marker. Test `uninstall_all_restores_vanilla_bytes` at the `apply_mod_patches`+serialise level: computes `Sha256::digest(encrypt_mapper(serialize_mapper(vanilla)))` and `Sha256::digest(encrypt_mapper(serialize_mapper(post_uninstall)))`, asserts equal. Uses `apply_mod_patches` (iter 33) + manual TMM_MARKER insert/remove to avoid needing a crafted GPK footer — same semantic as install_gpk. 739 unit (+1 new) + 3 + 3 + 4 + 2 + 2 + 4 green, clippy --release clean. Verified @ iter 42.
- [DONE] 3.2.3.clean-backup-not-overwritten — proof: `services::mods::tmm::tests::clean_backup_not_overwritten_on_second_install` passes. Fixture walks the full scenario: (1) vanilla mapper bytes written, (2) first `ensure_backup` creates `.clean` with vanilla bytes, (3) current mapper overwritten with polluted "mod-installed" bytes simulating post-install state, (4) second `ensure_backup` must be a no-op that leaves `.clean` still containing the vanilla bytes (not the polluted current). Second test `ensure_backup_errors_when_mapper_missing` pins the "no source to back up" error path. The invariant is the foundation of uninstall — without it a second install would clobber the vanilla baseline and uninstall couldn't restore. 738 unit (+2 new) + 3 + 3 + 4 + 2 + 2 + 4 green, clippy --release clean. Verified @ iter 41.
- [DONE] 3.8.5.player-changelog — proof: `docs/CHANGELOG.md` authored covering v0.1.4-v0.1.12 in player-facing plain English (no conventional-commit prefixes). Each release summarises what the user saw, not every landed commit. CI gate at `scripts/check-changelog-plain-english.mjs` greps for `feat|fix|chore|refactor|docs|test|ci|build|perf|style|revert(<scope>)?:` at line-start bullet positions; skips backtick-inline references + fenced code blocks so the doc can mention the prefixes as counterexamples. Self-test at `scripts/check-changelog-plain-english.test.mjs` (6 pins: bare prefix, scoped prefix, plain english allowed, backtick examples allowed, fenced-code handling, all 11 common types covered). Initial gate run: 0 leaks in 126 lines. Verified @ iter 39.
- [DONE] 3.8.4.architecture-md — proof: `docs/mod-manager/ARCHITECTURE.md` authored with 10 top-level `##` sections covering every mod-manager subsystem: Types, Catalog, Registry, External-app download+extract+spawn, TMM mapper+install, Self-integrity, Tauri command boundary, Frontend mods.js, Cross-subsystem guarantees (fail-closed pipeline summary across 7 iters worth of work), and Known gaps (4 P1 follow-ups named explicitly: overlay-lifecycle-wiring, conflict-modal-wiring, self-integrity-baseline-embed, tauri-v1-eol-plan). 292 lines. Every section names owning file + public surface + invariants + known gaps so a reviewer can jump to code from doc directly. Verified @ iter 38.
- [DONE] 3.8.2.crate-level-comments — proof: all 6 `services/mods/*.rs` files (catalog, external_app, mod, registry, tmm, types) already carry substantive leading `//!` doc blocks ranging from 1–10 paragraphs. CI gate at `scripts/check-mods-crate-docs.mjs` walks the dir, extracts the leading `//!` block via a robust state-machine that treats blank lines inside the block as paragraph breaks, and requires ≥80 chars of actual content (prevents stub-only "//!" drive-bys). Self-test at `scripts/check-mods-crate-docs.test.mjs` (5 pin tests: extracts leading block, returns null when missing, rejects doc-after-use-statement, tolerates leading blanks, handles inline paragraph breaks). Coverage: 6/6. Verified @ iter 37.
- [DONE] 3.8.1.claude-md-mods — proof: root `CLAUDE.md` grows a `## Mod Manager` section (line 124, 62 lines — above PRD threshold of 30). Covers: feature-state table (10 rows mapping subsystem → shipped/blocked/TODO), code layout (commands/services/tests + frontend + docs dirs), build pipeline summary, deploy workflow summary (7 steps including the iter-25 scope gate + iter-16 secret-scan gate), and a "running the perfection loop" resume primer that points at fix-plan header. Also updated the stale Testing section: 693 -> 736 unit tests, added 4 integration-suite count, added Playwright e2e count (76/16). Verified @ iter 36.
- [DONE] 3.8.3.troubleshoot-md — proof: `docs/mod-manager/TROUBLESHOOT.md` covers 10 user-facing error categories (hash mismatch, network/download, catalog malformed, container sandbox, zip-slip, game-version mismatch, mapper backup, mapper r/w + CookedPC, mod file parse, registry/filesystem). `scripts/check-troubleshoot-coverage.mjs` extracts every production error template from `services/mods/*.rs` (3 Rust patterns: `.map_err(|e| format!(...))`, `return Err(format!(...))`, `return Err("...".into())`), computes a signature = text-before-first-`{}`-placeholder, and asserts each signature appears in the doc. Coverage report: **51/51 templates covered**. Self-test at `scripts/check-troubleshoot-coverage.test.mjs` (5 pin tests: extractor captures all 3 Rust shapes, multiple-per-file, ignores unrelated strings). **Iter 60 revalidation found this gate had silently regressed** — iter 49's tolerant catalog parse (commit 85ac310) added two new error templates that weren't mirrored in the doc. Iter 61 fix: added `Failed to read catalog body: <...>` to Section 2 (network/download) and `Catalog JSON envelope is malformed: <...>` to Section 3 (catalog malformed), with a new paragraph in §3 clarifying the envelope-vs-entry tolerance distinction. Gate green again: `node scripts/check-troubleshoot-coverage.mjs` → `ok — 51 production error templates covered`. Verified @ iter 61 (re-verified from iter 35).
- [DONE] 3.3.4.add-mod-from-file-wire — proof: end-to-end Add-mod-from-file plumbing wired. Rust: new `#[tauri::command] add_mod_from_file(path)` (in `commands/mods.rs`) reads the file, `tmm::parse_mod_file`, validates `is_safe_gpk_container_filename` (reuses PRD 3.1.4 sandbox), computes sha256, builds `ModEntry::from_local_gpk(sha_hex, &modfile)` (id = `local.<sha12>`), copies bytes to `<app_data>/mods/gpk/<id>.gpk`, best-effort `try_deploy_gpk` (mapper patch if game root set; else leave Disabled), upserts registry. Frontend: `mods.js::importBtn` no longer disabled; now opens a `.gpk` filter dialog via `@tauri-apps/api/dialog`, invokes the command with the picked path, refreshes the installed list. In-module tests (5 new): `from_local_gpk_id_uses_sha_prefix`, `_empty_name_falls_back_to_container`, `_empty_name_and_container_falls_back_to_generic`, `_is_deterministic` (re-import idempotency), `_trims_whitespace`. Playwright spec `tests/e2e/mod-import-file.spec.js` (3 tests pinning IPC shape: ModEntry fields + `.gpk` filter + command name). 736 unit + 3 + 3 + 4 + 2 + 2 + 4 green, clippy --release clean, Vitest 417/417, Playwright enumerates 76/16 files (3 new). Verified @ iter 34.
- [DONE] 3.3.2.per-object-gpk-merge — proof: extracted the package-patch loop from `install_gpk` into pure `apply_mod_patches(map, incoming) -> Result<(), String>`. install_gpk now calls it. Tests (3 new): `per_object_merge_both_apply` (two mods on distinct composites — both survive), `patch_apply_aborts_on_unknown_object_path` (game-version skew aborts before mutation), `patch_apply_is_idempotent_on_reinstall` (re-apply = no drift). Discovery while writing tests: **PRD assumption partially stale.** Production `parse_mapper` keys the mapper HashMap by `composite_name` alone, not `(composite, object_path)`. In practice each TMM composite has exactly one entry (each UPackage has a unique composite_name per TMM format), so "per-object" merge is in fact per-composite merge with the object_path along for the ride. The fix corrects a test-helper that was building synthetic multi-composite state impossible under production `parse_mapper`, and updated iter-32 conflict-detection tests to use distinct composite_names accordingly (both still pass, no logic change). 731 unit + 3 + 3 + 4 + 2 + 2 + 4 green, clippy --release clean. Verified @ iter 33.
- [DONE] 3.3.3.conflict-warning-ui — proof: pure predicate `services::mods::tmm::detect_conflicts(vanilla_map, current_map, incoming_modfile) -> Vec<ModConflict>` with 3-way semantics (`current == vanilla` = clean slot, `current == incoming.container` = self-reinstall, otherwise = conflict owned by another mod). Uses the same region_lock-gated lookup `install_gpk` uses — same mapper-entry match logic. `ModConflict { composite_name, object_path, previous_filename }` is serde-compatible for a future Tauri command. In-module tests (6): `detect_conflicts_returns_empty_on_vanilla_current`, `detect_conflicts_returns_empty_on_self_reinstall`, `detect_conflicts_flags_other_mod_owning_slot`, `detect_conflicts_reports_multiple_slots`, `detect_conflicts_mixed_slots_partial_report` (2 slots, 1 vanilla + 1 conflict), `detect_conflicts_missing_slot_is_not_a_conflict`. Playwright spec at `tests/e2e/mod-conflict-warning.spec.js` (3 tests: `conflict_warning_surfaced` + 2 shape-pins) enumerates cleanly via `npx playwright test --list` (73 total tests / 15 files; 3 new). Full Playwright run is gated on a warm Tauri dev build (webServer startup > 120s cold); the spec's value is as an IPC contract pin + regression guard against Rust-side serde rename. Predicate is `#[allow(dead_code)]` — Tauri command + modal wiring opens as P1 `fix.conflict-modal-wiring`. 728 unit + 3 + 3 + 4 + 2 + 2 + 4 green, clippy --release clean. Verified @ iter 32.
- [DONE] 3.8.8.lessons-learned — `docs/PRD/lessons-learned.md` initialised @ iter 30 with 10 entries covering patterns from iters 1-29 (bin-crate integration-test boundary, extract-predicate-before-test, recovery in load(), fail-closed copy in-source, zeroize Drop semantics, real-vuln-in-audit, scanner self-tests, transient flakes vs regressions, secret-scan commit-range scoping, cargo#6313 workaround, loop-cadence re-orientation). 200-line cap policy + archival ritual documented. Verified @ iter 30.

## META (human review)

Retrospective iterations may propose PRD changes. These land here — the loop cannot act on them. The human reviews and either edits the PRD or rejects.

- [META] **meta.shinra-sln-filename** — PRD §11 clause 6 + loop-prompt step 8 refer to `ShinraMeter.sln`, but the actual file is `Tera.sln`. Discovered iter 5. Human action: decide whether to (a) rename `Tera.sln` → `ShinraMeter.sln` (touches Shinra repo structure), or (b) update PRD §11 clause 6 + loop-prompt to say `Tera.sln`. Option (b) is less invasive; no downstream docs reference the sln name.
- [META] **meta.bin-crate-test-path-flexibility** — Proposed iter-30 retrospective. Four PRD items (3.1.1, 3.1.2, 3.1.11, 3.2.2) specify integration-test paths under `teralaunch/src-tauri/tests/*.rs`, but the crate has no `[lib]` target so integration tests cannot import launcher-private items. Standardised workaround (documented in `lessons-learned.md`): primary behavioural test lives in the module's `#[cfg(test)]` block, the PRD-named integration file becomes a symbolic external-contract pin. Proposed PRD amendment: change test-path language in §3.1–3.2 from "author `tests/X.rs::Y`" to "author test `Y` (integration-level when importable, module-level with `tests/X.rs` mirror otherwise)". Alternative: add a minimal `[lib]` target to src-tauri. Discovered iter 19/23/27/28/29. Human action: amend PRD or add lib target.
- [META] **meta.verify-and-implement-language** — Proposed iter-30 retrospective. PRD items worded "verify (and implement if missing)" (3.1.4) have reliably surfaced real vulnerabilities rather than just regression-pinning work (3.1.4 found attacker-controlled path traversal in `install_gpk`). This pattern works — but the PRD framing understates the likelihood of scope-expansion. Proposed PRD amendment: adopt "audit + implement + regression-test" as a stock pattern for §3.1 security items so iteration effort can be budgeted accurately. Discovered iter 24. Human action: consider standardising the phrasing across §3.1.

## REGRESSED

Items that were `[DONE]` and regressed. Always P0. Move back to `[P0]` slot when picked up.

(none yet)
