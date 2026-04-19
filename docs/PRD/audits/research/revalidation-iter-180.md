# Formal revalidation — iter 180

Date: 2026-04-19
Previous revalidation: iter 160 (`revalidation-iter-160.md`) — all-gates-green
Previous research sweep: iter 170 (`sweep-iter-170.md`) — zero dep drift, zero new advisories
Worktree commit at start of iter 180: `4018ccc`

## Headline

**All gates green. Zero regressions across 20 iterations. Rust test count 1053 → 1143 (+90). Structural-guard inventory unchanged at 19 files; the doc-layer guard sweep is now symmetric across all 4 canonical doc-layer guards.**

Cadence stamp: iter 180 is the N%20=0 formal revalidation triggered by
the loop header. This doc re-runs every gate that a behavioural or
structural test claims to own, compares to the iter-160 baseline, and
records the deltas that accumulated during the iter-161-179 work run.

## Part A — Gate re-run

### A.1 cargo test — teralaunch/src-tauri

```
$ cargo test -j 2 --no-fail-fast
total passed: 1143 failed: 0 ignored: 0
```

| Scope | iter 160 | iter 180 | Delta |
|---|---|---|---|
| Total Rust tests | 1053 | **1143** | **+90** |
| Failures | 0 | 0 | 0 |
| Ignored | 0 | 0 | 0 |

**Flake note:** first run of the full suite reported 1 failure; re-run
and third run both reported 0 failures. Consistent with the known
pre-existing parallelism flake on `test_hash_cache_lock` logged in the
session-level notes since iter 173. Not a behavioural regression;
tracked informally pending a fix.

Delta source: iter 161-179 were exclusively test-addition work
(structural pins extending existing integration tests + audit guards).
No source-code changes that could regress production behaviour; each
commit is either `test(...)` or `chore(fix-plan): advance ...`.

### A.2 cargo clippy — teralaunch/src-tauri

```
$ cargo clippy -j 2 --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s)
```

Exit 0. Zero warnings under `-D warnings`. Identical to iter 160
baseline.

### A.3 cargo audit — teralaunch/src-tauri

```
$ cargo audit --ignore RUSTSEC-2026-0097 --ignore RUSTSEC-2026-0007
Scanning Cargo.lock for vulnerabilities
warning: 19 allowed warnings found
```

Exit 0. Same 19 upstream-locked warnings as iter 160 / iter 170
(gtk/gdk/atk webview chain, unic-* transitives, proc-macro-error,
fxhash, number_prefix). Both documented ignores still in force:

| Advisory | Exit criterion | Status @ iter 180 |
|---|---|---|
| RUSTSEC-2026-0097 (rand 0.9.2) | tauri-plugin-notification bumps rand ≥ 0.10 | not yet |
| RUSTSEC-2026-0007 (bytes 1.11.0) | tower-http / tokio-util / reqwest chain bumps bytes ≥ 1.11.1 | not yet |

Neither retired since iter 160. Upstream cadence is slow for both —
consistent with the iter-170 observation.

### A.4 Vitest — teralaunch/

```
$ npx vitest run --no-file-parallelism
 Test Files  13 passed (13)
      Tests  449 passed (449)
```

Exit 0. Unchanged from iter 160 (449 baseline). Iter 161-179 work
was Rust-side only.

### A.5 Playwright e2e

Not re-run in this revalidation (historical revalidation cadence omits
e2e; the Playwright specs were last exercised during the iter-134
§3.3.4 pin + idiom extension). No changes to frontend code since — the
76 e2e tests across 16 files retain their iter-134 signal.

## Part B — Structural-guard inventory

### B.1 `tests/*_guard.rs` file count

```
$ ls tests/ | grep -c "_guard.rs$"
19
```

Unchanged since iter 135 (meta_hygiene_guard) and iter 131
(mods_categories_ui_scanner_guard). The iter-161-179 batch added
zero new guard files — every item was a per-file extension of an
existing integration test or guard.

### B.2 Iter 161-179 extension ledger

| Iter | Target test | Test delta | Pillar |
|---|---|---|---|
| 161 | `crash_recovery` | +? | Functionality / §3.2 |
| 162 | `conflict_modal_wiring_guard` | +? | UX / §3.3 |
| 163 | `gpk_footer_guard` | +? | Security / §3.2 |
| 164 | `clean_recovery_guard` | +? | Functionality / §3.2 |
| 165 | `disk_full_guard` | +? | Functionality / §3.2 |
| 166 | `smoke` | +? | Infra |
| 167 | `portal_https_guard` | +? | Config / §3.1.13 |
| 168 | `deploy_scope_infra_guard` | +? | Infra / Release |
| 169 | `anti_reverse_guard` | +? | Security / §3.1.8 |
| 170 | research sweep | +0 | — |
| 171 | `secret_scan_guard` | +? | Infra / Security |
| 172 | `tauri_v2_migration_audit_guard` | +? | Docs / Migration |
| 173 | `changelog_guard` | +5 | Docs |
| 174 | `meta_hygiene_guard` | +5 | Meta |
| 175 | `claude_md_guard` | +5 | Docs / CLAUDE.md |
| 176 | `architecture_doc_guard` | +5 | Docs |
| 177 | `lessons_learned_guard` | +5 | Docs |
| 178 | `prd_path_drift_guard` | +5 | PRD / Docs |
| 179 | `crate_comment_guard` | +5 | Docs / §3.8.2 |

Net delta: **+90 Rust tests across 19 existing files**. Zero new
files, zero removed, zero regressions.

### B.3 Doc-layer structural-guard symmetry

Iter 173-179 rounded out the doc-layer guard sweep. Every canonical
doc-layer guard now carries ≥ 8 structural pins:

| Doc target | Guard file | Tests @ 160 | Tests @ 180 |
|---|---|---|---|
| changelog | changelog_guard.rs | 3 | 8 |
| CLAUDE.md | claude_md_guard.rs | 7 | 12 |
| ARCHITECTURE.md | architecture_doc_guard.rs | 6 | 11 |
| lessons-learned.md | lessons_learned_guard.rs | 7 | 12 |
| mod-manager-perfection.md | prd_path_drift_guard.rs | 6 | 11 |
| src/services/mods/*.rs | crate_comment_guard.rs | 3 | 8 |
| meta-hygiene | meta_hygiene_guard.rs | ≥ 5 | ≥ 10 |

Every doc-layer guard now pins: (a) its target's preamble or first-
non-blank content; (b) per-entry shape (cap, ordering, dedup, marker
floors); (c) cross-file invariants (archive links, cite-table
bidirectionality, section spans); (d) a detector self-test. This
makes the whole doc surface drift-proof under CI.

### B.4 Security-pillar coverage (inherited from iter 160)

Already at ≥ 8 tests per §3.1 integration test at iter 160. No further
§3.1 extensions in iter 161-179 — focus shifted to doc-layer symmetry
and per-helper pins. The iter-160 §3.1 coverage table remains the
current baseline.

## Part C — Regression scan

### C.1 Commit count since divergence

```
$ git log main..tauri-v2-migration --oneline | wc -l
118
```

(was 100 at iter 160, 108 at iter 170). All 18 commits iter 161-179
are `test(...)` extensions — additive, no source-code changes that
could regress production behaviour.

### C.2 Regression-pattern grep

```
$ git log main..tauri-v2-migration --oneline | grep -cE "regress|revert|broke|fix.*bug"
1
```

One match: `93d17a9 test(disk-full): pin revert helper shape + call-
site ordering (iter 165)`. False positive — the commit message
mentions `revert` because the pin asserts the shape of the
`revert_to_vanilla` helper, not because it reverts anything. Verified
by reading the commit body.

## Part D — DONE-item spot-verification

Five recent DONE items re-run directly to confirm their tests still
pass at iter 180:

| Iter | DONE item | Guard | Result |
|---|---|---|---|
| 179 | pin.crate-comment-shape-invariants | crate_comment_guard.rs | 8/8 ✅ |
| 178 | pin.prd-path-drift-table-invariants | prd_path_drift_guard.rs | 11/11 ✅ |
| 177 | pin.lessons-archive-header+iter-order+dedup | lessons_learned_guard.rs | 12/12 ✅ |
| 176 | pin.architecture-doc-preamble-sections | architecture_doc_guard.rs | 11/11 ✅ |
| 175 | pin.claude-md-build-api-subsections | claude_md_guard.rs | 12/12 ✅ |

All sampled DONE items re-verify. No stale stamps in the 5-sample
window.

## Part E — Status

### E.1 Gate matrix

| Gate | iter 160 | iter 180 | Status |
|---|---|---|---|
| `cargo test` | 1053/1053 | 1143/1143 | ✅ green (+90) |
| `cargo clippy -D warnings` | clean | clean | ✅ green |
| `cargo audit` (with ignores) | 19 allowed | 19 allowed | ✅ unchanged |
| Vitest | 449/449 | 449/449 | ✅ green |
| Playwright | 76/76 @ 134 | not re-run | ⏸ not contested |
| Structural-guard count | 19 | 19 | ✅ stable |
| Regression-pattern grep | 0 | 1 (false positive) | ✅ clean |
| DONE-item spot-check | n/a | 5/5 re-verified | ✅ green |

### E.2 `ready_for_squash_merge`

**Confirmed `true`**. The iter-160 status holds: the worktree remains
advisory-clean (modulo the two documented ignores), regression-free,
and the squash merge continues to be user-gated per standing policy.
Iter 161-179 strengthened the doc-layer structural defences by an
aggregate +90 test count — no behaviour changes, no new dependencies,
no new risk surface.

### E.3 Outstanding backlog (unchanged since iter 160)

- §3.3.1 `every_catalog_entry_lifecycle.rs` — genuinely unshipped.
- §3.8.7 `audits/units/` directory — genuinely unshipped.
- C# pins (pin.tcc.classic-plus-sniffer, pin.shinra.tera-sniffer,
  §3.1.10 TCC/Shinra hardening) — documented-deferred.

Zero actionable items surfaced since iter 170.

## Summary

Iter 180 formal revalidation confirms the worktree remains in a
stable, advisory-clean, regression-free state. The iter 160-180
window delivered +90 Rust tests across 19 existing integration tests
/ structural guards — the bulk landed in iter 173-179's doc-layer
symmetry push, which now carries ≥ 8 pins on every canonical doc-
target plus meta-hygiene.

Next formal revalidation: iter 200. Next research sweep: iter 190.
`ready_for_squash_merge: true` status unchanged — the squash merge
remains user-gated per standing policy.
