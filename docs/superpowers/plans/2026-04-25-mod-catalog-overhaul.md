# Mod Catalog Overhaul Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a redesigned mod manager detail panel and a fully-enriched mod catalog (101 existing + ~30–150 newly-discovered entries) in a single v0.1.27 release.

**Architecture:** Schema gets 8 new optional fields on `CatalogEntry` / `ModEntry` (tagline, featured_image, before_image, tags, gpk_files, compatibility_notes, last_verified_patch, download_count). Detail panel becomes hero-image-first with markdown long_description, before/after compare, lightbox screenshots, and a Details meta block. Row cards get a thumbnail + tagline + tag chips. A Python enrichment tool (`tools/enrich-catalog/enrich.py`) bulk-fills the new fields by parsing each mod's source URL (GitHub README or Tumblr post). Deep-research results feed net-new mod entries into the same pipeline. No release fires until every catalog entry is filled.

**Tech Stack:** Rust 2021 (Tauri v2 backend), vanilla JS + Vitest + Playwright (frontend), Python 3.10+ (enrichment tool only — not shipped to users), GitHub API via `gh`.

**Spec:** `docs/superpowers/specs/2026-04-25-mod-catalog-overhaul-design.md`

---

## File structure

| File | Status | Responsibility |
| --- | --- | --- |
| `teralaunch/src-tauri/src/services/mods/types.rs` | modify | Schema: add 8 fields to `CatalogEntry` + mirror on `ModEntry`; update `from_catalog` / `from_local_gpk` |
| `teralaunch/src-tauri/tests/fixtures/catalog-snapshot.json` | modify (late) | Test fixture; regenerated from live catalog after enrichment |
| `teralaunch/src/markdown.js` | create | In-house trusted-subset markdown renderer (~120 lines) |
| `teralaunch/src/mods.html` | modify | New detail-panel structure (hero, callout, before/after, lightbox slot) + row-card structure |
| `teralaunch/src/mods.css` | modify (or new section) | Hero, action row, callout, before/after grid, lightbox overlay, tag chip, thumbnail rules |
| `teralaunch/src/mods.js` | modify | Import markdown renderer; rewrite `openDetail`; add `openLightbox`, `toggleBeforeAfter`; row-card thumbnail fallback + tagline + tag chips; tag-filter wiring |
| `teralaunch/tests/markdown.test.js` | create | Vitest: XSS, links, lists, images, code, headings |
| `teralaunch/tests/mods-detail-render.test.js` | create | Vitest: `openDetail` populates new fields; thumb fallback chain |
| `teralaunch/tests/e2e/mods-detail.spec.js` | create | Playwright: hero loads, lightbox open/close, before/after toggle, tag filter |
| `tools/enrich-catalog/enrich.py` | create | Driver: takes `catalog.json`, emits `catalog.proposed.json` |
| `tools/enrich-catalog/handlers/github.py` | create | Fetch + parse GitHub README; extract images, description, last commit |
| `tools/enrich-catalog/handlers/tumblr.py` | create | Fetch Tumblr post HTML; extract images + body |
| `tools/enrich-catalog/patch-date-map.json` | create | Patch release dates → patch label lookup |
| `tools/enrich-catalog/README.md` | create | Usage docs (humans, not Claude) |
| (external repo) `external-mod-catalog/catalog.json` | modify | Bulk content PR with all enriched + new entries |

---

## Phase 1 — Schema additions

### Task 1: Add fields to `CatalogEntry` and `ModEntry`

**Files:**
- Modify: `teralaunch/src-tauri/src/services/mods/types.rs`

- [ ] **Step 1: Add fields to `CatalogEntry`**

In `CatalogEntry`, add the following fields right after `pub author: String,`:

```rust
    /// One-line punchy hook (≤90 chars). Row cards display this; falls
    /// back to short_description when missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tagline: Option<String>,

    /// Hero image at the top of the detail panel. 16:9 preferred, ≥1200w.
    /// For restyles, this is the "after" shot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub featured_image: Option<String>,

    /// Restyles only — paired "before" shot for side-by-side compare.
    /// Side-by-side panel only renders when both before_image and
    /// featured_image are present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_image: Option<String>,

    /// Searchable badges. e.g. ["minimap","quality-of-life","foglio"].
    /// Distinct from `category` (single-string filter).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// GPK files this mod replaces, e.g. ["S1UI_Chat2.gpk"]. Power-user
    /// info shown in Details row.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gpk_files: Vec<String>,

    /// Markdown. "Conflicts with X", "Broken on patch Y". Rendered in a
    /// yellow-tinted callout above the screenshot strip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility_notes: Option<String>,

    /// Last patch the mod was confirmed working on, e.g. "patch 113".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_verified_patch: Option<String>,

    /// Stub for future telemetry. UI does NOT render this yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_count: Option<u64>,
```

- [ ] **Step 2: Mirror the same fields on `ModEntry`**

In `ModEntry`, after the existing `pub credits: Option<String>,` block, add the identical 8 fields with the same `#[serde]` treatment.

- [ ] **Step 3: Run cargo check**

Run: `cd teralaunch/src-tauri && cargo check 2>&1 | tail -10`
Expected: clean build, no errors. Warnings about unused fields on `ModEntry` are fine for now.

- [ ] **Step 4: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/types.rs
git commit -m "feat(mods): add 8 catalog enrichment fields to schema"
```

---

### Task 2: Wire new fields through `from_catalog` and `from_local_gpk`

**Files:**
- Modify: `teralaunch/src-tauri/src/services/mods/types.rs`

- [ ] **Step 1: Update `ModEntry::from_catalog`**

After `screenshots: catalog.screenshots.clone(),`, add:

```rust
            tagline: catalog.tagline.clone(),
            featured_image: catalog.featured_image.clone(),
            before_image: catalog.before_image.clone(),
            tags: catalog.tags.clone(),
            gpk_files: catalog.gpk_files.clone(),
            compatibility_notes: catalog.compatibility_notes.clone(),
            last_verified_patch: catalog.last_verified_patch.clone(),
            download_count: catalog.download_count,
```

- [ ] **Step 2: Update `ModEntry::from_local_gpk`**

In the `Self { ... }` literal at the bottom of `from_local_gpk`, after `screenshots: Vec::new(),`, add:

```rust
            tagline: None,
            featured_image: None,
            before_image: None,
            tags: Vec::new(),
            gpk_files: Vec::new(),
            compatibility_notes: None,
            last_verified_patch: None,
            download_count: None,
```

- [ ] **Step 3: Update existing tests that build `CatalogEntry` literally**

Find every `CatalogEntry { ... }` literal in `types.rs::tests` (search for `CatalogEntry {`). Three test functions: `mod_entry_from_catalog_copies_relevant_fields`, `mod_entry_from_catalog_defaults_auto_launch_from_catalog`. Add the 8 new fields with default values:

```rust
            tagline: None,
            featured_image: None,
            before_image: None,
            tags: vec![],
            gpk_files: vec![],
            compatibility_notes: None,
            last_verified_patch: None,
            download_count: None,
```

- [ ] **Step 4: Run cargo check**

Run: `cd teralaunch/src-tauri && cargo check --tests 2>&1 | tail -10`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/types.rs
git commit -m "feat(mods): populate new fields in ModEntry::from_catalog"
```

---

### Task 3: Schema round-trip and field-population tests

**Files:**
- Modify: `teralaunch/src-tauri/src/services/mods/types.rs`

- [ ] **Step 1: Add deserialization test for new fields**

In the `tests` module, after `catalog_entry_deserializes_minimal_shape`, add:

```rust
    #[test]
    fn catalog_entry_deserializes_full_enriched_shape() {
        let json = r#"{
            "id": "test.full",
            "kind": "gpk",
            "name": "Full Mod",
            "author": "Tester",
            "short_description": "Test",
            "version": "1.0.0",
            "download_url": "https://example.com/x.gpk",
            "sha256": "abcd",
            "tagline": "Punchy hook",
            "featured_image": "https://example.com/hero.png",
            "before_image": "https://example.com/before.png",
            "tags": ["minimap","quality-of-life"],
            "gpk_files": ["S1UI_Map.gpk"],
            "compatibility_notes": "Conflicts with X",
            "last_verified_patch": "patch 113",
            "download_count": 42
        }"#;
        let entry: CatalogEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.tagline.as_deref(), Some("Punchy hook"));
        assert_eq!(entry.featured_image.as_deref(), Some("https://example.com/hero.png"));
        assert_eq!(entry.before_image.as_deref(), Some("https://example.com/before.png"));
        assert_eq!(entry.tags, vec!["minimap", "quality-of-life"]);
        assert_eq!(entry.gpk_files, vec!["S1UI_Map.gpk"]);
        assert_eq!(entry.compatibility_notes.as_deref(), Some("Conflicts with X"));
        assert_eq!(entry.last_verified_patch.as_deref(), Some("patch 113"));
        assert_eq!(entry.download_count, Some(42));
    }

    #[test]
    fn catalog_entry_minimal_shape_keeps_new_fields_default() {
        let json = r#"{
            "id": "test.min",
            "kind": "external",
            "name": "Minimal",
            "author": "Tester",
            "short_description": "Test",
            "version": "1.0.0",
            "download_url": "https://example.com/x.zip",
            "sha256": "abcd"
        }"#;
        let entry: CatalogEntry = serde_json::from_str(json).unwrap();
        assert!(entry.tagline.is_none());
        assert!(entry.featured_image.is_none());
        assert!(entry.before_image.is_none());
        assert!(entry.tags.is_empty());
        assert!(entry.gpk_files.is_empty());
        assert!(entry.compatibility_notes.is_none());
        assert!(entry.last_verified_patch.is_none());
        assert!(entry.download_count.is_none());
    }

    #[test]
    fn mod_entry_from_catalog_copies_new_fields() {
        let mut catalog = CatalogEntry {
            id: "x".into(),
            kind: ModKind::Gpk,
            name: "X".into(),
            author: "A".into(),
            short_description: "s".into(),
            long_description: "".into(),
            category: "".into(),
            license: "".into(),
            credits: "".into(),
            version: "1".into(),
            download_url: "".into(),
            sha256: "".into(),
            size_bytes: 0,
            source_url: None,
            icon_url: None,
            screenshots: vec![],
            executable_relpath: None,
            auto_launch_default: None,
            settings_folder: None,
            target_patch: None,
            composite_flag: None,
            updated_at: "".into(),
            tagline: Some("Hook".into()),
            featured_image: Some("hero".into()),
            before_image: Some("before".into()),
            tags: vec!["t1".into(), "t2".into()],
            gpk_files: vec!["A.gpk".into()],
            compatibility_notes: Some("note".into()),
            last_verified_patch: Some("patch 113".into()),
            download_count: Some(100),
        };
        let entry = ModEntry::from_catalog(&catalog);
        assert_eq!(entry.tagline.as_deref(), Some("Hook"));
        assert_eq!(entry.featured_image.as_deref(), Some("hero"));
        assert_eq!(entry.before_image.as_deref(), Some("before"));
        assert_eq!(entry.tags, vec!["t1", "t2"]);
        assert_eq!(entry.gpk_files, vec!["A.gpk"]);
        assert_eq!(entry.compatibility_notes.as_deref(), Some("note"));
        assert_eq!(entry.last_verified_patch.as_deref(), Some("patch 113"));
        assert_eq!(entry.download_count, Some(100));

        // suppress unused-mut on stable
        catalog.tagline = None;
    }
```

- [ ] **Step 2: Run tests to verify**

Run: `cd teralaunch/src-tauri && cargo test --bin tera-europe-classicplus-launcher services::mods::types:: 2>&1 | tail -15`
Expected: all 3 new tests pass alongside existing ones.

- [ ] **Step 3: Run the existing snapshot fixture deserialization**

Run: `cd teralaunch/src-tauri && cargo test --bin tera-europe-classicplus-launcher 2>&1 | grep -E "test result|FAILED" | head -30`
Expected: zero new failures (the 5 pre-existing failures from earlier sessions remain unchanged: changelog_guard newest_release, em-dash byte panics, is_process_running structural pins).

- [ ] **Step 4: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/types.rs
git commit -m "test(mods): cover new schema fields with round-trip tests"
```

---

## Phase 2 — Markdown renderer

### Task 4: In-house trusted-subset markdown renderer

**Files:**
- Create: `teralaunch/src/markdown.js`
- Create: `teralaunch/tests/markdown.test.js`

- [ ] **Step 1: Write the renderer test file first (failing)**

Create `teralaunch/tests/markdown.test.js`:

```js
import { describe, it, expect } from 'vitest';
import { renderMarkdown } from '../src/markdown.js';

describe('renderMarkdown', () => {
    it('escapes raw HTML so script tags are inert', () => {
        const out = renderMarkdown('<script>alert(1)</script>');
        expect(out).not.toContain('<script>');
        expect(out).toContain('&lt;script&gt;');
    });

    it('renders **bold** and *italic*', () => {
        const out = renderMarkdown('**a** and *b*');
        expect(out).toContain('<strong>a</strong>');
        expect(out).toContain('<em>b</em>');
    });

    it('renders [text](url) links with rel=noopener', () => {
        const out = renderMarkdown('[click](https://example.com)');
        expect(out).toMatch(/<a [^>]*href="https:\/\/example\.com"/);
        expect(out).toMatch(/rel="noopener noreferrer"/);
        expect(out).toMatch(/target="_blank"/);
    });

    it('rejects javascript: URLs in links', () => {
        const out = renderMarkdown('[evil](javascript:alert(1))');
        expect(out).not.toMatch(/href="javascript:/);
    });

    it('renders unordered lists', () => {
        const out = renderMarkdown('- one\n- two\n- three');
        expect(out).toContain('<ul>');
        expect(out).toContain('<li>one</li>');
        expect(out).toContain('<li>three</li>');
    });

    it('renders ordered lists', () => {
        const out = renderMarkdown('1. one\n2. two');
        expect(out).toContain('<ol>');
        expect(out).toContain('<li>one</li>');
    });

    it('renders headings up to h3 only', () => {
        const out = renderMarkdown('# h1\n## h2\n### h3\n#### h4');
        expect(out).toContain('<h1>h1</h1>');
        expect(out).toContain('<h2>h2</h2>');
        expect(out).toContain('<h3>h3</h3>');
        // h4 should be plain paragraph
        expect(out).not.toContain('<h4>');
        expect(out).toContain('#### h4');
    });

    it('renders paragraphs separated by blank lines', () => {
        const out = renderMarkdown('one\n\ntwo');
        expect(out).toMatch(/<p>one<\/p>\s*<p>two<\/p>/);
    });

    it('renders inline `code`', () => {
        const out = renderMarkdown('use `S1UI_Chat2.gpk`');
        expect(out).toContain('<code>S1UI_Chat2.gpk</code>');
    });

    it('renders fenced code blocks', () => {
        const out = renderMarkdown('```\nplain text\n```');
        expect(out).toContain('<pre><code>plain text');
    });

    it('renders images only when URL is http(s) or data:image/', () => {
        const ok = renderMarkdown('![alt](https://example.com/x.png)');
        expect(ok).toMatch(/<img [^>]*src="https:\/\/example\.com\/x\.png"/);
        expect(ok).toMatch(/loading="lazy"/);

        const evil = renderMarkdown('![alt](javascript:alert(1))');
        expect(evil).not.toMatch(/<img /);
    });

    it('strips on*= attributes from any inline HTML attempt', () => {
        const out = renderMarkdown('text with <img src="x" onerror="alert(1)">');
        expect(out).not.toContain('onerror');
    });

    it('returns empty string for null/undefined input', () => {
        expect(renderMarkdown(null)).toBe('');
        expect(renderMarkdown(undefined)).toBe('');
        expect(renderMarkdown('')).toBe('');
    });

    it('preserves plain text with no markdown', () => {
        const out = renderMarkdown('just plain text');
        expect(out).toContain('just plain text');
    });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd teralaunch && npx vitest run tests/markdown.test.js 2>&1 | tail -20`
Expected: FAIL — module `../src/markdown.js` not found.

- [ ] **Step 3: Write the renderer**

Create `teralaunch/src/markdown.js`:

```js
// Trusted-subset markdown renderer for catalog content.
// We author all input ourselves but treat it as untrusted by default.
// Subset: paragraphs, h1-h3, **bold**, *italic*, `inline code`, fenced
// code, [links](url), ![images](url), - and 1. lists. Anything else
// renders as plain escaped text.

const ESC = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
function escape(s) { return String(s).replace(/[&<>"']/g, ch => ESC[ch]); }

function isSafeUrl(url) {
    if (!url) return false;
    return /^https?:\/\//i.test(url) || /^data:image\//i.test(url);
}

function inline(text) {
    let s = escape(text);
    // images first (before regular links so the ! prefix doesn't get parsed as a link)
    s = s.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, (_, alt, url) => {
        if (!isSafeUrl(url)) return '';
        return `<img src="${escape(url)}" alt="${escape(alt)}" loading="lazy" />`;
    });
    s = s.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_, label, url) => {
        if (!isSafeUrl(url)) return escape(label);
        return `<a href="${escape(url)}" target="_blank" rel="noopener noreferrer">${escape(label)}</a>`;
    });
    s = s.replace(/`([^`]+)`/g, (_, code) => `<code>${code}</code>`);
    s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
    s = s.replace(/\*([^*]+)\*/g, '<em>$1</em>');
    return s;
}

function renderBlocks(src) {
    const lines = src.replace(/\r\n/g, '\n').split('\n');
    const out = [];
    let i = 0;

    while (i < lines.length) {
        const line = lines[i];

        // fenced code block
        if (/^```/.test(line)) {
            i++;
            const code = [];
            while (i < lines.length && !/^```/.test(lines[i])) {
                code.push(lines[i]);
                i++;
            }
            i++; // skip closing fence
            out.push(`<pre><code>${escape(code.join('\n'))}</code></pre>`);
            continue;
        }

        // headings (h1-h3 only)
        const h = line.match(/^(#{1,3})\s+(.*)$/);
        if (h) {
            const level = h[1].length;
            out.push(`<h${level}>${inline(h[2])}</h${level}>`);
            i++;
            continue;
        }

        // unordered list
        if (/^- /.test(line)) {
            const items = [];
            while (i < lines.length && /^- /.test(lines[i])) {
                items.push(`<li>${inline(lines[i].slice(2))}</li>`);
                i++;
            }
            out.push(`<ul>${items.join('')}</ul>`);
            continue;
        }

        // ordered list
        if (/^\d+\. /.test(line)) {
            const items = [];
            while (i < lines.length && /^\d+\. /.test(lines[i])) {
                items.push(`<li>${inline(lines[i].replace(/^\d+\. /, ''))}</li>`);
                i++;
            }
            out.push(`<ol>${items.join('')}</ol>`);
            continue;
        }

        // blank line
        if (line.trim() === '') {
            i++;
            continue;
        }

        // paragraph (consume until blank line or block start)
        const para = [];
        while (
            i < lines.length
            && lines[i].trim() !== ''
            && !/^```/.test(lines[i])
            && !/^#{1,3}\s+/.test(lines[i])
            && !/^- /.test(lines[i])
            && !/^\d+\. /.test(lines[i])
        ) {
            para.push(lines[i]);
            i++;
        }
        if (para.length > 0) {
            out.push(`<p>${inline(para.join(' '))}</p>`);
        }
    }

    return out.join('\n');
}

export function renderMarkdown(input) {
    if (input == null || input === '') return '';
    return renderBlocks(String(input));
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd teralaunch && npx vitest run tests/markdown.test.js 2>&1 | tail -20`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add teralaunch/src/markdown.js teralaunch/tests/markdown.test.js
git commit -m "feat(mods): in-house trusted-subset markdown renderer"
```

---

## Phase 3 — Detail panel HTML

### Task 5: Replace detail-panel structure

**Files:**
- Modify: `teralaunch/src/mods.html` (lines ~98–147 currently)

- [ ] **Step 1: Replace the entire `<div class="mods-detail-backdrop">` block**

Find the block starting at the `<!-- Detail Drill-down (Slide-over) -->` comment and ending at its matching `</div>` (just before `</main>`). Replace the inner `<aside>` contents with:

```html
<aside class="mods-detail" id="mods-detail" role="dialog" aria-modal="true" aria-labelledby="mods-detail-name">
    <button class="mods-detail-close" id="mods-detail-close" aria-label="Close" data-translate-aria-label="MODS_ARIA_CLOSE">×</button>

    <div class="mods-detail-hero" id="mods-detail-hero" hidden>
        <img class="mods-detail-hero-img" id="mods-detail-hero-img" alt="" />
    </div>

    <header class="mods-detail-header">
        <div class="mods-detail-icon" id="mods-detail-icon"></div>
        <div class="mods-detail-title-wrap">
            <h2 class="mods-detail-name" id="mods-detail-name">—</h2>
            <p class="mods-detail-byline">
                <span data-translate="MODS_DETAIL_BY">by</span>
                <span id="mods-detail-author">—</span>
                ·
                <span id="mods-detail-version">—</span>
                <span id="mods-detail-category-pill" class="mods-detail-category-pill" hidden></span>
                <span id="mods-detail-size-text"></span>
            </p>
            <div class="mods-detail-tags" id="mods-detail-tags" hidden></div>
        </div>
    </header>

    <div class="mods-detail-action-row" id="mods-detail-action-row">
        <!-- Primary action injected by mods.js (Install / Enable / Launch / etc.) -->
        <div id="mods-detail-primary-action"></div>
        <button class="mods-detail-secondary-btn" id="mods-detail-open-settings" hidden data-translate="MODS_DETAIL_OPEN_SETTINGS">Open settings folder</button>
        <a class="mods-detail-secondary-btn" id="mods-detail-source-link" href="#" target="_blank" rel="noopener noreferrer" hidden>
            <span data-translate="MODS_DETAIL_VIEW_SOURCE">View source</span>
            <span aria-hidden="true">↗</span>
        </a>
    </div>

    <div class="mods-detail-body">
        <section class="mods-detail-callout" id="mods-detail-callout" hidden>
            <div class="mods-detail-callout-title" data-translate="MODS_DETAIL_COMPAT">Compatibility</div>
            <div class="mods-detail-callout-body" id="mods-detail-callout-body"></div>
        </section>

        <section class="mods-detail-section" id="mods-detail-description-section">
            <h3 class="mods-detail-section-title" data-translate="MODS_DETAIL_ABOUT">About</h3>
            <div class="mods-detail-description" id="mods-detail-description"></div>
        </section>

        <section class="mods-detail-section" id="mods-detail-beforeafter-section" hidden>
            <h3 class="mods-detail-section-title" data-translate="MODS_DETAIL_BEFORE_AFTER">Before / After</h3>
            <div class="mods-detail-beforeafter">
                <figure class="mods-detail-ba-side">
                    <img id="mods-detail-before-img" alt="Before" />
                    <figcaption data-translate="MODS_DETAIL_BEFORE">Before</figcaption>
                </figure>
                <figure class="mods-detail-ba-side">
                    <img id="mods-detail-after-img" alt="After" />
                    <figcaption data-translate="MODS_DETAIL_AFTER">After</figcaption>
                </figure>
            </div>
        </section>

        <section class="mods-detail-section" id="mods-detail-screenshots-section" hidden>
            <h3 class="mods-detail-section-title" data-translate="MODS_DETAIL_SCREENSHOTS">Screenshots</h3>
            <div class="mods-detail-screenshots" id="mods-detail-screenshots"></div>
        </section>

        <section class="mods-detail-section">
            <h3 class="mods-detail-section-title" data-translate="MODS_DETAIL_DETAILS">Details</h3>
            <dl class="mods-detail-facts">
                <div class="mods-detail-fact">
                    <dt data-translate="MODS_DETAIL_AUTHOR">Author</dt>
                    <dd id="mods-detail-fact-author">—</dd>
                </div>
                <div class="mods-detail-fact" id="mods-detail-fact-license-row" hidden>
                    <dt data-translate="MODS_DETAIL_LICENSE">License</dt>
                    <dd id="mods-detail-fact-license">—</dd>
                </div>
                <div class="mods-detail-fact" id="mods-detail-fact-patch-row" hidden>
                    <dt data-translate="MODS_DETAIL_LAST_VERIFIED">Last verified</dt>
                    <dd id="mods-detail-fact-patch">—</dd>
                </div>
                <div class="mods-detail-fact" id="mods-detail-fact-gpkfiles-row" hidden>
                    <dt data-translate="MODS_DETAIL_GPK_FILES">GPK files</dt>
                    <dd id="mods-detail-fact-gpkfiles">—</dd>
                </div>
                <div class="mods-detail-fact" id="mods-detail-fact-credits-row" hidden>
                    <dt data-translate="MODS_DETAIL_ACKS">Acknowledgments</dt>
                    <dd id="mods-detail-fact-credits">—</dd>
                </div>
            </dl>
        </section>
    </div>
</aside>
<div class="mods-lightbox" id="mods-lightbox" hidden>
    <button class="mods-lightbox-close" id="mods-lightbox-close" aria-label="Close lightbox">×</button>
    <button class="mods-lightbox-nav prev" id="mods-lightbox-prev" aria-label="Previous">‹</button>
    <img id="mods-lightbox-img" alt="" />
    <button class="mods-lightbox-nav next" id="mods-lightbox-next" aria-label="Next">›</button>
</div>
```

- [ ] **Step 2: Verify HTML parses**

Run: `cd teralaunch && npx vite build 2>&1 | tail -10`
Expected: build succeeds (or skip if vite isn't configured for build; otherwise rely on dev-mode Playwright next).

- [ ] **Step 3: Commit**

```bash
git add teralaunch/src/mods.html
git commit -m "feat(mods): new detail-panel structure (hero, callout, before/after, lightbox slot)"
```

---

## Phase 4 — Detail panel CSS

### Task 6: Style the new detail-panel components

**Files:**
- Modify: `teralaunch/src/mods.css` (or wherever mods CSS currently lives — search for `.mods-detail-backdrop`)

- [ ] **Step 1: Locate mods CSS file**

Run: `cd teralaunch && grep -rn "mods-detail-backdrop" src/ --include="*.css" | head -3`
Note the file path that owns the existing detail styles.

- [ ] **Step 2: Append the new style block to that file**

Append to the file from step 1:

```css
/* === Mod detail — overhauled hero, action row, before/after, lightbox === */

.mods-detail-hero {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    overflow: hidden;
    background: linear-gradient(135deg, rgba(60, 60, 80, 0.6) 0%, rgba(20, 20, 32, 0.9) 100%);
}
.mods-detail-hero-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
}

.mods-detail-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 8px;
}
.mods-detail-tag {
    padding: 2px 10px;
    font-size: 11px;
    line-height: 18px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 999px;
    color: var(--text-secondary, #c9c9d6);
    cursor: pointer;
    transition: background-color 120ms ease;
}
.mods-detail-tag:hover {
    background: rgba(255, 255, 255, 0.16);
}

.mods-detail-category-pill {
    margin-left: 6px;
    padding: 1px 8px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    background: rgba(120, 140, 220, 0.18);
    border-radius: 4px;
}

.mods-detail-action-row {
    position: sticky;
    top: 0;
    z-index: 2;
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 12px 24px;
    background: rgba(18, 18, 28, 0.94);
    backdrop-filter: blur(8px);
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}
.mods-detail-action-row > #mods-detail-primary-action {
    flex: 1;
}
.mods-detail-secondary-btn {
    padding: 8px 14px;
    font-size: 13px;
    color: var(--text-primary, #f0f0f5);
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    text-decoration: none;
    cursor: pointer;
    transition: background-color 120ms ease;
}
.mods-detail-secondary-btn:hover {
    background: rgba(255, 255, 255, 0.12);
}

.mods-detail-callout {
    margin: 16px 24px 0;
    padding: 12px 16px;
    background: rgba(220, 180, 60, 0.1);
    border-left: 3px solid rgba(220, 180, 60, 0.7);
    border-radius: 4px;
}
.mods-detail-callout-title {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: rgba(220, 180, 60, 0.95);
    margin-bottom: 6px;
}
.mods-detail-callout-body p { margin: 4px 0; }

.mods-detail-description {
    line-height: 1.6;
    color: var(--text-primary, #e8e8ee);
}
.mods-detail-description h1,
.mods-detail-description h2,
.mods-detail-description h3 {
    margin-top: 18px;
    margin-bottom: 8px;
    font-weight: 600;
}
.mods-detail-description h1 { font-size: 18px; }
.mods-detail-description h2 { font-size: 16px; }
.mods-detail-description h3 { font-size: 14px; text-transform: uppercase; letter-spacing: 0.5px; }
.mods-detail-description p { margin: 8px 0; }
.mods-detail-description ul,
.mods-detail-description ol {
    margin: 8px 0;
    padding-left: 22px;
}
.mods-detail-description code {
    padding: 1px 5px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 3px;
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 0.92em;
}
.mods-detail-description pre {
    margin: 10px 0;
    padding: 10px 12px;
    background: rgba(0, 0, 0, 0.35);
    border-radius: 6px;
    overflow-x: auto;
}
.mods-detail-description pre code {
    background: transparent;
    padding: 0;
}
.mods-detail-description img {
    max-width: 100%;
    border-radius: 6px;
    margin: 8px 0;
}
.mods-detail-description a {
    color: var(--accent, #8aa6ff);
    text-decoration: underline;
    text-decoration-color: rgba(138, 166, 255, 0.4);
}

.mods-detail-beforeafter {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
}
.mods-detail-ba-side {
    margin: 0;
    cursor: pointer;
}
.mods-detail-ba-side img {
    width: 100%;
    aspect-ratio: 16 / 9;
    object-fit: cover;
    border-radius: 6px;
    display: block;
}
.mods-detail-ba-side figcaption {
    margin-top: 6px;
    font-size: 12px;
    color: var(--text-secondary, #b0b0c0);
    text-align: center;
}
@media (max-width: 600px) {
    .mods-detail-beforeafter { grid-template-columns: 1fr; }
}

.mods-detail-screenshots {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 10px;
}
.mods-detail-screenshots img {
    width: 100%;
    aspect-ratio: 16 / 9;
    object-fit: cover;
    border-radius: 4px;
    cursor: zoom-in;
    transition: transform 120ms ease;
}
.mods-detail-screenshots img:hover { transform: scale(1.02); }

.mods-lightbox {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.92);
}
.mods-lightbox img {
    max-width: 92vw;
    max-height: 92vh;
    object-fit: contain;
    border-radius: 4px;
}
.mods-lightbox-close,
.mods-lightbox-nav {
    position: absolute;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.18);
    color: white;
    font-size: 28px;
    width: 48px;
    height: 48px;
    border-radius: 50%;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
}
.mods-lightbox-close:hover,
.mods-lightbox-nav:hover { background: rgba(255, 255, 255, 0.16); }
.mods-lightbox-close { top: 24px; right: 24px; }
.mods-lightbox-nav.prev { left: 24px; top: 50%; transform: translateY(-50%); }
.mods-lightbox-nav.next { right: 24px; top: 50%; transform: translateY(-50%); }

/* === Row card thumbnail === */
.mods-row-thumb {
    width: 64px;
    height: 64px;
    border-radius: 6px;
    object-fit: cover;
    flex-shrink: 0;
    background: linear-gradient(135deg, rgba(80, 80, 100, 0.5), rgba(40, 40, 60, 0.7));
}
.mods-row-tags {
    display: flex;
    gap: 4px;
    margin-top: 4px;
}
.mods-row-tag {
    padding: 1px 6px;
    font-size: 10px;
    background: rgba(255, 255, 255, 0.06);
    border-radius: 4px;
    color: var(--text-secondary, #b0b0c0);
}

@media (prefers-reduced-motion: reduce) {
    .mods-detail-screenshots img { transition: none; }
    .mods-detail-screenshots img:hover { transform: none; }
    .mods-detail-secondary-btn { transition: none; }
}
```

- [ ] **Step 3: Verify CSS lints clean (if a lint config exists)**

Run: `cd teralaunch && npm run lint 2>&1 | tail -20 || echo "no lint script"`
Expected: clean or "no lint script".

- [ ] **Step 4: Commit**

```bash
git add teralaunch/src/mods.css
git commit -m "feat(mods): styles for hero, before/after, lightbox, tags"
```

---

## Phase 5 — Detail panel JS

### Task 7: Update `openDetail` to populate new fields and render markdown

**Files:**
- Modify: `teralaunch/src/mods.js`

- [ ] **Step 1: Add markdown import + ref captures**

At the top of the file (after the existing imports), add:

```js
import { renderMarkdown } from './markdown.js';
```

In `setupElements()` (after the existing `$detailSourceLink` capture at line ~145), add:

```js
        this.$detailHero = document.getElementById('mods-detail-hero');
        this.$detailHeroImg = document.getElementById('mods-detail-hero-img');
        this.$detailCategoryPill = document.getElementById('mods-detail-category-pill');
        this.$detailSizeText = document.getElementById('mods-detail-size-text');
        this.$detailTags = document.getElementById('mods-detail-tags');
        this.$detailCallout = document.getElementById('mods-detail-callout');
        this.$detailCalloutBody = document.getElementById('mods-detail-callout-body');
        this.$detailBeforeAfterSection = document.getElementById('mods-detail-beforeafter-section');
        this.$detailBeforeImg = document.getElementById('mods-detail-before-img');
        this.$detailAfterImg = document.getElementById('mods-detail-after-img');
        this.$detailFactPatch = document.getElementById('mods-detail-fact-patch');
        this.$detailFactPatchRow = document.getElementById('mods-detail-fact-patch-row');
        this.$detailFactGpkFiles = document.getElementById('mods-detail-fact-gpkfiles');
        this.$detailFactGpkFilesRow = document.getElementById('mods-detail-fact-gpkfiles-row');
        this.$lightbox = document.getElementById('mods-lightbox');
        this.$lightboxImg = document.getElementById('mods-lightbox-img');
        this.$lightboxClose = document.getElementById('mods-lightbox-close');
        this.$lightboxPrev = document.getElementById('mods-lightbox-prev');
        this.$lightboxNext = document.getElementById('mods-lightbox-next');
```

- [ ] **Step 2: Rewrite the body of `openDetail`**

Replace the entire `openDetail(id, context)` method (lines ~507–523) with:

```js
    openDetail(id, context) {
        if (!this.$detailBackdrop || !id) return;
        const inst = this.state.installed.find(m => m.id === id);
        const cat = this.state.catalog.find(m => m.id === id);
        const entry = context === 'browse' ? (cat || inst) : (inst || cat);
        if (!entry) return;

        // Title block
        this.$detailName.textContent = entry.name || id;
        this.$detailAuthor.textContent = entry.author || '—';
        this.$detailVersion.textContent = entry.version ? `v${entry.version}` : '';

        const category = entry.category || cat?.category || '';
        if (this.$detailCategoryPill) {
            this.$detailCategoryPill.hidden = !category;
            this.$detailCategoryPill.textContent = category;
        }
        const sizeBytes = entry.size_bytes ?? cat?.size_bytes ?? 0;
        if (this.$detailSizeText) {
            this.$detailSizeText.textContent = sizeBytes ? ` · ${formatMB(sizeBytes)}` : '';
        }

        // Tags
        const tags = entry.tags && entry.tags.length ? entry.tags : (cat?.tags || []);
        if (this.$detailTags) {
            if (tags.length === 0) {
                this.$detailTags.hidden = true;
                this.$detailTags.innerHTML = '';
            } else {
                this.$detailTags.hidden = false;
                this.$detailTags.innerHTML = tags
                    .map(t => `<button type="button" class="mods-detail-tag" data-tag="${escapeHtml(t)}">${escapeHtml(t)}</button>`)
                    .join('');
            }
        }

        // Hero image
        const hero = entry.featured_image || cat?.featured_image || '';
        if (this.$detailHero && this.$detailHeroImg) {
            if (hero) {
                this.$detailHero.hidden = false;
                this.$detailHeroImg.src = hero;
                this.$detailHeroImg.alt = entry.name || '';
            } else {
                this.$detailHero.hidden = true;
                this.$detailHeroImg.removeAttribute('src');
            }
        }

        // Icon (small) — kept for non-hero cases and corner badge
        this.$detailIcon.innerHTML = entry.icon_url
            ? `<img src="${escapeHtml(entry.icon_url)}" alt="" />`
            : toInitials(entry.name || id);

        // Action row — source link
        const sourceUrl = entry.source_url || cat?.source_url || '';
        if (this.$detailSourceLink) {
            this.$detailSourceLink.hidden = !sourceUrl;
            this.$detailSourceLink.href = sourceUrl || '#';
        }

        // Compatibility callout
        const compat = entry.compatibility_notes || cat?.compatibility_notes || '';
        if (this.$detailCallout && this.$detailCalloutBody) {
            if (compat) {
                this.$detailCallout.hidden = false;
                this.$detailCalloutBody.innerHTML = renderMarkdown(compat);
            } else {
                this.$detailCallout.hidden = true;
                this.$detailCalloutBody.innerHTML = '';
            }
        }

        // Description (markdown)
        const longDesc = entry.long_description || entry.description || cat?.short_description || '';
        this.$detailDescription.innerHTML = renderMarkdown(longDesc);

        // Before / after panel
        const beforeUrl = entry.before_image || cat?.before_image || '';
        if (this.$detailBeforeAfterSection && this.$detailBeforeImg && this.$detailAfterImg) {
            if (beforeUrl && hero) {
                this.$detailBeforeAfterSection.hidden = false;
                this.$detailBeforeImg.src = beforeUrl;
                this.$detailAfterImg.src = hero;
            } else {
                this.$detailBeforeAfterSection.hidden = true;
                this.$detailBeforeImg.removeAttribute('src');
                this.$detailAfterImg.removeAttribute('src');
            }
        }

        // Screenshots — exclude featured/before to avoid duplication
        const allShots = entry.screenshots || cat?.screenshots || [];
        const shots = allShots.filter(u => u !== hero && u !== beforeUrl);
        this.$detailScreenshotsSection.hidden = (shots.length === 0);
        this.$detailScreenshots.innerHTML = shots
            .map((url, idx) => `<img src="${escapeHtml(url)}" alt="" loading="lazy" data-shot-index="${idx}" />`)
            .join('');
        this._currentShots = shots;

        // Author / license / credits / patch / gpk_files in Details
        this.$detailFactAuthor.textContent = entry.author || '—';
        const license = entry.license || cat?.license || '';
        if (this.$detailFactLicenseRow) this.$detailFactLicenseRow.hidden = !license;
        if (this.$detailFactLicense) this.$detailFactLicense.textContent = license || '—';
        const credits = entry.credits || cat?.credits || '';
        if (this.$detailFactCreditsRow) this.$detailFactCreditsRow.hidden = !credits;
        if (this.$detailFactCredits) this.$detailFactCredits.textContent = credits || '—';
        const patch = entry.last_verified_patch || cat?.last_verified_patch || '';
        if (this.$detailFactPatchRow) this.$detailFactPatchRow.hidden = !patch;
        if (this.$detailFactPatch) this.$detailFactPatch.textContent = patch || '—';
        const gpkFiles = (entry.gpk_files && entry.gpk_files.length ? entry.gpk_files : (cat?.gpk_files || []));
        if (this.$detailFactGpkFilesRow) this.$detailFactGpkFilesRow.hidden = gpkFiles.length === 0;
        if (this.$detailFactGpkFiles) this.$detailFactGpkFiles.textContent = gpkFiles.join(', ') || '—';

        this.$detailBackdrop.hidden = false;
    },
```

- [ ] **Step 3: Add lightbox + tag handlers in `bindEvents`**

Append inside `bindEvents()` after the existing source-link click handler:

```js
        // Lightbox: click on a screenshot opens overlay
        this.$detailScreenshots?.addEventListener('click', (e) => {
            const img = e.target.closest('img[data-shot-index]');
            if (!img) return;
            const idx = parseInt(img.dataset.shotIndex, 10);
            this._openLightbox(idx);
        });
        this.$lightboxClose?.addEventListener('click', () => this._closeLightbox());
        this.$lightboxPrev?.addEventListener('click', () => this._stepLightbox(-1));
        this.$lightboxNext?.addEventListener('click', () => this._stepLightbox(1));
        this.$lightbox?.addEventListener('click', (e) => {
            if (e.target === this.$lightbox) this._closeLightbox();
        });
        document.addEventListener('keydown', (e) => {
            if (this.$lightbox?.hidden) return;
            if (e.key === 'Escape') this._closeLightbox();
            if (e.key === 'ArrowLeft') this._stepLightbox(-1);
            if (e.key === 'ArrowRight') this._stepLightbox(1);
        });

        // Tag click → set search query to the tag (Browse tab only)
        this.$detailTags?.addEventListener('click', (e) => {
            const t = e.target.closest('[data-tag]');
            if (!t) return;
            const tag = t.dataset.tag;
            this.closeDetail();
            this.setTab('browse');
            if (this.$search) {
                this.$search.value = tag;
                this.state.query = tag.toLowerCase();
                this.render();
            }
        });
```

- [ ] **Step 4: Add lightbox helper methods**

Add to the `ModsView` object (right before `closeDetail`):

```js
    _openLightbox(idx) {
        if (!this.$lightbox || !this._currentShots) return;
        if (idx < 0 || idx >= this._currentShots.length) return;
        this._lightboxIdx = idx;
        this.$lightboxImg.src = this._currentShots[idx];
        this.$lightbox.hidden = false;
    },
    _closeLightbox() {
        if (!this.$lightbox) return;
        this.$lightbox.hidden = true;
        this.$lightboxImg.removeAttribute('src');
    },
    _stepLightbox(delta) {
        if (!this._currentShots || this._currentShots.length === 0) return;
        const next = (this._lightboxIdx + delta + this._currentShots.length) % this._currentShots.length;
        this._openLightbox(next);
    },
```

- [ ] **Step 5: Run existing JS tests to confirm no regression**

Run: `cd teralaunch && npm test -- --run 2>&1 | tail -20`
Expected: all 464 existing tests still pass.

- [ ] **Step 6: Commit**

```bash
git add teralaunch/src/mods.js
git commit -m "feat(mods): populate hero, callout, before/after, lightbox, tags in detail panel"
```

---

### Task 8: Row card thumbnail + tagline + tags

**Files:**
- Modify: `teralaunch/src/mods.js` (the row-rendering function — search for `mods-row` or `renderRow`)

- [ ] **Step 1: Locate the row renderer**

Run: `cd teralaunch && grep -n "mods-row" src/mods.js | head -10`
Note the function that builds row HTML.

- [ ] **Step 2: Add thumb + tagline + tags to the row template**

Inside the row HTML template (typically a template-literal returning a string), find where the existing icon/initials block is and replace it with a thumbnail-first variant. Keep this concrete adjustment minimal — the exact replacement depends on the row template found in step 1. Append a `mods-row-tags` div after the existing description line:

```js
const thumb = entry.featured_image
    || (entry.screenshots && entry.screenshots[0])
    || entry.icon_url
    || '';
const thumbHtml = thumb
    ? `<img class="mods-row-thumb" src="${escapeHtml(thumb)}" alt="" loading="lazy" />`
    : `<div class="mods-row-thumb">${toInitials(entry.name || entry.id)}</div>`;
const tagline = entry.tagline || entry.description || entry.short_description || '';
const firstTags = (entry.tags || []).slice(0, 2);
const tagsHtml = firstTags.length === 0
    ? ''
    : `<div class="mods-row-tags">${firstTags.map(t => `<span class="mods-row-tag">${escapeHtml(t)}</span>`).join('')}</div>`;
```

Insert `${thumbHtml}` where the icon/initials block currently sits. Insert `${tagline}` where short_description is rendered. Insert `${tagsHtml}` after the description div.

- [ ] **Step 3: Run JS tests**

Run: `cd teralaunch && npm test -- --run tests/mods 2>&1 | tail -20`
Expected: tests pass; if any DOM-snapshot test breaks, update its expected HTML to match the new row shape.

- [ ] **Step 4: Commit**

```bash
git add teralaunch/src/mods.js
git commit -m "feat(mods): row cards show thumbnail, tagline, and tag chips"
```

---

### Task 9: Vitest covering `openDetail` new-field rendering

**Files:**
- Create: `teralaunch/tests/mods-detail-render.test.js`

- [ ] **Step 1: Write the failing test file**

Create:

```js
import { describe, it, expect, beforeEach } from 'vitest';
import { JSDOM } from 'jsdom';

// We import after DOM is available; mods.js attaches handlers but the
// rendering paths are pure functions of the mod entry.

describe('openDetail rendering', () => {
    let dom, doc;

    beforeEach(async () => {
        dom = new JSDOM(`
            <html><body>
                <div id="mods-detail-backdrop" hidden></div>
                <div id="mods-detail-hero" hidden><img id="mods-detail-hero-img" /></div>
                <div id="mods-detail-icon"></div>
                <h2 id="mods-detail-name"></h2>
                <span id="mods-detail-author"></span>
                <span id="mods-detail-version"></span>
                <span id="mods-detail-category-pill" hidden></span>
                <span id="mods-detail-size-text"></span>
                <div id="mods-detail-tags" hidden></div>
                <a id="mods-detail-source-link" href="#" hidden></a>
                <div id="mods-detail-callout" hidden><div id="mods-detail-callout-body"></div></div>
                <div id="mods-detail-description"></div>
                <section id="mods-detail-beforeafter-section" hidden>
                    <img id="mods-detail-before-img" />
                    <img id="mods-detail-after-img" />
                </section>
                <section id="mods-detail-screenshots-section" hidden>
                    <div id="mods-detail-screenshots"></div>
                </section>
                <dd id="mods-detail-fact-author"></dd>
                <div id="mods-detail-fact-license-row" hidden><dd id="mods-detail-fact-license"></dd></div>
                <div id="mods-detail-fact-credits-row" hidden><dd id="mods-detail-fact-credits"></dd></div>
                <div id="mods-detail-fact-patch-row" hidden><dd id="mods-detail-fact-patch"></dd></div>
                <div id="mods-detail-fact-gpkfiles-row" hidden><dd id="mods-detail-fact-gpkfiles"></dd></div>
                <div id="mods-lightbox" hidden><img id="mods-lightbox-img"/><button id="mods-lightbox-close"></button><button id="mods-lightbox-prev"></button><button id="mods-lightbox-next"></button></div>
            </body></html>
        `);
        doc = dom.window.document;
        global.document = doc;
        global.window = dom.window;
        global.HTMLElement = dom.window.HTMLElement;
    });

    it('shows hero when featured_image present', async () => {
        const { ModsView } = await import('../src/mods.js?t=' + Date.now());
        ModsView.setupElements();
        ModsView.state = {
            installed: [],
            catalog: [{
                id: 't', name: 'T', author: 'A', version: '1', kind: 'gpk',
                featured_image: 'https://example.com/hero.png',
                screenshots: [], tags: [], gpk_files: [],
            }],
        };
        ModsView.openDetail('t', 'browse');
        expect(doc.getElementById('mods-detail-hero').hidden).toBe(false);
        expect(doc.getElementById('mods-detail-hero-img').src).toContain('hero.png');
    });

    it('shows tags when present', async () => {
        const { ModsView } = await import('../src/mods.js?t=' + Date.now());
        ModsView.setupElements();
        ModsView.state = {
            installed: [],
            catalog: [{
                id: 't', name: 'T', author: 'A', version: '1', kind: 'gpk',
                tags: ['minimap', 'foglio'],
                screenshots: [], gpk_files: [],
            }],
        };
        ModsView.openDetail('t', 'browse');
        const tagsHost = doc.getElementById('mods-detail-tags');
        expect(tagsHost.hidden).toBe(false);
        expect(tagsHost.innerHTML).toContain('minimap');
        expect(tagsHost.innerHTML).toContain('foglio');
    });

    it('renders compat notes through markdown', async () => {
        const { ModsView } = await import('../src/mods.js?t=' + Date.now());
        ModsView.setupElements();
        ModsView.state = {
            installed: [],
            catalog: [{
                id: 't', name: 'T', author: 'A', version: '1', kind: 'gpk',
                compatibility_notes: 'Conflicts with **Other**',
                screenshots: [], tags: [], gpk_files: [],
            }],
        };
        ModsView.openDetail('t', 'browse');
        expect(doc.getElementById('mods-detail-callout').hidden).toBe(false);
        expect(doc.getElementById('mods-detail-callout-body').innerHTML).toContain('<strong>Other</strong>');
    });

    it('shows before/after only when both before_image and featured_image exist', async () => {
        const { ModsView } = await import('../src/mods.js?t=' + Date.now());
        ModsView.setupElements();
        ModsView.state = {
            installed: [],
            catalog: [{
                id: 't', name: 'T', author: 'A', version: '1', kind: 'gpk',
                featured_image: 'https://example.com/after.png',
                before_image: 'https://example.com/before.png',
                screenshots: [], tags: [], gpk_files: [],
            }],
        };
        ModsView.openDetail('t', 'browse');
        const ba = doc.getElementById('mods-detail-beforeafter-section');
        expect(ba.hidden).toBe(false);
    });

    it('hides before/after when only one image present', async () => {
        const { ModsView } = await import('../src/mods.js?t=' + Date.now());
        ModsView.setupElements();
        ModsView.state = {
            installed: [],
            catalog: [{
                id: 't', name: 'T', author: 'A', version: '1', kind: 'gpk',
                featured_image: 'https://example.com/after.png',
                screenshots: [], tags: [], gpk_files: [],
            }],
        };
        ModsView.openDetail('t', 'browse');
        expect(doc.getElementById('mods-detail-beforeafter-section').hidden).toBe(true);
    });

    it('shows gpk_files in details', async () => {
        const { ModsView } = await import('../src/mods.js?t=' + Date.now());
        ModsView.setupElements();
        ModsView.state = {
            installed: [],
            catalog: [{
                id: 't', name: 'T', author: 'A', version: '1', kind: 'gpk',
                gpk_files: ['S1UI_Chat2.gpk', 'S1UI_Inventory.gpk'],
                screenshots: [], tags: [],
            }],
        };
        ModsView.openDetail('t', 'browse');
        expect(doc.getElementById('mods-detail-fact-gpkfiles-row').hidden).toBe(false);
        expect(doc.getElementById('mods-detail-fact-gpkfiles').textContent).toBe('S1UI_Chat2.gpk, S1UI_Inventory.gpk');
    });
});
```

- [ ] **Step 2: Run the new tests**

Run: `cd teralaunch && npx vitest run tests/mods-detail-render.test.js 2>&1 | tail -25`
Expected: all 6 pass.

- [ ] **Step 3: Commit**

```bash
git add teralaunch/tests/mods-detail-render.test.js
git commit -m "test(mods): cover detail panel new-field rendering"
```

---

### Task 10: Playwright e2e for the detail panel

**Files:**
- Create: `teralaunch/tests/e2e/mods-detail.spec.js`

- [ ] **Step 1: Write the e2e**

Create:

```js
import { test, expect } from '@playwright/test';

const STUB_CATALOG = {
    version: 1,
    updated_at: '2026-04-25',
    mods: [{
        id: 'fixture.demo',
        kind: 'gpk',
        name: 'Demo Mod',
        author: 'Tester',
        short_description: 'Demo',
        long_description: 'Long **bold** description',
        version: '1.0.0',
        download_url: 'https://example.com/x.gpk',
        sha256: '0'.repeat(64),
        category: 'ui',
        tagline: 'Punchy hook',
        featured_image: 'https://example.com/after.png',
        before_image: 'https://example.com/before.png',
        tags: ['minimap', 'foglio'],
        gpk_files: ['S1UI_Chat2.gpk'],
        screenshots: ['https://example.com/s1.png', 'https://example.com/s2.png'],
        last_verified_patch: 'patch 113',
        license: 'MIT',
    }],
};

test.describe('Mod detail panel', () => {
    test.beforeEach(async ({ page }) => {
        await page.route('**/external-mod-catalog/**', r => r.fulfill({ json: STUB_CATALOG }));
        await page.goto('/'); // assumes baseURL is the dev server
        await page.waitForSelector('#mods-button, [data-translate="MODS_OPEN"]', { timeout: 10000 });
    });

    test('opens detail with hero, tags, before/after, lightbox', async ({ page }) => {
        // Open mods modal
        await page.click('#mods-button');
        await page.click('[data-tab="browse"]');
        await page.waitForSelector('.mods-row');
        // Open the detail panel
        await page.click('.mods-row-body');

        // Hero
        await expect(page.locator('#mods-detail-hero')).toBeVisible();
        // Tags
        await expect(page.locator('#mods-detail-tags')).toBeVisible();
        await expect(page.locator('.mods-detail-tag').first()).toHaveText(/minimap|foglio/);
        // Before/after
        await expect(page.locator('#mods-detail-beforeafter-section')).toBeVisible();
        // Open lightbox via screenshot click
        await page.click('#mods-detail-screenshots img >> nth=0');
        await expect(page.locator('#mods-lightbox')).toBeVisible();
        // Close via Escape
        await page.keyboard.press('Escape');
        await expect(page.locator('#mods-lightbox')).toBeHidden();
    });

    test('clicking a tag filters the browse list', async ({ page }) => {
        await page.click('#mods-button');
        await page.click('[data-tab="browse"]');
        await page.click('.mods-row-body');
        await page.click('.mods-detail-tag >> nth=0');
        await expect(page.locator('#mods-search')).toHaveValue(/minimap|foglio/);
    });
});
```

- [ ] **Step 2: Run the e2e**

Run: `cd teralaunch && npx playwright test tests/e2e/mods-detail.spec.js 2>&1 | tail -30`
Expected: tests pass against the dev server.

- [ ] **Step 3: Commit**

```bash
git add teralaunch/tests/e2e/mods-detail.spec.js
git commit -m "test(mods): e2e for new detail panel and lightbox"
```

---

## Phase 6 — Enrichment tool

### Task 11: Enrichment tool harness

**Files:**
- Create: `tools/enrich-catalog/enrich.py`
- Create: `tools/enrich-catalog/handlers/__init__.py`
- Create: `tools/enrich-catalog/handlers/github.py`
- Create: `tools/enrich-catalog/handlers/tumblr.py`
- Create: `tools/enrich-catalog/patch-date-map.json`
- Create: `tools/enrich-catalog/README.md`
- Create: `tools/enrich-catalog/.gitignore`

- [ ] **Step 1: Patch date map**

Create `tools/enrich-catalog/patch-date-map.json`:

```json
{
    "ranges": [
        {"from": "2014-01-01", "patch": "patch 80"},
        {"from": "2017-06-01", "patch": "patch 95"},
        {"from": "2018-06-01", "patch": "patch 100"},
        {"from": "2020-06-01", "patch": "patch 103"},
        {"from": "2022-01-01", "patch": "patch 110"},
        {"from": "2024-01-01", "patch": "patch 113"},
        {"from": "2025-06-01", "patch": "patch 115"}
    ]
}
```

- [ ] **Step 2: GitHub handler**

Create `tools/enrich-catalog/handlers/github.py`:

```python
"""Fetch + parse a GitHub README for catalog enrichment."""
from __future__ import annotations

import json
import re
import subprocess
from dataclasses import dataclass, field
from typing import Optional
from urllib.parse import urlparse


@dataclass
class GithubMeta:
    owner: str
    repo: str
    description: str = ""
    last_commit_date: str = ""
    readme_markdown: str = ""
    image_urls: list[str] = field(default_factory=list)
    repo_topics: list[str] = field(default_factory=list)


def parse_repo(url: str) -> Optional[tuple[str, str]]:
    p = urlparse(url)
    if "github.com" not in p.netloc:
        return None
    parts = [s for s in p.path.split("/") if s]
    if len(parts) < 2:
        return None
    return parts[0], parts[1]


def fetch(url: str) -> Optional[GithubMeta]:
    parsed = parse_repo(url)
    if not parsed:
        return None
    owner, repo = parsed

    # Repo description + topics + last commit date
    api = subprocess.run(
        ["gh", "api", f"repos/{owner}/{repo}"],
        capture_output=True, text=True
    )
    description, topics = "", []
    if api.returncode == 0:
        body = json.loads(api.stdout)
        description = body.get("description") or ""
        topics = body.get("topics") or []

    last_commit = subprocess.run(
        ["gh", "api", f"repos/{owner}/{repo}/commits", "--jq", ".[0].commit.author.date"],
        capture_output=True, text=True
    )
    last_commit_date = last_commit.stdout.strip() if last_commit.returncode == 0 else ""

    # Try main, then master
    readme = ""
    for branch in ("main", "master"):
        r = subprocess.run(
            ["curl", "-fsSL",
             f"https://raw.githubusercontent.com/{owner}/{repo}/{branch}/README.md"],
            capture_output=True, text=True
        )
        if r.returncode == 0 and r.stdout:
            readme = r.stdout
            break

    images = extract_image_urls(readme, owner, repo)

    return GithubMeta(
        owner=owner, repo=repo,
        description=description,
        last_commit_date=last_commit_date,
        readme_markdown=readme,
        image_urls=images,
        repo_topics=topics,
    )


def extract_image_urls(readme: str, owner: str, repo: str) -> list[str]:
    """Find ![](url) and <img src=...> URLs, normalising relative paths."""
    if not readme:
        return []
    out: list[str] = []
    for m in re.finditer(r"!\[[^\]]*\]\(([^)]+)\)", readme):
        out.append(m.group(1).strip())
    for m in re.finditer(r'<img[^>]+src="([^"]+)"', readme, re.I):
        out.append(m.group(1).strip())
    # Normalise relative
    fixed: list[str] = []
    for u in out:
        if u.startswith(("http://", "https://")):
            fixed.append(u)
        elif u.startswith("/"):
            fixed.append(f"https://raw.githubusercontent.com/{owner}/{repo}/HEAD{u}")
        else:
            fixed.append(f"https://raw.githubusercontent.com/{owner}/{repo}/HEAD/{u.lstrip('./')}")
    # Dedupe preserving order
    seen, deduped = set(), []
    for u in fixed:
        if u not in seen:
            seen.add(u)
            deduped.append(u)
    return deduped
```

- [ ] **Step 3: Tumblr handler**

Create `tools/enrich-catalog/handlers/tumblr.py`:

```python
"""Fetch + parse a Tumblr post for catalog enrichment."""
from __future__ import annotations

import re
import subprocess
from dataclasses import dataclass, field
from typing import Optional
from urllib.parse import urlparse


@dataclass
class TumblrMeta:
    url: str
    body_text: str = ""
    image_urls: list[str] = field(default_factory=list)


def fetch(url: str) -> Optional[TumblrMeta]:
    p = urlparse(url)
    if "tumblr.com" not in p.netloc:
        return None
    r = subprocess.run(
        ["curl", "-fsSL", "-A", "Mozilla/5.0 (compatible; mod-catalog-enricher)", url],
        capture_output=True, text=True
    )
    if r.returncode != 0:
        return None
    html = r.stdout

    images = re.findall(r'<img[^>]+src="(https://[^"]+\.(?:png|jpg|jpeg|webp|gif))"', html, re.I)
    # Dedupe
    seen, deduped = set(), []
    for u in images:
        if u in seen:
            continue
        seen.add(u)
        deduped.append(u)

    # Strip body — between <article> ... </article> if present, else first <p>
    body_match = re.search(r'<article[^>]*>(.*?)</article>', html, re.S)
    body_html = body_match.group(1) if body_match else ""
    body_text = re.sub(r"<[^>]+>", "", body_html or "")
    body_text = re.sub(r"\s+", " ", body_text).strip()[:1200]

    return TumblrMeta(url=url, body_text=body_text, image_urls=deduped)
```

- [ ] **Step 4: handlers package init**

Create `tools/enrich-catalog/handlers/__init__.py`:

```python
from . import github, tumblr  # noqa: F401
```

- [ ] **Step 5: Driver**

Create `tools/enrich-catalog/enrich.py`:

```python
#!/usr/bin/env python3
"""Bulk enrichment driver. Reads catalog.json, emits catalog.proposed.json
with auto-filled tagline, featured_image, before_image, screenshots,
long_description, last_verified_patch, gpk_files where they're missing.

Usage:
    python enrich.py --in catalog.json --out catalog.proposed.json
    python enrich.py --in catalog.json --out catalog.proposed.json --only classicplus.shinra,foglio1024.restyle-chat

Run after: review catalog.proposed.json by hand, then copy to
external-mod-catalog/catalog.json and PR.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from datetime import date, datetime
from pathlib import Path
from urllib.parse import urlparse

# Ensure repo-root-relative imports work regardless of CWD
sys.path.insert(0, str(Path(__file__).parent))
from handlers import github, tumblr  # noqa: E402


def patch_for_date(iso_date: str, ranges: list[dict]) -> str:
    if not iso_date:
        return ""
    try:
        d = datetime.fromisoformat(iso_date.replace("Z", "+00:00")).date()
    except ValueError:
        return ""
    cur = ""
    for r in ranges:
        if d >= date.fromisoformat(r["from"]):
            cur = r["patch"]
    return cur


def is_landscape_image_url(url: str) -> bool:
    """Cheap heuristic: trust filename hints, since we don't fetch bytes."""
    u = url.lower()
    if any(t in u for t in ("banner", "hero", "preview", "screenshot", "after")):
        return True
    if any(t in u for t in ("icon", "logo", "thumb", "avatar")):
        return False
    return True  # default optimistic


def likely_before_image(url: str) -> bool:
    return any(t in url.lower() for t in ("before", "vanilla", "original"))


def gpk_files_from_text(text: str) -> list[str]:
    if not text:
        return []
    out = set()
    for m in re.finditer(r"\bS1[A-Za-z0-9_]+\.gpk\b", text):
        out.add(m.group(0))
    for m in re.finditer(r"\bPC_Event_\d+\b", text):
        out.add(m.group(0) + ".gpk")
    return sorted(out)


def first_sentence(text: str, max_len: int = 90) -> str:
    s = re.split(r"(?<=[.!?])\s+", (text or "").strip(), maxsplit=1)[0]
    if len(s) > max_len:
        s = s[: max_len - 1].rstrip() + "…"
    return s


def trim_description(text: str, max_paragraphs: int = 3) -> str:
    if not text:
        return ""
    paragraphs = [p.strip() for p in re.split(r"\n\s*\n", text) if p.strip()]
    keep = paragraphs[:max_paragraphs]
    return "\n\n".join(keep)


def enrich_entry(entry: dict, patch_ranges: list[dict]) -> dict:
    src = entry.get("source_url") or ""
    if not src:
        return entry

    host = urlparse(src).netloc.lower()
    meta = None
    if "github.com" in host or "raw.githubusercontent.com" in host:
        meta = github.fetch(src)
    elif "tumblr.com" in host:
        meta = tumblr.fetch(src)

    if not meta:
        return entry

    # featured_image — first landscape candidate
    if not entry.get("featured_image") and meta.image_urls:
        candidates = [u for u in meta.image_urls if is_landscape_image_url(u) and not likely_before_image(u)]
        if candidates:
            entry["featured_image"] = candidates[0]

    # before_image — if any image filename matches the before-pattern
    if not entry.get("before_image"):
        for u in meta.image_urls:
            if likely_before_image(u):
                entry["before_image"] = u
                break

    # screenshots — remaining images, dedup against featured/before, cap 8
    if not entry.get("screenshots"):
        used = {entry.get("featured_image"), entry.get("before_image")}
        rest = [u for u in meta.image_urls if u not in used][:8]
        entry["screenshots"] = rest

    # long_description — first 3 paragraphs of README/body
    if not entry.get("long_description"):
        body = getattr(meta, "readme_markdown", "") or getattr(meta, "body_text", "")
        entry["long_description"] = trim_description(body)

    # tagline — repo description if ≤90 chars, else first sentence
    if not entry.get("tagline"):
        if hasattr(meta, "description") and meta.description and len(meta.description) <= 90:
            entry["tagline"] = meta.description
        else:
            body = getattr(meta, "readme_markdown", "") or getattr(meta, "body_text", "")
            entry["tagline"] = first_sentence(body)

    # last_verified_patch
    if not entry.get("last_verified_patch") and getattr(meta, "last_commit_date", ""):
        entry["last_verified_patch"] = patch_for_date(meta.last_commit_date, patch_ranges)

    # gpk_files
    if not entry.get("gpk_files"):
        body = getattr(meta, "readme_markdown", "") or getattr(meta, "body_text", "")
        entry["gpk_files"] = gpk_files_from_text(body)

    # tags from topics if available
    if not entry.get("tags") and getattr(meta, "repo_topics", []):
        entry["tags"] = list(meta.repo_topics)[:5]

    return entry


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--in", dest="src", required=True, type=Path)
    p.add_argument("--out", dest="dst", required=True, type=Path)
    p.add_argument("--only", default="", help="Comma-separated mod ids to enrich; default = all")
    args = p.parse_args()

    catalog = json.loads(args.src.read_text(encoding="utf-8"))
    patch_ranges = json.loads((Path(__file__).parent / "patch-date-map.json").read_text())["ranges"]

    only = set(filter(None, args.only.split(",")))
    enriched = []
    for entry in catalog["mods"]:
        if only and entry["id"] not in only:
            enriched.append(entry)
            continue
        print(f"-> {entry['id']}", file=sys.stderr)
        enriched.append(enrich_entry(entry, patch_ranges))

    catalog["mods"] = enriched
    args.dst.write_text(json.dumps(catalog, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"Wrote {args.dst} ({len(enriched)} entries)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 6: README**

Create `tools/enrich-catalog/README.md`:

```markdown
# Enrich Catalog

Auto-fills the new schema fields (tagline, featured_image, before_image,
screenshots, long_description, last_verified_patch, gpk_files, tags) from
each mod's `source_url`. Run locally; not shipped to users.

## Requirements
- Python 3.10+
- `gh` CLI authenticated (`gh auth status` should be green)
- `curl`

## Usage
```bash
# Single mod (smoke test)
python tools/enrich-catalog/enrich.py \
    --in /path/to/external-mod-catalog/catalog.json \
    --out /tmp/catalog.proposed.json \
    --only classicplus.shinra

# Full sweep
python tools/enrich-catalog/enrich.py \
    --in /path/to/external-mod-catalog/catalog.json \
    --out /tmp/catalog.proposed.json
```

## Workflow
1. Run the script against `external-mod-catalog/catalog.json`.
2. Diff `catalog.proposed.json` against the input — eyeball every entry.
3. Hand-edit obvious extraction mistakes (wrong hero image picked, generic
   tagline, missing compatibility note).
4. Copy `catalog.proposed.json` over `external-mod-catalog/catalog.json`.
5. Bump `version` in the catalog header.
6. Commit + PR to `external-mod-catalog/main`.

## Conventions
- Hero image preference: README banner > first wide image > none.
- `before_image` is only filled if a filename hints `before|vanilla|original`.
- Tags are repo topics if available — no auto-invention.
- Long descriptions cap at 3 paragraphs.
```

- [ ] **Step 7: gitignore**

Create `tools/enrich-catalog/.gitignore`:

```
*.proposed.json
__pycache__/
*.cache.json
```

- [ ] **Step 8: Smoke-test the script**

Run:
```bash
cd "$(git rev-parse --show-toplevel)"
git clone --depth 1 https://github.com/TERA-Europe-Classic/external-mod-catalog /tmp/emc 2>/dev/null || (cd /tmp/emc && git pull)
python tools/enrich-catalog/enrich.py --in /tmp/emc/catalog.json --out /tmp/proposed.json --only classicplus.shinra 2>&1 | tail -10
python -c "import json; d=json.load(open('/tmp/proposed.json'))['mods']; e=next(m for m in d if m['id']=='classicplus.shinra'); print('featured:', e.get('featured_image')); print('shots:', len(e.get('screenshots', []))); print('tagline:', e.get('tagline'))"
```
Expected: at least `tagline` and either `featured_image` or non-empty `screenshots[]` populated for Shinra.

- [ ] **Step 9: Commit**

```bash
git add tools/enrich-catalog/
git commit -m "feat(tools): catalog enrichment harness with GitHub + Tumblr handlers"
```

---

## Phase 7 — Bulk enrichment + new mod discovery

### Task 12: Run bulk enrichment over all 101 entries

**Files:**
- Modify: `external-mod-catalog/catalog.json` (in the other repo)

This phase is content-heavy and requires manual review per entry. The scripted output is a starting point, not the final answer.

- [ ] **Step 1: Clone the catalog repo into a sibling worktree**

```bash
cd /tmp
rm -rf emc
git clone https://github.com/TERA-Europe-Classic/external-mod-catalog emc
cd emc
git checkout -b enrich-catalog-2026-04-25
```

- [ ] **Step 2: Run the full enrichment**

```bash
cd "$(git -C /tmp/emc rev-parse --show-toplevel)"
LAUNCHER_REPO="$(realpath ~/Documents/GitHub/TERA\ EU\ Classic/TERA-Europe-ClassicPlus-Launcher)"
python "$LAUNCHER_REPO/tools/enrich-catalog/enrich.py" \
    --in catalog.json \
    --out catalog.proposed.json
```
Expected: `Wrote catalog.proposed.json (101 entries)`.

- [ ] **Step 3: Manual review — every entry**

Open `catalog.proposed.json` in your editor. For EACH of the 101 entries:

- Confirm `featured_image` actually shows what the mod does (not the author's profile pic, not an unrelated image). If wrong, replace with a known-good URL from the `screenshots` array.
- Confirm `before_image` is meaningful (only fill it for restyles where the comparison is the killer feature).
- Rewrite `tagline` if the auto-derived one is generic ("Custom for TERA Online"). Aim for ≤90 chars, descriptive of the visual change.
- Trim `long_description` if it pulled in install instructions or unrelated repo blurb.
- Add `compatibility_notes` for known conflicts (Foglio's restyles share scopes; pantypon's PC_Event mods can collide if multiple target the same slot).
- Confirm `gpk_files` is correct — script regex catches most but misses `S1Data_*.gpk` and accessory packs.
- Add 2-3 hand-picked `tags` if the script left it empty.

Estimate: 3-5 min per entry × 101 = 5-8 hours of focused review.

- [ ] **Step 4: Replace and commit**

```bash
mv catalog.proposed.json catalog.json
# Bump catalog version field manually (top of file): "version": <n+1>
git add catalog.json
git commit -m "feat: enrich all 101 catalog entries with hero images, taglines, and details"
```

- [ ] **Step 5: PR to external-mod-catalog**

```bash
git push -u origin enrich-catalog-2026-04-25
gh pr create --repo TERA-Europe-Classic/external-mod-catalog \
    --title "Enrich all 101 mods with hero images, taglines, and details" \
    --body "Bulk enrichment per docs/superpowers/specs/2026-04-25-mod-catalog-overhaul-design.md in the launcher repo. New fields populated for every entry."
```

Do NOT merge yet — the new-mod discovery PR (Task 13) lands in the same merge.

---

### Task 13: Add net-new mods from deep-research findings

**Files:**
- Modify: `external-mod-catalog/catalog.json` (continuation of the same branch as Task 12)

- [ ] **Step 1: Read the deep-research output**

```bash
RESEARCH_OUT=$(python "$HOME/.claude/skills/deep-research/scripts/research.py" --status v1_ChdLSGpzYVlTU090S2trZFVQaTU2ejRBTRIXS0hqc2FZU1NPdEtra2RVUGk1Nno0QU0 --raw 2>&1)
echo "$RESEARCH_OUT" | tee /tmp/discovery.md
```

- [ ] **Step 2: Dedupe candidates against current catalog**

For each candidate from the research output:
1. Extract its `source_url`.
2. `grep -i "$candidate_url" /tmp/emc/catalog.json` — skip if already present.
3. Note the unique candidates in `/tmp/new-mods.md`.

- [ ] **Step 3: Author each new entry**

For each unique candidate:
1. Add a stub to `catalog.json`:
   ```json
   {
       "id": "<author>.<slug>",
       "kind": "gpk",
       "name": "...",
       "author": "...",
       "short_description": "...",
       "version": "...",
       "download_url": "...",
       "sha256": "...",
       "category": "ui|cosmetic|effects|sound|qol",
       "source_url": "..."
   }
   ```
2. Compute SHA: `curl -fsSL <download_url> | sha256sum`. Confirm it matches a stable release asset, not a branch HEAD.
3. Run the enrichment script with `--only <new-id>` to fill the new fields.
4. Hand-review.

- [ ] **Step 4: Commit and push to the same branch**

```bash
cd /tmp/emc
git add catalog.json
git commit -m "feat: add N newly-discovered mods from deep-research sweep"
git push
```

- [ ] **Step 5: Merge the catalog PR**

After the launcher schema PR is verified locally (Task 9 + 10), merge the catalog PR to publish the new content live.

```bash
gh pr merge <pr-num> --squash --repo TERA-Europe-Classic/external-mod-catalog
```

---

## Phase 8 — Snapshot regen + final tests

### Task 14: Regenerate the test fixture from the live catalog

**Files:**
- Modify: `teralaunch/src-tauri/tests/fixtures/catalog-snapshot.json`

- [ ] **Step 1: Fetch the now-merged live catalog**

```bash
curl -fsSL https://raw.githubusercontent.com/TERA-Europe-Classic/external-mod-catalog/main/catalog.json \
    -o teralaunch/src-tauri/tests/fixtures/catalog-snapshot.json
```

- [ ] **Step 2: Run all Rust tests against the new fixture**

```bash
cd teralaunch/src-tauri && cargo test 2>&1 | grep -E "test result|FAILED" | head -20
```
Expected: zero new failures vs the pre-existing 5 (changelog drift, em-dash panics, is_process_running pins).

- [ ] **Step 3: Run all JS tests**

```bash
cd teralaunch && npm test -- --run 2>&1 | tail -20
```
Expected: 464+ tests pass (now plus the new markdown + mods-detail-render specs).

- [ ] **Step 4: Run e2e suite**

```bash
cd teralaunch && npx playwright test 2>&1 | tail -20
```
Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add teralaunch/src-tauri/tests/fixtures/catalog-snapshot.json
git commit -m "test: regenerate catalog snapshot from enriched live catalog"
```

---

## Phase 9 — Release v0.1.27

### Task 15: Final review + deploy

- [ ] **Step 1: Push the launcher branch and merge to main**

```bash
cd "$(git rev-parse --show-toplevel)"
git push origin <branch-name>
gh pr create --title "Mod catalog overhaul: rich detail panel, schema enrichment" \
    --body "Implements docs/superpowers/specs/2026-04-25-mod-catalog-overhaul-design.md. Ships alongside the bulk content PR in external-mod-catalog."
gh pr merge --squash
```

- [ ] **Step 2: Trigger the deploy workflow**

```bash
gh workflow run deploy.yml -R TERA-Europe-Classic/TERA-Europe-ClassicPlus-Launcher \
    --ref main -f bump=minor
```

Note: this is a `minor` bump (0.1.26 → 0.2.0) because the schema additions are user-facing functional changes; if you'd rather keep it as a patch (0.1.26 → 0.1.27), use `bump=patch`.

- [ ] **Step 3: Watch the run**

```bash
gh run list -R TERA-Europe-Classic/TERA-Europe-ClassicPlus-Launcher --limit 1
gh run watch <run-id> --exit-status
```

- [ ] **Step 4: Smoke-test the released NSIS installer**

Download from the GitHub release. Install. Open → Mods → Browse. Click any mod. Verify hero image renders, tags clickable, before/after toggle works on a restyle, lightbox opens on screenshot click, source link launches in browser.

---

## Self-review checklist (run before handoff)

- [ ] Every spec section has at least one task implementing it. Schema (Tasks 1-3 ✓), markdown renderer (Task 4 ✓), detail-panel HTML (Task 5 ✓), CSS (Task 6 ✓), JS rewrite (Task 7 ✓), row card (Task 8 ✓), tests (Tasks 9-10 ✓), enrichment tool (Task 11 ✓), bulk enrichment (Task 12 ✓), discovery (Task 13 ✓), snapshot regen (Task 14 ✓), release (Task 15 ✓).
- [ ] No "TODO", "TBD", or "implement appropriate handling" in any step. ✓
- [ ] Every code-changing step shows the actual code. ✓
- [ ] File paths are absolute or repo-relative, never ambiguous. ✓
- [ ] Type / method names consistent across tasks. (`renderMarkdown`, `_openLightbox`, `featured_image`, `before_image`, `gpk_files` — all spelled the same throughout.)
- [ ] Bite-sized: every step is one action (read/write/run/commit), 2-5 min. ✓
- [ ] Frequent commits: 12+ separate commits planned. ✓
