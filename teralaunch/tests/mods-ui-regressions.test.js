import { describe, it, expect, beforeEach, vi } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const REPO_ROOT = path.resolve(import.meta.dirname, '..', '..');
const MODS_HTML = path.join(REPO_ROOT, 'teralaunch/src/mods.html');
const MODS_JS = path.join(REPO_ROOT, 'teralaunch/src/mods.js');
const MODS_CSS = path.join(REPO_ROOT, 'teralaunch/src/mods.css');

describe('mods UI regression fixes', () => {
    beforeEach(() => {
        vi.resetModules();
        document.body.innerHTML = '<div id="mods-modal" hidden><div id="mods-modal-content"></div></div>';
        global.fetch = vi.fn(async () => ({
            text: async () => fs.readFileSync(MODS_HTML, 'utf8'),
        }));
        window.App = {
            updateAllTranslations: vi.fn(async () => {}),
            t: (_key, fallback) => fallback,
        };
    });

    it('reconciles installed update_available state once catalog data arrives on first open', async () => {
        const installed = [{
            id: 'classicplus.tcc',
            kind: 'external',
            name: 'TCC',
            author: 'Classic+',
            version: '1.0.0',
            status: 'enabled',
            enabled: true,
        }];
        const catalog = [{
            id: 'classicplus.tcc',
            kind: 'external',
            name: 'TCC',
            author: 'Classic+',
            version: '1.1.0',
            category: 'utility',
        }];

        const mockInvoke = vi.fn(async (cmd, args) => {
            if (cmd === 'list_installed_mods') {
                return installed.map((entry) => ({ ...entry }));
            }
            if (cmd === 'get_mods_catalog') {
                if (!args?.forceRefresh) {
                    await new Promise((resolve) => setTimeout(resolve, 10));
                }
                return { mods: catalog.map((entry) => ({ ...entry })) };
            }
            return null;
        });

        window.__TAURI__ = {
            core: { invoke: mockInvoke },
            tauri: { invoke: mockInvoke },
            event: { listen: vi.fn(async () => () => {}) },
            dialog: { open: vi.fn() },
        };

        const { ModsView } = await import('../src/mods.js');

        await ModsView.open();

        expect(ModsView.state.installed[0]?.status).toBe('update_available');
    });

    it('renders download tray items with dedicated progress and bytes regions', () => {
        const js = fs.readFileSync(MODS_JS, 'utf8');
        expect(js).toContain('mods-download-tray-progress');
        expect(js).toContain('mods-download-tray-bytes');
        expect(js).toContain('mods-download-tray-item-meta');
    });

    it('ships tray and progress-label CSS that stabilizes percentage width', () => {
        const css = fs.readFileSync(MODS_CSS, 'utf8');
        expect(css).toContain('.mods-download-tray');
        expect(css).toContain('.mods-download-tray-progress');
        expect(css).toMatch(/min-width:\s*4ch/);
        expect(css).toMatch(/font-variant-numeric:\s*tabular-nums/);
    });

    it('makes the destructive confirmation button black', () => {
        const css = fs.readFileSync(MODS_CSS, 'utf8');
        expect(css).toContain('.mods-onboarding-btn.danger');
        expect(css).toMatch(/\.mods-onboarding-btn\.danger\s*\{[^}]*background:\s*#000/i);
    });

    it('restores text selection in the mods search input', () => {
        const css = fs.readFileSync(MODS_CSS, 'utf8');
        expect(css).toMatch(/\.mods-search\s*\{[^}]*user-select:\s*text/i);
        expect(css).toMatch(/\.mods-search\s*\{[^}]*-webkit-user-select:\s*text/i);
    });
});
