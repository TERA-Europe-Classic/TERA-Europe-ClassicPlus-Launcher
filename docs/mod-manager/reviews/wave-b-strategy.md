# Wave B strategy — effects / performance families

This document assigns a provisional conversion strategy to the current Wave B
catalog entries so effect/performance work can proceed package-by-package rather
than as an undifferentiated "FPS pack" backlog.

Source inputs:

- `external-mod-catalog/catalog.json` snapshot `2026-04-22T05:22:29Z`
- `docs/mod-manager/GPK-PATCH-MIGRATION.md`
- current family queue in `docs/mod-manager/WAVE-QUEUE.md`

---

## Family: `fx-cleanup`

### Current queue

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

### Provisional per-mod strategy

| Mod id | Primary strategy | Why | Expected risk |
|---|---|---|---|
| `psina.postprocess` | `effect-replacement` | likely one-package post-process cleanup and the smallest Wave B probe | medium |
| `owyn.fps-pack-postprocess` | `effect-replacement` | same family as postprocess cleanup; likely best comparison pair with `psina.postprocess` | medium |
| `owyn.fps-pack-fx-enchant` | `effect-replacement` | effect-specific asset suppression, probably package-local | medium |
| `owyn.fps-pack-fx-awaken-archer` | `effect-replacement` | lineage batch; likely same structural pattern across class variants | medium |
| `owyn.fps-pack-fx-awaken-berserker` | `effect-replacement` | same as above | medium |
| `owyn.fps-pack-fx-awaken-sorcerer` | `effect-replacement` | same as above | medium |
| `owyn.fps-pack-fx-awaken-priest` | `effect-replacement` | same as above | medium |
| `owyn.fps-pack-fx-awaken-lancer` | `effect-replacement` | same as above | medium |
| `owyn.fps-pack-fx-awaken-slayer` | `effect-replacement` | same as above | medium |
| `owyn.fps-pack-fx-awaken-warrior` | `effect-replacement` | same as above | medium |

### Execution rules

1. Review `psina.postprocess` first to establish the baseline package shape.
2. Review `owyn.fps-pack-postprocess` second to test whether the same package
   admits a repeatable manifest pattern across different authors.
3. Treat the seven `fx-awaken-*` entries as one lineage batch:
   - same source family,
   - likely same asset category,
   - should share review notes if the package structure matches.
4. If any one class-specific Awakening package diverges structurally, split it
   out into its own blocker doc instead of pretending the whole batch is uniform.

### Current blocker assumptions

Wave B should stay blocked until each representative package is confirmed as one
of:

- payload-only replacement,
- class+payload replacement,
- export add/remove,
- or high-risk effect graph rewrite.

Do not assume effect packs are easier just because they are visually simpler to
describe.

---

## Family: `effects`

The current catalog overlaps heavily with `fx-cleanup`, so there is not yet a
distinct executable queue here.

### Rule

Before any dedicated `effects` work begins, each `category = effects` entry must
be classified into exactly one of:

1. `fx-cleanup` lineage batch,
2. true standalone `effects` package,
3. `misc` / bespoke effect one-off.

Until that reclassification is complete, `effects` remains a taxonomy bucket,
not an execution bucket.

---

## Recommended next action

1. Run a real package review for `psina.postprocess`.
2. Compare it with `owyn.fps-pack-postprocess`.
3. Use the first confirmed result to decide whether the awakening packs can be
   reviewed as a single repeated pattern or must split into individual blocker
   docs.
