# fix-plan.md

Mutable priority queue consumed by the `/loop` driving `docs/PRD/mod-manager-perfection.md`.

Each iteration: read the counter below, detect iteration type (work / research / revalidation / retrospective / blocked-retry), do the work, update this file.

## Loop header (machine-parseable — DO NOT reformat)

```yaml
iteration_counter: 14
last_work_iteration: 14
last_research_sweep: 10
last_revalidation: never
last_revalidation_status: never
last_retrospective: never
last_blocked_retry: never
last_investigation_iteration: 9
total_items_done: 10
total_items_regressed: 0
total_iterations_to_cap: 1000
```

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

- [P0] **sec.tauri-v1-eol-plan** — Tauri 2.0 stable shipped 2024-10-02; 1.x is security-backport-only with all feature work on v2. CSP-per-window, capability ACLs, and updater-signature-v2 are v2-only — gates PRD items 3.1.8 (anti-reverse), 3.1.9 (updater-downgrade), 3.1.12 (CSP unsafe-inline). Action: author `docs/PRD/audits/security/tauri-v2-migration.md` with migration scope + risk assessment, then decide stay-on-1 vs migrate. Acceptance: audit doc signed off with a concrete plan (either: migrate, with milestones; or: stay with documented compensating controls). Pillar: Security. Discovered iter 10 RESEARCH SWEEP.
- [P0] **3.1.13.portal-https** — Migrate `teralib/src/config/config.json` portal API URL from `http://192.168.1.128:8090` (current) to HTTPS endpoint before Classic+ public launch. Acceptance: config URL starts with `https://`; end-to-end login works against HTTPS endpoint; audit doc signed off. Pillar: Security. **Iter 9 status:** audit draft authored at `docs/PRD/audits/security/portal-https-migration.md` (commit dc604d0). Remaining acceptance gated on external human infra (production FQDN + TLS cert + reverse proxy). Re-attempt at BLOCKED RE-TRY every 50 iters or when human provides the endpoint.
- [P0] **3.1.6.secret-leak-scan** — Run gitleaks + trufflehog across all 5 repos; rotate + CI + audit doc. Acceptance: CI exits 0 on every repo; audit doc lists all rotated secrets. Pillar: Security. **Iter 13 status:** gitleaks scan complete; audit committed 01064c9. 4 follow-up items queued (fix.shinra-teradps-token, fix.launcher-vs-dir-tracked, infra.gitleaks-allowlist, infra.secret-scan-ci). Item stays P0 until all 4 close + CI gates are green.
- [P0] **fix.shinra-teradps-token** — True positive from iter-13 secret scan: `Data/WindowData.cs:84` hard-codes `TeraDpsToken` + `TeraDpsUser` defaults (upstream-era, predates Classic+ fork). Blank to `""`. No history rewrite (token is in every historical clone of upstream; forward-only fix is sufficient). Acceptance: defaults are empty strings; Shinra build still clean. Pillar: Security.
- [P0] **infra.secret-scan-ci** — Author `.github/workflows/secret-scan.yml` for each public repo (external-mod-catalog, TCC, ShinraMeter): run gitleaks against PR diff, fail on unallowed hits. Launcher + teralib (private) get the same workflow for defense in depth. Acceptance: CI exits 0 on clean tree; new secret in a PR fails the job. Pillar: Security.
- [P0] **3.1.8.anti-reverse-hardening** — Enable Rust release-profile LTO + strip + CFG + stack-canary; apply `cryptify`/`chamox` string obfuscation to all sensitive string literals (portal URLs, AuthKey-adjacent code, update-server URL, deploy paths). Author `docs/PRD/audits/security/anti-reverse.md` with build-output inspection (IDA/Ghidra screenshots showing obfuscated strings). Acceptance: audit doc signed off; release build flags verified in `Cargo.toml`. Pillar: Security.
- [P0] **3.1.10.tcc-shinra-binary-hardening** — Strip TCC + Shinra release-mode debug symbols; evaluate ConfuserEx / Obfuscar for IL-obfuscation on sensitive types (e.g. sniffer keys, session-decryption code). Author `docs/PRD/audits/security/tcc-shinra-binary-hardening.md`. Acceptance: release binaries show no `.pdb`-adjacent symbols; audit doc signed off. Pillar: Security.
- [P0] **3.1.7.zeroize-audit** — Audit every struct field holding session-sensitive data (AuthKey, password, cookies, ticket). Apply `Zeroizing<String>` or `#[zeroize(drop)]`. Author `teralaunch/src-tauri/tests/zeroize_audit.rs`. Acceptance: test asserts drop semantics for each struct; compiles; passes. Pillar: Security.
- [P0] **3.1.9.updater-downgrade-refuse** — Patch Tauri updater to refuse downgrades (compare current version vs `latest.json` version; reject older). Author `teralaunch/src-tauri/tests/updater_downgrade.rs::refuses_older_latest_json`. Acceptance: test passes with a signed older `latest.json` fixture. Pillar: Security.
- [P0] **3.1.11.self-integrity** — Implement launcher self-integrity check at startup (hash exe, compare against embedded baseline). Author `teralaunch/src-tauri/tests/self_integrity.rs::detects_tampered_exe`. Acceptance: test fails cleanly when exe bytes mutated; launcher shows a clear reinstall prompt. Pillar: Security.
- [P0] **3.1.12.csp-unsafe-inline** — Audit `tauri.conf.json` CSP; remove `unsafe-inline` for `script-src`. Migrate any inline scripts to external modules. Author `teralaunch/src-tauri/tests/csp_audit.rs::csp_denies_inline_scripts`. Acceptance: test asserts CSP contains no `'unsafe-inline'` in `script-src`. Pillar: Security.
- [P0] **3.1.14.deploy-scope-gate** — Add CI gate in `.github/workflows/deploy.yml` that greps every upload URL and fails the job if any target path is outside `/classicplus/` on kasserver. Author `tests/deploy_scope.spec.js` as the gate script. Acceptance: job red when a test-upload URL points at `/` or `/classic/`. Pillar: Security.
- [P0] **3.1.2.gpk-install-sha** — Author `teralaunch/src-tauri/tests/gpk_install_hash.rs::sha_mismatch_aborts_before_write_gpk`. Acceptance: test passes; 0 bytes touch dest on mismatch. Pillar: Security.
- [P0] **3.1.4.gpk-deploy-sandbox** — Verify (and implement if missing) path-confinement in `tmm.rs` deploy. Author `teralaunch/src-tauri/tests/gpk_deploy_sandbox.rs::deploy_path_clamped_inside_game_root` with ≥ 5 `..`-based vectors. Acceptance: all vectors rejected, `.clean` untouched. Pillar: Security.
- [P0] **3.1.5.http-allowlist** — Author `teralaunch/src-tauri/tests/http_allowlist.rs::every_mod_url_on_allowlist` that scans mods code for URL literals and asserts each matches the `tauri.conf.json` HTTP allowlist. Acceptance: test passes; any new URL added to code without allowlist update fails CI. Pillar: Security.
- [P0] **3.1.1.external-sha-fail-closed** — Add test `teralaunch/src-tauri/src/services/mods/external_app.rs::tests::sha_mismatch_aborts_before_write` asserting SHA mismatch aborts with 0 bytes on disk. Acceptance: test passes. Pillar: Security.
- [P0] **3.1.3.zip-slip-reject** — Add test `teralaunch/src-tauri/src/services/mods/external_app.rs::tests::extract_zip_rejects_zip_slip` with ≥ 3 attack vectors (absolute, `..`, drive-letter). Acceptance: all rejected; no file written outside install root. Pillar: Security.

### Functionality correctness (PRD §3.3)

- [P0] **3.3.2.per-object-gpk-merge** — Replace current last-install-wins composite-level overwrite with per-object merge: preserve prior entries whose `object` differs from the incoming mod's objects. Author `tmm.rs::tests::per_object_merge_both_apply`. Acceptance: two test mods patching same composite but different objects both apply; mapper contains both entry sets. Pillar: Functionality.
- [P0] **3.3.3.conflict-warning-ui** — Detect same `(composite, object)` tuple collision at install time; surface modal warning with last-installed-wins disclaimer + log entry. Author `teralaunch/tests/e2e/mod-conflict-warning.spec.js::conflict_warning_surfaced`. Acceptance: test passes. Pillar: Functionality.
- [P0] **3.3.4.add-mod-from-file-wire** — Wire `Add mod from file…` UI to: pick GPK, parse via `tmm::parse_mod_file`, SHA the bytes, deploy, register. Author `teralaunch/tests/e2e/mod-import-file.spec.js::user_imported_gpk_deploys`. Acceptance: test passes with fixture GPK. Pillar: Functionality.
- [P0] **3.3.5.tcc-ingame-verified** — Launch Classic+ live server, verify TCC overlay renders, class window populates, cooldowns tick. Author `docs/PRD/audits/functionality/tcc-ingame-verified.md` with 3 class screenshots (Warrior, Sorcerer, Priest). Acceptance: audit signed off. Pillar: Functionality.
- [P0] **3.3.6.shinra-ingame-verified** — Launch Classic+ live server, verify Shinra DPS ticks, encounter-log exports. Author `docs/PRD/audits/functionality/shinra-ingame-verified.md` with DPS sample + export. Acceptance: audit signed off. Pillar: Functionality.
- [P0] **3.3.7.tcc-discord-webhooks** — Restore TCC Discord webhook integration (BAM alerts, raid notifications, user-configured URL) removed in strip commit `88e6fe30`. Surface as Settings tab. Author `TCC.Core/ViewModels/SettingsWindowViewModel.cs::tests::discord_webhook_settings_roundtrip` + `docs/PRD/audits/functionality/tcc-discord-webhooks.md`. Acceptance: audit + test pass. Pillar: Functionality.
- [P0] **3.3.8.tcc-strip-audit** — Walk commit `88e6fe30` diff; classify every removed user-facing feature as RESTORED / OUT-OF-SCOPE / DEFERRED with written justification. Author `docs/PRD/audits/functionality/tcc-strip-audit.md`. Acceptance: audit signed off; each feature tagged. Pillar: Functionality.
- [P0] **3.2.11.multi-client-attach-once** — Launcher's `attach-once` spawn semantics for Shinra/TCC when a 2nd `TERA.exe` launches. Author `teralaunch/src-tauri/tests/multi_client.rs::second_client_no_duplicate_spawn`. Acceptance: test passes. Pillar: Reliability.
- [P0] **3.2.12.multi-client-partial-close** — Closing client #1 while #2 runs keeps overlays alive. Author `multi_client.rs::partial_close_keeps_overlays`. Acceptance: test passes. Pillar: Reliability.
- [P0] **3.2.13.multi-client-last-close** — Last client close terminates overlays. Author `multi_client.rs::last_close_terminates_overlays`. Acceptance: test passes. Pillar: Reliability.
- [P0] **3.2.2.crash-recovery** — Launcher SIGKILL mid-install recoverable on next boot (registry row → `Error` not indeterminate). Author `teralaunch/src-tauri/tests/crash_recovery.rs::mid_install_sigkill_recovers_to_error`. Acceptance: test passes. Pillar: Reliability.
- [P0] **3.3.11.catalog-expansion-sweep** — Use `deep-research` to scout GPK mods beyond GitHub: Tumblr, MEGA, Mediafire, Yandex, VK, Discord server archives. Add viable entries to `external-mod-catalog/catalog.json`. Author `docs/PRD/audits/functionality/catalog-expansion-sweep.md` with sources-exhausted list. Acceptance: audit signed off. Pillar: Functionality.

### Documentation (PRD §3.8) — blocks hand-off per PRD §11 clause 19

- [P0] **3.8.3.troubleshoot-md** — Author `docs/mod-manager/TROUBLESHOOT.md` covering the 10 most common user-facing errors, mapped 1:1 against every `.map_err(|e| format!(...))` template in mods code. Acceptance: CI grep gate: 100% template coverage. Pillar: Documentation.

## P1 — Major quality

### Reliability (PRD §3.2)

- [P1] **sec.tokio-rustsec-2025-0023** — RUSTSEC-2025-0023 (tokio broadcast-channel clone parallelism without `Sync` bound, 2025-04-07). Run `cargo audit` to confirm our pinned `tokio = "1.49"` is in affected range; bump to patched release. Acceptance: `cargo audit` clean on tokio; regression tests pass. Pillar: Security. Discovered iter 10 RESEARCH SWEEP.
- [P1] **sec.aes-gcm-rustsec-2023-0096-audit** — RUSTSEC-2023-0096 (aes-gcm `decrypt_in_place_detached` leaks plaintext on tag-verify failure). Grep teralaunch + teralib for `decrypt_in_place_detached`; if no callers, close as N/A. If callers exist, bump aes-gcm and refactor. Acceptance: grep returns zero matches OR patched version in use + test coverage. Pillar: Security. Discovered iter 10 RESEARCH SWEEP.
- [P1] **3.2.1.edge-cases-X1-X24** — Author 24 named tests across `teralaunch/tests/e2e/mod-*.spec.js` + `teralaunch/src-tauri/tests/mod_*.rs` covering edge cases X1–X24. Define X1–X24 in `docs/PRD/test-plan.md` (new file). Acceptance: 24/24 tests passing. Pillar: Reliability.
- [P1] **3.2.3.clean-backup-not-overwritten** — Test `tmm.rs::tests::clean_backup_not_overwritten_on_second_install`. Acceptance: test passes. Pillar: Reliability.
- [P1] **3.2.4.uninstall-all-restores-vanilla** — Test `full_cycle.rs::uninstall_all_restores_vanilla_bytes`. Acceptance: byte-for-byte equal via SHA-256. Pillar: Reliability.
- [P1] **3.2.5.offline-retry** — Test `mod-catalog-resilience.spec.js::offline_shows_retry`. Acceptance: test passes. Pillar: Reliability.
- [P1] **3.2.6.parse-error-filter** — Test `catalog-parse.test.js::malformed_entries_filtered`. Acceptance: test passes. Pillar: Reliability.
- [P1] **3.2.7.parallel-install-serialised** — Test `parallel_install.rs::same_id_serialised`. Acceptance: test passes; no double-write race. Pillar: Reliability.
- [P1] **3.2.8.disk-full-revert** — Test `disk_full.rs::revert_on_enospc`. Acceptance: partial writes reversed on ENOSPC. Pillar: Reliability.
- [P1] **3.2.9.clean-recovery-logic** — Implement + test `tmm.rs::tests::clean_recovery_logic`: first deploy recreates `.clean` if no GPK is currently patched; otherwise refuses with recovery instructions. Acceptance: test passes. Pillar: Reliability.

### Functionality (PRD §3.3)

- [P1] **3.3.1.every-catalog-entry-lifecycle** — Author `teralaunch/src-tauri/tests/every_catalog_entry_lifecycle.rs` iterating all 101 catalog ids through install → enable → spawn → cleanup → uninstall → mapper-restored. Acceptance: 101/101 green. Pillar: Functionality.
- [P1] **3.3.9.tcc-elinu-classes** — Verify TCC renders non-default race/gender/class combos from `elinu` datacenter. Author `docs/PRD/audits/functionality/tcc-elinu-classes.md` with screenshots. Acceptance: audit signed off. Pillar: Functionality.
- [P1] **3.3.10.shinra-elinu-classes** — Verify Shinra tracks non-default race/gender/class combos. Author `docs/PRD/audits/functionality/shinra-elinu-classes.md`. Acceptance: audit signed off. Pillar: Functionality.
- [P1] **3.3.12.fresh-install-defaults** — Test `commands/mods.rs::tests::fresh_install_defaults_enabled`. Acceptance: `enabled=true`, `auto_launch=true`, `status=Enabled`. Pillar: Functionality.
- [P1] **3.3.14.tcc-class-layouts-verified** — Verify all 13 TCC classes on Classic+ (no empty apex tiles, awakening present). Author `docs/PRD/audits/functionality/tcc-class-layouts-verified.md` with 13 screenshots. Acceptance: audit signed off. Pillar: Functionality.
- [P1] **3.3.15.toggle-intent-only** — Test `commands/mods.rs::tests::toggle_intent_only`: enable toggle is pure intent (no spawn, no kill). Acceptance: test passes; multi-client tests also pass. Pillar: Functionality.

### UX (PRD §3.4)

- [P1] **3.4.1.time-to-first-mod** — Test `mod-time-to-first-mod.spec.js::fresh_user_under_60s`. Acceptance: p95 ≤ 60 s across 10 runs on 10 Mbit/s. Pillar: UX.
- [P1] **3.4.2.modal-chrome** — Test `mod-modal-chrome.spec.js` (×/Esc/backdrop close). Acceptance: 3 sub-tests pass. Pillar: UX.
- [P1] **3.4.3.focus-trap** — Test `mod-accessibility.spec.js::focus_trapped`. Acceptance: test passes. Pillar: UX.
- [P1] **3.4.4.tray-surgical-update** — Test `mods-dom-perf.test.js::tray_surgical_update`. Acceptance: ≤ 3 DOM mutations per progress tick. Pillar: UX.
- [P1] **3.4.5.toggle-animation** — Test `mod-toggle-animation.spec.js`. Acceptance: ≥ 60 fps during 180 ms cubic-bezier. Pillar: UX.
- [P1] **3.4.6.scrollbar-palette** — Visual baseline `mod-modal-scrollbar.png`. Acceptance: visual diff ≤ 0.1 %. Pillar: UX.
- [P1] **3.4.7.no-jargon** — Test `i18n-jargon.test.js::no_jargon_in_translations` (blocklist: "composite", "mapper", "SHA", "TMM"). Acceptance: test passes. Pillar: UX.
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
- [P1] **3.6.3.progress-10hz** — Test `progress_rate.rs::at_least_10hz`. Acceptance: ≥ 10 events/s on 10 Mbit/s simulated link. Pillar: Performance.
- [P1] **3.6.4.search-one-frame** — Test `search-perf.test.js::under_one_frame`. Acceptance: ≤ 16 ms on 300 entries. Pillar: Performance.
- [P1] **3.6.5.scroll-60fps** — Test `mod-scroll-perf.spec.js` via Playwright tracing. Acceptance: 0 long tasks > 50 ms. Pillar: Performance.
- [P1] **3.6.6.bundle-size-gate** — Author `.github/workflows/deploy.yml` size-diff gate. Acceptance: CI fails if growth > 5 % vs previous tag. Pillar: Performance.

### i18n (PRD §3.7)

- [P1] **3.7.1.key-parity** — Test `i18n-parity.test.js::keys_equal_across_locales`. Acceptance: `keys(EN) == keys(FR) == keys(DE) == keys(RU)`. Pillar: i18n.
- [P1] **3.7.2.no-raw-key-leaks** — Test `mod-i18n.spec.js::no_raw_key_leaks` (4 locales). Acceptance: no `MODS_*` keys in DOM. Pillar: i18n.
- [P1] **3.7.3.language-switch-inplace** — Test `mod-language-switch.spec.js`. Acceptance: re-render in-place, no full reload. Pillar: i18n.
- [P1] **3.7.4.no-hardcoded-english** — Test `i18n-no-hardcoded.test.js` (grep-based). Acceptance: 0 matches in mods.js/html/app.js mod paths. Pillar: i18n.

### Documentation (PRD §3.8)

- [P1] **3.8.1.claude-md-mods** — Add `## Mod Manager` section to root `CLAUDE.md` (feature state + build + deploy). Acceptance: grep hits section; ≥ 30 lines. Pillar: Documentation.
- [P1] **3.8.2.crate-level-comments** — Add `//!` crate-level comment to every `teralaunch/src-tauri/src/services/mods/*.rs`. Acceptance: CI script reports 100 % coverage. Pillar: Documentation.
- [P1] **3.8.4.architecture-md** — Author `docs/mod-manager/ARCHITECTURE.md` with one section per subsystem. Acceptance: file exists with ≥ 1 section per subsystem. Pillar: Documentation.
- [P1] **3.8.5.player-changelog** — Author `docs/CHANGELOG.md` per release in plain English (no `feat:` / `fix:` prefixes). Acceptance: CI grep gate passes. Pillar: Documentation.
- [P1] **3.8.6.catalog-readme-schema** — Update `external-mod-catalog/README.md` schema to match actual JSON schema 1:1. Acceptance: CI equality check passes. Pillar: Documentation.
- [P1] **3.8.8.lessons-learned** — Initialise `docs/PRD/lessons-learned.md` (empty header; capped 200 lines; retrospective iteration maintains). Acceptance: file exists. Pillar: Documentation.

### Security follow-ups from iter-13 scan

- [P1] **infra.gitleaks-allowlist** — Author `.gitleaks.toml` in each repo with entries for the known false positives: launcher auth_service.rs test fixtures (`abc123def456`), TCC XAML brush keys (Tier1..5DungeonBrush, Tcc*Gradient*Brush). Keeps the iter-13 baseline of 0 real findings green for future CI runs. Acceptance: `gitleaks detect --source . --config .gitleaks.toml` returns 0 on every repo. Pillar: Security.

### Adversarial corpus (PRD §5.3)

- [P1] **adv.zip-slip** — Adversarial test: zip-slip path rejected. Covered by 3.1.3.
- [P1] **adv.gpk-deploy-escape** — Covered by 3.1.4.
- [P1] **adv.tampered-catalog** — Author adversarial test: catalog entry with wrong SHA returns Err + 0 bytes + registry Error. Acceptance: test passes. Pillar: Security.
- [P1] **adv.http-redirect-offlist** — Author test: HTTP redirect to non-allowlisted host is rejected by reqwest. Acceptance: test passes. Pillar: Security.
- [P1] **adv.replay-latest-json** — Covered by 3.1.9.
- [P1] **adv.tampered-exe** — Covered by 3.1.11.
- [P1] **adv.bogus-gpk-footer** — Already passing (parse_mod_file_rejects_non_tmm_gpks). Verify survives any tmm.rs refactor.
- [P1] **adv.composite-object-collision** — Covered by 3.3.3.
- [P1] **adv.sigkill-mid-download** — Author test: registry row recoverable to Error on boot, partial file removed. Acceptance: test passes. Pillar: Reliability.
- [P1] **adv.disk-full** — Covered by 3.2.8.

### Test pinning (PRD §5.4) — author before any refactor

- [P1] **pin.tmm.cipher** — Golden-file test for `tmm.rs` 3-pass cipher (GeneratePackageMapper XOR + middle-outward swap + Key1 shuffle). Acceptance: byte-for-byte pin of current cipher output on a fixture mapper. Pillar: Reliability.
- [P1] **pin.tmm.parser** — Golden-file test for `parse_mod_file` on a fixture GPK mod. Acceptance: fixture → expected `ModFile` struct byte-for-byte. Pillar: Reliability.
- [P1] **pin.tmm.merger** — Property-based test for composite-merge output stability on randomised mod sets. Acceptance: merge(A, B) == merge(A; apply B). Pillar: Reliability.
- [P1] **pin.external.download-extract** — Golden-file test for `external_app.rs` download + extract flow on a fixture zip. Acceptance: output tree byte-for-byte. Pillar: Reliability.
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

## META (human review)

Retrospective iterations may propose PRD changes. These land here — the loop cannot act on them. The human reviews and either edits the PRD or rejects.

- [META] **meta.shinra-sln-filename** — PRD §11 clause 6 + loop-prompt step 8 refer to `ShinraMeter.sln`, but the actual file is `Tera.sln`. Discovered iter 5. Human action: decide whether to (a) rename `Tera.sln` → `ShinraMeter.sln` (touches Shinra repo structure), or (b) update PRD §11 clause 6 + loop-prompt to say `Tera.sln`. Option (b) is less invasive; no downstream docs reference the sln name.

## REGRESSED

Items that were `[DONE]` and regressed. Always P0. Move back to `[P0]` slot when picked up.

(none yet)
