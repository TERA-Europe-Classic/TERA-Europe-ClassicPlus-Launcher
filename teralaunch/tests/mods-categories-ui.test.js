/**
 * fix.mods-categories-ui (iter 85) — user-reported layout + style defect
 * on the mods modal filter strip. Before iter 85:
 *
 *   - Kind-filter chips (All / External / GPK) lived inside a
 *     `.mods-filter-group` segmented-control wrapper (rounded-rect chips
 *     with a 6px radius inside a bordered box).
 *   - Category chips (All categories / Cosmetic / Effects / ...) were
 *     full pills (999px radius) floating in a row below.
 *   - Kind-filter sat inline with the search bar; categories hung on
 *     their own row below. Together that formed an "L-shape" with two
 *     stylistically inconsistent chip types.
 *
 * iter 85 merges both into one `.mods-filters-row` strip where kind and
 * category chips share the same `.mods-filter-chip` class (identical
 * pill geometry) and a thin vertical divider separates the two groups.
 *
 * This test pins the contract so a future refactor can't silently
 * regress the fix:
 *   (i)   Both groups emit the same `.mods-filter-chip` class.
 *   (ii)  DOM order inside `.mods-filters-row` is [kind group] →
 *         [divider] → [category group].
 *   (iii) Inside each group, exactly one chip has `.active`.
 *   (iv)  The `.mods-category-chip` class is GONE from source (proves
 *         the dead CSS and JS references were pruned — a future refactor
 *         that reintroduces the dual-class world would fail here).
 */
import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const REPO_ROOT = path.resolve(import.meta.dirname, '..', '..');
const MODS_HTML = path.join(REPO_ROOT, 'teralaunch/src/mods.html');
const MODS_JS = path.join(REPO_ROOT, 'teralaunch/src/mods.js');
const MODS_CSS = path.join(REPO_ROOT, 'teralaunch/src/mods.css');

describe('mods filter strip — fix.mods-categories-ui', () => {
    it('mods.html ships the .mods-filters-row container with kind → divider → category order', () => {
        const html = fs.readFileSync(MODS_HTML, 'utf8');
        const rowOpen = html.indexOf('class="mods-filters-row"');
        expect(rowOpen, '.mods-filters-row must exist').toBeGreaterThan(0);

        // Scope to the first 2000 chars after the row opens — comfortably
        // covers the two groups + divider without spilling into panes.
        const rowSlice = html.slice(rowOpen, rowOpen + 2000);

        const kindGroupIdx = rowSlice.indexOf('class="mods-filter-group"');
        const dividerIdx = rowSlice.indexOf('mods-filters-divider');
        const categoryRowIdx = rowSlice.indexOf('id="mods-category-row"');

        expect(kindGroupIdx, 'kind-filter group must be inside the filters row').toBeGreaterThan(0);
        expect(dividerIdx, 'divider must be inside the filters row').toBeGreaterThan(0);
        expect(categoryRowIdx, 'category row must be inside the filters row').toBeGreaterThan(0);

        expect(kindGroupIdx, 'kind group before divider').toBeLessThan(dividerIdx);
        expect(dividerIdx, 'divider before category row').toBeLessThan(categoryRowIdx);
    });

    it('both kind and category chips use the same .mods-filter-chip class', () => {
        const html = fs.readFileSync(MODS_HTML, 'utf8');
        // Kind chips: 3 fixed buttons with data-filter
        const kindChipMatches = html.match(/class="mods-filter-chip[^"]*"\s+data-filter=/g) || [];
        expect(kindChipMatches.length, 'three kind chips (All/External/GPK)').toBe(3);

        // Category chip seed (the "All categories" default): also uses
        // .mods-filter-chip — no more .mods-category-chip class in HTML.
        const seedCategoryChip = /class="mods-filter-chip[^"]*"\s+data-category="all"/.test(html);
        expect(seedCategoryChip, 'seed "All categories" chip uses .mods-filter-chip').toBe(true);
    });

    it('mods.js renders dynamic category chips with .mods-filter-chip', () => {
        const js = fs.readFileSync(MODS_JS, 'utf8');
        // The two dynamic-render lines inside renderCategoryChips() must
        // emit .mods-filter-chip, not the legacy .mods-category-chip.
        const emitsFilterChip = /<button class="mods-filter-chip\s/.test(js);
        expect(emitsFilterChip, 'renderCategoryChips must emit .mods-filter-chip').toBe(true);
    });

    it('the legacy .mods-category-chip class is gone from all sources', () => {
        // A future refactor that reintroduces .mods-category-chip would
        // bring back the two-style split. Hard-fail on any occurrence.
        for (const file of [MODS_HTML, MODS_JS, MODS_CSS]) {
            const src = fs.readFileSync(file, 'utf8');
            expect(
                src.includes('mods-category-chip'),
                `legacy class .mods-category-chip leaked back into ${path.basename(file)}`,
            ).toBe(false);
        }
    });

    it('kind-filter click handler is scoped to .mods-filter-group', () => {
        // Without the scope, the global `.mods-filter-chip` selector would
        // also hit category chips and double-bind them to setFilter(undefined).
        const js = fs.readFileSync(MODS_JS, 'utf8');
        const scopedClick = js.includes(
            ".mods-filter-group .mods-filter-chip"
        );
        expect(scopedClick, 'kind-filter click handler must scope to .mods-filter-group').toBe(true);
    });

    it('.mods-filter-chip and .mods-filter-chip.active have unified pill styling in CSS', () => {
        const css = fs.readFileSync(MODS_CSS, 'utf8');
        // Base rule: 999px border-radius (pill), 4px/10px padding, 11px font.
        // Grep a single block around the base `.mods-filter-chip {` declaration.
        const baseIdx = css.indexOf('.mods-filter-chip {');
        expect(baseIdx).toBeGreaterThan(0);
        const baseSlice = css.slice(baseIdx, baseIdx + 400);
        expect(baseSlice).toContain('border-radius: 999px');
        expect(baseSlice).toContain('padding: 4px 10px');
        expect(baseSlice).toContain('font-size: 11px');

        // Active rule must carry the teal border treatment (previously only
        // .mods-category-chip.active had it). Losing this drops the active
        // state back to the old segmented-control-looking chip.
        const activeIdx = css.indexOf('.mods-filter-chip.active {');
        expect(activeIdx).toBeGreaterThan(0);
        const activeSlice = css.slice(activeIdx, activeIdx + 200);
        expect(activeSlice, 'active state must set teal border').toMatch(/border-color:\s*rgba\(34,\s*211,\s*238/);
    });

    it('only one chip has .active inside each group — seed state', () => {
        // In the HTML seed, the kind group marks `All` as active and the
        // category row marks `All categories` as active. No chip is
        // accidentally double-marked, and no chip outside the filter row
        // carries .active from this strip.
        const html = fs.readFileSync(MODS_HTML, 'utf8');
        const rowOpen = html.indexOf('class="mods-filters-row"');
        const rowClose = html.indexOf('</div>', html.indexOf('id="mods-category-row"'));
        const rowSlice = html.slice(rowOpen, rowClose);

        const kindActive = (rowSlice.match(
            /class="mods-filter-chip active"\s+data-filter=/g,
        ) || []).length;
        const categoryActive = (rowSlice.match(
            /class="mods-filter-chip active"\s+data-category=/g,
        ) || []).length;

        expect(kindActive, 'exactly 1 kind chip starts active').toBe(1);
        expect(categoryActive, 'exactly 1 category chip starts active').toBe(1);
    });
});
