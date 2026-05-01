# foglio1024.restyle-paperdoll — catalog candidate

Foglio's PaperDoll restyle ported to v100.02 (x64 client).

## Files in this folder

- `payloads/RestylePaperdoll.gpk` — the deployable artifact (8,339,580 bytes, sha256 `324d4bcdb1bc3647d67837f4a603f0ad92bda1ec489e8203c54e56005888d3c0`). TMM-format composite GPK containing foglio's modded SWF spliced into a v100 GFxMovieInfo wrapper. Installs via `tmm::install_gpk` like any other GPK mod.
- `catalog-entry.json` — proposed entry for the `external-mod-catalog` repo. Drop-in compatible with `CatalogEntry` schema in `teralaunch/src-tauri/src/services/mods/types.rs`. The `download_url` field is a placeholder — must be filled with the upload URL before merging.

## How to install (immediate, local-only)

1. Open the launcher → Mods tab
2. Use **"Add Mod From File"** → select `payloads/RestylePaperdoll.gpk`
3. Launcher SHA-verifies, installs via `tmm::install_gpk`, patches `CompositePackageMapper.dat`
4. Enable, launch TERA, open character profile (P)

## How to publish (catalog flow)

1. Upload `payloads/RestylePaperdoll.gpk` to a stable host (GitHub release on `tera-europe-classic/external-mod-catalog` or similar).
2. Replace the `download_url` field in `catalog-entry.json` with the public URL.
3. Append the entry to `external-mod-catalog/catalog.json` and bump `updated_at`.
4. Update `docs/mod-manager/CATALOG-LIFECYCLE-MATRIX.md` row `foglio1024.restyle-paperdoll` from `pending` → `pass` for the columns that have evidence.

## What this mod does and does not do

**Does:**
- Replaces the paperdoll widget SWF with foglio's restyle.
- Silhouettes adapt to v100's numeric race/sex coding (`PaperDoll_<race>_<sex>`).
- Filled equipment slots show foglio's clean blue-bordered design (engine renders foglio's slot frame from the SWF's references).
- Hover state on equipment slots shows foglio's design.
- Character info window has foglio's overall layout/typography.

**Does not:**
- Show empty equipment slots when not hovered. **This is foglio's design intent**, not a bug — foglio's wiki documents a community variant by Risenio ("visible clears") that exists specifically to add always-visible empty slot frames.
- Mod the locked tier-2 perimeter slots (those use the shared `S1UIRES_Component.SlotComponent_Impossible` texture, which foglio did not redesign).
- Replace the vanilla gothic backdrop (foglio's mod targets the SWF and slot atlases, not the backdrop atlas; the v100 backdrop is preserved as designed by NA/EU TERA).

## Source provenance

- Original mod: https://github.com/foglio1024/tera-restyle (PaperDoll/p79 directory)
- foglio's source GPKs are file_version 610 (x32). They cannot be byte-dropped into v100 (x64). The artifact in `payloads/` was built by:
  1. Extracting the mod.gfx from foglio's compiled `S1UI_PaperDoll79.gpk`
  2. Splicing it into the v100 vanilla `S1UI_PaperDoll.gpk` GFxMovieInfo wrapper
  3. Wrapping as a TMM composite GPK targeting `S1UI_PaperDoll.PaperDoll`

## Texture-extension notes (out of scope for this catalog entry)

During the porting investigation, additional texture overrides were tested as separate hypothesis tests:
- `S1UI_PaperDoll.PaperDoll_I147` (filled slot atlas, 512x512) — overridden with foglio's `InventoryWindow/p80+/SlotComponent_New.dds` (2048x256) → makes filled slot frames render foglio's clean design
- `S1UI_PaperDoll.PaperDoll_I168` (hover slot atlas, 512x512) — same override → makes hover-state frame render foglio's design

These overrides PRODUCE visible improvements but are CUSTOM extensions built on top of foglio's mod — not part of foglio's actual release. They're not bundled into the catalog GPK because foglio never shipped them. If a "PaperDoll Restyle Plus" variant is desired, the I147/I168 overrides should be packaged as a separate catalog entry (e.g. `tera-europe-classic.restyle-paperdoll-plus`) so attribution remains clean.

For reference, the texture-name → role mapping for v100's paperdoll is preserved in user memory at `project_paperdoll_v100_architecture.md`.
