# Unit Audit — `classicplus.shinra`

**Category:** external
**Status:** draft
**Last verified:** 2026-04-20 by Lukas
**Iter:** 229

## Provenance

- **Upstream source:** https://github.com/neowutran/ShinraMeter (fork maintained for Classic+)
- **License:** MIT (see upstream LICENSE)
- **Obfuscated binaries?** No — WPF/.NET, source-available.
- **Version pinned:** per `catalog.json` `classicplus.shinra` entry
- **Download URL:** per `catalog.json`
- **SHA-256:** per `catalog.json` (verified on install; see
  `tests/bogus_gpk_footer.rs` + `tests/tampered_catalog.rs` for the
  install-path hash gate)

## Public surface

- Tauri commands consumed: `launch_external_app`, `stop_external_app`
- Settings: writes under `<app-data>/mods/external/classicplus.shinra/`
- Process: spawns `ShinraMeter.exe` via `std::process::Command`;
  attach-once predicate tied to TERA.exe count (see
  `services/mods/external_app.rs::should_attach_spawn`).
- Network: ShinraMeter connects to a remote DPS aggregator (user-
  configurable); the launcher does NOT proxy this.
- Memory: Shinra reads TERA.exe process memory for combat-log parsing
  (same vector used by Noctenium's read side).

## Risks

- **Memory reader:** Shinra reads TERA.exe memory; a malicious upstream
  build could read arbitrary addresses. Mitigation: SHA-256 pin in
  catalog + `fix.shinra-exec-path-pinned` guard.
- **Outbound DPS reporting:** user-configurable upstream URL; CSP
  connect-src does not cover Shinra (it's an external exe, not a
  webview). Users must understand the reporting URL is theirs to
  control.
- **Upstream fork divergence:** Classic+ fork carries custom patches;
  periodic re-sync with neowutran/ShinraMeter required. Track via
  Dependabot on the fork repo.

## Tests

- `teralaunch/src-tauri/tests/multi_client.rs` — attach-once predicate
  across multiple TERA.exe instances.
- `teralaunch/src-tauri/tests/shell_open_callsite_guard.rs` — guards
  the spawn call-site (PRD 3.1.5 + CVE-2025-31477).
- Manual: launch TERA, confirm Shinra overlay attaches; exit TERA,
  confirm Shinra process stops.

## Verification

- [x] Install-path SHA gate covered by `tampered_catalog.rs`
- [x] Spawn-path shell-scope covered by `shell_scope_pinned.rs` +
  `shell_open_callsite_guard.rs`
- [x] Attach-once predicate covered by `multi_client.rs`
- [ ] Live multi-client launch verified on real game (manual)
- [x] License declared (MIT)

## Sign-off

| Date | Verifier | Notes |
|------|----------|-------|
| 2026-04-20 | Lukas | Draft audit; live-game verification pending next TERA build cycle. |
