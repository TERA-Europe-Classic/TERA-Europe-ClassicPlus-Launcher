# Unit Audit — `launcher/services/mods/catalog.rs`

**Category:** launcher
**Status:** draft
**Last verified:** 2026-04-20 by `loop/iter-229`
**Iter:** `229`

## Provenance

- **Upstream source:** written in-house.
- **License:** project licence.
- **Obfuscated binaries?** no.
- **Version pinned:** follows launcher crate version.
- **Download URL:** n/a (internal module).
- **SHA-256:** covered by launcher self-integrity guard.

## Public surface

- `fetch_remote() -> Result<Catalog, String>` — HTTPS GET against `CATALOG_URL` (`raw.githubusercontent.com/TERA-Europe-Classic/external-mod-catalog/.../catalog.json`), tolerant parse, returns typed `Catalog`.
- `parse_catalog_tolerant(body: &str) -> Result<Catalog, String>` — two-phase parser:
  1. Envelope (`version`, `updated_at`, `mods` array) is hard-validated; missing fields / invalid JSON → `Err`.
  2. Each entry is deserialised individually; failures are logged and dropped, successes kept. A single malformed entry no longer torpedoes the whole catalog load.
- `load_cached() -> Option<Catalog>` / `save_cached(&Catalog)` — 24 h disk cache at `<app_data>/mods/catalog-cache.json`. Entries stale after 24 h; callers fall back to `fetch_remote`.

**Tauri commands registered:** none (callers in `commands/mods.rs` invoke these).

**Settings files written:** `<app_data>/mods/catalog-cache.json`.

**Ports / processes / network:** one outbound HTTPS GET against a hard-coded `raw.githubusercontent.com` URL on launcher startup (and on manual catalog refresh).

**Game memory locations patched / DLLs injected:** none.

## Risks

- **Outbound network** — single URL to raw.githubusercontent.com. Mitigations:
  - HTTPS-only (HTTP fails the Tauri-v2 `default-src` CSP).
  - No Authorization header, no credentials — the catalog is a public file.
  - `tests/http_allowlist.rs` + `tests/http_redirect_offlist.rs` pin the allow-list; a refactor that redirected off-allowlist trips the guard.
- **Untrusted JSON** — `parse_catalog_tolerant` feeds `serde_json::from_value` on every entry in isolation; unknown fields are ignored (default serde behaviour); each field is typed (`String`, `enum Kind`, `u64`, …) so a type-mismatch drops the one entry, not the whole file.
- **Cache file** — `catalog-cache.json` lives under `app_data`. A local attacker with write access to that dir can swap the cache contents, but the same attacker can already swap the entire launcher exe — treating the cache as trusted-if-present is consistent with the surrounding trust model.
- **Redirect handling** — httpclient follows up to N redirects; each redirect target is validated against the allow-list by the `http_redirect_offlist` test.

## Tests

Launcher-side tests:

- `src/services/mods/catalog.rs::tests` — envelope validation, tolerant parsing, cache round-trip.
- `tests/http_allowlist.rs`, `tests/http_redirect_offlist.rs` — outbound-network sandboxing.
- `tests/tampered_catalog.rs` — adversarial-corpus wiring guard.
- `tests/every_catalog_entry_lifecycle.rs` — iterates all 101 catalog entries through predicate gates (iter 229 addition).

Manual verification steps:

- Swap a single entry's `kind` field to an unknown value and confirm the catalog still loads with N-1 entries (not empty, not errored).
- Set `FILE_SERVER_URL` to a non-resolving host and confirm the offline banner surfaces (via `commands/util.rs` probe, iter-228 fix).

## Verification

- [x] Tests pass in CI (5/5 every_catalog_entry_lifecycle + 4 in-module tolerant-parse)
- [x] Sandbox predicates pass (HTTPS-only, allow-list enforced)
- [ ] Sha256 matches catalog.json (n/a — internal module)
- [x] License declared
- [x] Risks triaged and mitigated

## Sign-off

| Date | Verifier | Notes |
|------|----------|-------|
| 2026-04-20 | `loop/iter-229` | Initial draft. Awaiting human sign-off. |
