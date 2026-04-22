# Wave C blockers — cosmetic / model families

This document classifies the currently known high-risk cosmetic/model entries so
Wave C can be approached deliberately instead of being mixed into the UI/effect
pipeline.

Source inputs:

- `external-mod-catalog/catalog.json` snapshot `2026-04-22T05:22:29Z`
- current family queue in `docs/mod-manager/WAVE-QUEUE.md`
- `docs/mod-manager/GPK-PATCH-MIGRATION.md`

---

## Wave C rule

No Wave C mod should enter the curated-manifest path until all of the following
are known for that specific package:

1. exact vanilla target package path,
2. whether the mod is a pure asset replacement vs. skeletal/model graph rewrite,
3. whether disable/uninstall can restore the live client byte-for-byte,
4. whether multi-mod coexistence is even meaningful for that package.

These are **fail-closed** candidates by default.

---

## Known direct cosmetic-model entries

| Mod id | Name | Provisional classification | Why |
|---|---|---|---|
| `pantypon.red-miko-costume` | Red Miko Costume | `high-risk-manual` | costume/model replacement; likely package-specific art asset graph |
| `taylorswiftmodding.bunny-chu-mount` | Bunny Chu | `high-risk-manual` | mount/vehicle asset; likely multi-package dependency and high drift risk |

---

## Likely hidden Wave C entries currently sitting in `misc`

These should be reclassified before any conversion planning starts:

- `pantypon.cheerleader-shorts`
- `pantypon.elin-black-business-suit`
- `pantypon.elin-strawberry-maid`
- `pantypon.elin-pink-social-dress`
- `pantypon.pastel-pora-elinu-uniform`
- `pantypon.white-castanica-demon`
- `pantypon.white-pixie`
- `pantypon.better-dyeable-flight-suit`

### Provisional classification

All of the above are currently:

- `needs-target-package-confirmation`
- `needs-manual-redistribution/provenance-check`
- `high-risk-manual`

Because we do **not** yet know whether they are:

- single-package texture swaps,
- multi-package mesh/material rewrites,
- skeleton/animation dependent,
- or package-path outliers that do not belong in the main S1UI-style flow.

---

## Special-case package candidates outside standard UI path

These are not obviously part of the normal UI patch-manifest rollout and should
remain explicitly flagged:

- `NPC_DisPenser.gpk` lineage / dispenser NPC customization
  - likely world/NPC package path outside the standard UI families
  - classification: `bespoke-path-manual-review`

---

## Required blocker labels per Wave C mod

Each Wave C mod should eventually carry one or more of:

- `blocked-needs-target-package-confirmation`
- `blocked-needs-reference-vanilla`
- `blocked-high-risk-model-shape`
- `blocked-bespoke-package-path`
- `manual-review-required`

Do not collapse all Wave C work under `blocked-unsupported-export-shape`; that
label is too launcher-centric and hides the packaging/provenance risks that make
Wave C dangerous.

---

## Recommended next action

1. Reclassify the current `pantypon.*` entries out of `misc` in the queue docs.
2. Identify exact package targets for one costume entry and one mount entry.
3. Only then decide whether any Wave C entry is even eligible for a curated
   patch-manifest experiment, or whether the whole family should remain manual
   / deferred for the current milestone.
