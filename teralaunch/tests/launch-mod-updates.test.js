import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

const mockInvoke = vi.fn();

function encodeCredentials(username, password) {
    return btoa(JSON.stringify({ u: username, p: password }));
}

/**
 * Loads app.js with a stubbed Tauri shell and returns the App + the
 * `mod_download_progress` event injector. The default invoke handler
 * accepts `auto_update_enabled_mods` and `login`; tests override
 * specific commands by reassigning `commandMap` BEFORE calling
 * handleLaunchGame.
 */
async function loadAppForLaunchTest({ progressEvents = [], commandMap = {} } = {}) {
    vi.resetModules();

    globalThis.createRouter = vi.fn(() => ({ navigate: vi.fn() }));
    globalThis.startOAuth = vi.fn();

    let progressHandler = null;
    const fakeListen = vi.fn(async (event, handler) => {
        if (event === 'mod_download_progress') {
            progressHandler = handler;
        }
        return () => { progressHandler = null; };
    });

    window.__TAURI__ = {
        core: { invoke: mockInvoke },
        tauri: { invoke: mockInvoke },
        event: { listen: fakeListen },
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

    const defaultMap = {
        auto_update_enabled_mods: () => {
            for (const ev of progressEvents) {
                if (progressHandler) progressHandler({ payload: ev });
            }
            return { attempted: ['classicplus.shinra'], failed_ids: [] };
        },
        login: () => JSON.stringify({ Return: { AuthKey: 'auth', UserNo: 1001, CharacterCount: 1 }, Msg: 'success' }),
    };
    const merged = { ...defaultMap, ...commandMap };
    mockInvoke.mockImplementation(async (cmd, args) => {
        if (cmd in merged) return merged[cmd](args);
        return null;
    });

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
        vi.spyOn(console, 'warn').mockImplementation(() => {});
        localStorage.clear();
        sessionStorage.clear();
        document.body.innerHTML = '';
        window.showUpdateNotification = vi.fn();
        setActivePasswordAccount();
    });

    afterEach(() => {
        vi.restoreAllMocks();
    });

    it('delegates the update flow to auto_update_enabled_mods before launching', async () => {
        const app = await loadAppForLaunchTest();
        await app.handleLaunchGame();

        const autoCallIdx = mockInvoke.mock.calls.findIndex(([cmd]) => cmd === 'auto_update_enabled_mods');
        const launchCallIdx = mockInvoke.mock.calls.findIndex(([cmd]) => cmd === 'handle_launch_game');
        expect(autoCallIdx).toBeGreaterThanOrEqual(0);
        expect(launchCallIdx).toBeGreaterThanOrEqual(0);
        expect(autoCallIdx).toBeLessThan(launchCallIdx);
    });

    it('does NOT re-run the JS-side install_mod loop (Rust owns the flow)', async () => {
        const app = await loadAppForLaunchTest();
        await app.handleLaunchGame();
        expect(mockInvoke.mock.calls.some(([cmd]) => cmd === 'install_mod')).toBe(false);
        expect(mockInvoke.mock.calls.some(([cmd]) => cmd === 'list_installed_mods')).toBe(false);
        expect(mockInvoke.mock.calls.some(([cmd]) => cmd === 'get_mods_catalog')).toBe(false);
    });

    it('renders the standalone download tray (NOT the mod manager modal) while updating', async () => {
        const app = await loadAppForLaunchTest({
            progressEvents: [
                { id: 'classicplus.shinra', progress: 42, state: 'downloading' },
            ],
        });

        let trayDuringUpdate = null;
        const orig = window.LaunchUpdateTray.update.bind(window.LaunchUpdateTray);
        window.LaunchUpdateTray.update = (id, info) => {
            orig(id, info);
            // Snapshot the tray DOM the first time a row is added so we can
            // assert it actually rendered while the Rust call was in flight.
            if (!trayDuringUpdate) {
                trayDuringUpdate = document.getElementById('launch-update-tray');
            }
        };

        await app.handleLaunchGame();

        // The mod manager modal must NOT have been opened — the user
        // explicitly does not want the entire manager to pop up on launch.
        expect(window.ModsView.open).not.toHaveBeenCalled();
        // The standalone tray must have rendered while the update was in
        // flight (it is unmounted in the `finally` block once Rust returns).
        expect(trayDuringUpdate).not.toBeNull();
        expect(trayDuringUpdate.classList.contains('mods-download-tray-standalone')).toBe(true);
        expect(trayDuringUpdate.querySelector('.mods-download-tray-bar-fill')).not.toBeNull();
    });

    it('cleans the standalone tray off the DOM after auto_update_enabled_mods returns', async () => {
        const app = await loadAppForLaunchTest({
            progressEvents: [
                { id: 'classicplus.shinra', progress: 80, state: 'downloading' },
            ],
        });
        await app.handleLaunchGame();
        expect(document.getElementById('launch-update-tray')).toBeNull();
    });

    it('does NOT block launch when auto_update_enabled_mods reports partial failures', async () => {
        const app = await loadAppForLaunchTest({
            commandMap: {
                auto_update_enabled_mods: () => ({
                    attempted: ['classicplus.shinra'],
                    failed_ids: ['classicplus.shinra'],
                }),
            },
        });
        await app.handleLaunchGame();

        expect(mockInvoke.mock.calls.some(([cmd]) => cmd === 'handle_launch_game')).toBe(true);
        expect(window.showUpdateNotification).toHaveBeenCalledWith(
            'warning',
            expect.any(String),
            expect.stringContaining('classicplus.shinra'),
        );
    });

    it('does NOT block launch when the update command itself errors out', async () => {
        const app = await loadAppForLaunchTest({
            commandMap: {
                auto_update_enabled_mods: () => { throw new Error('catalog network down'); },
            },
        });
        await app.handleLaunchGame();

        expect(mockInvoke.mock.calls.some(([cmd]) => cmd === 'handle_launch_game')).toBe(true);
    });

    it('skips the success toast when nothing was attempted (no enabled mods needed updating)', async () => {
        const app = await loadAppForLaunchTest({
            commandMap: {
                auto_update_enabled_mods: () => ({ attempted: [], failed_ids: [] }),
            },
        });
        await app.handleLaunchGame();

        const successCalls = window.showUpdateNotification.mock.calls.filter(
            ([state]) => state === 'success'
        );
        expect(successCalls.length).toBe(0);
    });
});
