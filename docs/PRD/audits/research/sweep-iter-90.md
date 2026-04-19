# Research Sweep — Iter 90 (2026-04-19)

Eleventh research sweep since loop start. Delta against iter 80 (same
day, ~5 hours earlier). Worktree HEAD: `ef1c01d`.

## Scope

Focused delta against iter 80 — only report what CHANGED since that
sweep. Most categories return "no delta" because the calendar delta
is only hours. Two actionable items surfaced.

## Sources

- `gh api /repos/tauri-apps/plugins-workspace/releases` + `/tauri`
- `gh api /repos/tauri-apps/plugins-workspace/security-advisories`
- `gh api /repos/tauri-apps/tauri/security-advisories`
- `gh api /repos/TERA-Europe-Classic/external-mod-catalog/commits`
- `gh api /repos/gitleaks/gitleaks/releases`
- RustSec advisory-db via WebSearch (rustsec.org)

## Findings

### 1. time 0.3.45 — RUSTSEC-2026-0009 / CVE-2026-25727 ⬅ NEW ACTION

[RUSTSEC-2026-0009](https://rustsec.org/advisories/RUSTSEC-2026-0009.html)
— DoS via stack exhaustion on deep RFC 2822 input.

- **Affected**: `< 0.3.47`. Recursion limit added in 0.3.47.
- **Our resolved**: `time 0.3.45` in Cargo.lock. **In vulnerable range.**
- **CVSS**: 6.8 (medium). Published 2026-02-06 (predates iter 80's
  2025-10-01 cutoff window! Iter 80 missed this because it didn't
  sweep the time crate specifically).
- **Exploitability here**: requires attacker-controlled RFC 2822
  input fed into `time::parse`. We don't parse RFC 2822 explicitly
  anywhere in the app; time is pulled transitively (cookie-store,
  probably tauri-plugin-http) for timestamp arithmetic. Non-
  exploitable in our GET-on-allowlist pattern.
- **Fix**: `cargo update -p time --precise 0.3.47` (or later). Same
  shape as iter 81's rustls-webpki bump — pure lockfile change, no
  source touched, clears one advisory row.

**Action**: queue P2 `dep.time-bump` in fix-plan.

### 2. Tauri ecosystem — no new releases since iter 80

Last release batch at 2026-04-04 (before iter 80). All our pins still
match the latest:

| Pin | Our version | Latest | Delta |
|---|---|---|---|
| tauri | 2.10.3 | 2.10.3 (2026-03-04) | none |
| tauri-plugin-updater | 2.10.1 | 2.10.1 (2026-04-04) | none |
| tauri-plugin-http | 2.5.8 | 2.5.8 (2026-04-04) | none |
| tauri-plugin-dialog | 2.7.0 | 2.7.0 (2026-04-04) | none |
| tauri-plugin-fs | 2.5.0 | 2.5.0 (2026-04-04) | none |
| tauri-plugin-shell | 2.3.5 | 2.3.5 (2026-02-03) | none |

**Dep-dedup deferral (iter 87) holds unchanged** — tauri-plugin-http
2.5.8 still on reqwest 0.12; no 0.13 bump yet. Re-open criteria not
triggered.

### 3. Tauri / plugins-workspace advisories — no new entries

- tauri core: GHSA-57fm-592m-34r7 (2024), GHSA-2rcp-jvr4-r259 (2023),
  GHSA-wmff-grcw-jcfm (2023). All historical; all covered or N/A
  from iter 80 triage.
- plugins-workspace: GHSA-c9pr-q8gx-3mgp (CVE-2025-31477) — closed
  by our 2.3.5 pin + iter 86 explicit scope pin.

### 4. rustls-webpki — no new advisories since 0.103.12

iter 81 bumped to 0.103.12. RustSec site shows no newer advisory on
rustls-webpki. No action.

### 5. Malicious-crate removals — not in our tree ✓

Confirmed absence of logtrace / oncecell / postgress / if-cfg / serd /
xrvrv / lazystatic / envlogger via grep on Cargo.lock. These are
typosquats removed from crates.io on 2026-03-26 and 2026-04-01 per
Rust security blog. Clean.

### 6. gitleaks — patch release v8.30.1 (2026-03-21)

Iter 88 workflow pins `VER=8.30.0`. v8.30.1 is a patch bump; no rule
changes flagged in release notes. Safe to bump for currency.

**Action**: queue P3 `infra.gitleaks-bump-8.30.1` — trivial one-line
change in `.github/workflows/secret-scan.yml`.

### 7. Catalog upstream — unchanged

`gh api /repos/TERA-Europe-Classic/external-mod-catalog/commits`
shows no commits since iter 80's `33cf584` (2026-04-18T22:19Z). No
new mods published; no Shinra/TCC version bumps.

### 8. Vitest / Playwright — unchanged

Iter 80 flagged Vitest 2.1.8 → 4.x as cosmetic, queued P3 for
post-squash. Nothing newer to report.

## Recommendations → fix-plan action items

| Priority | ID | Acceptance |
|---|---|---|
| **P2** | `dep.time-bump` | `cargo update -p time --precise 0.3.47` (or latest). Closes RUSTSEC-2026-0009 / CVE-2026-25727. Trivial lockfile-only change; same pattern as iter 81's rustls-webpki bump. |
| **P3** | `infra.gitleaks-bump-8.30.1` | Bump `VER=8.30.0` → `VER=8.30.1` in `.github/workflows/secret-scan.yml`. Currency only, no rule changes. |

## Delta summary

Sweep-scale change between iter 80 and iter 90:

- **New P2**: dep.time-bump (one advisory row to clear).
- **New P3**: infra.gitleaks-bump-8.30.1 (currency).
- **No new Tauri-ecosystem blockers.**
- **No catalog drift.**
- **Dep-dedup deferral unchanged.**

Squash readiness unchanged (`ready_for_squash_merge: true`).

## Next sweep

Iter 100 per N%10=0 cadence (10 iters ahead). Focus suggestion:
- Re-check the dedup trigger (tauri-plugin-http → reqwest 0.13)
- Re-check all `cargo update --dry-run` deltas for new advisories
- Confirm catalog still matches Shinra/TCC upstream after any
  upstream releases
