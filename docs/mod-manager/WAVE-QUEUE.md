# GPK Review Queue

This queue translates the migration program into concrete review order so the
remaining GPK work can be executed family-by-family instead of as one vague
"convert all mods" blob.

Source inputs:

- `docs/mod-manager/GPK-PATCH-MIGRATION.md`
- `docs/PRD/mod-manager-perfection.md`
- `external-mod-catalog/catalog.json` snapshot `2026-04-22T05:22:29Z`

The family buckets below are **execution heuristics**, not final public schema.
They are meant to drive review order and workload slicing.

---

## Priority order

1. **Wave A — UI families**
   - highest player value
   - best chance of structured export-level patching
   - should produce the first curated patch-manifest artifacts
2. **Wave B — effects / performance**
   - smaller surface area
   - good second candidate for curated manifests once Wave A operations exist
3. **Wave C — cosmetic / model / bespoke**
   - highest drift risk
   - most likely to remain manual-review / blocked longer

---

## Wave A — UI families

### A1. `ui-remover`

**Status:** active conversion family

**Already reviewed / seeded**

- `foglio1024.ui-remover-flight-gauge`
- `foglio1024.ui-remover-bosswindow`
- `saltymonkey.message-clean`

**Next review order**

1. `foglio1024.ui-remover-character`
2. `foglio1024.ui-remover-buffs`
3. `foglio1024.ui-remover-party-window`
4. `foglio1024.ui-remover-lfg-board`
5. `foglio1024.ui-remover-lfg-member`
6. `saltymonkey.characterwindow-clean`
7. `saltymonkey.message-centered`
8. `saltymonkey.gageboss-extended`
9. `teralove.remove-artisan-icons`
10. `merusira.atlas-clean-onscreen-messages`

**Why this order**

- overlaps strongly with already-reviewed package families
- likely exercises the same v2 operations (`remove_export`, class swap, import edits)
- high user-visible payoff with relatively low lore burden

### A2. `restyle`

**Status:** active conversion family

**Already reviewed / seeded**

- `foglio1024.restyle-community-window`

**Next review order**

1. `foglio1024.restyle-inventory`
2. `foglio1024.restyle-paperdoll`
3. `foglio1024.restyle-minimap`
4. `foglio1024.restyle-skill-window`
5. `foglio1024.restyle-system-option`
6. `foglio1024.restyle-guild-window`
7. `foglio1024.restyle-store-window`
8. `foglio1024.restyle-production-list`
9. `foglio1024.restyle-production-create`
10. `foglio1024.restyle-parcelpost`
11. `foglio1024.restyle-servant-storage`
12. `foglio1024.restyle-equipment-combine`
13. `foglio1024.restyle-equipment-upgrade`
14. `foglio1024.restyle-trade-popup`
15. `foglio1024.restyle-interaction-popup`
16. `foglio1024.restyle-quickslot`
17. `foglio1024.restyle-ep-window`
18. `teralove.targetinfo`
19. `teralove.partywindowraidinfo`

**Why this order**

- prioritizes common always-visible UI packages first
- keeps the queue inside a mostly consistent source lineage (`foglio1024` restyle)
- defers the more bespoke `teralove` variants until the shared restyle pattern is understood

### A3. `ui-layout`

**Status:** should be reviewed after `ui-remover` and `restyle`

**Queue**

1. `neowutran.s1ui-chat2`
2. `foglio1024.s1ui-chat2-p75`
3. `saltymonkey.overlaymap-fixed`

**Notes**

- likely mixes true layout changes with brittle legacy fixes
- may require dedicated handling for chat-specific imports / overlay map drift

### A4. `ui-recolor`

**Status:** lower-risk candidate family once v2 payload/class operations are proven

**Queue**

1. `psina.gage-monster-hp`
2. `foglio1024.modern-ui-jewels-fix-icons`
3. `foglio1024.toolbox-transparent-damage`

**Notes**

- likely best family for proving simpler texture/material-only curated patches
- good target for the first "easy win" after the initial reviewed seed

### A5. `misc`

**Status:** hold as manual-review pool until A1-A4 produce stable patterns

**Sub-buckets to split later**

- crosshair / loading / HP-bar color tweaks
- badGUI / toolbox client one-offs
- mixed UI overlays
- cosmetics accidentally captured by current heuristics

**Rule**

Do not start broad misc conversion until each entry is reclassified into one of:

- true UI patch candidate
- effect/performance candidate
- cosmetic-model candidate
- permanently manual-only

---

## Wave B — effects / performance

### B1. `fx-cleanup`

**Queue**

1. `psina.postprocess`
2. `owyn.fps-pack-postprocess`
3. `owyn.fps-pack-fx-enchant`
4. `owyn.fps-pack-fx-awaken-archer`
5. `owyn.fps-pack-fx-awaken-berserker`
6. `owyn.fps-pack-fx-awaken-sorcerer`
7. `owyn.fps-pack-fx-awaken-priest`
8. `owyn.fps-pack-fx-awaken-lancer`
9. `owyn.fps-pack-fx-awaken-slayer`
10. `owyn.fps-pack-fx-awaken-warrior`

**Notes**

- likely shares repeated package/export patterns
- should be processed in one lineage batch to maximize pattern reuse

### B2. `effects`

**Queue**

- remaining entries explicitly tagged `category = effects` that do not fall into `fx-cleanup`

**Notes**

- catalog currently overlaps with `fx-cleanup`; reclassification is required before execution

---

## Wave C — cosmetic / model / bespoke

### C1. `cosmetic-model`

**Known queue (current heuristic hits)**

1. `pantypon.red-miko-costume`
2. `taylorswiftmodding.bunny-chu-mount`

**Likely hidden inside `misc` and needs reclassification**

- `pantypon.cheerleader-shorts`
- `pantypon.elin-black-business-suit`
- `pantypon.elin-strawberry-maid`
- `pantypon.elin-pink-social-dress`
- `pantypon.pastel-pora-elinu-uniform`
- `pantypon.white-castanica-demon`
- `pantypon.white-pixie`
- `pantypon.better-dyeable-flight-suit`

**Rule**

These stay high-risk-manual until we prove:

- current vanilla package ownership is stable on v100.02
- object/class drift is understood
- enable/disable/uninstall can be fail-closed without damaging the live client

---

## Execution rules

For every queued mod, completion means all of:

1. family assigned and recorded,
2. target package confirmed,
3. review doc exists or is updated,
4. blocker or supported operation is identified,
5. launcher-side applier requirements are mapped,
6. lifecycle verification slot exists in the catalog matrix.

No mod graduates from queue to "done" just because a package was eyeballed once.

---

## Immediate next actions

1. finish the four reviewed Wave A seeds by turning their blocker notes into concrete v2 applier requirements,
2. review `foglio1024.ui-remover-character` as the next `ui-remover` representative,
3. review `foglio1024.restyle-inventory` as the next `restyle` representative,
4. split the current `misc` pool into true UI / cosmetic / effect buckets before any conversion work starts there,
5. build a catalog lifecycle matrix that references this queue so every mod eventually gets an install/enable/uninstall verification slot.
