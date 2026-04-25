/**
 * Task 9 — openDetail() new-field rendering coverage.
 *
 * Pins the show/hide rules and content-population paths added in iter 35
 * for the catalog-driven detail panel: hero image, tags, compatibility
 * callout (markdown), before/after panel (gated on having BOTH images),
 * and the gpk_files fact row.
 *
 * The lightbox + tag-click handlers have richer interactions and live in
 * the Playwright e2e suite (Task 10) — this file stays focused on the
 * pure rendering branches inside openDetail.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { JSDOM } from 'jsdom';

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
        // mods.js destructures window.__TAURI__.{core,event} at module load,
        // so the stub must exist on the window the module sees. We point
        // global.window at the fresh JSDOM and seed the Tauri shape.
        dom.window.__TAURI__ = {
            core: { invoke: async () => undefined },
            tauri: { invoke: async () => undefined },
            event: { listen: async () => () => {} },
        };
        global.document = doc;
        global.window = dom.window;
        global.HTMLElement = dom.window.HTMLElement;
    });

    it('shows hero when featured_image present', async () => {
        const { ModsView } = await import('../src/mods.js?t=' + Date.now());
        ModsView.cacheDom();
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
        ModsView.cacheDom();
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
        ModsView.cacheDom();
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
        ModsView.cacheDom();
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
        ModsView.cacheDom();
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
        ModsView.cacheDom();
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
