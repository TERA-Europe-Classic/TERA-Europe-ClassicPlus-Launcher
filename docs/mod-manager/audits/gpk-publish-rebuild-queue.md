# GPK Publish/Rebuild Queue

- Source catalog: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\external-mod-catalog\catalog.json`
- Full per-mod matrix: `docs/mod-manager/audits/gpk-catalog-audit.md`
- PaperDoll resource blocker: `docs/mod-manager/audits/foglio-paperdoll-resource-blocker.md`
- Artifact cache used for header proof: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-mod-cache`

## Current gate result

Every catalog GPK now has a row in the audit matrix. The launcher must treat the downloaded artifact header as authoritative over catalog labels.

| Queue | Count | Release rule |
|---|---:|---|
| Install candidate, binary diff still needs package/mapper dry-run | 32 | May install only if patch derivation succeeds against v100.02 vanilla and the smoke test passes. |
| Structural x64 rebuild/manifest required | 2 | Must publish curated v100.02 x64 artifact/manifest before user-facing enable. |
| Legacy x32 rebuild/publish required | 132 | Must not install current bytes; rebuild old intent/assets onto v100.02 x64 and publish replacement SHA/URL. |

## Launcher install policy

1. Download artifact, verify SHA-256, then parse the GPK header.
2. Refuse `FileVersion < 0x381` before game-path or mapper work; those rows enter the rebuild queue.
3. Use catalog `gpk_files` as the target package when exactly one target is declared.
4. Refuse multi-`gpk_files` rows until they are split into one artifact per target or given real multi-GPK install support.
5. For composite packages, use launcher patch manifests against v100.02 vanilla bytes and apply them by replacing the resolved composite container slice in place; standalone mapper redirection is unsafe for multi-slice packages such as PaperDoll.

## Rebuild/publish workflow

For each `publish-x64-rebuild-required` row in `gpk-catalog-audit.md`:

1. Open the legacy artifact as the intent/source package.
2. Locate the matching v100.02 package through `PkgMapper.dat` + `CompositePackageMapper.dat`.
3. Reapply only the changed assets/properties/export intent onto the v100.02 x64 package.
4. Save as `FileVersion 897`, with composite `MOD:<objectPath>` metadata where applicable.
5. Reparse the saved artifact and verify no dangling imports/name/export references.
6. Publish the artifact, update catalog URL/SHA/size/`compatible_arch: "x64"`, and rerun the audit.
7. Install through the launcher, disable, re-enable, then run the matching smoke test below.

## Rebuild proofs

### `foglio1024.restyle-inventory`

- Source x32 artifact: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-mod-cache\69d4f66826fbf8611cea93103f0029401480f1cfb6a304afacc02b9f4ef3eda7.gpk`
- Vanilla x64 slice: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-rebuild\vanilla\S1UI_InventoryWindow.Inventory_dup.gpk`
- Publish candidate: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-rebuild\publish\foglio1024.restyle-inventory.S1UI_InventoryWindow.gpk`
- Target mapper object: `c7a706fb_1afec5d7_1a45.Inventory_dup`
- Catalog install target: `Inventory_dup.gpk` (the generic `S1UI_InventoryWindow.gpk` package name maps to multiple vanilla composite byte ranges)
- Header: `FileVersion 897`, `LicenseVersion 17`, LZO-compressed
- Size: `1174379`
- SHA-256: `36794fd54ac02da163ab62bb58c1fcd73ae5b293632d75288b7b201ef36a1215`
- Splice proof: `splice-x32-payloads` changed only `Inventory_dup` (`Core.GFxUI.GFxMovieInfo`) via `--gfx-swap`; name/import/export counts stayed `90/7/53`, so patch derivation remains within the supported replace-payload shape.
- Round-trip proof: `decompress-only` read the final compressed artifact and produced a valid `FileVersion 897` package.
- Remaining gate: publish the candidate to a trusted GitHub artifact URL, update the external catalog URL/SHA/size/`compatible_arch: "x64"`/`gpk_files`, then install/disable/re-enable and smoke-test the Inventory window in game.

### `foglio1024.restyle-paperdoll`

- Source x32 artifact: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-mod-cache\7f890f4b3fff3f7b12f617063c909f4868b8e648d26b93abcc21c110f656c4e1.gpk`
- Vanilla x64 slice: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-rebuild\vanilla\S1UI_PaperDoll.PaperDoll_dup.gpk`
- Publish candidate: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-rebuild\publish\foglio1024.restyle-paperdoll.S1UI_PaperDoll.gpk`
- Target mapper object: `c7a706fb_268926b3_1ddcb.PaperDoll_dup`
- Catalog install target: `PaperDoll_dup.gpk` (the generic `S1UI_PaperDoll` package name maps to multiple vanilla composite byte ranges)
- Header: `FileVersion 897`, `LicenseVersion 17`, LZO-compressed
- Size: `1481594`
- SHA-256: `f26b59c3e76c989364d5d178541e184031101cc281400775706e2fed7aadddbd`
- Splice proof: `splice-x32-payloads` changed only `PaperDoll_dup` (`Core.GFxUI.GFxMovieInfo`) via `--rename PaperDoll=PaperDoll_dup --only-class Core.GFxUI.GFxMovieInfo --gfx-swap`; name/import/export counts stayed `180/7/142`, and the GFx wrapper now preserves the vanilla 552-byte native tail after the swapped embedded movie, so patch derivation remains within the supported replace-payload shape.
- Round-trip proof: `decompress-only` read the final compressed artifact and produced a valid `FileVersion 897` package.
- Remaining gate: publish the GFx-only candidate to a trusted GitHub artifact URL, then install/disable/re-enable and smoke-test the PaperDoll/character equipment window in game. Shared `S1UIRES_Skin` / `S1UIRES_Component` resource restoration is blocked until an x64-native `Texture2D` serializer/validator exists; see `foglio-paperdoll-resource-blocker.md`.

### `foglio1024.restyle-minimap`

- Source x32 artifact: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-mod-cache\e4afc7afdb125a9ed1afee8a2202e1680356e45b2dda4f2576dac7dad6b667f9.gpk`
- Vanilla x64 slice: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-rebuild\vanilla\S1UI_MiniMap.MiniMap_dup.gpk`
- Publish candidate: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-rebuild\publish\foglio1024.restyle-minimap.S1UI_MiniMap.gpk`
- Target mapper object: `c7a706fb_238e51ef_1dc38.MiniMap_dup`
- Catalog install target: `MiniMap_dup.gpk` (the generic `S1UI_MiniMap` package name maps to multiple vanilla composite byte ranges)
- Header: `FileVersion 897`, `LicenseVersion 17`, LZO-compressed
- Size: `980008`
- SHA-256: `8bd6c6b730c1c07670a8fbaf3897cbd3b214a9b059797bea2d6bc662988290a6`
- Splice proof: `splice-x32-payloads` changed only `MiniMap_dup` (`Core.GFxUI.GFxMovieInfo`) via `--rename MiniMap=MiniMap_dup --only-class Core.GFxUI.GFxMovieInfo --gfx-swap`; name/import/export counts stayed `234/7/194`, so patch derivation remains within the supported replace-payload shape. Texture-only x32 exports not present in v100.02 were skipped by class filter.
- Round-trip proof: `decompress-only` read the final compressed artifact and produced a valid `FileVersion 897` package.
- Remaining gate: publish the candidate to a trusted GitHub artifact URL, then install/disable/re-enable and smoke-test the minimap in game.

### `deathdefying.ui-remover-quest-tracker`

- Source x32 artifact: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-mod-cache\f269f57e7889046a386ad1a6029738a30ea777402ae12ab3860fe380d018fd2f.gpk`
- Vanilla x64 slice: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-rebuild\vanilla\S1UI_ProgressBar.ProgressBar_dup.gpk`
- Publish candidate: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-rebuild\publish\deathdefying.ui-remover-quest-tracker.S1UI_ProgressBar.gpk`
- Target mapper object: `c7a706fb_4c3e9e9c_1df33.ProgressBar_dup`
- Catalog install target: `ProgressBar_dup.gpk`
- Header: `FileVersion 897`, `LicenseVersion 17`, LZO-compressed
- Size: `114301`
- SHA-256: `2c1a92657484c830ed89a161ee4719bcfe8bf50a9ad5d2956663371b3b0f6f5a`
- Root-cause proof: the upstream URL is named `S1UI_QuestTaskInfo.gpk`, but the x32 source exports `ProgressBar` / `S1UI_ProgressBar` objects; splicing against `QuestTaskInfo_dup` correctly no-opped.
- Splice proof: `splice-x32-payloads` changed only `ProgressBar_dup` (`Core.GFxUI.GFxMovieInfo`) via `--rename ProgressBar=ProgressBar_dup --only-class Core.GFxUI.GFxMovieInfo --gfx-swap`; name/import/export counts stayed `53/7/17`, so patch derivation remains within the supported replace-payload shape. Texture exports were intentionally skipped by class filter.
- Round-trip proof: `decompress-only` read the final compressed artifact and produced a valid `FileVersion 897` package.
- Remaining gate: publish the catalog branch so the raw URL becomes live, then install/disable/re-enable and smoke-test the hidden quest tracker in game.

### `foglio1024.badgui-loader`

- Source x32 artifact: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-mod-cache\ec470822e9b65924b3f8fc168c6aa5c9ac0cb0187f4962176d0296f3ed08ab85.gpk`
- Vanilla x64 slice: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-rebuild\vanilla\S1UI_UpdateNotification.UpdateNotification_dup.gpk`
- Publish candidate: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-rebuild\publish\foglio1024.badgui-loader.S1UI_UpdateNotification.gpk`
- Target mapper object: `c7a706fb_c4a999af_1e25f.UpdateNotification_dup`
- Catalog install target: `UpdateNotification_dup.gpk`
- Header: `FileVersion 897`, `LicenseVersion 17`, LZO-compressed
- Size: `692996`
- SHA-256: `b3becc997cad30b23911ccec0c693f91c16cef1470149b1c7e6b375f951b5a4f`
- Splice proof: `splice-x32-payloads` changed only `UpdateNotification_dup` (`Core.GFxUI.GFxMovieInfo`) via `--rename UpdateNotification=UpdateNotification_dup --only-class Core.GFxUI.GFxMovieInfo --gfx-swap`; name/import/export counts stayed `82/7/45`, so patch derivation remains within the supported replace-payload shape. The x32 `n9` texture export was intentionally skipped by class filter.
- Round-trip proof: `decompress-only` read the final compressed artifact and produced a valid `FileVersion 897` package.
- Remaining gate: publish the catalog branch so the raw URL becomes live, then install/disable/re-enable and smoke-test the update notification / badGUI loader surface in game.

### `foglio1024.modern-ui-ep-window`

- Source x32 artifact: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-mod-cache\622a400570ff60ab9050acf413d123c156550a4a23f3edbce16348a89b392ff4.gpk`
- Vanilla x64 slice: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-rebuild\vanilla\S1UI_EpWindow.EpWindow_dup.gpk`
- Publish candidate: `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\.gpk-rebuild\publish\foglio1024.modern-ui-ep-window.S1UI_EpWindow.gpk`
- Target mapper object: `c7a706fb_9fd5e48e_1d8bf.EpWindow_dup`
- Catalog install target: `EpWindow_dup.gpk`
- Header: `FileVersion 897`, `LicenseVersion 17`, LZO-compressed
- Size: `1259151`
- SHA-256: `bfd20ead1ce0d10142c96cafbb263ee7f3f108d05186516e8012f62adf07acc6`
- Splice proof: `splice-x32-payloads` changed only `EpWindow_dup` (`Core.GFxUI.GFxMovieInfo`) via `--rename EpWindow=EpWindow_dup --only-class Core.GFxUI.GFxMovieInfo --gfx-swap`; name/import/export counts stayed `156/7/117`, so patch derivation remains within the supported replace-payload shape. Texture-only x32 exports were intentionally skipped by class filter.
- Round-trip proof: `decompress-only` read the final compressed artifact and produced a valid `FileVersion 897` package.
- Remaining gate: publish the catalog branch so the raw URL becomes live, then install/disable/re-enable and smoke-test the EP window in game.

## Smoke test groups

Use the per-row smoke text in `gpk-catalog-audit.md` as the exact mod-level script. These groups keep manual testing bounded:

- **Flight/resource gauges**: mount a flying mount, spend and restore flight stamina, confirm the advertised gauge change and no crash.
- **Boss gauges**: enter a boss/training encounter that shows boss HP, confirm the boss-gauge change and no crash.
- **Chat UI**: open chat, send/receive a message, switch tabs, confirm the style/layout and input remain intact.
- **General UI windows**: open the named UI surface from the package/mod title and confirm the catalog description is visibly true.
- **Costumes/models/mounts/pets/weapons**: preview or equip the affected item/model, rotate camera, trigger idle/movement/animation, and check textures/materials.
- **FX/performance packs**: trigger the affected class skill/effect or combat context and confirm the intended reduction/change without missing assets.
- **Standalone added packages**: enable/disable/re-enable and verify the file is created only under the approved CookedPC path and removed/restored on disable.

## Known high-risk rows

- `foglio1024.ui-remover-flight-gauge`: current artifact header is x32; prior payload-only attempts no-op/crash because the mod changes `ObjectRedirector` exports into `Texture2D` and needs import/export graph rebuild.
- `foglio1024.ui-remover-bosswindow`: x64 structural row; needs boss gauge payload replacement plus redirector export removal validated against v100.02 vanilla.
- `saltymonkey.message-clean`: x64 structural row; needs object graph/import/name/export validation before publish.
- Multi-target rows such as `psina.postprocess`, `teralove.partywindowraidinfo`, and `teralove.targetinfo`: must be split or receive true multi-GPK artifact support; URL filename is not a safe install target.
