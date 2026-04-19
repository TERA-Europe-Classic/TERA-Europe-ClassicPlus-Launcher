# Research sweep + revalidation — iter 100 (DOUBLE-DUTY)

Date: 2026-04-19
Previous sweep: iter 90 (`sweep-iter-90.md`)
Previous revalidation: iter 72 (all-gates-green)
Worktree commit at start of iter 100: `3360952`

## Part A — Research sweep

### A.1 Dependency tree (cargo tree -d)

Top duplicate-version findings, compared against iter-90 / iter-87 state:

| Dep | Versions | Source | Delta vs iter 90 | Action |
|---|---|---|---|---|
| `reqwest` | 0.12.28 + 0.13.2 | ours on 0.12; tauri-plugin-updater alone on 0.13 | unchanged | deferred per iter-87 `dep.dedupe-reqwest-zip` (upstream-gated on tauri-plugin-http still on 0.12) |
| `cookie` | 0.16.2 + 0.18.1 | ours on 0.16; reqwest 0.12 chain on 0.18 | unchanged | tied to reqwest-chain bump — same deferral |
| `cookie_store` | 0.21.1 + 0.22.0 | reqwest_cookie_store 0.8 on 0.21; reqwest 0.12 on 0.22 | unchanged | tied to reqwest-chain bump |
| `env_logger` | 0.10.2 + 0.11.8 | bin crate on 0.10; teralib lib on 0.11 | unchanged | low value — single-binary ship, dedup is cosmetic |
| `bitflags` | 1.3.2 + 2.10.0 | ecosystem-wide | unchanged | pervasive; not actionable |
| `getrandom` | 0.1.16 + 0.2.17 | legacy rand 0.7 chain via phf_generator; newer chain via cryptify | unchanged | upstream ecosystem constraint |
| `rustls-webpki` | 0.103.12 | single version | ✅ cleaned iter 81 | — |
| `time` | 0.3.47 | single version | ✅ cleaned iter 91 | — |

Net delta since iter 90: zero new drift. Two iter-80/iter-90 queue items closed (rustls-webpki, time). The reqwest/cookie chain remains the dominant deferral, tracked in `docs/PRD/audits/security/dep-dedup-investigation.md`.

### A.2 Advisory scan

**`cargo audit` not installed locally.** Can't run RUSTSEC DB scan from this sweep. Candidate for P-slot `infra.cargo-audit-ci`: add `cargo-audit` to CI pipeline so every push gets an advisory scan. Low scope — single workflow file addition.

Manual scan of recent advisories (WebFetch-informed, iter 90 delta only): no new RUSTSEC entries affecting our direct dep set since iter 90. Advisories affecting the reqwest 0.13 / tauri-plugin-updater chain still considered non-exploitable in our use-pattern per iter-87 investigation notes.

### A.3 Upstream release notes (delta since iter 90)

Scanned for major-version bumps or CVEs on: tauri, reqwest, rustls, zip, tokio, serde.

| Package | Our pin | Latest delta | Action |
|---|---|---|---|
| tauri | 2.10.3 | 2.10.x series, no 2.11 yet | hold |
| tauri-plugin-shell | 2.3.5 | unchanged | hold — sec.shell-scope-hardening locked @ iter 86 |
| tauri-plugin-updater | 2.10.1 | unchanged | hold (reqwest 0.13 carrier) |
| reqwest | 0.12.28 | unchanged | hold — upstream-gated |
| rustls | via rustls-webpki 0.103.12 | unchanged | hold — fresh since iter 81 |
| zip | 4.x (via pin.external) | unchanged | hold — golden test pins output tree, not crate version |
| tokio | (transitive) | stable | hold |
| serde | (transitive) | stable | hold |

Net: no action-items surfaced. The ecosystem is quiet since iter 90. This is consistent with Rust crates.io's slow pace around April 2026.

### A.4 New P-slot candidates

- **P3 `infra.cargo-audit-ci`** — add `cargo install cargo-audit` + `cargo audit` step to `.github/workflows/`. Low scope (single workflow file), high value (zero-effort ongoing advisory coverage). Queued for post-squash.

No other candidates surfaced.

## Part B — Revalidation

### B.1 Full test suite

| Suite | Result | Delta vs iter 72 |
|---|---|---|
| Rust (`cargo test -j 2 --no-fail-fast`) | 860/860 pass (18 test binaries) | +83 (iter 72: 777) |
| Rust (clippy `-D warnings`) | clean | no regression |
| JS (vitest `--no-file-parallelism`) | 449/449 pass (13 files) | +32 (iter 72: 417) |

Test-count delta (+83 Rust, +32 JS) reflects iter 73-99 pin / adversarial-corpus / fix / drift-guard additions.

### B.2 Regression scan

`git log main..tauri-v2-migration --oneline` — 40 commits since the divergence. Every commit is additive (test/fix/docs/deps bump) — no REGRESSED flags. Top recent commits visually inspected:

- `3360952 test(mods): pin PRD §3.2.9 clean-recovery double-arm + fix 6th drift` — additive
- `bdbdb8e test(mods): extend PRD drift-guard pin list from 7 to 20 criteria` — additive
- `1ced792 docs(prd): fix §3 measurement-path drift + pin with drift guard` — additive
- `0990473 test(mods): pin adv.tampered-catalog wiring` — additive
- `b9712c6 test(mods): pin SIGKILL mid-download filesystem retry invariants` — additive

No commits touched `[DONE]` item implementation in a regressive way.

### B.3 Spot-check pinned invariants

Grep-verified key `#[test] fn` still grep-findable at claimed locations (drift-guard `every_pin_source_file_has_named_test` already enforces this for 22 pins, but a spot-scan across a 7-fn set is cheap insurance):

| Fn | File |
|---|---|
| `sha_mismatch_aborts_before_write` | `services/mods/external_app.rs` ✅ |
| `extract_zip_rejects_zip_slip` | `services/mods/external_app.rs` ✅ |
| `deploy_path_clamped_inside_game_root` | `services/mods/tmm.rs` ✅ |
| `uninstall_all_restores_vanilla_bytes` | `services/mods/tmm.rs` ✅ |
| `per_object_merge_both_apply` | `services/mods/tmm.rs` ✅ |
| `clean_recovery_logic_creates_backup_from_vanilla_current` | `services/mods/tmm.rs` ✅ |
| `mid_install_sigkill_recovers_to_error` | `services/mods/registry.rs` ✅ |

All present.

### B.4 Revalidation status

**all-gates-green**

- 860/860 Rust (+83 vs iter 72), 449/449 JS (+32 vs iter 72)
- Clippy `-D warnings` clean
- Zero REGRESSED items across 40 commits since divergence
- 22-pin drift-guard + spot-check both confirm structural stability

## Summary

Research sweep: no new action items beyond a single P3 infra candidate (`cargo-audit-ci`). Revalidation: all-gates-green with sizeable positive delta on test counts (+83 Rust, +32 JS) driven by iter-73-99 pin coverage work.

Worktree `ready_for_squash_merge: true` status unchanged. Net iter-100 risk delta: zero.
