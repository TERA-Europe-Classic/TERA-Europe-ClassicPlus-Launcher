import { describe, it, expect, beforeEach, vi } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const REPO_ROOT = path.resolve(import.meta.dirname, '..', '..');
const MODS_HTML = path.join(REPO_ROOT, 'teralaunch/src/mods.html');

describe('mods keyboard contract', () => {
    beforeEach(() => {
        vi.resetModules();
        document.body.innerHTML = '';
        global.fetch = vi.fn(async () => ({
            text: async () => fs.readFileSync(MODS_HTML, 'utf8'),
        }));
        window.App = {
            updateAllTranslations: vi.fn(async () => {}),
            t: (_key, fallback) => fallback,
        };
    });

    it('uses native keyboard-focusable controls for the primary mods UI actions', () => {
        const html = fs.readFileSync(MODS_HTML, 'utf8');

        expect(html).toContain('<button class="mods-tab active"');
        expect(html).toContain('<button id="mods-import-btn"');
        expect(html).toContain('<button id="mods-folder-btn"');
        expect(html).toContain('<button class="mods-filter-chip active" data-filter="all"');
        expect(html).toContain('<button class="mods-filter-chip active" data-category="all"');
        expect(html).toContain('<input type="text" id="mods-search"');
    });

    it('modalConfirm resolves false on Escape and true on Enter', async () => {
        window.__TAURI__ = {
            core: { invoke: vi.fn() },
            tauri: { invoke: vi.fn() },
            event: { listen: vi.fn(async () => () => {}) },
            dialog: { open: vi.fn() },
        };

        const { ModsView } = await import('../src/mods.js');

        const escapePromise = ModsView.modalConfirm({ title: 'Uninstall TCC?' });
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        await expect(escapePromise).resolves.toBe(false);

        const enterPromise = ModsView.modalConfirm({ title: 'Uninstall TCC?' });
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
        await expect(enterPromise).resolves.toBe(true);
    });
});
