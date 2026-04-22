# Catalog lifecycle matrix

Generated from `external-mod-catalog/catalog.json` (`updated_at=2026-04-22T05:22:29Z`).

This matrix is the execution ledger for PRD criteria 3.3.1, 3.2.11-3.2.13, and the broader launcher mod-manager verification pass. Every shipped catalog entry must eventually have every column filled with `pass`, `blocked`, or a review doc reference.

## Column checklist semantics

Use the same meaning for every row:

- **install** — catalog entry downloads, verifies, and reaches an installed state without partial-write leftovers.
- **enable-intent** — launcher toggle or install default records intended enabled state without conflating that with runtime spawn.
- **launch-attach** — on game launch, external apps attach once and curated/GPK mods apply in the expected runtime path.
- **exit-cleanup** — game exit removes only transient runtime state; multi-client expectations remain respected.
- **uninstall** — removing the mod cleans launcher-side state and mod files without touching unrelated entries.
- **restore/cleanup** — mapper / backup / external-app cleanup restores the client or app state to the expected baseline.
- **review/doc** — evidence artifact for the row: passing test, audit doc, blocker review, or a specific follow-up document.

## Per-entry lifecycle checklist

For each catalog id, completion means all of the following questions have an explicit answer:

1. Does install succeed from the current catalog metadata and SHA-verified payload?
2. Does first enable/disable behave as pure intent where required?
3. Does game launch attach/apply the mod exactly once in the supported runtime path?
4. Does game exit leave the system in the correct post-run state?
5. Does uninstall remove the mod cleanly?
6. Does cleanup restore mapper / runtime state byte-for-byte or fail closed with recovery instructions?
7. Is there a review doc or test artifact proving the row's current state?

| id | kind | family/notes | install | enable-intent | launch-attach | exit-cleanup | uninstall | restore/cleanup | review/doc |
|---|---|---|---|---|---|---|---|---|---|
| `classicplus.shinra` | `external` | first-party | pending | pending | pending | pending | pending | pending | pending |
| `classicplus.tcc` | `external` | first-party | pending | pending | pending | pending | pending | pending | pending |
| `psina.postprocess` | `gpk` | effects | pending | pending | pending | pending | pending | pending | pending |
| `psina.gage-monster-hp` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `neowutran.s1ui-chat2` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `saltymonkey.message-clean` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `saltymonkey.message-centered` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `saltymonkey.overlaymap-fixed` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `saltymonkey.gageboss-extended` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `saltymonkey.characterwindow-clean` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `teralove.partywindowraidinfo` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `teralove.targetinfo` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-community-window` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-ep-window` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-equipment-combine` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-equipment-upgrade` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-guild-window` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-interaction-popup` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-inventory` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-minimap` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-paperdoll` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-parcelpost` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-production-create` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-production-list` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-servant-storage` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-skill-window` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-store-window` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-system-option` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-quickslot` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-trade-popup` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.restyle-warehouse` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.modern-ui-jewels-fix-inventory` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.modern-ui-jewels-fix-paperdoll` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.modern-ui-jewels-fix-icons` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.ui-remover-bosswindow` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.ui-remover-targetinfo` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.ui-remover-buffs` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.ui-remover-character` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.ui-remover-flight-gauge` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.ui-remover-lfg-board` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.ui-remover-lfg-member` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.ui-remover-party-window` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.ui-remover-raid-window` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.toolbox-gagebar-topscreen` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.toolbox-transparent-damage` | `gpk` | effects | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.toolbox-thinkblob` | `gpk` | fun | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.badgui-loader` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `foglio1024.s1ui-chat2-p75` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `taorelia.restyle-community` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `taorelia.restyle-guild` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `taorelia.restyle-inventory` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `taorelia.restyle-paperdoll` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `taorelia.restyle-warehouse` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `taorelia.restyle-interaction-popup` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `taorelia.restyle-production-create` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `owyn.fps-pack-postprocess` | `gpk` | performance | pending | pending | pending | pending | pending | pending | pending |
| `owyn.fps-pack-fx-enchant` | `gpk` | performance | pending | pending | pending | pending | pending | pending | pending |
| `owyn.fps-pack-fx-awaken-archer` | `gpk` | performance | pending | pending | pending | pending | pending | pending | pending |
| `owyn.fps-pack-fx-awaken-berserker` | `gpk` | performance | pending | pending | pending | pending | pending | pending | pending |
| `owyn.fps-pack-fx-awaken-sorcerer` | `gpk` | performance | pending | pending | pending | pending | pending | pending | pending |
| `owyn.fps-pack-fx-awaken-priest` | `gpk` | performance | pending | pending | pending | pending | pending | pending | pending |
| `owyn.fps-pack-fx-awaken-lancer` | `gpk` | performance | pending | pending | pending | pending | pending | pending | pending |
| `owyn.fps-pack-fx-awaken-slayer` | `gpk` | performance | pending | pending | pending | pending | pending | pending | pending |
| `owyn.fps-pack-fx-awaken-warrior` | `gpk` | performance | pending | pending | pending | pending | pending | pending | pending |
| `catannadev.pink-crosshair` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `catannadev.red-crosshair` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `catannadev.pink-hp-bar` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `catannadev.colored-hp-bar` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `catannadev.colored-hp-bar-2` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `catannadev.yellow-orange-hp-bar` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `catannadev.pink-loading-progress` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `catannadev.red-loading-progress` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `teralove.remove-artisan-icons` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `merusira.atlas-clean-onscreen-messages` | `gpk` | ui | pending | pending | pending | pending | pending | pending | pending |
| `pantypon.cheerleader-shorts` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `pantypon.elin-black-business-suit` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `pantypon.elin-strawberry-maid` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `pantypon.elin-pink-social-dress` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `pantypon.pastel-pora-elinu-uniform` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `pantypon.red-miko-costume` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `pantypon.white-castanica-demon` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `pantypon.white-pixie` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `pantypon.better-dyeable-flight-suit` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `pantypon.castanic-sleepy-running-togs` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `pantypon.charcoal-eldritch-no-skirt` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `pantypon.elin-sugar-alice` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `pantypon.elin-rose-gold-raincoat` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `pantypon.elin-english-lavender-raincoat` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `pantypon.pink-picnic-dress` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `taylorswiftmodding.bear-animal-mask` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `taylorswiftmodding.happy-kitty-mask` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `taylorswiftmodding.bunny-chu-mount` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `taylorswiftmodding.candied-dragon` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `taylorswiftmodding.bunny-coco` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `taylorswiftmodding.hanbok-bubblegum-princess` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `taylorswiftmodding.peaches-and-jeans` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `taylorswiftmodding.pikachu-raincoat` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `taylorswiftmodding.i-can-haz-backpack` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `taylorswiftmodding.maria-maria` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `taylorswiftmodding.pinky-kun` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
| `taylorswiftmodding.blacker-bow` | `gpk` | cosmetic | pending | pending | pending | pending | pending | pending | pending |
