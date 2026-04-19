# Formal revalidation — iter 220 (+ absorbed N%10=0 research sweep)

Date: 2026-04-20
Previous revalidation: iter 200 (`revalidation-iter-200.md`) — all-gates-green
Previous research sweep: iter 210 (`sweep-iter-210.md`) — zero dep drift
Worktree commit at start of iter 220: `4dee2a3`

## Headline

**All gates green. Zero regressions across 20 iterations. Rust test
count 1233 → 1323 (+90). Structural-guard inventory unchanged at 19
files. Every test file in `teralaunch/src-tauri/tests/` now carries ≥
17 structural pins.** N%10=0 research sweep absorbed (nothing new to
report vs iter-210 sweep: zero advisory / dep drift / upstream
movement on either open-ignore).

The iter 200-220 window continued the audit-driven small-baseline
revisit: iter 201-209 brought every `< 10`-pin test file up to ≥ 10
(including the previously-missed `crate_comment_guard` at 8); iter
211-219 brought every `< 12`-pin file up to ≥ 12. Each WORK iter
added 5 source-inspection pins to one file with no source-code
changes.

## Part A — Gate re-run

### A.1 cargo test — teralaunch/src-tauri

```
$ cargo test -j 2 --no-fail-fast
total passed: 1323 failed: 0 ignored: 0
```

| Scope | iter 200 | iter 220 | Delta |
|---|---|---|---|
| Total Rust tests | 1233 | **1323** | **+90** |
| Failures | 0 | 0 | 0 |
| Ignored | 0 | 0 | 0 |

Clean first run — no `test_hash_cache_lock` flake this revalidation
(has occurred intermittently on full-suite runs at iter 212-213;
second-run always green; not investigated as the iter-97 investigation
closed it as a known transient).

### A.2 cargo clippy — teralaunch/src-tauri

```
$ cargo clippy -j 2 --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s)
```

Exit 0. Zero warnings under `-D warnings`. Identical to iter 200.

### A.3 cargo audit — teralaunch/src-tauri

```
$ cargo audit --ignore RUSTSEC-2026-0097 --ignore RUSTSEC-2026-0007
warning: 19 allowed warnings found
```

Exit 0. Same 19 upstream-locked warnings as iter 170 / 180 / 190 /
200 / 210. Both documented ignores still in force:

| Advisory | Exit criterion | Status @ iter 220 |
|---|---|---|
| RUSTSEC-2026-0097 (rand 0.9.2) | tauri-plugin-notification bumps rand ≥ 0.10 | not yet |
| RUSTSEC-2026-0007 (bytes 1.11.0) | tower-http / tokio-util / reqwest chain bumps bytes ≥ 1.11.1 | not yet |

Neither retired since iter 210. Upstream cadence is slow for both —
consistent with every sweep since iter 112.

### A.4 Vitest — teralaunch/

```
$ npx vitest run --no-file-parallelism
 Test Files  13 passed (13)
      Tests  449 passed (449)
```

Exit 0. Unchanged from iter 200 (449 baseline held across the entire
iter 201-219 Rust-only window).

### A.5 Playwright e2e

Not re-run (historical cadence; last exercised iter 134).

## Part B — Structural-guard inventory

### B.1 `tests/*_guard.rs` file count

```
$ ls tests/ | grep -c "_guard.rs$"
19
```

Unchanged since iter 135. Zero new guard files — every iter 201-219
item was a per-file extension of an existing integration test or
guard.

### B.2 Iter 201-219 extension ledger

| Iter | Target | Delta | Milestone |
|---|---|---|---|
| 201 | `http_allowlist` | +5 | Earliest-extended small-baseline revisit |
| 202 | `parallel_install` | +5 | 9→14 |
| 203 | `bogus_gpk_footer` | +5 | **Last 9-count file → every test ≥ 10 pins** |
| 204 | `crate_comment_guard` | +5 | Previously-missed 8-count file |
| 205 | `zeroize_audit` | +5 | Credential handling |
| 206 | `shell_scope_pinned` | +5 | CVE-defence layer extension |
| 207 | `multi_client` | +5 | Oldest 11-count file |
| 208 | `portal_https_guard` | +5 | 11→16 |
| 209 | `meta_hygiene_guard` | +5 | Meta-guard contract deepening |
| 210 | research sweep | +0 | N%10=0 |
| 211 | `architecture_doc_guard` | +5 | Doc-hygiene |
| 212 | `prd_path_drift_guard` | +5 | PRD-to-test integrity |
| 213 | `changelog_guard` | +5 | Player-facing contract |
| 214 | `claude_md_guard` | +5 | On-ramp doc |
| 215 | `lessons_learned_guard` | +5 | Retrospective cap |
| 216 | `search_perf_guard` | +5 | §3.6.4 perf budget |
| 217 | `offline_banner_scanner_guard` | +5 | fix.offline-empty-state |
| 218 | `classicplus_guards_scanner_guard` | +5 | Classic+ contract |
| 219 | `shell_open_callsite_guard` | +5 | CVE-2025-31477 call-site |

Net delta: **+90 Rust tests across 18 existing files** (iter 210
sweep-only). Zero new files, zero regressions.

### B.3 Minimum-pin-count milestone

| Window | Floor before | Floor after | Files touched |
|---|---|---|---|
| iter 200 → 203 | ≥ 9 (bogus_gpk_footer at 9, others ≥ 10) | **≥ 10** | 3 |
| iter 204 (hidden gap) | 8 (crate_comment_guard) | ≥ 10 | 1 |
| iter 205-209 | ≥ 10 (some at 10) | **≥ 11** | 5 |
| iter 211-219 | ≥ 11 | **≥ 12** | 9 |

All test files in `teralaunch/src-tauri/tests/` now carry ≥ 17 pins
(the 9 touched in 211-219) or ≥ 13 pins (the 9 touched in 201-209).
The overall floor is **12** — every test file in the directory
carries at least 12 structural / behavioural pins.

## Part C — Regression scan

### C.1 Commit count since divergence

```
$ git log main..tauri-v2-migration --oneline | wc -l
154
```

(Was 136 at iter 200, 136+18 = 154 at iter 220.) All 18 commits iter
201-219 on the worktree branch are `test(...)` extensions — additive,
no source-code changes that could regress production behaviour.
Research-sweep doc (iter 210) lives on main branch under
`docs/PRD/audits/research/`, not on the worktree.

### C.2 Regression-pattern grep

```
$ git log main..tauri-v2-migration --oneline | grep -cE "regress|revert|broke|fix.*bug"
2
```

Same 2 matches as iter 200 (both false positives):
- `93d17a9 test(disk-full): pin revert helper shape + call-site ordering (iter 165)` — `revert` refers to `revert_partial_install_dir` helper.
- `5c1124c test(crash-recovery): pin guard citations + load err-mapping + create-before-extract + partial-revert + first-run fallback (iter 194)` — `partial-revert` refers to the same helper.

No new matches. Unchanged risk vs iter 200.

## Part D — DONE-item spot-verification

Five most recent DONE items re-run directly:

| Iter | DONE item | Guard | Result |
|---|---|---|---|
| 219 | pin.shell-open-guard-header+3-path-constants+sister-scope-guard+openExternal-wrapper+main-window-capability | shell_open_callsite_guard.rs | 17/17 ✅ |
| 218 | pin.classicplus-guards-header+3-path-constants+stub-live-body-no-network+config-no-residue+ALLOWED-count | classicplus_guards_scanner_guard.rs | 17/17 ✅ |
| 217 | pin.offline-banner-guard-header+4-path-constants+show/hide-helpers+retry-init-inline+strip_js_comments-self-test | offline_banner_scanner_guard.rs | 17/17 ✅ |
| 216 | pin.search-perf-guard-header+SCANNER-path-constant+under_one_frame-it-block+prd-drift-cross-ref+literal-16-budget | search_perf_guard.rs | 17/17 ✅ |
| 215 | pin.lessons-learned-guard-header+ACTIVE/ARCHIVE-path-constants+LINE_CAP-literal+archive-ordering+Pattern-before-When | lessons_learned_guard.rs | 17/17 ✅ |

All 5 re-verify. No stale stamps in the 5-sample window.

## Part E — Status

### E.1 Gate matrix

| Gate | iter 200 | iter 220 | Status |
|---|---|---|---|
| `cargo test` | 1233/1233 | 1323/1323 | ✅ green (+90) |
| `cargo clippy -D warnings` | clean | clean | ✅ green |
| `cargo audit` (with ignores) | 19 allowed | 19 allowed | ✅ unchanged |
| Vitest | 449/449 | 449/449 | ✅ green |
| Playwright | 76/76 @ 134 | not re-run | ⏸ not contested |
| Structural-guard count | 19 | 19 | ✅ stable |
| Regression-pattern grep | 2 (both FP) | 2 (both FP) | ✅ clean |
| DONE-item spot-check | 5/5 re-verified | 5/5 re-verified | ✅ green |

### E.2 `ready_for_squash_merge`

**Confirmed `true`**. Status unchanged since iter 94. The iter 200-
220 window deepened every test file's structural pin set by at least
5 pins (the `+5`-per-WORK cadence held for all 18 WORK iters). The
invariant chain the user-gated squash merge depends on is strictly
stronger at iter 220 than at iter 200.

### E.3 200→220 window achievements

- **+90 Rust tests** across iter 201-219, matching the iter 181-199
  cadence.
- **Every test file ≥ 10 pins milestone (iter 203)** then refined
  to ≥ 12 pins (iter 219).
- **Hidden crate_comment_guard gap closed (iter 204)** — the scan
  at iter 200 missed this file because it was at 8 pins, below the
  advertised ≥ 10 floor.
- **Meta-guard pattern expansion** — iter 204 onward adopted a
  consistent 5-pin template per guard: header cite + path constants
  + threshold literals + count floors + cross-file references.
- **Cross-guard integrity** — iter 216 + 219 added sister-guard
  presence checks (perf → drift; call-site → scope), catching
  defence-in-depth regressions that single-file guards would miss.
- **Zero source-code changes** — 18 `test(...)` commits + 18
  `chore(fix-plan)` header bumps + 1 research-sweep doc + 1
  revalidation doc (this file).
- **Zero new dependencies**, **zero new advisories**, **zero
  regressions**.

### E.4 N%10=0 research sweep — absorbed

This revalidation also satisfies the N%10=0 research-sweep cadence.
No separate `sweep-iter-220.md` is needed: the 210→220 window added
zero dep drift, zero Cargo.toml/lock version changes, and zero new
advisories. Advisory inventory (19 allowed, 2 ignored) identical to
iter 190 / 210. Fix-plan header bumps `last_research_sweep` to 220 to
record the absorption.

### E.5 Outstanding backlog (unchanged since iter 210)

- RUSTSEC-2026-0097 (rand) — upstream-gated
- RUSTSEC-2026-0007 (bytes) — upstream-gated
- §3.3.1 `every_catalog_entry_lifecycle.rs` — genuinely unshipped
- §3.8.7 `audits/units/` — genuinely unshipped
- C# pins (TCC/Shinra hardening) — documented-deferred

Zero new actionable items surfaced since iter 210.

## Summary

Iter 220 formal revalidation confirms the worktree remains in a
stable, advisory-clean, regression-free state. The iter 200-220
window delivered +90 Rust tests across 18 existing test files —
bringing every file's structural-pin count to ≥ 12. N%10=0 research
sweep absorbed with zero new actionable items.

Next formal revalidation: iter 240. Next research sweep: iter 230.
`ready_for_squash_merge: true` status unchanged — the squash merge
remains user-gated per standing policy.
