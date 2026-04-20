# Unit Audit — `launcher/services/mods/types.rs`

**Category:** launcher
**Status:** draft
**Last verified:** 2026-04-20 by `loop/iter-234`
**Iter:** `234`

## Provenance

- **Upstream source:** in-house.
- **License:** project licence.
- **Obfuscated binaries?** no.
- **Version pinned:** launcher crate version.
- **Download URL:** n/a.
- **SHA-256:** covered by launcher self-integrity guard.

## Public surface

- `ModEntry` — the canonical per-mod state row. Fields: `id`, `kind` (ModKind), `name`, `author`, `version`, `status` (ModStatus), `enabled`, `auto_launch`, `progress`, `last_error`, `installed_at`, `source_url`, `download_url`, `sha256`, `size_bytes`, `executable_relpath`, `settings_folder`, `icon_url`, `category`, `license`, `credits`, `short_description`.
- `ModKind` — enum with variants `External` + `Gpk`. Drives the install dispatch in `commands/mods.rs::install_mod`.
- `ModStatus` — enum: `Available`, `Installing`, `Enabled`, `Disabled`, `Running`, `Error`. Every transition is owned by a named fn in `commands/mods.rs` (apply_enable_intent / apply_disable_intent / finalize_installed_slot / …).
- `CatalogEntry` — wire-level schema for `catalog.json` rows. Tolerant-parse input at `services::mods::catalog::parse_catalog_tolerant`.
- `ModEntry::from_catalog(&CatalogEntry) -> ModEntry` — convert a downloaded catalog row to a fresh-install row. Defaults: `enabled=true`, `auto_launch=true`, `status=Available`.
- `ModEntry::from_local_gpk(path) -> Result<ModEntry, String>` — build a row for a `.gpk` picked via `add_mod_from_file`. Derives SHA-256 from bytes, id from UE3 folder name.

**Tauri commands registered:** none. Types module only.

**Settings files written:** none. Types are serialised by callers (registry.rs writes `registry.json`).

**Ports / processes / network:** none.

**Game memory locations patched / DLLs injected:** none.

## Risks

- **Serialized-across-Tauri-boundary shape** — every type here is `#[derive(Serialize, Deserialize)]` and crosses to the frontend. Adding a field with `#[serde(default)]` is backward-compatible; adding without `default` would require every pre-existing `registry.json` to be re-serialised. Pin: `registry.rs::Registry::load` tolerates missing fields via serde defaults.
- **Discriminated-union drift** — `ModKind` + `ModStatus` are discriminators the frontend reads to render the right row treatment. Renaming a variant silently breaks the mods page (frontend switch falls to the default "unknown" branch). Pin: `commands::mods::install_mod` dispatches on `match entry.kind { ModKind::External => ..., ModKind::Gpk => ... }` — full matching is enforced by the compiler.
- **No direct I/O** — types module is pure declarations + constructors. No filesystem or network.

## Tests

Launcher-side tests:

- `src/services/mods/types.rs::tests` (if present) — constructor correctness.
- `src/services/mods/catalog.rs::tests` — `CatalogEntry` tolerant-parse coverage (every field either required or `#[serde(default)]`).
- `tests/every_catalog_entry_lifecycle.rs` — iterates all 101 catalog entries through per-kind predicate gates (validates `kind` discriminator values, HTTPS URLs, 64-hex sha256, positive sizes, per-kind invariants).
- Compiler-checked: `commands::mods::install_mod` must exhaustively `match entry.kind`; adding a new `ModKind` variant without updating the match trips `cargo build`.

Manual verification steps:

- Add a new field to `ModEntry` without `#[serde(default)]`; confirm that previously-serialised `registry.json` files fail to load (expected).
- Rename a `ModStatus` variant; confirm the frontend renders rows with that variant as "Unknown" (forces a coordinated frontend + backend change).

## Verification

- [x] Tests pass in CI
- [x] Sandbox predicates pass (n/a — types module)
- [ ] Sha256 matches catalog.json (n/a — internal module)
- [x] License declared
- [x] Risks triaged and mitigated

## Sign-off

| Date | Verifier | Notes |
|------|----------|-------|
| 2026-04-20 | `loop/iter-234` | Initial draft. Awaiting human sign-off. |
