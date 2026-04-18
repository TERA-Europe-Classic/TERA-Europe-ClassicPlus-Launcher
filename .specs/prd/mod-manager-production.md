# PRD — Mod Manager: Production-Ready State

**Status:** authoritative — the ralph loop drives against this document until every
acceptance criterion in `acceptance-criteria.md` is green and every scenario in
`test-plan.md` is automated and passing.

**Version:** 1.0 — 2026-04-19
**Owner:** TERA Europe Classic+ Launcher
**Scope:** the Mods feature of the Tauri launcher (`teralaunch/`), the TMM-style
GPK deployer (`teralib` + `services/mods/tmm.rs`), and the external mod catalog
(`TERA-Europe-Classic/external-mod-catalog`).

---

## 1. Vision

The mod manager is the **single surface** a TERA Europe Classic+ player touches
to discover, install, update, enable, disable, and uninstall client-side mods.
Everything the launcher ships — from the Shinra/TCC overlays to community GPK
recolors, FPS packs, and costume edits — flows through it.

Production-ready means: a non-technical player can find a mod, install it, see
it take effect in-game on next launch, update it without thinking about it, and
uninstall it without residue. The process cannot break the game client, leak
secrets, leave orphaned processes, corrupt the composite mapper, or crash the
launcher.

"No possible areas for improvement" is the stop condition (see §12).

## 2. Goals

### 2.1 Functional goals

1. **Browse** the external catalog with search, category filters, count badges,
   multi-language strings, and mod detail pages.
2. **Install** external zip mods (Shinra, TCC) and GPK mods (foglio, Taorelia,
   Owyn, CatAnnaDev, pantypon, etc.) with streamed download progress,
   SHA-256 integrity verification, and atomic on-disk commit.
3. **Deploy** installed GPKs into the game via a full TMM-compatible
   `CompositePackageMapper.dat` patch — byte-for-byte output equivalent to
   VenoMKO/TMM so mods actually render in-game.
4. **Enable / Disable** flips a per-mod flag. External apps auto-spawn at
   game launch and auto-close at game exit. GPKs take effect on next game
   launch via the mapper.
5. **Update** detects catalog version drift on load and on launcher boot.
   Shows an in-app banner on launch; flips affected rows to `update_available`
   inside the mods modal. Click → reinstall the catalog entry, which
   overwrites cleanly.
6. **Uninstall** terminates any running external mod, deletes files, restores
   the vanilla CompositePackageMapper.dat entries for a GPK, optionally wipes
   user settings (prompted).
7. **Onboarding** shows a dismissible first-launch welcome card explaining
   what the mod manager is. Persisted once.
8. **Launch-time banner** surfaces "N mod updates available" at boot with a
   click-to-open Mods action.

### 2.2 Non-functional goals

- **Performance:** download bar updates continuously (time-throttled, ~60 ms
  emit interval, no 5 % steps). No full-list re-renders on progress — DOM is
  patched surgically. 60 fps animations. Mods modal opens in <150 ms.
- **Reliability:** every downloaded artefact is SHA-256-verified. Partial
  downloads are cleaned up. Interrupted installs leave no half-state in the
  registry. Hash mismatches never write to disk.
- **Security:** all mod downloads go through `reqwest` with the Tauri HTTP
  allowlist. No arbitrary URL execution. GPK deploy never writes outside the
  configured game root. Composite mapper backups (`.clean`) are untouched by
  subsequent installs.
- **Accessibility:** keyboard navigation across every control (Tab, Shift-Tab,
  Enter, Esc). ARIA roles on dialogs, toggles, lists. Respects
  `prefers-reduced-motion`. Focus trap inside the modal. Color contrast ≥ 4.5:1.
- **Internationalisation:** every user-facing string is in `translations.json`
  for EN, FR, DE, RU. No hard-coded English strings outside translation-source
  files.
- **Resilience to offline / catalog down:** launcher still works; Mods shows
  a useful empty-state message with the actual error; Installed tab still
  functions from the local registry.
- **Launcher and catalog decoupling:** catalog updates take effect without a
  new launcher release.

### 2.3 Quality goals

- **Test coverage:** every command in `src-tauri/src/commands/mods.rs` has
  unit + integration tests. Every public function in `services/mods/tmm.rs`
  has fixture-based tests including the encrypt/decrypt/parse/serialize
  round-trip and a fixture that matches VenoMKO/TMM's reference output
  byte-for-byte. Every branch of `mods.js` has a Vitest test. Every user
  flow has a Playwright E2E test.
- **Zero warnings:** `cargo build --release` and `cargo clippy --all-targets
  -- -D warnings` emit zero warnings. `tsc --noEmit` (if TS is used) is
  clean. Vitest and Playwright both exit 0 in CI.
- **Zero UI regressions:** Playwright captures desktop + tablet + mobile
  screenshots of Browse, Installed, Detail panel, Onboarding, Download
  tray, Update banner, every filter combination. Visual diffs fail the
  build.
- **Zero console noise:** no `console.error` or `console.warn` during
  nominal flows. All warnings are either eliminated or explained with a
  comment referencing the root cause.

## 3. Personas & scenarios

**Lux, new player.** Installed the launcher an hour ago. Opens Mods,
expects to browse what's available, install Shinra with one click, and have
it work the next time they hit Launch.

**Neo, long-time user.** Has TCC + 6 GPK mods installed. Updates hit the
catalog overnight. On next launcher boot, sees the banner, clicks it,
updates in 30 s, plays.

**Pix, tinkerer.** Imports their own `.gpk` file from disk. Toggles it on
and off between runs to compare. Expects their CompositePackageMapper.dat
to be restored exactly after each disable/uninstall.

**Riko, non-English speaker.** German UI. Every label, tooltip, empty
state, and dialog reads correctly in German.

**Mal, offline user.** Catalog host is briefly unreachable. Installed mods
still render, toggle still works, the Browse tab shows a helpful message
with a Retry button — not a silent empty state.

## 4. Out of scope (v1 production)

- Mobile / tablet layouts (desktop-only product).
- Uploading user-authored mods to the catalog.
- A mod-to-mod dependency / conflict resolver.
- Server-side mod signing / code-signing of third-party mods.

## 5. System architecture

```
+------------------------+        +-------------------------+
|  Launcher UI (Tauri)   |        |  External mod catalog   |
|  - index.html + app.js | -----> |  GitHub Pages JSON      |
|  - mods.html + mods.js |   fetch|  (version, url, sha256) |
|  - modals, banner      |        +-------------------------+
+-----------+------------+
            | invoke
            v
+------------------------+        +-------------------------+
|  Rust commands         |        |  Noctenium mirror       |
|  commands/mods.rs      | <----- |  127.0.0.1:7803 (game)  |
|  services/mods/        |        +-------------------------+
|   - catalog.rs         |
|   - external_app.rs    |        +-------------------------+
|   - registry.rs        |        |  TMM mapper             |
|   - tmm.rs             | -----> |  S1Game/CookedPC        |
|   - mods_state.rs      |        |  CompositePackageMapper |
+------------------------+        +-------------------------+
```

The UI never touches disk or the network directly — every side effect goes
through a Tauri command.

## 6. Functional requirements (detailed)

### 6.1 Browse

- **B1** Fetch catalog from
  `https://raw.githubusercontent.com/TERA-Europe-Classic/external-mod-catalog/main/catalog.json`
  via `get_mods_catalog`. Cache locally with a short TTL.
- **B2** Render a row per entry with: icon (if `icon_url` is non-empty and the
  image actually loads — never a placeholder), name, author, short
  description, primary-action cell (Install / Update / Uninstall / Toggle).
- **B3** Filter chips: `all | external | gpk` (kind) and dynamic category
  chips populated from whatever `category` strings the loaded catalog
  advertises. Default: All.
- **B4** Search box matches name + author + category + short description
  (case-insensitive, substring).
- **B5** Count badges: Installed tab ("Installed 3") and Browse tab
  ("Browse 89"). Installed count is total rows. Browse count is catalog
  size minus currently-installed ids. Updates on every render regardless
  of active tab.
- **B6** Mods that are already installed never appear in Browse.

### 6.2 Install — external (zip)

- **I1** Download the zip to memory via streaming reqwest with progress
  callbacks throttled to ~60 ms wall-clock.
- **I2** Verify SHA-256 before writing anything to disk. Mismatch →
  registry row goes to `Error` with the mismatch message, nothing on disk.
- **I3** Clear the destination directory, extract the zip (zip-slip
  protected), verify the advertised `executable_relpath` exists.
- **I4** Fresh install defaults: `enabled=true`, `auto_launch=true`,
  `status=Enabled`. User can untoggle.
- **I5** Progress events are emitted at download start (0), at least every
  ~60 ms during streaming, and at completion (100). `install_external_mod`
  caps the download phase at 95 so extraction can occupy 95–100.

### 6.3 Install — GPK

- **G1** Resolve `<app_data>/mods/gpk/<id>.gpk` as the download destination.
- **G2** Same stream-download + SHA-256 verify + atomic write as I1/I2.
- **G3** After download, call `tmm::install_gpk(game_root, source_gpk)` to
  deploy into the game:
  - Parse the mod file footer (magic `0x9E2A83C1` at EOF, backwards-chained
    i32 offsets, `MOD:` folder-name prefix).
  - Ensure `.clean` backup of `CompositePackageMapper.dat` exists. If not,
    write one now from the current vanilla state.
  - Copy the GPK into `S1Game/CookedPC/<mod_id>.gpk` (or the folder the
    TMM footer declares).
  - Decrypt the mapper (3-pass cipher), patch composite entries so their
    incomplete-path matches route to the mod GPK, re-encrypt, atomically
    write.
  - Append an entry to `ModList.tmm`.
- **G4** On deploy failure, the registry row carries `last_error` with a
  human-readable reason ("mapper patch failed: …"); the GPK stays on disk
  so the user can retry; status goes to `Enabled` with the error attached
  (not `Error` — the download still succeeded).

### 6.4 Enable / Disable

- **E1** Toggle is **pure intent** — click does not spawn or kill a
  process. It flips `enabled`, `auto_launch`, `status`.
- **E2** `spawn_auto_launch_external_apps` runs at game-launch time in
  `commands/game.rs` before `run_game`. It launches every `ModKind::External`
  mod whose process isn't already running, then bumps its status to
  `Running`.
- **E3** `stop_auto_launched_external_apps` runs after `game_ended` fires.
  It terminates every installed External mod's process regardless of
  current enabled flag (covers untoggled-mid-session). Flips status back
  to `Enabled` or `Disabled` to match the current toggle.
- **E4** Checkbox `click` handler does not call `preventDefault()` — the
  browser commits the flip natively so the switch animation is instant
  even if the IPC round-trip takes a moment.
- **E5** Disabling a GPK restores its vanilla entries in
  `CompositePackageMapper.dat` from `.clean`; re-encrypts; deletes the
  container GPK in `CookedPC` if no other installed GPK shares the same
  container; leaves the source `.gpk` in `<app_data>/mods/gpk/` so the
  user can re-enable without re-downloading.

### 6.5 Update

- **U1** Catalog's `version` field is the source of truth. When the
  launcher reloads the Installed list, it compares each installed row's
  `version` against the catalog's. Strict inequality → status becomes
  `UpdateAvailable`. Skip if the row is in a transient state
  (`installing`, `error`, `running`, `starting`).
- **U2** The Installed tab renders an Update primary button in place of
  the toggle for `update_available` rows.
- **U3** Clicking Update re-invokes `install_mod` with the current catalog
  entry. Backend overwrites the extracted folder (external) or the GPK
  file (gpk) and re-patches the mapper.
- **U4** Boot-time: `checkModUpdatesOnLaunch` runs once 1.5 s after the
  home page paints. If any installed mods are outdated, it shows the
  bottom-right banner listing the first three by name + a "+N more" tail.
  Click → opens Mods modal. × → hidden until next launch (no persistence).

### 6.6 Uninstall

- **R1** `uninstall_mod(id, delete_settings?)`:
  - External: stop the process by name if running; delete the
    install folder; if `delete_settings` is true and the catalog entry
    advertises a `settings_folder`, delete that too.
  - GPK: restore vanilla mapper entries from `.clean`; re-encrypt;
    delete the container GPK in `CookedPC`; delete
    `<app_data>/mods/gpk/<id>.gpk`; trim the `ModList.tmm` entry.
  - Registry entry removed.
- **R2** The UI confirmation dialog is a custom `modalConfirm` Promise
  (not `window.confirm`, which is unreliable inside WebView2). Uninstall
  never fires until the user clicks Confirm.
- **R3** If the user had the "also delete settings" checkbox ticked in the
  dialog, pass that through as `delete_settings=true`.
- **R4** Uninstall for a running external mod always stops the process
  first. Error reporting surfaces any stop failure ("Could not stop
  ShinraMeter.exe, try closing it and retrying.").

### 6.7 Onboarding

- **O1** First time the user loads the launcher after v0.1.6+, the
  onboarding card appears before they can click Launch. Dismissal is
  persisted to `localStorage.mods_onboarding_seen = "true"`.
- **O2** Card has two buttons: "Got it" (dismiss) and "Open mods"
  (dismiss + open the modal).

### 6.8 Download tray

- **T1** Active installs appear as a pill in the bottom-right, showing
  name, bytes received / total, percentage. Updates surgical (only width
  and label patched — no re-render).
- **T2** On completion, the pill fades out; on error, it persists with a
  Retry action.

## 7. TMM/GPK deployment correctness

The mapper is the load-bearing thing in production. Every requirement
below **must** hold byte-for-byte against VenoMKO/TMM's reference
behaviour.

- **M1** 3-pass cipher (`GeneratePackageMapper` XOR + middle-outward
  pair swap + 16-byte `Key1 = [12,6,9,4,3,14,1,10,13,2,7,15,0,8,5,11]`
  shuffle) is identity under encrypt→decrypt and decrypt→encrypt.
- **M2** Textual mapper format
  `<filename>?<obj>,<comp>,<off>,<size>,|...!` parses into a
  `HashMap<composite_name, MapperEntry>` and serialises back to an
  identical byte sequence.
- **M3** `incomplete_paths_equal` matches composite-name prefix and
  path suffix via the TMM split rule (split at `.`, then
  `rfind('_')` on the composite half).
- **M4** `parse_mod_file` reads the footer backwards after verifying the
  magic number. Non-TMM GPKs are rejected cleanly.
- **M5** `install_gpk` first ensures `.clean` backup, then copies the
  GPK, then re-encrypts the patched mapper — in that order so a failure
  mid-way never leaves the mapper in a half-patched state.
- **M6** `uninstall_gpk` restores vanilla entries exactly — the user
  can uninstall every mod and the game boots identically to a vanilla
  install. Verified via a checksum test.

## 8. Edge cases (must be explicitly covered by tests)

| # | Edge case | Required behaviour |
|---|-----------|-------------------|
| 1 | Game root not configured | Install + deploy: downloaded OK, deploy skipped with a clear `last_error`. Retry action reruns deploy only. |
| 2 | `CookedPC` is read-only | Deploy fails with `last_error`; download preserved; no mapper changes. |
| 3 | SHA-256 mismatch mid-download | File never touches the dest path; registry row goes to `Error`; no partial file on disk. |
| 4 | Network drops mid-download | Reqwest surfaces an error → `Error` status; no partial artefact. |
| 5 | Catalog host 5xx | Browse empty-state with the actual HTTP status and a Retry button. Installed list still works from registry. |
| 6 | `classicplus.shinra` installed then catalog renames it | Old id's mod keeps working; user sees it in Installed under the old name until they uninstall/reinstall. |
| 7 | User's game path changed | Next deploy fails gracefully; surface "Game path not found" with a Settings link. |
| 8 | UAC prompt during external-app spawn | Handled (ShellExecuteExW). If user denies, `last_error` is "launch denied by user". |
| 9 | Process is already running from a prior session | `spawn_app` detects it and skips; status still flips to Running. |
| 10 | Disk full mid-install | Fail cleanly, reverse any partial writes, surface "Out of disk space". |
| 11 | Long filename / Unicode / spaces | URL-encode in HTTP requests; unescape on local file I/O; never double-encode. |
| 12 | Two installs of the same id in parallel | Serialise via per-id mutex or registry check — no double-write races. |
| 13 | `.clean` backup deleted manually by user | On next deploy, recreate from the current mapper *only if* no currently-installed GPK is patched; otherwise refuse and surface a recovery instruction. |
| 14 | Mod file is corrupt | `parse_mod_file` returns an explicit error the UI shows; the GPK is deleted from `<app_data>/mods/gpk/`. |
| 15 | WebView2 `window.confirm()` silently returns true | Never use `window.confirm`. Always use `modalConfirm`. |
| 16 | Catalog comes back with a corrupt entry (missing fields) | Serde tolerates missing optional fields; entries without `sha256` or `download_url` are filtered out at load time with a warning. |
| 17 | `TCC.exe` exits on its own during a session | Status falls back to `Enabled` when the process is detected as gone on next tick. |
| 18 | User clicks Uninstall twice quickly | Second click is a no-op because the row is already removed from the registry. |
| 19 | Launcher crashes mid-install | Registry persists; on next boot, the row is `Installing` with no ongoing download → flips to `Error` with "interrupted" message and a Retry button. |
| 20 | prefers-reduced-motion is set | All transitions (banner slide-in, toggle thumb slide, progress-bar width) respect the OS preference. |
| 21 | Screen reader / keyboard-only user | All interactive elements reachable via Tab. Esc closes the modal and the confirm dialog. Enter activates the focused button. |
| 22 | Empty catalog (0 entries) | Browse shows a friendly empty state, not a broken layout. |
| 23 | Mods folder missing | Created on first install attempt. |
| 24 | Two external mods share the same exe name | Resolve by install-root + relpath, not by exe name alone. |

## 9. UI / UX requirements

- **UX1** Mods modal close is a Windows-style top-right × inside a
  titlebar (not a floating chip). Red hover. Keyboard Esc closes.
- **UX2** Backdrop click closes the modal (unless a confirm dialog is
  open). Focus trap stays inside the modal while it's open.
- **UX3** Scroll within the modal uses a cyan-gradient pill scrollbar,
  matching the launcher palette. Scoped under `#mods-page`.
- **UX4** All transitions respect `prefers-reduced-motion`.
- **UX5** Download tray sticks to the bottom-right and stacks
  vertically when multiple installs are active. Items animate in/out.
- **UX6** Progress bar moves continuously (no stutter). Width patched
  surgically; no parent re-render.
- **UX7** Toggle thumb animates 180 ms cubic-bezier on state change.
- **UX8** Detail panel opens on row click (outside the action button).
  Closes on Esc, × button, or backdrop click.
- **UX9** Overflow menu (…) on each Installed row has Details, Open
  source (when `source_url` present), Uninstall. Click-outside closes it.
- **UX10** Visual regression screenshots captured at 1280×720 (the
  launcher's fixed window size) for every pane + filter + state.

## 10. Performance requirements

- **P1** `get_mods_catalog` TTFB ≤ 500 ms on a warm cache; ≤ 3 s cold.
- **P2** Streamed download reports progress at ≥ 10 events / second on
  a 10 Mbit/s connection (time-throttled to 60 ms).
- **P3** No full list re-render on each progress tick — only the bar
  width and label are patched.
- **P4** Opening the Mods modal < 150 ms (first paint to interactive).
- **P5** Typing in the search box feels instantaneous — filter runs on
  `input` with no debounce needed for ≤ 300 rows.
- **P6** Scroll inside the Installed/Browse panes is 60 fps. No
  synchronous layout thrashing from per-frame bindings.
- **P7** Frontend bundle size does not regress > 5 % per release.

## 11. Success criteria / Acceptance

Track in `acceptance-criteria.md`. Release blockers are:

1. Every criterion in §2.1 Functional goals passes an automated test in
   CI.
2. Every edge case in §8 has a named test in `test-plan.md`, and that
   test is passing.
3. Visual regression shows zero unjustified diffs against the
   checked-in baselines.
4. `cargo clippy --all-targets -- -D warnings` and `cargo test --release`
   exit 0 on Windows.
5. `npm test` and `npm run test:e2e` exit 0.
6. No `console.warn` / `console.error` during a full Playwright happy-path
   run.
7. Manual spot check: install Shinra + 3 GPK mods on a clean Classic+
   install; launch the game; all mods apply; game doesn't crash; uninstall
   everything; launch again; game runs vanilla.

## 12. Ralph loop mechanics & termination

The ralph loop iterates `sdd:plan` → `sdd:implement` → `tdd:write-tests`
→ `code-review:review-local-changes` → `reflexion:reflect` →
`git:commit` on a checklist driven by `acceptance-criteria.md`.

**Per-iteration contract:**

1. Pick the next unchecked acceptance criterion.
2. If no test covers it yet, write one first (`tdd:test-driven-development`).
3. Implement the smallest change that makes the test pass.
4. Run the full test suite; fix any regressions.
5. Run `code-review:review-local-changes`; fix all ≥80-confidence findings.
6. Run `reflexion:reflect` before presenting the diff.
7. Commit via `git:commit` (conventional format, no emoji).
8. Update the checklist; re-enter the loop.

**Loop terminates only when:**

- Every checkbox in `acceptance-criteria.md` is `[x]`.
- `cargo test --release` and `npm test` both exit 0.
- `npm run test:e2e` exits 0 with zero visual regressions.
- `cargo clippy --all-targets -- -D warnings` exits 0.
- A full Playwright run produces zero console warnings / errors.
- `code-review:review-local-changes` produces zero findings ≥80
  confidence on the cumulative diff since the last release tag.
- `reflexion:critique` on the full mod-manager surface returns
  consensus "no further improvement possible" across all three judges
  (Requirements, Architecture, Code Quality).
- Repository is in a state where a new release can be cut with a single
  `gh workflow run deploy.yml`.

Anything short of this list = not done. The loop does not stop because
"we've run 20 iterations" or "it looks good enough". The checklist is
the oracle.

## 13. Risks

| Risk | Mitigation |
|------|------------|
| TMM format drift if TERA updates | Test fixtures captured from real `CompositePackageMapper.dat` checked in; watched by CI so any format change fails loudly. |
| Catalog hostname change | Base URL lives in `config.json`, overridable; resilient to DNS failures via retries + cached offline fallback. |
| Third-party mod breaks after a game patch | Not our job to fix; surface the error clearly, offer a one-click disable, don't hang the launcher. |
| Windows UAC denies launch | ShellExecuteExW path shows a message, `last_error` carries the reason, user can retry. |

## 14. References

- VenoMKO/TMM (https://github.com/VenoMKO/TMM) — CompositeMapper.cpp + Mod.cpp
  are the authoritative reference for §7.
- CLAUDE.md (repo root) — launcher build + deploy procedure.
- `external-mod-catalog` README — catalog entry schema.
