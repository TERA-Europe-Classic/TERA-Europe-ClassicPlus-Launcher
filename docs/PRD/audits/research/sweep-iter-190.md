# Research sweep — iter 190

Date: 2026-04-19
Previous sweep: iter 170 (`sweep-iter-170.md`)
Previous revalidation: iter 180 (`revalidation-iter-180.md`) — all-gates-green
Worktree commit at start of iter 190: `13afeb4`

## Headline

**Zero new advisories, zero new dep drift, zero test regressions. Test count 1098 → 1188 (+90 since iter 170, +45 since iter 180 revalidation).** Advisory total 19 allowed warnings unchanged from iter 170/180. Iter 181-189 shifted focus from doc-layer guards (iter 173-179) to scanner guards + CVE-defence guards (iter 181-188 scanner sweep; iter 188 shell-scope-hardening; iter 189 tampered-catalog).

Iter-112 `--ignore RUSTSEC-2026-0097` (rand) and `--ignore RUSTSEC-2026-0007` (bytes) still stand; neither exit criterion fired in the iter 170-190 window.

## Part A — Research sweep

### A.1 Audit totals (iter-112 ignores applied)

```
$ cargo audit --ignore RUSTSEC-2026-0097 --ignore RUSTSEC-2026-0007
warning: 19 allowed warnings found
```

Exit 0. Same 19 upstream-locked warnings as iter 170 / iter 180.

Full-scan counts (without ignores): 1 vulnerability (bytes 1.11.0,
RUSTSEC-2026-0007) + 22 allowed warnings (= 19 + the 3 extra
occurrences the ignore covers). Matches the raw Cargo.lock state
captured at iter 170 — audit database entries added to RustSec
between sweeps hit crates we don't depend on.

| Advisory | Exit criterion | Status @ iter 190 |
|---|---|---|
| RUSTSEC-2026-0097 (rand 0.9.2) | tauri-plugin-notification bumps rand ≥ 0.10 | not yet |
| RUSTSEC-2026-0007 (bytes 1.11.0) | tower-http / tokio-util / reqwest chain bumps bytes ≥ 1.11.1 | not yet |

Neither retired since iter 170. Upstream cadence is slow for both —
consistent with prior sweeps.

### A.2 Raw advisory ID inventory (unchanged from iter 170)

```
RUSTSEC-2024-0370          proc-macro-error unmaintained
RUSTSEC-2024-0411..0420    gtk-rs GTK3 chain unmaintained (11 entries)
RUSTSEC-2024-0429          glib unsoundness (allowed)
RUSTSEC-2025-0057          fxhash unmaintained
RUSTSEC-2025-0075          unic-langid-impl unmaintained
RUSTSEC-2025-0080          unic-char-range unmaintained
RUSTSEC-2025-0081          unic-char-property unmaintained
RUSTSEC-2025-0098          unic-ucd-version unmaintained
RUSTSEC-2025-0100          unic-ucd-segment unmaintained
RUSTSEC-2025-0119          number_prefix unmaintained
RUSTSEC-2026-0007          bytes integer overflow      (ignored)
RUSTSEC-2026-0097          rand ≥ 0.9 weak-seed x3     (ignored)
```

Identical to the iter-170 inventory. No new crate-level advisories
hit our dep closure in the 20-iter window.

### A.3 cargo tree -d delta vs iter 170

Unchanged. The worktree has not pulled `Cargo.toml` or `Cargo.lock`
changes since iter 82's rustls-webpki bump (the last dep-touching
commit). Every iter 171-189 commit is `test(...)` additions.

### A.4 Tauri plugin / ecosystem notes

- `tauri-plugin-shell = "2"` pinned in Cargo.toml (iter 188
  `cargo_toml_keeps_tauri_plugin_shell_dep` asserts this).
- Our pinned `plugins.shell.open: true` (iter 86) still matches the
  2.x post-CVE-2025-31477 safe-scheme-allowlist default.
- No Tauri minor/major bumps observed on our tracked crates.

### A.5 Commits since main divergence

```
$ git log main..tauri-v2-migration --oneline | wc -l
127
```

(Was 108 at iter 170, 118 at iter 180.) All 19 commits iter 171-189
are `test(...)` — additive, no behaviour changes.

Regression-pattern grep: 1 match (`test(disk-full)` commit iter 165
contains `revert` referring to the `revert_to_vanilla` helper being
pinned). False positive; unchanged from iter 180.

## Part B — Iter 171-189 ledger

| Iter | Target | Delta | Pillar |
|---|---|---|---|
| 171 | `secret_scan_guard` | +? | Infra / Security |
| 172 | `tauri_v2_migration_audit_guard` | +? | Docs / Migration |
| 173 | `changelog_guard` | +5 | Docs |
| 174 | `meta_hygiene_guard` | +5 | Meta |
| 175 | `claude_md_guard` | +5 | Docs |
| 176 | `architecture_doc_guard` | +5 | Docs |
| 177 | `lessons_learned_guard` | +5 | Docs |
| 178 | `prd_path_drift_guard` | +5 | Docs |
| 179 | `crate_comment_guard` | +5 | Docs |
| 180 | revalidation | +0 | — |
| 181 | `search_perf_guard` | +5 | Perf / UX |
| 182 | `offline_banner_scanner_guard` | +5 | UX / §3.2 |
| 183 | `classicplus_guards_scanner_guard` | +5 | Config / Security |
| 184 | `shell_open_callsite_guard` | +5 | Security / §3.1.6 |
| 185 | `i18n_no_hardcoded_guard` | +5 | i18n / §3.7.4 |
| 186 | `mods_categories_ui_scanner_guard` | +5 | UX / iter-85 |
| 187 | `i18n_scanner_guard` | +5 | i18n / §3.4.7+§3.7.1 |
| 188 | `shell_scope_pinned` | +5 | Security / §3.1.6 |
| 189 | `tampered_catalog` | +5 | Security / §3.1.4 |

Aggregate: +90 Rust tests. Zero new files. Zero new deps.

### B.1 Milestone — scanner sweep complete (iter 181-187)

Every `tests/*_scanner_guard.rs` and scanner-associated guard that
had not received an extension since its creation iteration now
carries ≥ 12 pins with defense-in-depth coverage on real production
files (`src/app.js` / `src/mods.js` / `src/mods.html` / `src/mods.css`
/ `src/translations.json` / `src/index.html` / `capabilities/
migrated.json` / `teralib/src/config/config.json`).

Scanner guards now at uniform depth:

| Guard | @ 170 | @ 190 |
|---|---|---|
| search_perf_guard | 7 | 12 |
| offline_banner_scanner_guard | 7 | 12 |
| classicplus_guards_scanner_guard | 7 | 12 |
| shell_open_callsite_guard | 7 | 12 |
| i18n_no_hardcoded_guard | 8 | 13 |
| mods_categories_ui_scanner_guard | 8 | 13 |
| i18n_scanner_guard | 10 | 15 |

### B.2 Milestone — CVE-defence chain deepened (iter 188-189)

| Guard | @ 170 | @ 190 |
|---|---|---|
| shell_scope_pinned (§3.1.6) | 5 | 10 |
| shell_open_callsite_guard (§3.1.6) | 7 | 12 |
| tampered_catalog (§3.1.4) | 8 | 13 |

All three now carry ≥ 10 pins with real-file defense-in-depth
against scope/capability/downloader drift.

## Part C — Actionables

**Zero actionables surface this sweep.** The state remains stable,
advisory-clean (modulo the two documented ignores), and regression-
free. Every open item from iter 170 remains open:

- RUSTSEC-2026-0097 (rand) — upstream-gated
- RUSTSEC-2026-0007 (bytes) — upstream-gated
- §3.3.1 `every_catalog_entry_lifecycle.rs` — genuinely unshipped
- §3.8.7 `audits/units/` — genuinely unshipped
- C# pins (TCC/Shinra hardening) — documented-deferred

Next research sweep: iter 200. Next revalidation: iter 200 (same
tick — consider sweeping first then revalidating so the
revalidation baseline catches any 200-tick dep drift fresh).

## Summary

Iter 190 research sweep confirms zero dep-closure change since iter
170 and zero new advisories against our Cargo.lock. Iter 171-189
delivered +90 Rust tests across 19 existing guard / integration
files — a clean scanner sweep (iter 181-187) followed by pivot onto
earliest-extended small-baseline guards (iter 188-189). Three
CVE-defence guards (shell_scope_pinned + shell_open_callsite +
tampered_catalog) now carry real-file defence-in-depth at ≥ 10 pins
each.

`ready_for_squash_merge: true` status unchanged — the squash merge
remains user-gated per standing policy.
