# PRD — Mod Manager Production Perfection

**Status:** Active
**Owner:** Lukas Teixeira Dopcke + agent fleet (driven by `/loop`)
**Target state:** A Windows launcher that lets TERA Europe Classic+ players install, update, uninstall, enable/disable external mods (Shinra, TCC) and GPK mods (community-sourced) with zero jargon and zero manual steps — production-ready, reverse-resistant, auto-updating, and fully decoupled from the catalog cadence.

**Scope (5 repos):**
- `teralaunch/` + `teralib/` — the Tauri launcher
- `external-mod-catalog/` — the catalog GitHub repo
- `TCC/` — Classic+ fork of Tera-Custom-Cooldowns
- `ShinraMeter/` — Classic+ fork of ShinraMeter

**Completion sentinel:** `MOD-MANAGER-PERFECTION-COMPLETE`

This is the authoritative spec. Every loop iteration reads this plus `fix-plan.md` plus `CLAUDE.md` and picks the next work item.

> **Loop rule:** emit `MOD-MANAGER-PERFECTION-COMPLETE` ONLY when every clause in §11 Exit Criteria is objectively true and provable via a passing test, a git SHA, or a signed-off audit doc.

---

## 1. Mission

TERA Europe Classic+ players need a single launcher surface to discover, install, update, and uninstall client-side mods. Without this, they'd manually hunt Classic+-compatible versions of Shinra, TCC, and dozens of community GPK mods across GitHub / Tumblr / MEGA / VK — prone to mistakes, missing security updates, and broken state left behind. The launcher's mod manager replaces all of that with a curated catalog, a TMM-compatible GPK deployer, automatic updates bypassing broken upstream updaters, multi-client-aware process management, and zero user jargon. Perfect means: every flow works end-to-end for a non-technical player; every error surfaces a clear cause and recovery; every download is SHA-verified; the launcher binary resists reverse engineering; every stripped TCC/Shinra feature that has Classic+ value is restored; no feature in scope is left broken or missing.

## 2. Non-goals

- **Non-Windows platforms.** Launcher is Windows-only (Tauri v1 WebView2, ShellExecuteExW, winreg). No macOS, no Linux, no Wine commitment.
- **Languages other than EN/FR/DE/RU.** No JA, KR, TW, ES.
- **Mobile or tablet layouts.** Launcher is fixed 1280×720 desktop.
- **Uploading user-authored mods to the catalog.** Local drop-in works; no "share to catalog" UI.
- **Server-side signing of community mods.** SHA-256 integrity only.
- **Running the launcher without a configured game path.** Play + GPK deploy require a valid `S1Game` folder; user is guided to Settings when missing.
- **Keeping Moongourd / Firebase / LFG-write / Cloud telemetry stubbed in TCC** — these stay stubbed (no production signal, external service dependencies, or v100.02 relevance).
- **Automatic resolution of semantic mod conflicts** that aren't file-level overlaps. We detect (composite, object) tuple collisions and warn; we don't solve runtime-interaction conflicts.
- **Legacy Classic (v35 era) or post-v100.02 patches.** Game version pinned to `10002` with region key family `EUC`.
- **Supporting old ShinraMeter / TCC releases that predate the Classic+ fork pin.** End users get the launcher's curated version only.

## 3. Pillars & success criteria

Priority order (from §4): Security > Reliability > Functionality > UX > Accessibility > Performance > i18n > Documentation.

Every criterion is **test-measurable** or maps to a signed-off audit doc. "Feels done" is never acceptable.

### 3.1 Security (priority 1) — "No mod download is written to disk without matching the catalog's SHA-256, no extracted archive can write outside its install root, no deploy path can escape the configured game root, only the allow-listed URLs in `tauri.conf.json` are reachable, and the launcher binary resists reverse engineering."

| # | Criterion | Measurement path | Threshold |
|---|-----------|------------------|-----------|
| 3.1.1 | SHA-256 mismatch fail-closes before any disk write for external installs | `teralaunch/src-tauri/src/services/mods/external_app.rs::tests::sha_mismatch_aborts_before_write` | test passes; 0 bytes touch dest |
| 3.1.2 | SHA-256 mismatch fail-closes before any disk write for GPK installs | `teralaunch/src-tauri/src/services/mods/external_app.rs::tests::sha_mismatch_aborts_before_write_gpk` | test passes |
| 3.1.3 | Zip extraction rejects paths escaping the install root | `teralaunch/src-tauri/src/services/mods/external_app.rs::tests::extract_zip_rejects_zip_slip` (zip-slip adversarial) + `teralaunch/src-tauri/src/services/mods/external_app.rs::tests::golden_extract_multi_entry_tree` (iter-94 §5.4 golden output-tree pin — binary fidelity across ASCII / 256-byte binary / nested dirs) | tests pass with ≥ 3 attack vectors |
| 3.1.4 | TMM deploy path is clamped inside the configured game root | `teralaunch/src-tauri/src/services/mods/gpk.rs::tests::deploy_path_clamped_inside_game_root` | test passes with ≥ 5 `..`-based vectors |
| 3.1.5 | HTTP allowlist in `tauri.conf.json` covers every URL the mods code can request; any other URL aborts | `teralaunch/src-tauri/tests/http_allowlist.rs::every_mod_url_on_allowlist` (allowlist coverage) + `teralaunch/src-tauri/tests/http_redirect_offlist.rs::external_app_download_client_disables_redirects` (iter-77 adv.http-redirect-offlist — blocks 3xx bounce-out of allowlist) | tests pass |
| 3.1.6 | No hardcoded secrets in any of the 5 repos; no leaks in git history | `docs/PRD/audits/security/secret-leak-scan.md` + `.github/workflows/secret-scan.yml` (trufflehog + git-secrets) | CI exits 0; audit doc signed off |
| 3.1.7 | All session-sensitive strings (ticket, AuthKey, password) zeroize on drop | `teralaunch/src-tauri/tests/zeroize_audit.rs::zeroize_derives_compose_with_skip_attribute` (representative — pins the derive-plus-skip pattern that `GlobalAuthInfo` / `LaunchParams` use; plus 3 companion tests in the same file) | tests pass |
| 3.1.8 | Launcher anti-reverse hardening applied: LTO + strip + CFG + stack-canary + string obfuscation on sensitive strings | `docs/PRD/audits/security/anti-reverse.md` | audit doc signed off with build-output inspection |
| 3.1.9 | Tauri updater refuses downgrade (replay-attack guard) | `teralaunch/src-tauri/tests/updater_downgrade.rs::refuses_older_latest_json` | test passes |
| 3.1.10 | TCC / Shinra release binaries stripped of debug symbols; IL-obfuscated where feasible | `docs/PRD/audits/security/tcc-shinra-binary-hardening.md` | audit doc signed off |
| 3.1.11 | Self-integrity check at launcher startup (detects modified exe) | `teralaunch/src-tauri/tests/self_integrity.rs::detects_tampered_exe` | test passes |
| 3.1.12 | CSP in `tauri.conf.json` has no `unsafe-inline` for scripts | `teralaunch/src-tauri/tests/csp_audit.rs::csp_denies_inline_scripts` | test passes |
| 3.1.13 | Portal API migrated to HTTPS before Classic+ public launch | `teralib/src/config/config.json` + `docs/PRD/audits/security/portal-https-migration.md` | audit doc signed off; config URL starts with `https://` |
| 3.1.14 | Deploy pipeline never touches outside `/classicplus/` on kasserver | `.github/workflows/deploy.yml` grep-based gate + `tests/deploy_scope.spec.js` | CI fails if any upload URL outside `/classicplus/` |

### 3.2 Reliability (priority 2) — "No failure mode ever leaves the launcher in an unusable state; every error surfaces a clear cause and a retry/recovery path."

| # | Criterion | Measurement path | Threshold |
|---|-----------|------------------|-----------|
| 3.2.1 | 24 edge cases X1–X24 each have a named test that asserts the user sees a clear message + retry/recovery action | `teralaunch/tests/e2e/mod-*.spec.js` + `teralaunch/src-tauri/tests/mod_*.rs` | 24/24 tests passing |
| 3.2.2 | Launcher crashes mid-install recoverable on next boot | `teralaunch/src-tauri/src/services/mods/registry.rs::tests::mid_install_sigkill_recovers_to_error` (behavioural, end-to-end) + `teralaunch/src-tauri/src/services/mods/registry.rs::tests::recover_stuck_installs_flips_installing_to_error` (direct recovery-predicate pin) + `teralaunch/src-tauri/tests/crash_recovery.rs` (JSON contract + filesystem retry invariants) | tests pass |
| 3.2.3 | `.clean` backup is never overwritten by a subsequent install | `teralaunch/src-tauri/src/services/mods/gpk.rs::tests::clean_backup_not_overwritten_on_second_install` (backup stability contract) + `teralaunch/src-tauri/src/services/mods/gpk.rs::tests::golden_cipher_encrypt_zeros_16` (iter-92 §5.4 mapper-cipher byte-for-byte pin — encrypt/decrypt that transforms the backup must stay stable across refactors) | tests pass |
| 3.2.4 | Uninstall everything → mapper byte-for-byte equals `.clean` | `teralaunch/src-tauri/src/services/mods/gpk.rs::tests::uninstall_all_restores_vanilla_bytes` | test passes (SHA-256 equal) |
| 3.2.5 | Offline catalog shows Retry UI; Installed tab works from registry | `teralaunch/tests/e2e/mod-catalog-resilience.spec.js::offline_shows_retry` | test passes |
| 3.2.6 | Catalog parse error caught; filtered entries logged | `teralaunch/src-tauri/src/services/mods/catalog.rs::tests::malformed_entries_filtered` + companion tests (malformed_envelope_is_hard_error, every_entry_malformed_returns_empty_catalog, empty_mods_array_yields_empty_catalog) | tests pass |
| 3.2.7 | Parallel install of same id serialised (no double-write race) | `teralaunch/src-tauri/tests/parallel_install.rs::same_id_serialised` (integration) + `teralaunch/src-tauri/src/services/mods/registry.rs::tests::same_id_serialised_second_claim_refused` (direct atomic-claim predicate pin) | tests pass |
| 3.2.8 | Disk full mid-install reverses partial writes | `teralaunch/src-tauri/tests/disk_full.rs::revert_on_enospc` | test passes |
| 3.2.9 | `.clean` deleted manually: first deploy recreates if no GPK is currently patched; otherwise refuses with recovery instructions | `teralaunch/src-tauri/src/services/mods/gpk.rs::tests::clean_recovery_logic_creates_backup_from_vanilla_current` (recreate path) + `teralaunch/src-tauri/src/services/mods/gpk.rs::tests::clean_recovery_logic_refuses_when_current_is_modded` (refuse path); plus 2 companion tests (nop-when-backup-exists, errors-when-mapper-missing) in the same file; plus `teralaunch/src-tauri/tests/clean_recovery.rs::recover_clean_mapper_is_a_tauri_command_and_delegates_to_gpk` (Tauri-command wiring guard) | tests pass |
| 3.2.10 | Corrupt GPK (magic mismatch) rejected cleanly | `teralaunch/src-tauri/src/services/mods/gpk.rs::tests::parse_mod_file_rejects_non_tmm_gpks` (9-fixture adversarial corpus) + `teralaunch/src-tauri/src/services/mods/gpk.rs::tests::golden_v1_fixture_parses_to_expected_modfile` (iter-89 §5.4 parser golden pin — byte-level current-state capture) + `teralaunch/src-tauri/tests/bogus_gpk_footer.rs::parse_mod_file_retains_magic_check_fallback` (iter-79 structural guard — magic-check branch must remain) | tests pass |
| 3.2.11 | Multi-client attach-once: 2nd `TERA.exe` launch doesn't spawn 2nd Shinra/TCC | `teralaunch/src-tauri/tests/multi_client.rs::second_client_no_duplicate_spawn` | test passes |
| 3.2.12 | Closing client #1 while #2 is running keeps Shinra/TCC alive | `teralaunch/src-tauri/tests/multi_client.rs::partial_close_keeps_overlays` | test passes |
| 3.2.13 | Last client closes → Shinra/TCC terminated | `teralaunch/src-tauri/tests/multi_client.rs::last_close_terminates_overlays` | test passes |

### 3.3 Functionality (priority 3) — "Every documented flow — install, update, uninstall, enable, disable, import-from-file, multi-client auto-launch-once — works end-to-end for both external apps (Shinra, TCC) and GPK mods, and every mod advertised in the catalog actually renders correctly in-game on Classic+ v100.02."

| # | Criterion | Measurement path | Threshold |
|---|-----------|------------------|-----------|
| 3.3.1 | Every catalog id: install → enable → game-launch-spawn → game-exit-cleanup → uninstall → mapper-restored exits 0 | `teralaunch/src-tauri/tests/every_catalog_entry_lifecycle.rs` | 101/101 entries green |
| 3.3.2 | Per-object GPK merge: two mods patching same composite but different objects both apply | `teralaunch/src-tauri/src/services/mods/gpk.rs::tests::per_object_merge_both_apply` (behavioural) + `teralaunch/src-tauri/src/services/mods/gpk.rs::tests::golden_merger_commutes_on_disjoint_slots` (iter-93 §5.4 merger golden — 2-mod commutativity) + `teralaunch/src-tauri/src/services/mods/gpk.rs::tests::golden_merger_three_disjoint_mods_all_orders_agree` (6-permutation convergence — catches path-dependence that could hide at n=2) | tests pass |
| 3.3.3 | Same (composite, object) conflict surfaces UI warning and last-installed wins | `teralaunch/src-tauri/src/services/mods/gpk.rs::tests::detect_conflicts_flags_other_mod_owning_slot` (Rust predicate — "detect" half) + `teralaunch/src-tauri/src/services/mods/gpk.rs::tests::golden_merger_last_install_wins_on_overlap` (iter-93 — "last-installed wins" half) + `teralaunch/src-tauri/tests/conflict_modal.rs::preview_mod_install_conflicts_is_a_tauri_command_and_delegates_to_gpk` (Tauri-command wiring guard) + `teralaunch/tests/e2e/mod-conflict-warning.spec.js::conflict_warning_surfaced` (IPC contract for UI warning) | tests pass |
| 3.3.4 | "Add mod from file…" accepts a local GPK, parses, verifies, deploys | `teralaunch/tests/e2e/mod-import-file.spec.js::user_imported_gpk_deploys` (Playwright IPC flow) + `teralaunch/src-tauri/tests/add_mod_from_file_wiring.rs` (5-wire Rust-side source-inspection guard) | tests pass |
| 3.3.5 | TCC verified in-game on Classic+ live server: overlay renders, class window populates, cooldowns tick | `docs/PRD/audits/functionality/tcc-ingame-verified.md` | audit doc signed off with 3 class screenshots (Warrior, Sorcerer, Priest) |
| 3.3.6 | Shinra verified in-game on Classic+ live server: DPS ticks, encounter log exports | `docs/PRD/audits/functionality/shinra-ingame-verified.md` | audit doc signed off with DPS + export sample |
| 3.3.7 | TCC Discord webhook integration restored (BAM alerts, raid notifications, user-configured URL) | `docs/PRD/audits/functionality/tcc-discord-webhooks.md` + `TCC.Core/ViewModels/SettingsWindowViewModel.cs::tests::discord_webhook_settings_roundtrip` | audit + test pass |
| 3.3.8 | TCC strip-audit: every user-facing feature killed by the strip pass that has Classic+ value is either re-enabled or formally documented as out-of-scope | `docs/PRD/audits/functionality/tcc-strip-audit.md` | audit doc signed off; each feature marked RESTORED / OUT-OF-SCOPE / DEFERRED with justification |
| 3.3.9 | TCC non-default race/gender/class combinations from `elinu` render correctly | `docs/PRD/audits/functionality/tcc-elinu-classes.md` + TCC screenshots of non-default combos | audit + screenshots signed off |
| 3.3.10 | Shinra non-default race/gender/class combinations from `elinu` track correctly | `docs/PRD/audits/functionality/shinra-elinu-classes.md` | audit signed off |
| 3.3.11 | Catalog expansion beyond GitHub: Tumblr / MEGA / Mediafire / VK / Discord archives scraped; every viable mod added to catalog | `docs/PRD/audits/functionality/catalog-expansion-sweep.md` | audit doc signed off with sources exhausted list |
| 3.3.12 | Fresh install defaults: `enabled=true`, `auto_launch=true`, `status=Enabled` | `teralaunch/src-tauri/src/commands/mods.rs::tests::fresh_install_defaults_enabled` | test passes |
| 3.3.13 | Update detection: catalog version drift flips row to `update_available`; banner appears on launcher boot | `teralaunch/tests/e2e/mod-update-flow.spec.js::version_drift_shows_update` | test passes |
| 3.3.14 | TCC class layouts render correctly for all 13 classes on Classic+ (no empty apex tiles, no missing awakening) | `docs/PRD/audits/functionality/tcc-class-layouts-verified.md` | audit signed off with 13 class screenshots |
| 3.3.15 | Enable toggle is pure intent (no spawn, no kill); game-launch spawns; game-exit stops | `teralaunch/src-tauri/src/commands/mods.rs::tests::toggle_intent_only` (enable direction) + `teralaunch/src-tauri/src/commands/mods.rs::tests::toggle_disable_intent_only` (disable direction) + `teralaunch/src-tauri/src/commands/mods.rs::tests::toggle_command_bodies_do_not_spawn_or_kill` (source-inspection structural guard) + multi-client tests above | tests pass |

### 3.4 UX (priority 4) — "A non-technical player can open the launcher, install Shinra, install two GPK mods, click Launch, and see everything applied in-game — with zero jargon, zero required retries, and a Windows-style visual language that matches the rest of the launcher."

| # | Criterion | Measurement path | Threshold |
|---|-----------|------------------|-----------|
| 3.4.1 | Time-to-first-mod (fresh user → Shinra installed → game launching) ≤ 60 s on 10 Mbit/s | `teralaunch/tests/e2e/time-to-first-mod.spec.js::fresh_user_under_60s` | p95 ≤ 60 s across 10 runs |
| 3.4.2 | Mods modal close is a top-right × in titlebar, red hover, Esc closes, backdrop-click closes | `teralaunch/tests/e2e/mod-modal-chrome.spec.js` (3 sub-tests) | tests pass |
| 3.4.3 | Focus trap inside modal | `teralaunch/tests/e2e/mod-accessibility.spec.js::focus_trapped` | test passes |
| 3.4.4 | Download tray updates surgically (DOM patch only, no re-render) | `teralaunch/tests/mods-dom-perf.test.js::tray_surgical_update` | DOM mutation count ≤ 3 per progress tick |
| 3.4.5 | Toggle thumb animates 180 ms cubic-bezier | `teralaunch/tests/e2e/mod-toggle-animation.spec.js` | frame-rate ≥ 60 fps during animation |
| 3.4.6 | Scrollbar matches launcher palette (cyan gradient) | `teralaunch/tests/e2e/__screenshots__/mod-modal-scrollbar.png` baseline | visual diff ≤ 0.1 % |
| 3.4.7 | No jargon leaks to user copy (term blocklist: "composite", "mapper", "SHA", "TMM") | `teralaunch/tests/i18n-jargon.test.js::no_jargon_in_translations` | test passes |
| 3.4.8 | Broken-mod recovery UX: every download/deploy failure shows a Retry action with a human-readable reason | `teralaunch/tests/e2e/mod-error-recovery.spec.js` (4 sub-tests per failure mode) | tests pass |
| 3.4.9 | Overflow menu (⋯) opens and closes on outside-click | `teralaunch/tests/e2e/mod-overflow-menu.spec.js` | test passes |

### 3.5 Accessibility (priority 5) — "The entire mod manager is fully navigable by keyboard alone, every interactive element has an accessible name, all text meets 4.5:1 contrast, and every animation respects `prefers-reduced-motion`."

| # | Criterion | Measurement path | Threshold |
|---|-----------|------------------|-----------|
| 3.5.1 | Keyboard-only happy path: install → enable → uninstall via Tab/Enter/Esc only | `teralaunch/tests/e2e/mod-keyboard-only.spec.js::full_flow_keyboard_only` | test passes |
| 3.5.2 | axe-core scan: 0 serious violations on Browse, Installed, Detail, Banner, Confirm | `teralaunch/tests/e2e/mod-axe-scan.spec.js` | 5 scans, each 0 serious |
| 3.5.3 | Contrast ≥ 4.5:1 on all text in the mods UI | axe-core contrast module (part of 3.5.2) | 0 contrast violations |
| 3.5.4 | All interactive elements have accessible names | axe-core name module (part of 3.5.2) | 0 name violations |
| 3.5.5 | `prefers-reduced-motion` honoured by banner, toggle thumb, progress bar, modal open | `teralaunch/tests/e2e/mod-reduced-motion.spec.js` | 4 sub-tests pass |
| 3.5.6 | Tab order: tabs → toolbar → category chips → first row → tray — follows DOM order | `teralaunch/tests/e2e/mod-tab-order.spec.js` | test passes |

### 3.6 Performance (priority 6) — "The mods modal opens in under 150 ms on a cold cache; downloads complete at the connection's full available throughput without client-side throttling, retries, or spurious failures; progress bars update continuously without stutter; and search on a 300-entry catalog responds in one paint frame."

| # | Criterion | Measurement path | Threshold |
|---|-----------|------------------|-----------|
| 3.6.1 | Mods modal open → first paint, p95 ≤ 150 ms, cold cache, 20 runs | `teralaunch/tests/e2e/mod-modal-perf.spec.js::cold_open_under_150ms` | p95 ≤ 150 ms |
| 3.6.2 | Download throughput ≥ 90 % of raw `curl` baseline on same URL | `teralaunch/src-tauri/tests/download_throughput.rs::matches_curl_baseline` | achieved ≥ 0.9 × baseline |
| 3.6.3 | Progress events emit ≥ 10/s on 10 Mbit/s simulated link | `teralaunch/src-tauri/tests/progress_rate.rs::at_least_10hz` | events/s ≥ 10 |
| 3.6.4 | Search on 300 entries responds in ≤ 16 ms (one paint frame) | `teralaunch/tests/search-perf.test.js::under_one_frame` | test passes |
| 3.6.5 | Scroll inside panes stays 60 fps (no long tasks > 50 ms) | `teralaunch/tests/e2e/mod-scroll-perf.spec.js` via Playwright tracing | 0 long tasks > 50 ms |
| 3.6.6 | Launcher bundle size growth ≤ 5 % per release (vs previous tag) | `.github/workflows/deploy.yml` size-diff gate | CI fails if growth > 5 % |

### 3.7 i18n (priority 7) — "Every user-visible string in the mods feature is translated in EN / FR / DE / RU, no hard-coded English leaks through, and language switching re-renders the modal without a full reload."

| # | Criterion | Measurement path | Threshold |
|---|-----------|------------------|-----------|
| 3.7.1 | Key parity: `keys(EN) == keys(FR) == keys(DE) == keys(RU)` | `teralaunch/tests/i18n-parity.test.js::keys_equal_across_locales` | test passes |
| 3.7.2 | No raw `MODS_*` keys ever appear in DOM | `teralaunch/tests/e2e/mod-i18n.spec.js::no_raw_key_leaks` (4 locales) | 4 sub-tests pass |
| 3.7.3 | Language switch re-renders modal in-place (no full reload) | `teralaunch/tests/e2e/mod-language-switch.spec.js` | test passes |
| 3.7.4 | No hard-coded English in `teralaunch/src/mods.js`, `mods.html`, `app.js` mod paths | `teralaunch/tests/i18n-no-hardcoded.test.js::no new hardcoded English outside the allowlist` (grep-based; strict-zero enforced since iter 77 burn-down) | 0 leaks outside allowlist |

### 3.8 Documentation (priority 8) — "A new contributor can read `CLAUDE.md` + `docs/mod-manager/` and ship a catalog change or a launcher feature without having to read source; a non-technical end user can read `docs/mod-manager/TROUBLESHOOT.md` and resolve the 10 most common failures unaided."

| # | Criterion | Measurement path | Threshold |
|---|-----------|------------------|-----------|
| 3.8.1 | `CLAUDE.md` has a Mod Manager section covering feature state + build + deploy | `CLAUDE.md` grep for `## Mod Manager` | section exists, ≥ 30 lines |
| 3.8.2 | Every `teralaunch/src-tauri/src/services/mods/*.rs` has crate-level `//!` comment | CI script grepping for `//!` in each file | 100 % coverage |
| 3.8.3 | `docs/mod-manager/TROUBLESHOOT.md` covers every user-facing error template | CI script matching `.map_err(|e\| format!(...))` templates to doc headings | 100 % coverage |
| 3.8.4 | `docs/mod-manager/ARCHITECTURE.md` one page per subsystem | file exists with ≥ 1 section per subsystem (mods.rs, tmm.rs, catalog.rs, external_app.rs, registry.rs, mods_state.rs, mods.js) | file exists |
| 3.8.5 | Per-release player-facing `docs/CHANGELOG.md` in plain English (no conventional-commit prefixes) | grep for `^feat\|^fix\|^chore` in CHANGELOG entries | 0 matches |
| 3.8.6 | `external-mod-catalog/README.md` schema matches actual JSON schema 1:1 | CI check: JSON-Schema generated from a fixture entry equals documented schema | equal |
| 3.8.7 | Per-unit audit doc exists for every entry in the catalog + every External app + every launcher module | `docs/PRD/audits/units/*.md` — see §5.5 | count ≥ 101 + 2 + 7 = 110 |
| 3.8.8 | `docs/PRD/lessons-learned.md` exists, capped 200 lines, archived when full | retrospective iteration asserts line count + archive presence | ≤ 200 lines |

## 4. Pillar priority order

1. Security
2. Reliability
3. Functionality
4. UX
5. Accessibility
6. Performance
7. i18n
8. Documentation

Used when two items are equal priority — pillar higher in this list wins.

## 5. Testing regime

### 5.1 Existing tests (from Phase 2 inventory)

**Launcher (`teralaunch/`):**
- Rust: 628 `#[test]` across 31 files in `src-tauri/src/`, 28 in `teralib/src/`.
- JS: 7 Vitest files, ~5,106 lines (`app`, `classicplus-guards`, `coverage-utils`, `forumLinks`, `router`, `utils`) + 1 Playwright file (`launcher.spec.js`, 866 lines).
- Gaps: no `teralaunch/src-tauri/tests/` integration dir; 1 monolithic Playwright file.

**TCC:**
- Zero test methods in the fork's main projects. Only Dragablz submodule has tests.
- Gap: no test harness at all.

**ShinraMeter:**
- Zero test projects.
- Gap: no test harness at all.

**Catalog:**
- Zero CI, zero validation tests.
- Gap: no JSON-Schema, no reachability gate, no size-sanity gate.

### 5.2 New tests required

Every new test must include path, scope, acceptance. Grouped by pillar:

**Security (14 tests):**
- `teralaunch/src-tauri/src/services/mods/external_app.rs::tests::sha_mismatch_aborts_before_write_gpk` — criterion 3.1.2; accepts when SHA mismatch returns `Err` with 0 bytes written. (Inline: bin-crate tests can't import `download_file`, so behavioural pin lives beside the fn.)
- `teralaunch/src-tauri/src/services/mods/gpk.rs::tests::deploy_path_clamped_inside_game_root` — criterion 3.1.4; accepts with ≥ 5 `..`-based attack vectors blocked. (Inline: traversal predicate is `is_safe_gpk_container_filename`, guarded at the call site in `install_gpk`.)
- `teralaunch/src-tauri/tests/http_allowlist.rs` — criterion 3.1.5; accepts when every URL in mods code matches `tauri.conf.json` allowlist.
- `.github/workflows/secret-scan.yml` + `docs/PRD/audits/security/secret-leak-scan.md` — criterion 3.1.6; accepts when trufflehog + git-secrets exit 0 on all 5 repos.
- `teralaunch/src-tauri/tests/zeroize_audit.rs` — criterion 3.1.7; accepts when every session-sensitive struct member uses `Zeroizing<String>` or `#[zeroize(drop)]`.
- `docs/PRD/audits/security/anti-reverse.md` — criterion 3.1.8; accepts with CFG, stack-canary, string-obfuscation coverage audit.
- `teralaunch/src-tauri/tests/updater_downgrade.rs` — criterion 3.1.9; accepts when older signed `latest.json` is rejected.
- `docs/PRD/audits/security/tcc-shinra-binary-hardening.md` — criterion 3.1.10; accepts with symbol-strip + IL-obfuscation evidence.
- `teralaunch/src-tauri/tests/self_integrity.rs` — criterion 3.1.11; accepts when tampered exe is detected at startup.
- `teralaunch/src-tauri/tests/csp_audit.rs` — criterion 3.1.12; accepts when CSP has no `unsafe-inline` for `script-src`.
- `docs/PRD/audits/security/portal-https-migration.md` — criterion 3.1.13; accepts when `config.json` API URL is HTTPS and matches production.
- `tests/deploy_scope.spec.js` + `.github/workflows/deploy.yml` gate — criterion 3.1.14; accepts when any upload URL outside `/classicplus/` fails the job.

**Reliability (13 tests):**
- See §3.2 table — each criterion maps to a specific test path above.
- 12 Playwright `mod-*.spec.js` files mapped to edge cases X1–X24 per `test-plan.md` (to be rewritten in `docs/PRD/test-plan.md`).

**Functionality (15 tests + 8 audit docs):**
- See §3.3 table.

**UX (9 tests + visual baselines):**
- See §3.4 table.

**Accessibility (6 tests):**
- See §3.5 table.

**Performance (6 tests):**
- See §3.6 table.

**i18n (4 tests):**
- See §3.7 table.

**Documentation (8 gates):**
- See §3.8 table.

### 5.3 Adversarial / attack / fuzzing tests

Security pillar requires adversarial coverage. Each attack: expected outcome.

| Attack / vector | Expected outcome |
|-----------------|------------------|
| Zip-slip path (`../../evil`) | `extract_zip` returns `Err`; no file on disk; log entry |
| GPK deploy `..` escape | `install_gpk` returns `Err`; no mapper mutation; `.clean` untouched |
| Tampered catalog entry (wrong SHA) | Install returns `Err`; 0 bytes touch dest; registry row `Error` |
| HTTP redirect to non-allowlisted host | reqwest rejects; log entry |
| Replay-attack `latest.json` (older version) | updater refuses; logs warning |
| Tampered launcher exe | Self-integrity check fails at startup; user prompted to reinstall |
| Crafted GPK with bogus footer magic | `parse_mod_file` returns `Err`; source deleted |
| Two mods claim same (composite, object) | Conflict warning UI; last-installed wins; audit log entry |
| SIGKILL mid-download | Registry row recoverable to `Error` on boot; partial file removed |
| Disk full during install | Partial writes reversed; `Error` status; no mapper mutation |

### 5.4 Test-pinning for legacy refactor

Before any refactor touching:
- `teralaunch/src-tauri/src/services/mods/gpk.rs` (cipher / parser / merger)
- `teralaunch/src-tauri/src/services/mods/external_app.rs` (download + extract)
- `TCC/TeraPacketParser/Sniffing/ClassicPlusSniffer.cs`
- `ShinraMeter/DamageMeter.Sniffing/TeraSniffer.cs`

Author golden-file or property-based tests that capture current behaviour byte-for-byte. Refactor PR must pass pinned tests unchanged.

### 5.5 Per-unit audit artefacts

For each discrete unit in scope: produce `docs/PRD/audits/units/<unit-slug>.md` with the skill-standard header (category, status, license, obfuscated, source provenance, public surface, settings, risks, tests, verification plan).

**Units in scope:**
- 99 catalog GPK entries → 99 audit docs under `docs/PRD/audits/units/gpk/<id>.md`.
- 2 External apps (Shinra, TCC) → `docs/PRD/audits/units/external/<id>.md`.
- 9 launcher modules:
  - `commands/mods.rs`
  - `services/mods/catalog.rs`
  - `services/mods/external_app.rs`
  - `services/mods/registry.rs`
  - `services/mods/tmm.rs`
  - `services/mods/mods_state.rs`
  - `services/mods/types.rs`
  - `teralaunch/src/mods.js`
  - `teralaunch/src/mods.html` + `mods.css`
- 13 TCC per-class layouts → `docs/PRD/audits/units/tcc/<class>.md`.

**Total: 123 audit docs.** The loop batches these with `sadd:subagent-driven-development`.

## 6. Per-unit workflow

1. Read the unit's audit doc (create if absent using §5.5 header).
2. `code-review:review-local-changes` against the unit's file(s).
3. Pillar-specific checks:
   - **Security**: SHA verification path, input sanitisation, path confinement.
   - **Reliability**: error-recovery UX, retry action, log entry on failure.
   - **Functionality**: the unit's public API covered by a test.
   - **UX**: user-facing strings free of jargon; i18n keys present.
   - **Accessibility**: interactive elements named + keyboard-reachable.
   - **Performance**: no synchronous long tasks > 50 ms in hot paths.
4. Test-pinning if refactoring (§5.4).
5. Create / update per-unit test if missing.
6. Commit with conventional message referencing the criterion id (e.g. `feat(mods): 3.3.4 wire Add-mod-from-file GPK deploy`).

## 7. Adversarial audit architecture

- **Main coder**: loop's primary agent implementing work.
- **Validators**:
  - `code-review:review-local-changes` (6 parallel reviewers) pre-commit.
  - `reflexion:critique` (3 judges) on contested designs.
  - REVALIDATION iteration re-runs all recent proofs.
- **Static analyzers** (all must exit clean):
  - Rust: `cargo clippy --all-targets --release -- -D warnings`
  - C#: `dotnet build -c Release -warnaserror`
  - JS: project lint (there isn't one yet — add ESLint or Biome)
- **Spec-as-test**: every §3 criterion points at a test file; the test's pass/fail IS the criterion state.
- **Silence is not success**: after running a command, read stderr + check exit code + confirm expected artefacts exist.

## 8. Work queue protocol

All active work lives in `docs/PRD/fix-plan.md`. Priority: `[P0] > [P1] > [P2]`. Ties broken by pillar priority (§4).

### 8.1 `[DONE]` strict rules

Moves to `[DONE]` only when ALL of:
1. Automated test / benchmark / signed-off audit proves the criterion.
2. Proof is committed (SHA in entry).
3. Proof still passes on current `HEAD`.

Format: `[DONE] <criterion-id> <title> — commit <sha>, proof: <test path>, verified @ iter <N>`. Stamp updated each REVALIDATION.

### 8.2 `[BLOCKED]` last resort

An item may ONLY become `[BLOCKED]` after ALL of:
- 3 independent attempts via different approaches
- `reflexion:critique` on the stuck item (3 judges)
- `sdd:brainstorm` for alternatives
- Research spawn (`Explore` / `general-purpose` / `deep-research`)
- At least one workaround attempt delivering ≥ 80 % value differently

Entry MUST include: what was tried, why each failed, specific human input needed.

Re-tried every 50 iterations. Immediately re-tried when a related item becomes DONE.

### 8.3 Revalidation (every 20 iterations)

Re-runs proofs of recent DONE items. Regression → `[P0] REGRESSED`. Full test suite. §3 criteria re-checked.

### 8.4 Retrospective (every 30 iterations)

Apply `kaizen:plan-do-check-act`. Last 30 commits. Patterns. `reflexion:memorize` → `docs/PRD/lessons-learned.md`. PRD changes land as `[META]`.

### 8.5 Research sweep (every 10 iterations)

Check for: library updates, security advisories, upstream TCC/Shinra changes, new GPK mods surfacing on Tumblr / MEGA / Discord archives.

## 9. CEK skill invocation

| Work type | Skill chain |
|-----------|-------------|
| Architecture change | `sdd:brainstorm` → `sdd:add-task` → `sdd:plan` → `sdd:implement` |
| Bug fix | `tdd:test-driven-development` (RED first, always) |
| Per-unit audit (parallel) | `sadd:subagent-driven-development` |
| Contested design | `reflexion:critique` |
| Recurring bug | `kaizen:why` / `kaizen:cause-and-effect` / `kaizen:analyse-problem` |
| Pre-commit | `code-review:review-local-changes` |
| Record decision | `reflexion:memorize` |
| GPK scout beyond GitHub | `deep-research` |

Always apply `kaizen:kaizen` and `ddd:software-architecture` continuously.

## 10. Context hygiene

- Main conversation stays lean. Heavy investigation / per-unit audit → spawn subagent (`Explore`, `general-purpose`, `sadd:*`).
- Scratchpads → `.loop-scratchpad/` files.
- `/compact` every ~20k tokens of accumulated output.

## 11. Exit criteria

Emit `MOD-MANAGER-PERFECTION-COMPLETE` ONLY when ALL of:

1. **Launcher**: `cd teralaunch/src-tauri && cargo clippy --all-targets --release -- -D warnings` exits 0.
2. **Launcher**: `cd teralaunch/src-tauri && cargo test --release` exits 0.
3. **Launcher**: `cd teralaunch && npm test` exits 0.
4. **Launcher**: `cd teralaunch && npm run test:e2e` exits 0.
5. **TCC**: `dotnet build TCC.sln -c Release -warnaserror` exits 0.
6. **TCC**: `dotnet test TCC.sln -c Release` exits 0 (after a test project is authored).
7. **Shinra**: `dotnet build ShinraMeter.sln -c Release -warnaserror` exits 0.
8. **Shinra**: `dotnet test ShinraMeter.sln -c Release` exits 0 (after a test project is authored).
9. **Catalog**: JSON validity + schema + reachability CI exits 0 (to-be-authored workflow).
10. **Secret scan**: `trufflehog` + `git-secrets` across all 5 repos exits 0.
11. `docs/PRD/fix-plan.md` has zero `[P0]`, `[P1]`, `[P2]` items. Only `[DONE]`, `[BLOCKED]`, `[META]` allowed. Every `[BLOCKED]` has the strict triplet.
12. Every `[DONE]` has `verified @ iter N` where N > (current_iter − 40).
13. Last 2 REVALIDATION iterations passed clean.
14. Every §3 criterion maps to a passing test or signed-off audit.
15. Adversarial corpus (§5.3) runs clean.
16. 123 per-unit audit docs exist (§5.5).
17. Launcher + TCC + Shinra all produce signed releases end-to-end (`gh workflow run deploy.yml` for launcher, tag-based for TCC/Shinra).
18. All 5 repos `git status` clean.
19. `docs/CHANGELOG.md` + `docs/mod-manager/TROUBLESHOOT.md` + release-package artefacts produced for human review (hand-off per Q28).
20. `reflexion:critique` on the cumulative diff since the first iteration returns consensus "no further improvements warranted" across Requirements + Architecture + Code Quality judges.

Anything short = loop keeps going.

## 12. Safety valves

- **Max iterations:** 1000. Hard cap; at iter 1000 without sentinel, emit `docs/PRD/status-report.md` and halt.
- **Destructive freeze** (from Q21):
  - No `git reset --hard` with uncommitted user work.
  - No `git push --force` except the secret-leak remediation path.
  - No `rm -rf` outside `<app_data>/mods/*`, build outputs (`target/`, `release/`, `node_modules/`, `bin/`, `obj/`), explicit temp fixtures.
  - No `--no-verify`, no `--no-gpg-sign`, no editing `.git/config`.
  - No `git filter-repo` / `bfg` except confirmed secret-leak remediation (DRR required).
  - No dropping user-data dirs (`registry.json`, `.clean`, user config).
  - No disabling SHA-256 verification.
  - No CDN change from kasserver `/classicplus/`.
  - No mid-loop PRD edit. Only fix-plan mutates. PRD changes land as `[META]`.
- **Push policy:** push allowed to all 5 repos (launcher is private; TCC/Shinra are public forks under TERA-Europe-Classic org).
- **Release policy:** `gh workflow run deploy.yml` authorised; tag-based releases for TCC / Shinra authorised.
- **No amending commits.** New commits only.
- **Gated actions:** opening GitHub issues / PRs / comments, public Discord posts. Loop asks before firing.
- **Recovery:** fix-plan.md is source of truth. Re-invoke `/loop` with same prompt to resume.

## 13. Glossary

- **Classic+ (CP).** The TERA Europe private server running TERA v100.02.
- **v100.02.** TERA client version; internal `ReleaseVersion = 10002`. Separate from "EU Classic" (v35-era).
- **EUC.** Region key family (EU-Classic key schedule). Classic+ reuses it for session decryption — not the same as being a Classic server.
- **Noctenium.** The DLL injected into `TERA.exe` providing the mirror socket at `127.0.0.1:7803`.
- **Mirror socket.** The launcher-provided local TCP endpoint that re-emits decrypted game packets for overlays.
- **GPK.** Unreal Engine 3 package file (TERA's asset format). Client-side mods override specific composite entries.
- **CompositePackageMapper.dat.** TERA's encrypted mapper indexing composite name → (object, package, offset, size) entries. Patched by TMM-style mods.
- **TMM.** [VenoMKO/TMM](https://github.com/VenoMKO/TMM) — open-source TERA Mod Manager. Our deployer ports its 3-pass cipher + mapper format.
- **`.clean` backup.** Snapshot of the vanilla `CompositePackageMapper.dat` stored on first patch. Restored on full uninstall.
- **TCC.** Tera-Custom-Cooldowns — user-facing cooldown / group / chat overlay. Our Classic+ fork is read-only against the server.
- **Shinra (Meter).** DPS meter. Our Classic+ fork pins `ReleaseVersion = 10002`.
- **Catalog.** `TERA-Europe-Classic/external-mod-catalog/catalog.json` — curated list of Classic+-compatible mods.
- **P0/P1/P2.** Fix-plan priority slots (blocker, major quality, polish).
- **DONE / BLOCKED / REGRESSED / META.** Fix-plan lifecycle states.
- **Ralph loop.** Huntley's "one item per loop, tests as backpressure" pattern; borrowed here.
- **Reflexion.** Shinn 2023's verbal-feedback memory pattern; used for retrospectives.
- **kaizen.** Continuous improvement discipline; applied throughout.

## 14. Change control

PRD is append-only during a loop run. Goal changes require stopping the loop and explicit human re-authorisation. Agent proposals for PRD changes land as `[META]` entries in fix-plan — human reviews and acts.
