import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

const mockInvoke = vi.fn();

function encodeCredentials(username, password) {
    return btoa(JSON.stringify({ u: username, p: password }));
}

async function loadAppForLaunchTest() {
    vi.resetModules();

    globalThis.createRouter = vi.fn(() => ({ navigate: vi.fn() }));
    globalThis.startOAuth = vi.fn();

    window.__TAURI__ = {
        core: { invoke: mockInvoke },
        tauri: { invoke: mockInvoke },
        event: { listen: vi.fn(async () => () => {}) },
        window: { appWindow: { minimize: vi.fn(), close: vi.fn() } },
        dialog: { message: vi.fn(async () => {}), ask: vi.fn(async () => false) },
        shell: { open: vi.fn() },
        updater: { checkUpdate: vi.fn(), installUpdate: vi.fn() },
        process: { relaunch: vi.fn() },
        app: { getVersion: vi.fn(async () => '1.0.0') },
        fs: { writeTextFile: vi.fn() },
    };

    await import('../src/app.js');
    const app = window.App;

    app.updateUI = vi.fn();
    app.updateUIForGameStatus = vi.fn();
    app.updateAccountDisplay = vi.fn();
    app.updateLaunchButtonState = vi.fn();
    app.startGameStatusRecoveryInterval = vi.fn();
    app.checkLeaderboardConsent = vi.fn(async () => false);
    app.maybeShowModsOnboarding = vi.fn(() => false);
    app.t = (_key, fallback) => fallback || _key;
    window.showUpdateNotification = vi.fn();
    window.ModsView = { open: vi.fn(async () => {}) };
    app.statusEl = document.createElement('div');
    app.state.isAuthenticated = true;
    app.state.isUpdateAvailable = false;
    app.state.isGameLaunching = false;

    return app;
}

function setActivePasswordAccount() {
    localStorage.setItem('tera_accounts', JSON.stringify([{
        userNo: '1001',
        userName: 'ElinuUser',
        credentials: encodeCredentials('ElinuUser', 'secret'),
    }]));
    sessionStorage.setItem('active_account_id', '1001');
}

describe('launch-time mod update gate', () => {
    beforeEach(() => {
        mockInvoke.mockReset();
        vi.spyOn(console, 'log').mockImplementation(() => {});
        vi.spyOn(console, 'error').mockImplementation(() => {});
        localStorage.clear();
        sessionStorage.clear();
        document.body.innerHTML = '';
        window.showUpdateNotification = vi.fn();
        setActivePasswordAccount();
    });

    afterEach(() => {
        vi.restoreAllMocks();
    });

    it('updates enabled outdated installed mods before launching the game', async () => {
        const catalogMod = {
            id: 'classicplus.shinra',
            name: 'Shinra Meter',
            version: '3.0.6-classicplus',
        };
        mockInvoke.mockImplementation(async (cmd) => {
            if (cmd === 'get_mods_catalog') return { mods: [catalogMod] };
            if (cmd === 'list_installed_mods') return [{
                id: 'classicplus.shinra',
                name: 'Shinra Meter',
                version: '3.0.5-classicplus',
                enabled: true,
                status: 'enabled',
            }];
            if (cmd === 'login') return JSON.stringify({ Return: { AuthKey: 'auth', UserNo: 1001, CharacterCount: 1 }, Msg: 'success' });
            return null;
        });

        const app = await loadAppForLaunchTest();

        await app.handleLaunchGame();

        const installCall = mockInvoke.mock.calls.find(([cmd]) => cmd === 'install_mod');
        const launchCall = mockInvoke.mock.calls.find(([cmd]) => cmd === 'handle_launch_game');
        expect(installCall).toEqual(['install_mod', { entry: catalogMod }]);
        expect(launchCall).toBeDefined();
        expect(mockInvoke.mock.calls.findIndex(([cmd]) => cmd === 'install_mod'))
            .toBeLessThan(mockInvoke.mock.calls.findIndex(([cmd]) => cmd === 'handle_launch_game'));
    });

    it('shows visible feedback while enabled mods update before launch', async () => {
        const catalogMod = {
            id: 'classicplus.shinra',
            name: 'Shinra Meter',
            version: '3.0.6-classicplus',
        };
        mockInvoke.mockImplementation(async (cmd) => {
            if (cmd === 'get_mods_catalog') return { mods: [catalogMod] };
            if (cmd === 'list_installed_mods') return [{
                id: 'classicplus.shinra',
                name: 'Shinra Meter',
                version: '3.0.5-classicplus',
                enabled: true,
                status: 'enabled',
            }];
            if (cmd === 'login') return JSON.stringify({ Return: { AuthKey: 'auth', UserNo: 1001, CharacterCount: 1 }, Msg: 'success' });
            return null;
        });

        const app = await loadAppForLaunchTest();

        await app.handleLaunchGame();

        expect(window.showUpdateNotification).toHaveBeenCalledWith(
            'checking',
            'Updating mods before launch',
            'Shinra Meter (1/1)',
            true,
        );
    });

    it('opens the downloads dialog while enabled mods update before launch', async () => {
        const catalogMod = {
            id: 'classicplus.shinra',
            name: 'Shinra Meter',
            version: '3.0.6-classicplus',
        };
        mockInvoke.mockImplementation(async (cmd) => {
            if (cmd === 'get_mods_catalog') return { mods: [catalogMod] };
            if (cmd === 'list_installed_mods') return [{
                id: 'classicplus.shinra',
                name: 'Shinra Meter',
                version: '3.0.5-classicplus',
                enabled: true,
                status: 'enabled',
            }];
            if (cmd === 'login') return JSON.stringify({ Return: { AuthKey: 'auth', UserNo: 1001, CharacterCount: 1 }, Msg: 'success' });
            return null;
        });

        const app = await loadAppForLaunchTest();

        await app.handleLaunchGame();

        expect(window.ModsView.open).toHaveBeenCalledTimes(1);
        expect(window.ModsView.open.mock.invocationCallOrder[0])
            .toBeLessThan(mockInvoke.mock.invocationCallOrder[mockInvoke.mock.calls.findIndex(([cmd]) => cmd === 'install_mod')]);
    });

    it('does not update disabled outdated mods before launching the game', async () => {
        const catalogMod = {
            id: 'classicplus.shinra',
            name: 'Shinra Meter',
            version: '3.0.6-classicplus',
        };
        mockInvoke.mockImplementation(async (cmd) => {
            if (cmd === 'get_mods_catalog') return { mods: [catalogMod] };
            if (cmd === 'list_installed_mods') return [{
                id: 'classicplus.shinra',
                name: 'Shinra Meter',
                version: '3.0.5-classicplus',
                enabled: false,
                status: 'disabled',
            }];
            if (cmd === 'login') return JSON.stringify({ Return: { AuthKey: 'auth', UserNo: 1001, CharacterCount: 1 }, Msg: 'success' });
            return null;
        });

        const app = await loadAppForLaunchTest();

        await app.handleLaunchGame();

        expect(mockInvoke.mock.calls.some(([cmd]) => cmd === 'install_mod')).toBe(false);
        expect(mockInvoke.mock.calls.some(([cmd]) => cmd === 'handle_launch_game')).toBe(true);
    });

    it('skips installed mods that are no longer present in the catalog before launch', async () => {
        mockInvoke.mockImplementation(async (cmd) => {
            if (cmd === 'get_mods_catalog') return { mods: [] };
            if (cmd === 'list_installed_mods') return [{
                id: 'classicplus.retired',
                name: 'Retired Mod',
                version: '1.0.0',
                enabled: true,
                status: 'enabled',
            }];
            if (cmd === 'login') return JSON.stringify({ Return: { AuthKey: 'auth', UserNo: 1001, CharacterCount: 1 }, Msg: 'success' });
            return null;
        });

        const app = await loadAppForLaunchTest();

        await app.handleLaunchGame();

        expect(window.ModsView.open).not.toHaveBeenCalled();
        expect(mockInvoke.mock.calls.some(([cmd]) => cmd === 'install_mod')).toBe(false);
        expect(mockInvoke.mock.calls.some(([cmd]) => cmd === 'handle_launch_game')).toBe(true);
    });

    it('stops launch when an enabled mod update fails', async () => {
        const catalogMod = {
            id: 'classicplus.shinra',
            name: 'Shinra Meter',
            version: '3.0.6-classicplus',
        };
        mockInvoke.mockImplementation(async (cmd) => {
            if (cmd === 'get_mods_catalog') return { mods: [catalogMod] };
            if (cmd === 'list_installed_mods') return [{
                id: 'classicplus.shinra',
                name: 'Shinra Meter',
                version: '3.0.5-classicplus',
                enabled: true,
                status: 'enabled',
            }];
            if (cmd === 'install_mod') throw new Error('download failed');
            if (cmd === 'login') return JSON.stringify({ Return: { AuthKey: 'auth', UserNo: 1001, CharacterCount: 1 }, Msg: 'success' });
            return null;
        });

        const app = await loadAppForLaunchTest();

        await app.handleLaunchGame();

        expect(mockInvoke.mock.calls.some(([cmd]) => cmd === 'install_mod')).toBe(true);
        expect(mockInvoke.mock.calls.some(([cmd]) => cmd === 'handle_launch_game')).toBe(false);
    });
});
