# Test Plan — Mod Manager Production Readiness

Every acceptance criterion in `acceptance-criteria.md` is implemented by
one or more of the tests below. The ralph loop may not mark a criterion
complete unless at least one named test here is green for it.

## Test layers

| Layer | Runner | Location | Purpose |
|-------|--------|----------|---------|
| Rust unit | `cargo test --release` | `teralaunch/src-tauri/src/**/*` | Pure-function correctness (TMM cipher, parser, hash check). |
| Rust integration | `cargo test --release` | `teralaunch/src-tauri/tests/` | Command-level flows using a tempfile game root + mock reqwest. |
| Frontend unit | Vitest | `teralaunch/tests/*.test.js` | Pure JS: `filterMatches`, `formatCategoryLabel`, render helpers. |
| Frontend DOM | Vitest + jsdom | `teralaunch/tests/*.test.js` | Builds `mods.html` into jsdom; drives the DOM via events. |
| Playwright E2E | `npm run test:e2e` | `teralaunch/tests/e2e/` | End-to-end flows with a stubbed Tauri bridge + mock catalog host. |
| Playwright visual | `npm run test:e2e` | Same | Screenshot comparisons of Browse, Installed, Detail, Banner states. |
| Playwright perf | `npm run test:e2e` | Same | Traces: modal open time, progress event rate, scroll fps. |
| Playwright a11y | `npm run test:e2e` | Same | axe-core scan per page state. |

## A. Browse — tests

| Test | Criterion | Layer |
|------|-----------|-------|
| `catalog_fetch_and_render` | A1 | Playwright |
| `catalog_offline_shows_retry` | A2, M1 | Playwright + stub 503 |
| `filter_chips_narrow_list` | A3 | Vitest DOM |
| `category_chips_populated_from_catalog` | A4 | Vitest DOM |
| `search_matches_name_author_category_description` | A5 | Vitest |
| `installed_count_updates_on_every_render` | A6 | Vitest DOM |
| `browse_count_updates_on_every_render` | A7 | Vitest DOM |
| `installed_mods_absent_from_browse` | A8 | Vitest DOM |
| `no_icon_placeholder_when_url_missing` | A9 | Playwright visual |
| `layout_fits_1280x720_without_scrollbar` | A10 | Playwright visual |

## B/C. Install — tests

| Test | Criterion | Layer |
|------|-----------|-------|
| `install_external_streams_with_progress` | B1, B2 | Rust integration |
| `sha256_mismatch_aborts_before_write` | B3, C1 | Rust unit |
| `executable_verified_post_extract` | B4 | Rust integration |
| `fresh_install_defaults_to_enabled` | B5, C5 | Rust unit |
| `zip_slip_rejected` | B6 | Rust unit |
| `install_gpk_invokes_tmm_deploy` | C3 | Rust integration |
| `install_gpk_deploy_failure_preserves_file` | C4 | Rust integration |

## D. TMM correctness — tests

| Test | Criterion | Layer |
|------|-----------|-------|
| `encrypt_decrypt_roundtrip` | D1 | Rust unit |
| `parse_serialise_roundtrip_real_fixture` | D2 | Rust unit + checked-in fixture |
| `incomplete_paths_equal_matches_fixtures` | D3 | Rust unit |
| `parse_mod_file_accepts_tmm_gpk` | D4 | Rust unit + fixture |
| `parse_mod_file_rejects_vanilla_gpk` | D4 | Rust unit |
| `install_gpk_creates_clean_backup_once` | D5, D6 | Rust integration |
| `uninstall_gpk_restores_vanilla_bytes` | D7 | Rust integration |
| `modlist_tmm_updated_on_install_and_uninstall` | D8 | Rust integration |

## E. Enable / Disable — tests

| Test | Criterion | Layer |
|------|-----------|-------|
| `enable_mod_external_does_not_spawn` | E1 | Rust unit |
| `disable_mod_external_does_not_kill` | E2 | Rust unit |
| `spawn_auto_launch_runs_before_run_game` | E3 | Rust integration |
| `autolaunch_bumps_status_to_running` | E4 | Rust integration |
| `stop_auto_launched_runs_after_game_end` | E5 | Rust integration |
| `stop_auto_launched_ignores_enabled_flag` | E6 | Rust integration |
| `status_after_autostop_matches_toggle` | E7 | Rust integration |
| `toggle_click_does_not_preventdefault` | E8 | Vitest DOM |
| `toggle_thumb_animates_on_change` | E9 | Playwright visual |

## F. Update detection — tests

| Test | Criterion | Layer |
|------|-----------|-------|
| `load_installed_flips_update_available_on_version_drift` | F1, F2 | Vitest |
| `update_button_rendered_when_status_is_update_available` | F3 | Vitest DOM |
| `click_update_reinstalls_catalog_entry` | F4 | Playwright |
| `check_mod_updates_runs_once_on_boot` | F5 | Vitest |
| `banner_title_matches_count` | F6 | Vitest DOM |
| `banner_subtitle_lists_first_three_plus_more` | F7 | Vitest DOM |
| `banner_body_click_opens_modal` | F8 | Playwright |
| `banner_dismiss_hides_until_next_launch` | F9 | Playwright |
| `banner_respects_reduced_motion` | F10 | Playwright |

## G. Uninstall — tests

| Test | Criterion | Layer |
|------|-----------|-------|
| `uninstall_removes_registry_entry` | G1 | Rust unit |
| `uninstall_external_stops_process_first` | G2 | Rust integration |
| `uninstall_external_deletes_folder` | G3 | Rust integration |
| `uninstall_with_delete_settings_true_removes_settings` | G4 | Rust integration |
| `uninstall_gpk_restores_mapper_and_deletes_container` | G5 | Rust integration |
| `uninstall_gpk_deletes_source_file` | G6 | Rust integration |
| `uninstall_gpk_trims_modlist` | G7 | Rust integration |
| `modal_confirm_used_not_window_confirm` | G8 | CI grep: `grep -rn "window\\.confirm"` → 0 matches |
| `uninstall_waits_for_confirm_click` | G9 | Playwright |

## H. Onboarding — tests

| Test | Criterion | Layer |
|------|-----------|-------|
| `first_visit_shows_onboarding_card` | H1 | Playwright |
| `got_it_persists_seen_flag` | H2 | Vitest DOM |
| `open_mods_button_opens_modal` | H3 | Playwright |
| `subsequent_launch_skips_onboarding` | H4 | Playwright |

## I. Download tray — tests

| Test | Criterion | Layer |
|------|-----------|-------|
| `tray_item_appears_on_install_start` | I1 | Playwright |
| `tray_updates_surgically` | I2 | Vitest DOM perf harness |
| `tray_item_fades_on_success` | I3 | Playwright |
| `tray_item_persists_on_error` | I4 | Playwright |

## J. UX polish — tests

| Test | Criterion | Layer |
|------|-----------|-------|
| `modal_close_button_top_right_red_hover` | J1 | Playwright visual |
| `escape_closes_modal` | J2 | Playwright |
| `backdrop_click_closes_modal_except_when_confirm_open` | J3 | Playwright |
| `focus_trapped_inside_modal` | J4 | Playwright a11y |
| `tab_order_sensible` | J5 | Playwright a11y |
| `accessible_names_on_all_controls` | J6 | axe scan |
| `contrast_ratio_45` | J7 | axe scan |
| `scrollbar_matches_palette` | J8 | Playwright visual |

## K. i18n — tests

| Test | Criterion | Layer |
|------|-----------|-------|
| `every_key_present_in_all_locales` | K1 | Vitest: diff JSON keys across locales |
| `no_hard_coded_english_in_mod_code` | K2 | CI grep: runtime string literals in allowed files only |
| `language_switch_rerenders_modal` | K3 | Playwright |

## L. Performance — tests

| Test | Criterion | Layer |
|------|-----------|-------|
| `modal_open_under_150ms` | L1 | Playwright perf |
| `progress_events_10hz_on_10mbps` | L2 | Rust integration + a fake slow reader |
| `search_responds_in_one_frame_100_entries` | L3 | Vitest perf harness |
| `scroll_stays_60fps_no_long_tasks` | L4 | Playwright tracing |
| `bundle_size_within_5pct` | L5 | CI size comparison |

## M. Resilience & security — tests

| Test | Criterion | Layer |
|------|-----------|-------|
| `offline_catalog_shows_retry` | M1 | Playwright + stubbed 503 |
| `catalog_parse_error_caught` | M2 | Vitest |
| `entries_missing_fields_filtered` | M3 | Vitest |
| `http_allowlist_covers_all_urls` | M4 | Rust integration + ad-hoc outbound URL audit test |
| `deploy_path_clamped_inside_game_root` | M5 | Rust unit |
| `extracted_zip_paths_clamped` | M6 | Rust unit |

## N. CI hygiene — gates

| Gate | Criterion |
|------|-----------|
| `cargo clippy --all-targets -- -D warnings` | N1 |
| `cargo test --release` | N2 |
| `npm test` | N3 |
| `npm run test:e2e` | N4 |
| console-noise check in Playwright | N5 |
| deploy.yml end-to-end green | N6 |

## O. Docs — checks

| Check | Criterion |
|-------|-----------|
| CLAUDE.md has a Mod Manager section | O1 |
| Every `services/mods/*.rs` has a `//!` module comment | O2 |
| `external-mod-catalog/README.md` explains schema | O3 |
| `docs/mod-manager/TROUBLESHOOT.md` exists with 10 FAQs | O4 |

## Edge cases — tests (X-series)

All edge cases X1–X24 from `acceptance-criteria.md` have a named test in
this file. Implementation groups them into one test file per concern:

- `tests/e2e/mod-install-happy-path.spec.js` (X11, X22, X23)
- `tests/e2e/mod-install-network-failures.spec.js` (X3, X4, X5)
- `tests/e2e/mod-deploy-game-paths.spec.js` (X1, X2, X7)
- `tests/e2e/mod-uac-spawn.spec.js` (X8, X9, X17)
- `tests/e2e/mod-uninstall-idempotency.spec.js` (X18)
- `tests/e2e/mod-clean-backup-recovery.spec.js` (X13)
- `tests/e2e/mod-corrupt-gpk.spec.js` (X14)
- `tests/e2e/mod-crash-recovery.spec.js` (X19)
- `tests/e2e/mod-confirm-dialog.spec.js` (X15)
- `tests/e2e/mod-catalog-resilience.spec.js` (X6, X16)
- `tests/e2e/mod-parallel-installs.spec.js` (X12)
- `tests/e2e/mod-disk-full.spec.js` (X10)
- `tests/e2e/mod-accessibility.spec.js` (X20, X21)
- `tests/e2e/mod-dup-exe-names.spec.js` (X24)

## Visual regression baselines

Checked-in baselines live at
`teralaunch/tests/e2e/__screenshots__/<browser>/<platform>/...`. First
run captures; subsequent runs diff. Any diff > 0.1 % fails the build.

States captured:

1. Empty Installed + Browse loading skeleton
2. Populated Installed (3 mods)
3. Populated Browse (89 mods, All filter)
4. Browse with category filter active ("ui")
5. Browse with search active ("fog")
6. Detail panel open
7. Detail panel with screenshots
8. Onboarding dialog
9. Download in progress
10. Download tray with two active items
11. Update-available row
12. Launch-time update banner
13. Modal mid-close animation (reduced-motion on and off)
14. Confirm dialog
15. Overflow menu open

## Ralph loop's running test command

Single command that MUST exit 0 at the end of every loop iteration:

```bash
# from teralaunch/
cd src-tauri && cargo clippy --all-targets --release -- -D warnings \
  && cargo test --release \
  && cd .. \
  && npm test \
  && npm run test:e2e
```

If any step fails, the loop resumes on that failure before touching the
next checklist item.
