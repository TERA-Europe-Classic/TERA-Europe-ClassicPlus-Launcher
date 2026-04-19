# Unit Audit — `<unit-slug>`

**Category:** gpk | external | launcher | tcc
**Status:** draft | signed-off
**Last verified:** YYYY-MM-DD by `<verifier>`
**Iter:** `<iter number>`

## Provenance

- **Upstream source:** `<URL>` (repo, gist, forum thread, archive, …)
- **License:** SPDX identifier or `Unknown` (flag as risk below)
- **Obfuscated binaries?** yes | no — if yes, describe tooling and
  why the launcher ships the obfuscated form.
- **Version pinned:** `<version>`
- **Download URL:** `<URL from catalog.json>`
- **SHA-256:** `<64 hex>`

## Public surface

What does the unit expose to the user / the launcher / the game?

- Tauri commands registered: `<none | list>`
- Settings files written under `<app-data>` or the mod's slot dir
- Ports listened on, processes spawned, network endpoints reached
- Game memory locations patched, DLLs injected

## Risks

Enumerate every flag a security-minded reviewer would raise:

- Unverified upstream (no code signing, no SBOM)
- Closed-source / obfuscated
- Makes outbound network requests
- Reads / writes outside its slot dir
- Known CVEs in bundled deps
- Parses untrusted binary (document the parser + fuzz coverage)

## Tests

- Launcher-side tests that exercise this unit: `<test file names>`
- Upstream tests (if any): `<URL>`
- Manual verification steps (for cases CI can't cover — live game
  spawn, mapper backup/restore, etc.)

## Verification

- [ ] Tests pass in CI
- [ ] Sandbox predicates pass (paths, shell scope, CSP)
- [ ] Sha256 matches catalog.json
- [ ] License declared
- [ ] Risks triaged and mitigated or documented as accepted

## Sign-off

| Date | Verifier | Notes |
|------|----------|-------|
| YYYY-MM-DD | `<name>` | — |
