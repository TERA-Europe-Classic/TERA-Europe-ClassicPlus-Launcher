# Wave A blocker matrix

This file consolidates the four initial Wave A reviews into one execution-facing
matrix so the next manifest/applier revision can target the minimum operation
set required to unblock real mods.

Source review docs:

- `flight-gauge-v100-review.md`
- `boss-window-v100-review.md`
- `message-window-cleaned-v100-review.md`
- `community-window-v100-review.md`

---

## Summary

All four reviewed Wave A seeds are currently blocked as
`blocked-unsupported-export-shape`, but they are **not blocked for the same
reason**. The launcher should not treat them as one generic "structural diff"
blob.

The minimum reviewed capability set still clusters into four concrete needs:

1. **export class replacement + payload replacement**
2. **export removal**
3. **import/name table mutation**
4. **export creation**

---

## Per-mod matrix

| Mod | Package | Export shape delta | Import delta | Minimum reviewed capability needed | Current blocker |
|---|---|---|---|---|---|
| UI Remover: Flight Gauge | `S1UI_ProgressBar.gpk` | 14 redirectors become `Texture2D` exports | `32 -> 4` | class+payload replacement **and** import/name edits | `blocked-unsupported-export-shape` |
| UI Remover: Boss Window | `S1UI_GageBoss.gpk` | 1 export removed (`GageBoss_I1C`) | same count | export removal | `blocked-unsupported-export-shape` |
| Message window — cleaned | `S1UI_Message.gpk` | `181 -> 1` exports | `366 -> 414` | bulk export removal **and** import/name edits | `blocked-unsupported-export-shape` |
| Restyle: Community Window | `S1UI_CommunityWindow.gpk` | `63 -> 64`; redirectors become `Texture2D`; +1 export | `132 -> 6` | export creation + class+payload replacement + import/name edits | `blocked-unsupported-export-shape` |

---

## Capability implications for v2

### 1. `replace_export_class_and_payload`

Needed immediately for:

- Flight Gauge
- Community Window

Why:

- both convert current vanilla `ObjectRedirector` exports into real `Texture2D`
  exports
- payload replacement alone is insufficient because the export class is not
  stable across vanilla vs modded packages

### 2. `remove_export`

Needed immediately for:

- Boss Window
- Message window — cleaned

Why:

- Boss Window is the cleanest first proof that reviewed export removal is not a
  theoretical need
- Message window proves removal must eventually scale beyond a one-off single
  export delete

### 3. `import_patches` / `name_patches`

Needed immediately for:

- Flight Gauge
- Message window — cleaned
- Community Window

Why:

- these are not just export-payload problems; the surrounding object graph
  changes materially
- import/name edits are the boundary between a safe curated patch and replaying
  an old whole-file package

### 4. export creation

Needed immediately for:

- Community Window

Why:

- this is the first reviewed Wave A seed that explicitly needs net-new export
  shape, not only mutations/removals

---

## Recommended implementation order

1. **Boss Window** as the first narrow reviewed capability proof
   - smallest review surface
   - demonstrates `remove_export`
2. **Flight Gauge** next
   - proves class+payload replacement plus import edits on a focused package
3. **Community Window** after that
   - expands to export creation on top of the previous capabilities
4. **Message window — cleaned** last of the seed set
   - largest structural delta
   - best stress test once the core machinery already exists

---

## Rules for unblocking

A Wave A seed only moves from blocker to candidate when all of the following are
true:

1. the needed operation(s) exist in `patch_manifest.rs`,
2. the applier can validate and execute them against current vanilla packages,
3. the converter can emit a reviewed candidate artifact for the package,
4. a regression test pins the operation with fixture-backed evidence,
5. the blocker note in the per-mod review doc is updated to reference the exact
   unblocking capability.

Until then, keep the seed rows blocked and documented rather than silently
falling back to whole-file replacement.
