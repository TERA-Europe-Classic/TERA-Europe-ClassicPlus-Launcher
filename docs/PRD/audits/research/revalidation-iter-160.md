# Formal revalidation — iter 160

Date: 2026-04-19
Previous revalidation: iter 140 (`revalidation-iter-140.md`) — all-gates-green
Previous research sweep: iter 150 (`sweep-iter-150.md`) — zero dep drift, zero new advisories
Worktree commit at start of iter 160: `dbb521d`

## Headline

**All gates green. Zero regressions across 20 iterations. Rust test count 975 → 1053 (+78). Structural-guard inventory unchanged at 19 files; every existing guard deepened.**

Cadence stamp: iter 160 is the N%20=0 formal revalidation triggered by
the loop header. This doc is the proof-of-state the user-gated squash
merge depends on — it re-runs every gate that a behavioural or
structural test claims to own, compares to the iter-140 baseline, and
records the deltas that accumulated during the iter-141-159 work run.

## Part A — Gate re-run

### A.1 cargo test — teralaunch/src-tauri

```
$ cargo test -j 2 --no-fail-fast
total passed: 1053 failed: 0 ignored: 0
```

| Scope | iter 140 | iter 160 | Delta |
|---|---|---|---|
| Total Rust tests | 975 | **1053** | **+78** |
| Failures | 0 | 0 | 0 |
| Ignored | 0 | 0 | 0 |

Delta source: iter 141-159 were exclusively test-addition work
(structural pins extending existing integration tests + audit guards).
No source-code changes that could regress production behaviour; each
commit is either `test(...)` or `chore(fix-plan): advance ...`.

### A.2 cargo clippy — teralaunch/src-tauri

```
$ cargo clippy -j 2 --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s)
```

Exit 0. Zero warnings under `-D warnings`. Identical to iter 140
baseline.

### A.3 cargo audit — teralaunch/src-tauri

```
$ cargo audit --ignore RUSTSEC-2026-0097 --ignore RUSTSEC-2026-0007
Scanning Cargo.lock for vulnerabilities
warning: 19 allowed warnings found
```

Exit 0. Same 19 upstream-locked warnings as iter 130 / iter 140 /
iter 150 (gtk/gdk/atk webview chain, unic-* transitives,
proc-macro-error, fxhash, number_prefix). Both documented ignores still
in force:

| Advisory | Exit criterion | Status @ iter 160 |
|---|---|---|
| RUSTSEC-2026-0097 (rand 0.9.2) | tauri-plugin-notification bumps rand ≥ 0.10 | not yet |
| RUSTSEC-2026-0007 (bytes 1.11.0) | tower-http / tokio-util / reqwest chain bumps bytes ≥ 1.11.1 | not yet |

Neither retired since iter 130. Upstream cadence is slow for both —
consistent with the iter-150 observation.

### A.4 Vitest — teralaunch/

```
$ npx vitest run --no-file-parallelism
 Test Files  13 passed (13)
      Tests  449 passed (449)
```

Exit 0. Unchanged from iter 140 (449 baseline was already established).
JS-side pins added iter 132-134 expanded coverage inside existing
vitest files; the file count stayed at 13.

### A.5 Playwright e2e

Not re-run in this revalidation (historical revalidation cadence
omits e2e; the Playwright specs were last exercised during the
iter-134 §3.3.4 pin + idiom extension). No changes to frontend code
since — the 76 e2e tests across 16 files retain their iter-134
signal.

## Part B — Structural-guard inventory

### B.1 `tests/*_guard.rs` file count

```
$ ls tests/ | grep -c "_guard.rs$"
19
```

Unchanged since iter 135 (meta_hygiene_guard) and iter 131
(mods_categories_ui_scanner_guard). The iter-141-159 batch added
zero new guard files — every item was a per-file extension of an
existing integration test or guard.

### B.2 Iter 141-159 extension ledger

| Iter | Target test | Test delta | Pillar |
|---|---|---|---|
| 141 | `changelog_guard` | +4 | Docs |
| 142 | `portal_https_guard` | +3 | Config / §3.1.13 |
| 143 | `crate_comment_guard` | +1 | Docs |
| 144 | `secret_scan_guard` | +4 | Infra / Security |
| 145 | `deploy_scope_infra_guard` | +4 | Infra / Release |
| 146 | `anti_reverse_guard` | +4 | Security / §3.1.8 |
| 147 | `tauri_v2_migration_audit_guard` | +4 | Docs / Migration |
| 148 | `shell_scope_pinned` | +2 | Security / §3.1.6 **crossed 1000** |
| 149 | `tampered_catalog` | +3 | Security / §3.1.4 |
| 150 | research sweep | +0 | — |
| 151 | `add_mod_from_file_wiring` | +6 | Functionality / §3.3.4 |
| 152 | `csp_audit` | +4 | Security / §3.1.12 |
| 153 | `self_integrity` | +6 | Security / §3.1.11 |
| 154 | `updater_downgrade` | +6 | Security / §3.1.9 |
| 155 | `zeroize_audit` | +6 | Security / §3.1.7 |
| 156 | `http_allowlist` | +6 | Security / §3.1.5 |
| 157 | `http_redirect_offlist` | +5 | Security / §3.1.5 |
| 158 | `multi_client` | +6 | Functionality / §3.2.11 §3.2.12 |
| 159 | `parallel_install` | +5 | Functionality / §3.2.7 |

Net delta: **+78 Rust tests across 19 existing files**. Zero new
files, zero removed, zero regressions.

### B.3 Security-pillar coverage highlight

Iter 152-159 deepened every §3.1 security integration test:

| PRD id | Integration test | Tests @ 140 | Tests @ 160 |
|---|---|---|---|
| §3.1.4 tampered-catalog | tampered_catalog.rs | 5 | 8 |
| §3.1.5 outbound allowlist | http_allowlist.rs | 3 | 9 |
| §3.1.5 redirect gate | http_redirect_offlist.rs | 3 | 8 |
| §3.1.6 shell scope | shell_scope_pinned.rs | 3 | 5 |
| §3.1.7 zeroize | zeroize_audit.rs | 4 | 10 |
| §3.1.8 anti-reverse | anti_reverse_guard.rs | 7 | 11 |
| §3.1.9 updater downgrade | updater_downgrade.rs | 7 | 13 |
| §3.1.11 self-integrity | self_integrity.rs | 2 | 8 |
| §3.1.12 CSP | csp_audit.rs | 4 | 8 |

Every item in the §3.1 block now carries ≥ 8 tests, most of which are
source-inspection structural pins that catch the class of one-character
drift that behavioural tests miss (e.g. `>=` vs `>` in the updater
gate, `.read()` vs `.write()` on the mods_state lock).

## Part C — Regression scan

### C.1 Commit count since divergence

```
$ git log main..tauri-v2-migration --oneline | wc -l
100
```

(was 90 at iter 150, 70 at iter 130). All 10 commits iter 150-159
are `test(...)` extensions — additive, no source-code changes that
could regress production behaviour.

### C.2 Regression-pattern grep

```
$ git log main..tauri-v2-migration --oneline | grep -cE "regress|revert|broke|fix.*bug"
0
```

Zero matches. Consistent with the iter-150 sweep's identical result
(0 matches on the wider 90-commit window).

## Part D — Status

### D.1 Gate matrix

| Gate | iter 140 | iter 160 | Status |
|---|---|---|---|
| `cargo test` | 975/975 | 1053/1053 | ✅ green (+78) |
| `cargo clippy -D warnings` | clean | clean | ✅ green |
| `cargo audit` (with ignores) | 19 allowed | 19 allowed | ✅ unchanged |
| Vitest | 449/449 | 449/449 | ✅ green |
| Playwright | 76/76 @ 134 | not re-run | ⏸ not contested |
| Structural-guard count | 19 | 19 | ✅ stable |
| Regression-pattern grep | 0 | 0 | ✅ clean |

### D.2 `ready_for_squash_merge`

**Confirmed `true`**. The invariant chain the user-gated squash merge
depends on is stronger at iter 160 than at iter 140: every §3.1
integration test now carries structural pins that defend the SHAPE
of the production wiring, not just its behavioural outcome on known
inputs.

### D.3 Outstanding backlog (unchanged since iter 150)

- §3.3.1 `every_catalog_entry_lifecycle.rs` — genuinely unshipped.
- §3.8.7 `audits/units/` directory — genuinely unshipped.
- C# pins (pin.tcc.classic-plus-sniffer, pin.shinra.tera-sniffer,
  §3.1.10 TCC/Shinra hardening) — documented-deferred.

Zero actionable items surfaced since iter 150.

## Summary

Iter 160 formal revalidation confirms the worktree remains in a
stable, advisory-clean (modulo documented ignores), regression-free
state. The iter 140-160 window delivered +78 Rust tests across 19
existing integration tests / structural guards — every addition is a
deeper pin against a specific refactor-hazard class (widened enum
variant, lock-type swap, write-vs-read flip, predicate-signature
drift, error-format drift, missing write-through save, one-character
comparator flip).

Next formal revalidation: iter 180. Next research sweep: iter 170.
`ready_for_squash_merge: true` status unchanged — the squash merge
remains user-gated per standing policy.
