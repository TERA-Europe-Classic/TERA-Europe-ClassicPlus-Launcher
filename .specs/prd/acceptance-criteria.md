# Acceptance Criteria — Mod Manager Production Readiness

**Oracle** for the ralph loop. Every row is a testable statement backed by
at least one automated test (Vitest, Rust unit, or Playwright E2E). The
ralph loop marks a box `[x]` only when the associated test(s) is both
present in the codebase and green in CI. It does not stop until every
box is checked.

Cross-references: `mod-manager-production.md` (PRD), `test-plan.md`
(test implementations), `ralph-loop-instructions.md` (how to iterate).

---

## A. Browse

- [ ] **A1** Catalog loads from the remote URL and renders all entries.
- [ ] **A2** Catalog failure surfaces the HTTP error string with a Retry
      button; Installed tab still functions.
- [ ] **A3** Filter chips (`all`/`external`/`gpk`) narrow the list; active
      chip highlights.
- [ ] **A4** Category chips populate dynamically from catalog data; "All
      categories" resets.
- [ ] **A5** Search matches across name, author, category, and short
      description; case-insensitive; substring.
- [ ] **A6** Installed count badge reflects `state.installed.length` on
      every render.
- [ ] **A7** Browse count badge reflects `catalog.length - installed.length`
      on every render.
- [ ] **A8** Installed mods never appear in Browse.
- [ ] **A9** Icon cell renders only when `icon_url` is non-empty *and* the
      image loads (no placeholder initials). Verified via Playwright.
- [ ] **A10** Layout at 1280×720 shows no overflow or horizontal scrollbar.

## B. Install — External (zip)

- [ ] **B1** `install_external_mod` streams bytes with a progress callback.
- [ ] **B2** Progress callback throttles to ≥ 10 events/sec wall-clock
      (≈60 ms interval). Never emits on every chunk.
- [ ] **B3** SHA-256 mismatch aborts before any disk write; registry row =
      `Error`.
- [ ] **B4** Extracted executable path is verified post-extraction.
- [ ] **B5** Registry row lands at `enabled=true`, `auto_launch=true`,
      `status=Enabled`.
- [ ] **B6** Zip-slip paths are rejected.

## C. Install — GPK

- [ ] **C1** `install_gpk_mod` streams and verifies SHA-256.
- [ ] **C2** Progress callback throttles to ≥ 10 events/sec wall-clock.
- [ ] **C3** After download, `tmm::install_gpk` is invoked; `last_error`
      captures deploy failures.
- [ ] **C4** Deploy failure leaves the GPK on disk for retry; status is
      `Enabled` with `last_error` set (not `Error`).
- [ ] **C5** Fresh install defaults match B5.

## D. TMM correctness

- [ ] **D1** Encrypt → decrypt is identity (unit test, `tmm.rs`).
- [ ] **D2** Parse → serialise round-trip equals input bytes for a
      real-world mapper fixture (unit test).
- [ ] **D3** `incomplete_paths_equal` matches known prefix/suffix pairs
      from fixture cases (unit test).
- [ ] **D4** `parse_mod_file` accepts a real TMM-produced GPK and rejects
      a vanilla GPK with `Err`.
- [ ] **D5** `install_gpk` creates `.clean` backup on first patch; no-op
      if `.clean` already exists.
- [ ] **D6** `install_gpk` leaves `.clean` untouched during subsequent
      installs.
- [ ] **D7** `uninstall_gpk` restores vanilla entries exactly; a
      byte-for-byte comparison against `.clean` passes for the affected
      composite names.
- [ ] **D8** `ModList.tmm` is maintained: entries added on install,
      removed on uninstall.

## E. Enable / Disable

- [ ] **E1** Enable toggle click does not spawn a process (unit test:
      `enable_mod` for External kind only flips flags).
- [ ] **E2** Disable click does not kill a running process (unit test).
- [ ] **E3** `spawn_auto_launch_external_apps` runs before `run_game` in
      `launch_game_command` (integration test).
- [ ] **E4** Process status flips to `Running` on successful auto-launch.
- [ ] **E5** `stop_auto_launched_external_apps` runs after `game_ended`.
- [ ] **E6** Auto-stop terminates **every** installed External mod's
      process regardless of current enabled flag.
- [ ] **E7** Status after auto-stop reflects the current toggle
      (`Enabled` or `Disabled`), not `Running`.
- [ ] **E8** Checkbox click handler does not call `event.preventDefault()`
      for toggle actions (DOM test).
- [ ] **E9** Toggle thumb animates 180 ms cubic-bezier on state change
      (visual test).

## F. Update detection

- [ ] **F1** `loadInstalled` flips a row to `update_available` when
      `catalog.version !== row.version` and the row is not in a
      transient state.
- [ ] **F2** Row status comparison skips `installing`, `error`, `running`,
      `starting`.
- [ ] **F3** `update_available` rows render an Update primary button in
      place of the toggle.
- [ ] **F4** Click-Update re-invokes `install_mod` with the catalog entry;
      the on-disk version is overwritten; registry row refreshes.
- [ ] **F5** `checkModUpdatesOnLaunch` runs once 1.5 s after home-page
      paint and skips on subsequent calls.
- [ ] **F6** Banner title matches count ("1 mod update available" / "3 mod
      updates available").
- [ ] **F7** Banner subtitle lists the first three names + "+N more" tail
      when count > 3.
- [ ] **F8** Clicking the banner body opens the Mods modal.
- [ ] **F9** Clicking × dismisses the banner; does not re-appear until
      next launch.
- [ ] **F10** Banner respects `prefers-reduced-motion`.

## G. Uninstall

- [ ] **G1** `uninstall_mod` removes the registry entry.
- [ ] **G2** External uninstall stops the running process first; surfaces
      a clear error if the stop fails.
- [ ] **G3** External uninstall deletes the install folder.
- [ ] **G4** External uninstall with `delete_settings=true` removes the
      advertised `settings_folder`.
- [ ] **G5** GPK uninstall restores the vanilla mapper entries and
      deletes the container in `CookedPC`.
- [ ] **G6** GPK uninstall deletes the source `.gpk` from
      `<app_data>/mods/gpk/`.
- [ ] **G7** GPK uninstall trims the `ModList.tmm` entry.
- [ ] **G8** `modalConfirm` gates the destructive action; `window.confirm`
      is never called (code scan test).
- [ ] **G9** Uninstall does not fire until the user clicks Confirm
      (Playwright test).

## H. Onboarding

- [ ] **H1** First visit shows the onboarding card.
- [ ] **H2** "Got it" dismiss persists `mods_onboarding_seen=true`.
- [ ] **H3** "Open mods" button dismisses + opens the modal.
- [ ] **H4** Subsequent launches do not show the card.

## I. Downloads tray

- [ ] **I1** Active download appears in the bottom-right tray.
- [ ] **I2** Tray item updates surgically (width + label only) on
      progress events.
- [ ] **I3** On success, the item fades out after 2 s.
- [ ] **I4** On error, the item persists with a Retry action.

## J. UX polish

- [ ] **J1** Modal × button is top-right, inside the titlebar, red hover.
- [ ] **J2** Esc closes the modal.
- [ ] **J3** Backdrop click closes the modal unless a confirm dialog is
      open.
- [ ] **J4** Focus is trapped inside the modal.
- [ ] **J5** Keyboard Tab order is sensible (tabs → toolbar → category
      chips → first row → tray).
- [ ] **J6** All interactive elements have accessible names (aria-label
      or visible text).
- [ ] **J7** Color contrast ≥ 4.5:1 on all text (axe scan).
- [ ] **J8** Scrollbar matches the launcher palette (cyan gradient).

## K. Internationalisation

- [ ] **K1** Every new key is present in EN, FR, DE, RU entries.
- [ ] **K2** No hard-coded English strings in mods.html / mods.js / app.js
      paths (grep-based CI check).
- [ ] **K3** Language switch re-renders the modal with translated strings
      without a full page reload.

## L. Performance

- [ ] **L1** Modal open → first paint ≤ 150 ms on a cold cache (Playwright
      perf test).
- [ ] **L2** Progress events emit ≥ 10 /s on a 10 Mbit/s simulated link.
- [ ] **L3** Search filter responds within one paint frame on a
      100-entry catalog (Vitest perf harness).
- [ ] **L4** 60 fps scroll inside the Installed / Browse panes
      (Playwright tracing, no long tasks > 50 ms).
- [ ] **L5** Bundle size regression gate: < 5 % growth vs previous
      release.

## M. Resilience & security

- [ ] **M1** Offline catalog surfaces an error + Retry; doesn't crash.
- [ ] **M2** Catalog parse errors are caught and reported; no app-wide
      failure.
- [ ] **M3** Entries missing `sha256` or `download_url` are filtered out
      at load time with a console warning.
- [ ] **M4** HTTP allowlist in `tauri.conf.json` covers every URL the
      mods code can request.
- [ ] **M5** Deploy path is clamped inside the configured `game_root`
      (no `..` escapes).
- [ ] **M6** Extracted zip paths are clamped inside the install root.

## N. Build / CI hygiene

- [ ] **N1** `cargo clippy --all-targets -- -D warnings` exits 0.
- [ ] **N2** `cargo test --release` exits 0.
- [ ] **N3** `npm test` exits 0.
- [ ] **N4** `npm run test:e2e` exits 0 with no visual regressions.
- [ ] **N5** No `console.warn` or `console.error` in a Playwright happy-path
      run.
- [ ] **N6** Deploy workflow (`gh workflow run deploy.yml`) produces a
      signed release end-to-end in a single run.

## O. Documentation

- [ ] **O1** `CLAUDE.md` documents the mods feature at a level new
      contributors can follow.
- [ ] **O2** Each Rust module under `services/mods/` has a crate-level
      rustdoc comment explaining its role.
- [ ] **O3** The catalog schema is documented in
      `external-mod-catalog/README.md` and referenced from this PRD.
- [ ] **O4** Troubleshooting guide lives at `docs/mod-manager/TROUBLESHOOT.md`
      covering the 10 most likely end-user failure modes.

---

## Edge cases (from PRD §8) — must each map to a test

- [ ] **X1** Game root not configured.
- [ ] **X2** `CookedPC` read-only.
- [ ] **X3** SHA-256 mismatch mid-download.
- [ ] **X4** Network drop mid-download.
- [ ] **X5** Catalog host 5xx.
- [ ] **X6** Renamed catalog id keeps old row working.
- [ ] **X7** Game path changed after install.
- [ ] **X8** UAC prompt during external-app spawn.
- [ ] **X9** External app already running.
- [ ] **X10** Disk full mid-install.
- [ ] **X11** Unicode filenames.
- [ ] **X12** Parallel installs of the same id.
- [ ] **X13** `.clean` deleted by user.
- [ ] **X14** Corrupt GPK.
- [ ] **X15** `window.confirm` never used.
- [ ] **X16** Catalog entry missing fields filtered out.
- [ ] **X17** TCC exits on its own during a session.
- [ ] **X18** Double-click Uninstall is idempotent.
- [ ] **X19** Launcher crash mid-install recoverable.
- [ ] **X20** `prefers-reduced-motion` honoured.
- [ ] **X21** Keyboard-only navigation across entire modal.
- [ ] **X22** Empty catalog.
- [ ] **X23** Mods folder missing.
- [ ] **X24** Two External mods share an exe name.
