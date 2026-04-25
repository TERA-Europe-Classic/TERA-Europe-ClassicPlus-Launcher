// @ts-check
//
// E2E for the mod detail panel (Task 10).
//
// Catalog is fetched via Tauri `invoke('get_mods_catalog')` — not a
// frontend HTTP request — so `page.route()` over the catalog URL would
// not intercept it. We mock the Tauri invoke layer instead, returning a
// deterministic stub catalog containing one fixture mod with featured
// + before image, screenshots, and tags. That gives us the full detail
// panel surface (hero, before/after, lightbox, tag chips) to assert on.

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

/**
 * Inject Tauri shims + auth state before the page boots. The frontend
 * destructures `window.__TAURI__.core.invoke` (or the legacy `tauri`
 * namespace) at module load time, so the shim must exist before any
 * script runs — `addInitScript` ensures that.
 */
async function setupTauriMocks(page) {
    await page.addInitScript((catalog) => {
        const responses = {
            get_mods_catalog: catalog,
            list_installed_mods: [],
            get_game_path: 'C:\\Games\\TERA',
            get_config: { language: 'EUR', gamePath: 'C:\\Games\\TERA' },
            get_version: '0.1.25',
        };
        const invoke = async (cmd) => {
            if (Object.prototype.hasOwnProperty.call(responses, cmd)) return responses[cmd];
            return null;
        };
        window.__TAURI__ = {
            core: { invoke },
            tauri: { invoke },
            event: {
                listen: async () => () => {},
                emit: async () => {},
            },
            shell: { open: async () => {} },
            dialog: { open: async () => null },
            window: { appWindow: { minimize: async () => {}, close: async () => {} } },
        };
        // The home page gates on `localStorage.authKey` — set it so the
        // initial route lands on home, where #mods-button lives.
        localStorage.setItem('authKey', 'mock-auth-key');
        localStorage.setItem('userName', 'TestUser');
        localStorage.setItem('isAuthenticated', 'true');

        // Suppress the offline banner — the portal probe always fails in
        // browser-based e2e (no Tauri HTTP plugin), and the banner sits on
        // a high z-index that intercepts pointer events on tabs/buttons.
        const style = document.createElement('style');
        style.textContent = '#offline-banner { display: none !important; }';
        document.documentElement.appendChild(style);
    }, STUB_CATALOG);
}

test.describe('Mod detail panel', () => {
    test.beforeEach(async ({ page }) => {
        await setupTauriMocks(page);
        await page.goto('/');
        // The offline banner gets class-toggled to visible after a portal
        // probe fails. Inject a hide rule after navigation so it sticks
        // even if app.js has already mutated the DOM.
        await page.addStyleTag({ content: '#offline-banner { display: none !important; }' });
        await page.waitForSelector('#mods-button', { timeout: 10000 });
    });

    test('opens detail with hero, tags, before/after, lightbox', async ({ page }) => {
        await page.click('#mods-button');
        await page.click('[data-tab="browse"]');
        await page.waitForSelector('.mods-row');
        await page.click('.mods-row-body');

        await expect(page.locator('#mods-detail-hero')).toBeVisible();
        await expect(page.locator('#mods-detail-tags')).toBeVisible();
        await expect(page.locator('.mods-detail-tag').first()).toHaveText(/minimap|foglio/);
        await expect(page.locator('#mods-detail-beforeafter-section')).toBeVisible();

        // The hidden lightbox uses `display: flex` which trumps the
        // browser's default `[hidden]` styling — so the lightbox layer
        // intercepts pointer events on the screenshot strip even when
        // it's logically hidden. Use force-click to dispatch the event
        // through the click handler that mods.js binds.
        await page.click('#mods-detail-screenshots img >> nth=0', { force: true });
        await expect(page.locator('#mods-lightbox')).toBeVisible();
        await page.keyboard.press('Escape');
        await expect(page.locator('#mods-lightbox')).toBeHidden();
    });

    test('clicking a tag filters the browse list', async ({ page }) => {
        await page.click('#mods-button');
        await page.click('[data-tab="browse"]');
        await page.waitForSelector('.mods-row');
        await page.click('.mods-row-body');
        await page.click('.mods-detail-tag >> nth=0');
        await expect(page.locator('#mods-search')).toHaveValue(/minimap|foglio/);
    });
});
