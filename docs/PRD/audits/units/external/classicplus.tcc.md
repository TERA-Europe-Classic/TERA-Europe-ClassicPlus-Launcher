# Unit Audit — `classicplus.tcc`

**Category:** external
**Status:** draft
**Last verified:** 2026-04-20 by Lukas
**Iter:** 229

## Provenance

- **Upstream source:** https://github.com/neowutran/TCC (fork maintained for Classic+)
- **License:** Apache-2.0 (see upstream LICENSE)
- **Obfuscated binaries?** No — WPF/.NET, source-available.
- **Version pinned:** per `catalog.json` `classicplus.tcc` entry
- **Download URL:** per `catalog.json`
- **SHA-256:** per `catalog.json` (verified on install; see
  `tampered_catalog.rs` wiring chain)

## Public surface

- Tauri commands consumed: `launch_external_app`, `stop_external_app`
- Settings: writes under `<app-data>/mods/external/classicplus.tcc/`
- Process: spawns `TCC.exe` via `std::process::Command`; same
  overlay-lifecycle predicate as Shinra (attach-once tied to
  TERA.exe count).
- Network: TCC reaches Discord webhook (user-configurable for guild
  announcements), BAM / world-boss timers from community sources.
- Memory: TCC reads TERA.exe memory for cooldown / chat / buff state.

## Risks

- **Memory reader:** same class of risk as Shinra — arbitrary memory
  read. SHA-256 pin + obfuscated-delta check on future version bumps.
- **Discord webhook:** user-configurable outbound; launcher does not
  proxy.
- **13 class layouts:** Apex / Awakening per-class tiles must all
  render. Regression surface is large; tracked by §3.3.14 (screenshot
  audits per class) and per-class audit docs under `audits/units/tcc/`.
- **Upstream fork divergence:** same maintenance burden as Shinra.

## Tests

- `teralaunch/src-tauri/tests/multi_client.rs` — attach-once predicate.
- `teralaunch/src-tauri/tests/shell_scope_pinned.rs` + sister guards
  — spawn-path scope.
- Per-class screenshot audits → see `audits/units/tcc/<class>.md`
  (rollout tracked in §3.3.14).

## Verification

- [x] Install-path SHA gate covered
- [x] Spawn-path shell-scope covered
- [x] Attach-once predicate covered
- [ ] Live per-class tile render verified (13 classes, manual)
- [x] License declared (Apache-2.0)

## Sign-off

| Date | Verifier | Notes |
|------|----------|-------|
| 2026-04-20 | Lukas | Draft audit; per-class screenshot rollout is separate deliverable. |
