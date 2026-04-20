# Unit Audit — `launcher/services/mods/tmm.rs`

**Category:** launcher
**Status:** draft
**Last verified:** 2026-04-20 by `loop/iter-229`
**Iter:** `229`

## Provenance

- **Upstream source:** written in-house (TERA-Europe-Classic/TERA-Europe-ClassicPlus-Launcher). TMM footer format referenced from community reverse-engineering notes; GPK / UE3 header layout referenced from VenoMKO/TeraCoreLib C++ source (authoritative).
- **License:** project licence (matches repository root).
- **Obfuscated binaries?** no — Rust source shipped as part of the launcher binary.
- **Version pinned:** `teralaunch/src-tauri/src/services/mods/tmm.rs` (internal module, follows launcher crate version).
- **Download URL:** n/a (compiled into the launcher exe).
- **SHA-256:** n/a — self-integrity guard (`tests/self_integrity.rs`) pins the launcher exe at release time, which covers this module.

## Public surface

What `services::mods::tmm` exposes to the rest of the launcher:

- `parse_mod_file(bytes) -> Result<ModFile, String>` — parses TMM v1/v2+ footers, returns a typed descriptor.
- `install_gpk(game_root, source_gpk, modfile) -> Result<(), String>` — TMM-stamped happy path: copies the .gpk into `CookedPC/`, patches `CompositePackageMapper.dat` per `ModFile.packages`, ensures a `.clean` backup of the vanilla mapper.
- `install_legacy_gpk(game_root, source_gpk) -> Result<String, String>` — iter-228 addition for non-TMM GPKs: reads the UE3 FolderName header, copies file into `CookedPC/<name>.gpk` with `.vanilla-bak`, rewrites every mapper entry whose `object_path` ends in `.<folder_name>`.
- `extract_package_folder_name(bytes) -> Option<String>` — pure UE3 FString parser. Returns `None` on malformed/truncated/bad-magic input.
- `uninstall_gpk(…)` / `uninstall_legacy_gpk(…)` — symmetric removal paths.
- `detect_conflicts(…)` — returns a list of composite packages claimed by more than one installed mod.
- `encrypt_mapper` / `decrypt_mapper` — 3-step cipher (XOR key2 → swap → permute key1) per TeraCoreLib FMapper.cpp.
- `ensure_backup(game_root)`, `recover_missing_clean(game_root)` — vanilla-mapper backup helpers.

**Tauri commands registered:** none directly. Commands in `commands/mods.rs` invoke these functions.

**Settings files written:** `<game_root>/S1Game/CookedPC/CompositePackageMapper.dat{,.clean,.vanilla-bak}` and `<game_root>/S1Game/CookedPC/<mod>.gpk{,.vanilla-bak}`.

**Ports / processes / network:** none. Pure CPU + local filesystem.

**Game memory locations patched / DLLs injected:** none. All modding is content-only (compiled GPKs; no runtime injection).

## Risks

- **Parses untrusted binary** — adversarial `.gpk` / `.tmm` footers reach this code. Mitigated by:
  - `parse_mod_file` bounds-checks every read; adversarial corpus in `parse_mod_file_rejects_non_tmm_gpks` + `tests/bogus_gpk_footer.rs`.
  - `extract_package_folder_name` sanity-caps length at 256 and refuses oversized / beyond-buffer reads (iter-229 unit tests).
  - `is_safe_gpk_container_filename` gates every write path against `..`, absolute paths, drive letters — no install or uninstall can escape `CookedPC/`.
- **Crypto keys embedded** — `KEY1` (16-byte permutation) + `KEY2` (`b"GeneratePackageMapper"`) are public values used by TERA's own tooling; no secrecy claim.
- **Mapper clobber hazard** — `.clean` backup is written once-and-only-once (`ensure_backup_copies_src_to_dst_not_reverse` pin). Without this, a second install could overwrite the vanilla baseline with a polluted mapper and break uninstall.
- **No outbound network** — downloads are handled in `services/mods/catalog.rs` and `services/mods/external_app.rs`; this module only touches the local filesystem.

## Tests

Launcher-side tests that exercise this unit:

- `src/services/mods/tmm.rs::tests::*` — 33+ in-module tests covering cipher, parser, merger, adversarial fuzz, legacy GPK integration.
- `tests/smoke.rs` — high-level startup shape.
- `tests/bogus_gpk_footer.rs` — adversarial corpus wiring guard.
- `tests/conflict_modal.rs` — `detect_conflicts` wiring.
- `tests/clean_recovery.rs` — `.clean` backup / recovery invariants.
- `tests/tampered_catalog.rs` — catalog-side errors bubble back correctly.

Upstream tests: n/a (in-house code).

Manual verification steps (not automatable):

- Install a real TMM-stamped mod on a live `S1Game/CookedPC/` and relaunch TERA — overlay GPKs must apply.
- Install a non-TMM drop-in (e.g. `foglio1024/flight-gauge.gpk`) via `add_mod_from_file` — the mapper entries ending in `.S1UI_Gauge` must rewrite.
- Uninstall both and verify `.clean` (TMM) / `.vanilla-bak` (legacy) restore the vanilla mapper + file.

## Verification

- [x] Tests pass in CI (1394 Rust passing @ iter 229)
- [x] Sandbox predicates pass (`is_safe_gpk_container_filename` at every FS boundary)
- [ ] Sha256 matches catalog.json (n/a — internal module, not shipped as separate artefact)
- [x] License declared (project licence covers this file)
- [x] Risks triaged and mitigated (see above)

## Sign-off

| Date | Verifier | Notes |
|------|----------|-------|
| 2026-04-20 | `loop/iter-229` | Initial draft. Risks enumerated; integration tests landed at iter 229 (install_legacy_gpk + uninstall round-trip). Awaiting human sign-off. |
