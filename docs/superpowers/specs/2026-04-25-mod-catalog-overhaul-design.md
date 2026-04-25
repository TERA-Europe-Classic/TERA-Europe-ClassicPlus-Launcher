# Mod Catalog Overhaul — Design Spec

**Date:** 2026-04-25
**Status:** Approved
**Author:** Lukas (TNC97) + Claude

## Problem

The launcher's mod manager ships 101 mods today. Users can't tell what a
mod actually does or what it looks like:

- 0/101 entries have `icon_url` populated
- 0/101 entries have `screenshots`
- `long_description` averages 150 chars (basically a re-phrased one-liner)
- The detail panel renders title + author + version + a paragraph + a
  hidden source link — no hero image, no before/after for restyles, no
  tags, no compatibility info
- Many published TERA Online client-side GPK mods are not yet in the
  catalog at all (forum/Discord/Tumblr-distributed mods, regional
  modders, niche cosmetics)

Result: a Tumblr post for a community-made Elin costume gives a better
preview than our launcher does. That's the bar to clear.

## Non-goals

- No new mod *types* — still only `external` (separate-process apps) and
  `gpk` (composite-package patches).
- No reviews, ratings, comments, or telemetry server. (Schema reserves
  `download_count` as a stub but we don't fill it.)
- No catalog-side moderation tooling. The catalog stays a single
  hand-edited JSON in `TERA-Europe-Classic/external-mod-catalog`.
- No translation pipeline for mod content — `long_description` and
  `tagline` are author-supplied; the launcher's i18n strings are
  separate and out of scope here.

## Schema additions

### `CatalogEntry` (Rust + JSON)

All new fields optional. Existing entries stay valid as-is.

| Field | Type | Use |
| --- | --- | --- |
| `tagline` | `Option<String>` | One-line punchy hook ≤90 chars; row cards display this. Distinct from `short_description` which is dense. |
| `featured_image` | `Option<String>` (URL) | Hero image at top of detail panel. 16:9, ≥1200w preferred. For restyles, this is the "after" shot. |
| `before_image` | `Option<String>` (URL) | Restyles only — paired "before" shot for side-by-side compare. Paired-display only fires when both `before_image` and `featured_image` exist. |
| `tags` | `Vec<String>` | e.g. `["minimap","quality-of-life","foglio"]`. Searchable; row cards show first 2. Distinct from `category`. |
| `gpk_files` | `Vec<String>` | Files this mod replaces, e.g. `["S1UI_Chat2.gpk"]`. Detail panel "Details" row, helps power users diagnose conflicts. |
| `compatibility_notes` | `Option<String>` | Markdown. "Conflicts with X", "Broken on patch Y". Rendered in a yellow-tinted callout above the screenshot strip. |
| `last_verified_patch` | `Option<String>` | e.g. `"patch 113"`. Last patch the mod was confirmed working on; appears in Details row. |
| `download_count` | `Option<u64>` | Stub for future telemetry; UI does NOT render it now. |

### `ModEntry` mirrors

`ModEntry` already mirrors the catalog fields it needs. Add the same
fields with the same `#[serde(skip_serializing_if = "Option::is_none")]`
treatment so the registry round-trips cleanly when a user has the mod
installed and we later add new fields to the catalog.

### `long_description` rendering

Existing field stays as a plain string but the renderer becomes
markdown-aware. Backwards compatible: existing plain-text descriptions
render unchanged. New entries can use `**bold**`, `*italic*`, `[text](url)`,
`- list`, `1. list`, paragraph breaks, fenced code, headings up to `###`.
HTML in source is escaped — markdown is the only formatting path.

In-house renderer; no third-party dep. ~80–120 lines of JS. Trusted
subset only:
- escape `< > & " '` first, then run regex-driven inline + block passes
- never produce `<script>`, `<iframe>`, `style=`, `on*=`, `javascript:` URLs
- images are `<img>` only when the URL is `http(s)://` or starts with `data:image/`
- nothing else

Why not pull in a lib: `marked` is ~30 KB minified plus a sanitizer
dependency; the subset above is a fraction of that and the catalog
content is authored by us so we don't need to defend against arbitrary
adversarial markdown.

## Detail panel redesign

```
┌─────────────────────────────────────────────────────────┐
│ [HERO IMAGE — featured_image, full-bleed, 16:9]         │
│                                                          │
├─────────────────────────────────────────────────────────┤
│ [icon] MOD NAME                                  [×]    │
│         by Author · v1.2.3 · UI · 2.1 MB                │
│         [tag1] [tag2] [tag3]                            │
├─────────────────────────────────────────────────────────┤
│ [PRIMARY ACTION]    [Open settings folder]    [Source ↗]│
├─────────────────────────────────────────────────────────┤
│  ⚠ Compatibility (only if compatibility_notes present)   │
│    <markdown rendered>                                   │
├─────────────────────────────────────────────────────────┤
│ ABOUT                                                    │
│ <markdown long_description>                              │
├─────────────────────────────────────────────────────────┤
│ BEFORE / AFTER  (only if before_image present)           │
│ ┌─────────┐  ┌─────────┐                                 │
│ │ before  │  │ after   │   ↔  click to flip              │
│ └─────────┘  └─────────┘                                 │
├─────────────────────────────────────────────────────────┤
│ MORE SCREENSHOTS  (screenshots[] — featured/before omitted)│
│ [thumb] [thumb] [thumb] →  click → lightbox              │
├─────────────────────────────────────────────────────────┤
│ DETAILS                                                  │
│ Author              Foglio1024                           │
│ License             MIT                                  │
│ Last verified       Patch 113                            │
│ GPK files           S1UI_Chat2.gpk                       │
│ Acknowledgments     ...                                  │
└─────────────────────────────────────────────────────────┘
```

Notes:
- Hero image fallback: if `featured_image` missing, use the existing
  banner gradient + initials block (current state). Never show a broken
  `<img>`.
- Action row stays sticky at the top of scroll so install/uninstall is
  always reachable on long detail panels.
- Lightbox: click a screenshot → full-viewport overlay with prev/next +
  Esc-to-close. ~40 lines of JS, no dep.
- Before/After panel: side-by-side on desktop, stacked on narrow
  viewports. Single click toggles "before" full-frame for clearer compare.

## Row card redesign

Current row: `[icon-or-initials] Name · short_description [STATUS] [ACTION]`.
After:

```
[64×64 thumb]  MOD NAME             [tag] [tag]    [STATUS]
              tagline (one line, falls back to short_description)
              by Author · v1.2.3 · 2.1 MB
```

Thumb source priority: `featured_image` (cropped square) →
`screenshots[0]` → `icon_url` → initials fallback.

Tags row: first 2 tags only; `+N` chip if more. Click a tag = filters the
list to that tag (Browse tab only).

## Discovery + enrichment workflow

### A. Existing 101 mods — bulk enrichment

Build `tools/enrich-catalog/enrich.py` in the launcher repo (Python, run
locally; not shipped to users). Flow per entry:

1. Parse `source_url`. Three handlers:
   - GitHub repo → fetch `README.md` via `raw.githubusercontent.com/<repo>/HEAD/README.md`, extract image URLs from markdown, fetch repo description + last commit date via `gh api repos/<repo>`, fetch latest release tag for `version` sanity-check.
   - Tumblr post → fetch HTML, extract `<img>` URLs and the post body text.
   - Other → log + skip; manual fill.
2. Auto-fill candidates:
   - `featured_image` ← first image in README that's wider than tall AND ≥600w (tumblr: largest image)
   - `before_image` ← image whose alt text or surrounding caption matches `/before|original|vanilla/i`
   - `screenshots[]` ← remaining images (deduped, capped at 8)
   - `long_description` ← first 1–3 paragraphs of README (escape, then markdown-passthrough)
   - `tagline` ← repo description if ≤90 chars, else first sentence of README
   - `last_verified_patch` ← most recent commit date, mapped to the patch active on that date (lookup table I'll embed in the script)
   - `gpk_files` ← already-known where catalog has it; otherwise grep README for `\bS1[A-Za-z0-9_]+\.gpk\b` and `\bPC_Event_\d+\b`-style names
3. Write `catalog.proposed.json` with auto-fills merged on top of existing fields.
4. I review every entry by hand, fix obvious extraction mistakes, write a
   stronger `tagline` where the auto-derived one is generic. Estimate
   ~3 min × 101 = ~5 hours of focused review.
5. Commit to `external-mod-catalog/main` as one PR. Bump catalog
   `version` field.

### B. New mod discovery

Driven by the in-flight Gemini deep-research task. When it lands:

1. Dedupe candidates against current catalog by `source_url` (case-insensitive).
2. For each net-new candidate, run the same enrichment pipeline (section A) plus a manual sanity check that:
   - The download URL is stable (versioned release asset, not `main` branch)
   - The license is permissive enough to redistribute (or we link rather than mirror)
   - The mod is for the Classic-era client (32-bit, S1Game), not Bullshit-era 64-bit
3. Add to the catalog. Estimate 30–150 net-new entries depending on what surfaces.

### C. Hash discipline

GPK / external-app entries already require `sha256` of the asset. New
catalog entries must SHA the actual download artifact, not the commit
hash of the repo. Script verifies this gate before allowing entry to
land.

## File layout / blast radius

| File | Change |
| --- | --- |
| `teralaunch/src-tauri/src/services/mods/types.rs` | Add 8 fields to `CatalogEntry` and mirror on `ModEntry`; update `from_catalog` and `from_local_gpk` to fill them. |
| `teralaunch/src-tauri/tests/fixtures/catalog-snapshot.json` | Regenerate from live catalog after bulk enrichment lands. |
| `teralaunch/src/mods.html` | Replace detail-panel `<aside>` block with the new structure. |
| `teralaunch/src/mods.css` (or wherever mods styles live) | Add hero, action row, before/after, lightbox, tag, callout styles. |
| `teralaunch/src/mods.js` | New `renderMarkdown()` helper, `openLightbox()`, before/after toggle, tag-filter wiring. Update `openDetail()` and the row renderer. |
| `tools/enrich-catalog/enrich.py` | New tool, not shipped. |
| `tools/enrich-catalog/patch-date-map.json` | TERA Classic patch date → name lookup table. |
| `external-mod-catalog/catalog.json` | Bulk content PR. Lives in the other repo. |

## Testing

- Rust: `types.rs` unit tests for new fields' defaults and round-trip; integration test loading the regenerated fixture.
- JS: Vitest tests for `renderMarkdown()` (XSS, links, lists, images, code), for `openDetail()` populating the new fields, for the row-card thumb fallback chain, for the lightbox open/close.
- E2E: Playwright spec opens the modal, asserts hero loads, clicks before/after, clicks a screenshot → lightbox, clicks a tag → filters list.
- Visual regression: screenshots at desktop / tablet / narrow breakpoints (per global frontend-design rule).

## Sequencing

1. **Implementation A** — schema + UI changes in the launcher repo, lands as a feature branch but does NOT deploy. Snapshot fixture stays at the pre-enrichment shape; tests still pass against it.
2. **Bulk enrichment B** — `enrich.py` runs against current catalog, I review/edit, lands as a PR to `external-mod-catalog`. Updates the live catalog all 101 entries.
3. **Discovery C** — net-new mods author into the live catalog after dedupe.
4. **Snapshot regeneration** — fetch the now-fully-enriched live catalog, write back to `tests/fixtures/catalog-snapshot.json`. Tests now exercise the rich content.
5. **Deploy v0.1.27** — the launcher PR from step 1 + the snapshot bump from step 4 land on `main` together. `gh workflow run deploy.yml`. Users get the new UI and the rich catalog in one shot.

User explicit constraint: no release until ALL mods are filled. Steps 1–4 must complete before 5.

## Risk register

| Risk | Mitigation |
| --- | --- |
| Some upstream READMEs have no images. | Detail panel falls back gracefully; row card uses initials. Manual screenshot capture for high-priority mods (Foglio1024 set, Shinra, TCC). |
| Tumblr / GitHub rate-limit during enrichment. | Script caches HTTP responses to disk; reruns are free. |
| pantypon / TheTaylorSwiftOfModding cosmetic mods all share preview format — auto-extraction may pick wrong image. | Spot-check during manual review pass; templated fixers in the script per author handle. |
| Discovered mods have unclear licensing. | Default to "link rather than mirror" (catalog `download_url` points to the upstream release page rather than us re-hosting). |
| Markdown XSS via author-supplied content. | Renderer is whitelist-based, escapes first, blocks all script vectors. Catalog is authored by us, but the renderer treats input as untrusted by default. |
| Hero images blow up bandwidth on first launch. | `<img loading="lazy">` everywhere except the visible-on-mount featured image; consider a sub-CDN if catalog grows past ~250 entries. |

## Out of scope (deferred)

- Server-side telemetry for `download_count`
- Mod ratings / reviews
- User-uploaded screenshots
- Localised mod descriptions
- Author profile pages
- Catalog moderation tooling
