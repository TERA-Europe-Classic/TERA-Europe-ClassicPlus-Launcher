# Unit Audit — `psina.postprocess` (GPK exemplar)

**Category:** gpk
**Status:** draft
**Last verified:** 2026-04-20 by Lukas
**Iter:** 229

## Provenance

- **Upstream source:** per `catalog.json` `source_url` for
  `psina.postprocess`
- **License:** per `catalog.json` `license` (verify matches upstream)
- **Obfuscated binaries?** No — GPK is a binary data container, not
  executable code. UE3 loads it as game asset data.
- **Version pinned:** per `catalog.json`
- **Download URL:** per `catalog.json`
- **SHA-256:** per `catalog.json`

## Public surface

- File: `<app-data>/mods/gpk/psina.postprocess.gpk` (1 file)
- Mapper patches: on enable, `tmm::install_gpk` patches
  `CompositePackageMapper.dat` to swap the affected object paths.
  Vanilla mapper backed up as `.clean` before first patch (see
  `clean_recovery.rs` wiring).
- No process spawn. No network. No settings written outside the
  registry.json entry.

## Risks

- **Binary parser surface:** `tmm::parse_mod_file` walks the GPK
  footer; a malformed GPK could crash the parser. Mitigation: four
  fail-closed gates in `install_gpk` before filesystem touch (see
  `bogus_gpk_footer.rs::install_gpk_has_four_fail_closed_gates`).
- **Container sandbox:** GPK's own container-filename field is
  sandboxed via `tmm::is_safe_gpk_container_filename` before deploy
  (path-traversal guard).
- **Mapper corruption:** mapper-patch operation is transactional via
  `.clean` backup; worst case a regression is reversible via the
  Recovery button (`recover_clean_mapper` Tauri command).

## Tests

- `teralaunch/src-tauri/tests/bogus_gpk_footer.rs` — four fail-closed
  gates on the install path.
- `teralaunch/src-tauri/tests/tampered_catalog.rs` — SHA mismatch →
  finalize_error wiring.
- `teralaunch/src-tauri/tests/clean_recovery.rs` — `.clean` backup +
  recover-missing wiring.
- `teralaunch/src-tauri/tests/conflict_modal.rs` — preview-only
  predicate.

## Verification

- [x] Install-path hash gate covered by `tampered_catalog.rs`
- [x] Container sandbox covered by `bogus_gpk_footer.rs`
- [x] Mapper patch + `.clean` backup covered by `clean_recovery.rs`
- [ ] Visual regression check post-install (manual — compare
  in-game postprocess look before/after enable)

## Sign-off

| Date | Verifier | Notes |
|------|----------|-------|
| 2026-04-20 | Lukas | Draft exemplar; same template applies to the other 98 GPK catalog entries. |
