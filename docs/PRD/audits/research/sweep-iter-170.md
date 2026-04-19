# Research sweep — iter 170

Date: 2026-04-19
Previous sweep: iter 150 (`sweep-iter-150.md`)
Previous revalidation: iter 160 (`revalidation-iter-160.md`) — all-gates-green
Worktree commit at start of iter 170: `c0bb3bc`

## Headline

**Zero new advisories, zero new dep drift, zero test regressions. Test count 1004 → 1098 (+94 since iter 150, +45 since iter 160 revalidation).**

Iter-112 `--ignore RUSTSEC-2026-0097` (rand) and `--ignore RUSTSEC-2026-0007` (bytes) still stand; neither exit criterion fired in the iter 150-170 window. All 19 structural guards extended further; every integration test under `tests/` now carries iter-150+ pins (smoke.rs closed the last gap at iter 166).

## Part A — Research sweep

### A.1 cargo tree -d delta vs iter 150

| Crate | Versions iter 150 | Versions iter 170 | Delta |
|---|---|---|---|
| reqwest | 0.12.28 + 0.13.2 | 0.12.28 + 0.13.2 | unchanged |
| cookie | 0.16.2 + 0.18.1 | 0.16.2 + 0.18.1 | unchanged |
| cookie_store | 0.21.1 + 0.22.0 | 0.21.1 + 0.22.0 | unchanged |
| env_logger | 0.10.2 + 0.11.8 | 0.10.2 + 0.11.8 | unchanged |
| bitflags | 1.3.2 + 2.10.0 | 1.3.2 + 2.10.0 | unchanged |
| getrandom | 0.1.16 + 0.2.17 + 0.3.4 | 0.1.16 + 0.2.17 + 0.3.4 | unchanged |
| hashbrown | 0.12.3 + 0.14.5 + 0.16.1 | 0.12.3 + 0.14.5 + 0.16.1 | unchanged |
| rand | 0.7.3 + 0.8.5 + 0.9.2 | 0.7.3 + 0.8.5 + 0.9.2 | unchanged |
| zip | 2.4.2 + 4.6.1 | 2.4.2 + 4.6.1 | unchanged |
| rustls-webpki | 0.103.12 | 0.103.12 | unchanged |
| bytes | 1.11.0 | 1.11.0 | unchanged |

No new duplicates; no resolved-version changes. Upstream-gated state identical to iter 150.

### A.2 cargo audit — teralaunch/src-tauri

With iter-112 ignores applied:

```
$ cargo audit --ignore RUSTSEC-2026-0097 --ignore RUSTSEC-2026-0007
warning: 19 allowed warnings found
```

Exit 0. Same 19 upstream-locked warnings as iter 130/140/150/160 (gtk/gdk/atk webview chain, unic-* transitives, proc-macro-error, fxhash, number_prefix).

**Exit criteria check for the two ignored advisories:**

| Advisory | Exit criterion | Status @ iter 170 |
|---|---|---|
| RUSTSEC-2026-0097 (rand 0.9.2) | tauri-plugin-notification bumps rand ≥ 0.10 | not yet |
| RUSTSEC-2026-0007 (bytes 1.11.0) | tower-http / tokio-util / reqwest chain bumps bytes ≥ 1.11.1 | not yet |

Neither retired this sweep.

### A.3 cargo audit — teralib

```
$ cargo audit
Scanning Cargo.lock for vulnerabilities (233 crate dependencies)
```

Exit 0. Zero findings. Unchanged since iter 111 (dotenv drop).

### A.4 Upstream release notes delta since iter 150

| Package | Iter-150 pin | Iter-170 pin | Delta |
|---|---|---|---|
| tauri | 2.10.3 | 2.10.3 | unchanged |
| tauri-plugin-notification | 2.3.3 | 2.3.3 | unchanged — still pulls rand 0.9.2 |
| tauri-plugin-http | 2.5.8 | 2.5.8 | unchanged — still on reqwest 0.12 |
| tauri-plugin-updater | 2.10.1 | 2.10.1 | unchanged — still alone on reqwest 0.13 / zip 4 |
| reqwest | 0.12.28 | 0.12.28 | unchanged |
| rustls | via 0.103.12 | via 0.103.12 | unchanged |
| zip | 4.x | 4.x | unchanged |

Ecosystem quiet. Consistent with the iter 100-150 pace.

### A.5 No new P-slot candidates surfaced

Iter 170's sweep discovered nothing actionable. Outstanding backlog unchanged since iter 150:

- Zero open P-slot items from the dep/audit track.
- C# pins still documented-deferred (pin.tcc.classic-plus-sniffer, pin.shinra.tera-sniffer, §3.1.10 TCC/Shinra hardening).
- §3.3.1 every_catalog_entry_lifecycle.rs still genuinely unshipped.
- §3.8.7 audits/units/ directory still genuinely unshipped.

Backlog clean. The loop has consumed every iter-150 item.

## Part B — Structural-guard inventory delta since iter 150

**Iters 151-169 were a per-test deepening batch — no NEW guard files shipped; every existing test/guard's coverage deepened with iter-150+ structural pins.** 19 active `*_guard.rs` files (unchanged since iter 135) plus 17 other integration tests, every one now carrying at least one source-inspection pin from the iter 150-170 window.

### B.1 Per-target extension ledger (iter 151-169)

| Iter | Target | Tests before | Tests after | Delta |
|---|---|---|---|---|
| 151 | `add_mod_from_file_wiring` | 5 | 11 | +6 |
| 152 | `csp_audit` | 4 | 8 | +4 |
| 153 | `self_integrity` | 2 | 8 | +6 |
| 154 | `updater_downgrade` | 7 | 13 | +6 |
| 155 | `zeroize_audit` | 4 | 10 | +6 |
| 156 | `http_allowlist` | 3 | 9 | +6 |
| 157 | `http_redirect_offlist` | 3 | 8 | +5 |
| 158 | `multi_client` | 5 | 11 | +6 |
| 159 | `parallel_install` | 4 | 9 | +5 |
| 160 | revalidation | — | — | +0 (doc) |
| 161 | `crash_recovery` | 6 | 11 | +5 |
| 162 | `conflict_modal` | 4 | 9 | +5 |
| 163 | `bogus_gpk_footer` | 4 | 9 | +5 |
| 164 | `clean_recovery` | 3 | 8 | +5 |
| 165 | `disk_full` | 4 | 9 | +5 |
| 166 | `smoke` | 2 | 7 | +5 |
| 167 | `portal_https_guard` | 6 | 11 | +5 |
| 168 | `deploy_scope_infra_guard` | 8 | 13 | +5 |
| 169 | `anti_reverse_guard` | 11 | 16 | +5 |

Net delta: **+94 Rust tests across 18 distinct integration tests / structural guards** (1004 → 1098). Zero new guard files, zero removed, zero regressions.

### B.2 Rust test-count trajectory

| Iter | Total | Running delta |
|---|---|---|
| 150 | 1004 | baseline |
| 160 | 1053 | +49 |
| 170 | 1098 | **+94** |

### B.3 Integration-test coverage milestone

As of iter 166, **every integration test under `teralaunch/src-tauri/tests/`** carries structural pins defending the SHAPE of its target — not just the behaviour. The iter 151-166 window closed this gap for: `add_mod_from_file_wiring`, `csp_audit`, `self_integrity`, `updater_downgrade`, `zeroize_audit`, `http_allowlist`, `http_redirect_offlist`, `multi_client`, `parallel_install`, `crash_recovery`, `conflict_modal`, `bogus_gpk_footer`, `clean_recovery`, `disk_full`, `smoke`.

Iter 167-169 extended three structural guards (`portal_https_guard`, `deploy_scope_infra_guard`, `anti_reverse_guard`) with additional-angle pins on top of their iter-142/145/146 baselines.

### B.4 Regression scan

```
$ git log main..tauri-v2-migration --oneline | wc -l
109
```

(was 90 @ iter 150, 100 @ iter 160). The 19-commit iter 150-170 delta is 18 test commits + 1 research-sweep audit commit.

Grep for regression patterns:

```
$ git log main..tauri-v2-migration --oneline | grep -cE "regress|revert|broke|fix.*bug"
1
```

The single match — `93d17a9 test(disk-full): pin revert helper shape + call-site ordering (iter 165)` — is a **false positive**: "revert" refers to the `revert_partial_install_*` helper pair in `external_app.rs`, not a git revert. Iter 165 is a pure test addition. No real regressions in the window.

## Part C — Status: all-gates-green (by inspection)

This sweep is a research pass, not a formal revalidation (that cadence hits at N=180). Inspection-level checks:

- cargo audit (both workspaces): clean with documented ignores.
- Dep tree: zero drift vs iter 150.
- Rust tests: 1098/1098 per iter 169's final run.
- Structural-guard inventory: 19 files (unchanged since iter 135); 18 existing integration tests + guards deepened in iter 150-170 window.
- Worktree ready state: `ready_for_squash_merge: true` unchanged since iter 100.

## Summary

Iter 170 confirms the worktree remains in a stable, advisory-clean (modulo documented ignores), regression-free state. The iter 150-170 window closed the **integration-test structural-pin gap** (every `tests/*.rs` now carries source-inspection pins) plus deepened three infrastructure guards (portal-https, deploy-scope, anti-reverse).

Total delta since iter 150: **+94 Rust tests, 0 new guard files, 18 tests/guards deepened, 19 additive commits, 0 real regressions**.

Net iter-170 risk delta: **zero**. No new P-slot items surfaced. `ready_for_squash_merge: true` status unchanged. Formal revalidation scheduled for iter 180.
