# TERA Europe Classic+ Launcher — Changelog

Player-facing release notes. Each version calls out what changed for you,
not every commit that landed. For the full developer history, see the
git log.

The most recent release is at the top.

---

## Unreleased

Nothing yet. Next release will include the improvements currently being
polished in the main branch.

---

## 0.1.20 — TCC interop handoff and safer GPK migration groundwork

- TCC launch now hands off the shared Noctenium/Agnitor map-export path,
  which is the plumbing needed for Classic+ packet-tool compatibility
  without relying on old Toolbox-only assumptions.
- GPK installs are now stricter when a curated migration artifact exists.
  If the launcher detects a managed patch manifest for a mod, it refuses
  the older whole-file fallback path instead of silently deploying a
  drift-prone package over your live client files.
- Curated-manifest blocks now also stop later enable/rebuild paths for
  launcher-managed catalog GPK mods, so the launcher does not quietly
  fall back to the old legacy deploy path after the initial install step.

---

## 0.1.14 — Update-install flow, GPK mapper patch, discovered-mods CSV

- The "Check for launcher updates" button now actually installs the
  update. Previously it detected and announced "Update available" but
  never triggered the download — another Tauri v1 to v2 API miss from
  the framework migration (v2 requires an explicit
  `update.downloadAndInstall()` call). Button now prompts to confirm,
  downloads, installs, and relaunches.
- Legacy GPK install rewrites `CompositePackageMapper.dat` entries so
  the mod actually overrides its target composite in-game. Previous
  release only copied the file into CookedPC/, which TERA's engine
  ignored because the composite-mapper still pointed at the vanilla
  packed location. Flight-gauge-style .gpk mods should now hide their
  target UI after relaunch.
- Shipped 61 discovered-mod entries at
  `docs/PRD/audits/research/tera-mods-discovered.csv` for catalog
  expansion work. These are tera-proxy / tera-toolbox Node.js modules;
  they'd need a new launcher ModKind + toolbox integration to install
  automatically.

---

## 0.1.13 — Portal-offline fix, legacy GPK install, filter alignment

- The "Can't reach the portal server" banner no longer shows when the
  launcher has no patch server configured. Classic+ ships without one
  today, so the banner was falsely firing on every start.
- "Update check not available" was a Tauri v1 to v2 API regression from
  the framework migration. Fixed.
- Legacy (non-TMM) GPK mods install now: the launcher reads the UE3
  PackageName from the mod's header, copies the file into CookedPC/
  with the matching filename, backs up the vanilla file as
  `<name>.gpk.vanilla-bak`, and patches CompositePackageMapper.dat to
  route the composite at the new file. Covers historical community
  mods that predate the TMM format.
- Account dropdown no longer opens behind the offline banner when both
  are visible. Dropdown z-index bumped above the banner.
- Filter-strip chips align with the tab row now. The first chip's text
  starts at the same horizontal position as the tab labels, not 4 px
  to the left.
- Search bar grew an X clear button on the right that appears whenever
  the field is non-empty.

---

## 0.1.12 — Update detection and launch-time banner

Your mods now know when a new version is out. The launcher compares each
installed mod against the public catalog on startup; if any mod is behind,
a banner at the top offers to update them with one click.

- The catalog fetch now also drives an "updates available" indicator so
  you don't have to browse looking for version numbers.

---

## 0.1.11 — Browse tab polish and reliable deploys

A cleanup round across the Browse tab and the deploy pipeline.

- Browse tab shows a live count of available mods and a category filter.
- Fresh installs are enabled by default so you don't have to toggle
  every mod after clicking Install.
- Toggle switches in the Installed tab are now intent-only — flipping
  them doesn't try to spawn or kill the mod process, just marks what
  you want to run next game launch.
- Scrollbar restyled to match the launcher's visual palette.
- Deploy pipeline made more robust: intermittent FTPS hang-up errors no
  longer fail releases when the file is actually uploaded.

---

## 0.1.10 — Smoother progress bars

- Download progress bars now move smoothly instead of jumping in 5%
  increments. The old throttle is gone; you see every real update.
- Deploy pipeline now uses `curl` for FTPS uploads instead of the older
  WinSCP bridge — faster and more reliable on the GitHub Actions runner.

---

## 0.1.9 — Real TMM deployer and elevation fix

GPK mods now deploy for real. The launcher parses TMM-format mod files,
backs up your vanilla `CompositePackageMapper.dat` as `.clean`, patches
the mapper to route the right composite packages at the mod's GPK, and
restores on uninstall.

- Mods installed into a game that lives under `C:\Program Files\` now
  prompt for elevation cleanly instead of silently failing to write.
- Uninstall confirmation dialog actually asks before removing files
  (previously a slip could wipe without a prompt).
- Icons refresh after install — no more stale placeholder images after
  a mod lands.

---

## 0.1.8 — Overlay polish and more mods

- Installed-tab rows without icons now render cleanly (empty tile,
  not a broken image).
- The Enable / Disable toggle is a proper toggle, not a button that
  flashes.
- Download progress no longer flickers during bursty chunks.
- 8 additional GPK mods in the catalog.
- Titlebar close button renders correctly on non-default themes.

---

## 0.1.7 — GPK install v1 and row polish

First pass at GPK mod installation lands. Picking a GPK mod from Browse
downloads it, verifies the hash, and stores it under your mod library.
(The mapper patch — making the game actually load the mod — arrives in
0.1.9.)

- Per-row action icons in the Installed tab (launch, stop, settings).
- Overflow menu for less-common actions (open mod folder, remove).
- External-app executable names detected correctly so launcher status
  reflects what's actually running.

---

## 0.1.6 — Mods modal popup and onboarding

Mods moved from a permanent header tab to a top-right popup, keeping the
main launcher chrome clean. First-time users get a short onboarding flow
that explains what each type of mod does.

---

## 0.1.5 — Detail panel opens cleanly, i18n fixes

- Clicking a mod now reliably opens the detail panel. Previously the
  panel could open on page load, stealing focus.
- Translation keys that weren't being resolved now show the correct
  localised text instead of the raw key string.

---

## 0.1.4 — Mod Manager v1

First public cut of the integrated mod manager. Browse the catalog,
install Shinra Meter and TCC directly from the launcher, see credits and
source links for each mod. Foundation for the full GPK deployer landing
in 0.1.9.

---

## 0.1.3 and earlier

Classic+ identity work, v100.02 API adaptation, launcher versioning.
See the git log for development history.
