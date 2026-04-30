/**
 * Verifies the new runUpdateCheck() flow on mod-manager open:
 *   - calls Rust command `check_mod_updates`
 *   - re-loads installed mods so the registry-side `update_available`
 *     flip is reflected in the UI without forcing the user to reopen
 *   - is best-effort (network failure does not throw)
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { JSDOM } from 'jsdom';

describe('ModsView.runUpdateCheck', () => {
    let dom, doc, invokeMock;

    beforeEach(async () => {
        dom = new JSDOM(`<html><body></body></html>`);
        doc = dom.window.document;
        invokeMock = vi.fn();
        dom.window.__TAURI__ = {
            core: { invoke: invokeMock },
            tauri: { invoke: invokeMock },
            event: { listen: async () => () => {} },
        };
        global.document = doc;
        global.window = dom.window;
        global.HTMLElement = dom.window.HTMLElement;
    });

    it('invokes check_mod_updates and refreshes the installed list', async () => {
        invokeMock.mockImplementation(async (cmd) => {
            if (cmd === 'check_mod_updates') return [{ id: 'classicplus.shinra' }];
            if (cmd === 'list_installed_mods') return [{
                id: 'classicplus.shinra',
                name: 'Shinra',
                kind: 'external',
                status: 'update_available',
                version: '3.0.8-classicplus',
                enabled: true,
            }];
            return null;
        });

        const { ModsView } = await import('../src/mods.js?t=' + Date.now());
        ModsView.state.catalog = [];
        await ModsView.runUpdateCheck();

        expect(invokeMock).toHaveBeenCalledWith('check_mod_updates');
        expect(invokeMock).toHaveBeenCalledWith('list_installed_mods');
        const installed = ModsView.state.installed;
        expect(Array.isArray(installed)).toBe(true);
        expect(installed[0]?.status).toBe('update_available');
    });

    it('swallows command errors so the manager stays usable offline', async () => {
        invokeMock.mockImplementation(async () => { throw new Error('network down'); });
        const { ModsView } = await import('../src/mods.js?t=' + Date.now());
        ModsView.state.catalog = [];
        ModsView.state.installed = [];
        await expect(ModsView.runUpdateCheck()).resolves.toBeUndefined();
    });
});
