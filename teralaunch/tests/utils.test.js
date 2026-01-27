import { describe, it, expect, beforeEach, vi } from 'vitest';
import { calculateRemainingSize, calculateResumeSnapshot } from '../src/utils/download.js';
import {
    shouldDisableLaunch,
    getStatusKey,
    getDlStatusKey,
    getProgressUpdateMode,
    getUpdateErrorMessage,
    getPathChangeResetState,
    INITIAL_STATE,
} from '../src/utils/updateState.js';

function getVisibilityState(state) {
    const isDownloading = state.currentUpdateMode === 'download';
    const isPaused = state.currentUpdateMode === 'paused';
    const showDownloadInfo = state.isUpdateAvailable && (isDownloading || isPaused);
    return { isDownloading, isPaused, showDownloadInfo };
}

function shouldShowProgress(state) {
    return state.isUpdateAvailable && state.currentUpdateMode !== 'ready';
}

describe('Visibility Logic', () => {
    describe('updateElementsVisibility', () => {
        it('shows elements during download', () => {
            const { isDownloading, isPaused, showDownloadInfo } = getVisibilityState({
                currentUpdateMode: 'download',
                isUpdateAvailable: true,
            });
            expect(showDownloadInfo).toBe(true);
            expect(isDownloading).toBe(true);
            expect(isPaused).toBe(false);
        });

        it('shows size info but hides speed during pause', () => {
            const { isDownloading, isPaused, showDownloadInfo } = getVisibilityState({
                currentUpdateMode: 'paused',
                isUpdateAvailable: true,
            });
            expect(showDownloadInfo).toBe(true);
            expect(isDownloading).toBe(false);
            expect(isPaused).toBe(true);
        });

        it('hides all download info when no update available', () => {
            const { showDownloadInfo } = getVisibilityState({
                currentUpdateMode: 'download',
                isUpdateAvailable: false,
            });
            expect(showDownloadInfo).toBe(false);
        });

        it('hides all during file_check', () => {
            const { showDownloadInfo } = getVisibilityState({
                currentUpdateMode: 'file_check',
                isUpdateAvailable: true,
            });
            expect(showDownloadInfo).toBe(false);
        });

        it('hides all during complete', () => {
            const { showDownloadInfo } = getVisibilityState({
                currentUpdateMode: 'complete',
                isUpdateAvailable: false,
            });
            expect(showDownloadInfo).toBe(false);
        });
    });

    describe('Progress percentage visibility', () => {
        it('shows during download mode', () => {
            expect(shouldShowProgress({ isUpdateAvailable: true, currentUpdateMode: 'download' })).toBe(true);
        });

        it('hides during ready mode', () => {
            expect(shouldShowProgress({ isUpdateAvailable: true, currentUpdateMode: 'ready' })).toBe(false);
        });

        it('hides when no update available', () => {
            expect(shouldShowProgress({ isUpdateAvailable: false, currentUpdateMode: 'download' })).toBe(false);
        });
    });
});

describe('Pause Button Logic', () => {
    function getPauseButtonState(isPaused) {
        return {
            icon: isPaused ? './assets/vector-3.svg' : './assets/pause-icon.svg',
            alt: isPaused ? 'Resume' : 'Pause',
        };
    }

    it('shows pause icon during download', () => {
        const { icon, alt } = getPauseButtonState(false);
        expect(icon).toBe('./assets/pause-icon.svg');
        expect(alt).toBe('Pause');
    });

    it('shows play icon when paused', () => {
        const { icon, alt } = getPauseButtonState(true);
        expect(icon).toBe('./assets/vector-3.svg');
        expect(alt).toBe('Resume');
    });
});

describe('UPDATE_CHECK_ENABLED Edge Cases', () => {
    it('handles disabled updates', () => {
        const UPDATE_CHECK_ENABLED = false;

        const result = {
            isUpdateAvailable: false,
            isFileCheckComplete: true,
            currentUpdateMode: 'complete',
            currentProgress: 100,
        };

        if (!UPDATE_CHECK_ENABLED) {
            expect(result.isUpdateAvailable).toBe(false);
            expect(result.currentProgress).toBe(100);
        }
    });
});

describe('Language Selector', () => {
    it('gets language name from code', () => {
        const languages = {
            GER: 'GERMAN',
            EUR: 'ENGLISH',
            FRA: 'FRENCH',
            RUS: 'RUSSIAN',
        };

        expect(languages['GER']).toBe('GERMAN');
        expect(languages['EUR']).toBe('ENGLISH');
        expect(languages['UNKNOWN'] || 'UNKNOWN').toBe('UNKNOWN');
    });
});

describe('Toggle Language Selector', () => {
    it('enables selector', () => {
        const wrapper = {
            classList: { add: vi.fn(), remove: vi.fn() },
        };
        const styled = { style: { pointerEvents: '' } };

        const enable = true;
        if (enable) {
            wrapper.classList.remove('disabled');
            styled.style.pointerEvents = 'auto';
        }

        expect(wrapper.classList.remove).toHaveBeenCalledWith('disabled');
        expect(styled.style.pointerEvents).toBe('auto');
    });

    it('disables selector', () => {
        const wrapper = {
            classList: { add: vi.fn(), remove: vi.fn() },
        };
        const styled = { style: { pointerEvents: '' } };

        const enable = false;
        if (!enable) {
            wrapper.classList.add('disabled');
            styled.style.pointerEvents = 'none';
        }

        expect(wrapper.classList.add).toHaveBeenCalledWith('disabled');
        expect(styled.style.pointerEvents).toBe('none');
    });
});

describe('Modal Toggle', () => {
    function toggleModal(modal, show) {
        modal.classList.toggle('show', show);
        modal.style.display = show ? 'block' : 'none';
    }

    it('shows modal', () => {
        const modal = {
            classList: { toggle: vi.fn(), contains: vi.fn(() => false) },
            style: { display: '' },
        };
        toggleModal(modal, true);
        expect(modal.classList.toggle).toHaveBeenCalledWith('show', true);
        expect(modal.style.display).toBe('block');
    });

    it('hides modal', () => {
        const modal = {
            classList: { toggle: vi.fn(), contains: vi.fn(() => true) },
            style: { display: 'block' },
        };
        toggleModal(modal, false);
        expect(modal.classList.toggle).toHaveBeenCalledWith('show', false);
        expect(modal.style.display).toBe('none');
    });
});

describe('Resume Size Helpers', () => {
    it('subtracts existing bytes when computing remaining size', () => {
        const files = [
            { size: 100, existing_size: 20 },
            { size: 200, existing_size: 0 },
            { size: 50 },
        ];
        expect(calculateRemainingSize(files)).toBe(330);
    });

    it('uses remaining size to compute resume snapshot when total known', () => {
        const files = [{ size: 500, existing_size: 200 }];
        const snapshot = calculateResumeSnapshot(1000, 100, files);
        expect(snapshot.remainingSize).toBe(300);
        expect(snapshot.newTotalSize).toBe(1000);
        expect(snapshot.clampedDownloaded).toBe(700);
    });

    it('falls back to previous downloaded bytes when remaining is unknown', () => {
        const files = [{ size: 100, existing_size: 0 }];
        const snapshot = calculateResumeSnapshot(0, 250, files);
        expect(snapshot.newTotalSize).toBe(100);
        expect(snapshot.clampedDownloaded).toBe(250);
    });

    it('does not decrease downloaded snapshot when computed value is lower', () => {
        const files = [{ size: 1000, existing_size: 100 }];
        const snapshot = calculateResumeSnapshot(0, 400, files);
        expect(snapshot.clampedDownloaded).toBe(400);
    });
});

describe('Update State Helpers', () => {
    it('disables launch when update error is present', () => {
        expect(shouldDisableLaunch({
            disabled: false,
            currentUpdateMode: 'ready',
            updateError: true,
        })).toBe(true);
    });

    it('keeps ready mode when progress arrives after completion', () => {
        expect(getProgressUpdateMode({
            currentUpdateMode: 'ready',
            isDownloadComplete: true,
            isUpdateAvailable: false,
        })).toBe('ready');
    });

    it('returns backend error message when available', () => {
        expect(getUpdateErrorMessage({ message: 'Backend failed' }, 'fallback')).toBe('Backend failed');
    });

    it('falls back when error is empty string', () => {
        expect(getUpdateErrorMessage('', 'fallback')).toBe('fallback');
    });

    it('converts empty object to string representation', () => {
        expect(getUpdateErrorMessage({}, 'fallback')).toBe('[object Object]');
    });

    it('resets download state on path change', () => {
        const reset = getPathChangeResetState();
        expect(reset.currentUpdateMode).toBe('file_check');
        expect(reset.downloadedSize).toBe(0);
        expect(reset.totalSize).toBe(0);
        expect(reset.updateError).toBe(false);
    });

    it('returns update error status when error exists', () => {
        const state = { updateError: true };
        expect(getStatusKey(state)).toBe('UPDATE_ERROR_MESSAGE');
        expect(getDlStatusKey({ ...state, currentUpdateMode: 'download' })).toBe('UPDATE_ERROR_MESSAGE');
    });

    it('returns downloading status during active download', () => {
        const state = { updateError: false, isUpdateAvailable: true, currentUpdateMode: 'download' };
        expect(getStatusKey(state)).toBe('DOWNLOADING_FILES');
        expect(getDlStatusKey({
            ...state,
            isFileCheckComplete: false,
            isUpdateAvailable: true,
            currentUpdateMode: 'download',
        })).toBe('DOWNLOADING_FILES');
    });
});

describe('Error Message Display', () => {
    it('shows error message', () => {
        const errorContainer = {
            textContent: '',
            style: { display: 'none' },
        };

        const message = 'Test Error';
        errorContainer.textContent = message;
        errorContainer.style.display = 'block';

        expect(errorContainer.textContent).toBe('Test Error');
        expect(errorContainer.style.display).toBe('block');
    });
});

describe('Loading Indicator', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
    });

    it('creates indicator if not exists', () => {
        let loadingIndicator = document.getElementById('loading-indicator');
        if (!loadingIndicator) {
            loadingIndicator = document.createElement('div');
            loadingIndicator.id = 'loading-indicator';
            loadingIndicator.innerHTML = '<div class="spinner"></div>';
            document.body.appendChild(loadingIndicator);
        }
        loadingIndicator.style.display = 'flex';

        expect(document.getElementById('loading-indicator')).not.toBeNull();
        expect(loadingIndicator.style.display).toBe('flex');
    });

    it('hides existing indicator', () => {
        const indicator = document.createElement('div');
        indicator.id = 'loading-indicator';
        indicator.style.display = 'flex';
        document.body.appendChild(indicator);

        const loadingIndicator = document.getElementById('loading-indicator');
        if (loadingIndicator) {
            loadingIndicator.style.display = 'none';
        }

        expect(loadingIndicator.style.display).toBe('none');
    });
});

describe('Game Status', () => {
    const t = (key) => key;
    function getStatusText(isRunning) {
        return isRunning ? t('GAME_STATUS_RUNNING') : t('GAME_STATUS_NOT_RUNNING');
    }

    it('returns running status text', () => {
        expect(getStatusText(true)).toBe('GAME_STATUS_RUNNING');
    });

    it('returns not running status text', () => {
        expect(getStatusText(false)).toBe('GAME_STATUS_NOT_RUNNING');
    });
});

describe('External URL Opening', () => {
    it('uses shell.open when available', () => {
        const mockShellOpen = vi.fn();
        const __TAURI__ = {
            shell: { open: mockShellOpen },
        };

        const openExternal = (url) => {
            if (__TAURI__ && __TAURI__.shell && __TAURI__.shell.open) {
                __TAURI__.shell.open(url);
            }
        };

        openExternal('https://example.com');

        expect(mockShellOpen).toHaveBeenCalledWith('https://example.com');
    });
});

describe('Notification', () => {
    it('creates notification element', () => {
        const type = 'success';
        const message = 'Test notification';

        const notification = document.createElement('div');
        notification.className = `custom-notification ${type}`;
        notification.textContent = message;

        expect(notification.className).toBe('custom-notification success');
        expect(notification.textContent).toBe('Test notification');
    });

    it('handles error type', () => {
        const type = 'error';

        const notification = document.createElement('div');
        notification.className = `custom-notification ${type}`;

        expect(notification.className).toBe('custom-notification error');
    });
});

describe('Hash File Progress', () => {
    it('calculates progress correctly', () => {
        const state = {
            hashFileProgress: 50,
            processedFiles: 25,
            totalFiles: 50,
        };

        expect(state.hashFileProgress).toBe(50);
        expect(`${state.processedFiles}/${state.totalFiles}`).toBe('25/50');
    });

    it('formats progress text', () => {
        const state = {
            hashFileProgress: 75.5,
            processedFiles: 30,
            totalFiles: 40,
        };

        const progressText = `Progress ${state.processedFiles}/${state.totalFiles} (${state.hashFileProgress.toFixed(2)}%)`;

        expect(progressText).toBe('Progress 30/40 (75.50%)');
    });
});

describe('Config Path Error', () => {
    it('detects tera_config.ini error', () => {
        const error = { message: 'Error reading src/tera_config.ini' };

        const isConfigError =
            error &&
            error.message &&
            typeof error.message === 'string' &&
            error.message.toLowerCase().includes('src/tera_config.ini');

        expect(isConfigError).toBe(true);
    });

    it('handles other errors', () => {
        const error = { message: 'Some other error' };

        const isConfigError =
            error &&
            error.message &&
            typeof error.message === 'string' &&
            error.message.toLowerCase().includes('src/tera_config.ini');

        expect(isConfigError).toBe(false);
    });
});

describe('Login Response Parsing', () => {
    function isLoginSuccess(response) {
        return response && response.Return && response.Msg === 'success';
    }

    function getErrorMessage(response) {
        return response ? (response.Msg || 'LOGIN_ERROR') : 'LOGIN_ERROR';
    }

    it('parses successful login response', () => {
        const response = {
            Return: { AuthKey: 'test-key', UserNo: '123', CharacterCount: '5', Permission: '2' },
            Msg: 'success',
        };
        expect(isLoginSuccess(response)).toBe(true);

        const formatted = {
            AuthKey: response.Return.AuthKey,
            UserNo: Number(response.Return.UserNo),
            CharacterCount: response.Return.CharacterCount,
            Permission: Number(response.Return.Permission),
            Privilege: 0,
        };
        expect(formatted.AuthKey).toBe('test-key');
        expect(formatted.UserNo).toBe(123);
        expect(formatted.Permission).toBe(2);
    });

    it('handles failed login with error message', () => {
        const response = { Msg: 'INVALID_CREDENTIALS' };
        expect(isLoginSuccess(response)).toBeFalsy();
        expect(getErrorMessage(response)).toBe('INVALID_CREDENTIALS');
    });

    it('handles failed login without error message', () => {
        const response = {};
        expect(isLoginSuccess(response)).toBeFalsy();
        expect(getErrorMessage(response)).toBe('LOGIN_ERROR');
    });

    it('handles null response', () => {
        expect(isLoginSuccess(null)).toBeFalsy();
        expect(getErrorMessage(null)).toBe('LOGIN_ERROR');
    });
});

describe('Update Check Flags', () => {
    function isCheckNeeded(state, isLogin) {
        return isLogin ? !state.updateCheckPerformedOnLogin : !state.updateCheckPerformedOnRefresh;
    }

    function markCheckPerformed(state, isLogin) {
        if (isLogin) {
            state.updateCheckPerformedOnLogin = true;
        } else {
            state.updateCheckPerformedOnRefresh = true;
        }
    }

    it('needs check on login when not performed', () => {
        const state = { updateCheckPerformedOnLogin: false, updateCheckPerformedOnRefresh: false };
        expect(isCheckNeeded(state, true)).toBe(true);
    });

    it('does not need check on login when already performed', () => {
        const state = { updateCheckPerformedOnLogin: true, updateCheckPerformedOnRefresh: false };
        expect(isCheckNeeded(state, true)).toBe(false);
    });

    it('needs check on refresh when not performed', () => {
        const state = { updateCheckPerformedOnLogin: false, updateCheckPerformedOnRefresh: false };
        expect(isCheckNeeded(state, false)).toBe(true);
    });

    it('does not need check on refresh when already performed', () => {
        const state = { updateCheckPerformedOnLogin: false, updateCheckPerformedOnRefresh: true };
        expect(isCheckNeeded(state, false)).toBe(false);
    });

    it('marks login check as performed', () => {
        const state = { updateCheckPerformedOnLogin: false, updateCheckPerformedOnRefresh: false };
        markCheckPerformed(state, true);
        expect(state.updateCheckPerformedOnLogin).toBe(true);
        expect(state.updateCheckPerformedOnRefresh).toBe(false);
    });

    it('marks refresh check as performed', () => {
        const state = { updateCheckPerformedOnLogin: false, updateCheckPerformedOnRefresh: false };
        markCheckPerformed(state, false);
        expect(state.updateCheckPerformedOnLogin).toBe(false);
        expect(state.updateCheckPerformedOnRefresh).toBe(true);
    });
});

describe('Files to Update Calculation', () => {
    it('calculates total size from files', () => {
        const files = [
            { path: 'file1.gpk', size: 1000 },
            { path: 'file2.gpk', size: 2000 },
            { path: 'file3.gpk', size: 500 },
        ];

        const totalSize = files.reduce((total, file) => total + file.size, 0);

        expect(totalSize).toBe(3500);
        expect(files.length).toBe(3);
    });

    it('handles empty files list', () => {
        const files = [];

        const totalSize = files.reduce((total, file) => total + file.size, 0);

        expect(totalSize).toBe(0);
        expect(files.length).toBe(0);
    });
});

describe('INITIAL_STATE Export', () => {
    it('exports INITIAL_STATE object', () => {
        expect(INITIAL_STATE).toBeDefined();
        expect(typeof INITIAL_STATE).toBe('object');
    });

    it('contains all required state properties', () => {
        expect(INITIAL_STATE).toHaveProperty('lastLogMessage');
        expect(INITIAL_STATE).toHaveProperty('lastLogTime');
        expect(INITIAL_STATE).toHaveProperty('speedHistory');
        expect(INITIAL_STATE).toHaveProperty('isUpdateAvailable');
        expect(INITIAL_STATE).toHaveProperty('isDownloadComplete');
        expect(INITIAL_STATE).toHaveProperty('currentUpdateMode');
        expect(INITIAL_STATE).toHaveProperty('downloadedSize');
        expect(INITIAL_STATE).toHaveProperty('totalSize');
        expect(INITIAL_STATE).toHaveProperty('updateError');
    });

    it('has correct default values', () => {
        expect(INITIAL_STATE.isUpdateAvailable).toBe(false);
        expect(INITIAL_STATE.isDownloadComplete).toBe(false);
        expect(INITIAL_STATE.currentUpdateMode).toBe(null);
        expect(INITIAL_STATE.downloadedSize).toBe(0);
        expect(INITIAL_STATE.totalSize).toBe(0);
        expect(INITIAL_STATE.updateError).toBe(false);
        expect(INITIAL_STATE.speedHistory).toEqual([]);
        expect(INITIAL_STATE.lastLogMessage).toBe(null);
    });

    it('getPathChangeResetState references INITIAL_STATE correctly', () => {
        const resetState = getPathChangeResetState();

        expect(resetState.isFileCheckComplete).toBe(INITIAL_STATE.isFileCheckComplete);
        expect(resetState.isUpdateAvailable).toBe(INITIAL_STATE.isUpdateAvailable);
        expect(resetState.downloadedSize).toBe(INITIAL_STATE.downloadedSize);
        expect(resetState.totalSize).toBe(INITIAL_STATE.totalSize);
        expect(resetState.updateError).toBe(INITIAL_STATE.updateError);
    });

    it('getPathChangeResetState overrides currentUpdateMode to file_check', () => {
        const resetState = getPathChangeResetState();
        expect(resetState.currentUpdateMode).toBe('file_check');
        expect(INITIAL_STATE.currentUpdateMode).toBe(null);
    });
});

describe('escapeHtml Function', () => {
    function escapeHtml(str) {
        if (str === null || str === undefined) return '';
        return String(str)
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#039;');
    }

    it('escapes ampersand character', () => {
        expect(escapeHtml('Tom & Jerry')).toBe('Tom &amp; Jerry');
    });

    it('escapes less-than character', () => {
        expect(escapeHtml('5 < 10')).toBe('5 &lt; 10');
    });

    it('escapes greater-than character', () => {
        expect(escapeHtml('10 > 5')).toBe('10 &gt; 5');
    });

    it('escapes double quote character', () => {
        expect(escapeHtml('He said "hello"')).toBe('He said &quot;hello&quot;');
    });

    it('escapes single quote character', () => {
        expect(escapeHtml("It's working")).toBe('It&#039;s working');
    });

    it('escapes multiple special characters', () => {
        expect(escapeHtml('<script>alert("XSS & hack\'s")</script>'))
            .toBe('&lt;script&gt;alert(&quot;XSS &amp; hack&#039;s&quot;)&lt;/script&gt;');
    });

    it('handles null input', () => {
        expect(escapeHtml(null)).toBe('');
    });

    it('handles undefined input', () => {
        expect(escapeHtml(undefined)).toBe('');
    });

    it('handles empty string', () => {
        expect(escapeHtml('')).toBe('');
    });

    it('handles normal strings without special characters', () => {
        expect(escapeHtml('Hello World')).toBe('Hello World');
        expect(escapeHtml('Test123')).toBe('Test123');
    });

    it('converts numbers to strings and escapes if needed', () => {
        expect(escapeHtml(123)).toBe('123');
        expect(escapeHtml(0)).toBe('0');
    });
});

describe('URLS Configuration', () => {
    const URLS = {
        launcher: {
            download: "https://web.tera-germany.de/gameserver/Tera-Germany_Launcher.exe",
            versionCheck: "https://web.tera-germany.de/classic/version.json",
            versionInfo: "https://web.tera-germany.de/gameserver/version.json",
        },
        content: {
            news: "https://web.tera-germany.de/classic/Launcher_StartPage_News.json",
            patchNotes: "https://web.tera-germany.de/classic/patchnotes.json",
            serverStatus: "https://web.tera-germany.de/classic/serverlist.json?lang=ger&sort=3",
        },
        external: {
            register: "https://reg.tera-europe-classic.de/register.php",
            forum: "https://forum.crazy-esports.com/forum/board/42-tera-europe-classic/",
            discord: "https://discord.gg/DARHAaNBYS",
            support: "https://helpdesk.crazy-esports.com",
            privacy: "https://forum.crazy-esports.com/index.php?datenschutzerklaerung/",
        },
    };

    it('has all required top-level categories', () => {
        expect(URLS).toHaveProperty('launcher');
        expect(URLS).toHaveProperty('content');
        expect(URLS).toHaveProperty('external');
    });

    it('has all launcher URLs defined', () => {
        expect(URLS.launcher).toHaveProperty('download');
        expect(URLS.launcher).toHaveProperty('versionCheck');
        expect(URLS.launcher).toHaveProperty('versionInfo');
    });

    it('has all content URLs defined', () => {
        expect(URLS.content).toHaveProperty('news');
        expect(URLS.content).toHaveProperty('patchNotes');
        expect(URLS.content).toHaveProperty('serverStatus');
    });

    it('has all external URLs defined', () => {
        expect(URLS.external).toHaveProperty('register');
        expect(URLS.external).toHaveProperty('forum');
        expect(URLS.external).toHaveProperty('discord');
        expect(URLS.external).toHaveProperty('support');
        expect(URLS.external).toHaveProperty('privacy');
    });

    it('all URLs are valid strings', () => {
        expect(typeof URLS.launcher.download).toBe('string');
        expect(typeof URLS.launcher.versionCheck).toBe('string');
        expect(typeof URLS.content.news).toBe('string');
        expect(typeof URLS.external.register).toBe('string');
    });

    it('all URLs start with https', () => {
        expect(URLS.launcher.download).toMatch(/^https:\/\//);
        expect(URLS.content.news).toMatch(/^https:\/\//);
        expect(URLS.external.register).toMatch(/^https:\/\//);
    });

    it('URLs are non-empty', () => {
        expect(URLS.launcher.download.length).toBeGreaterThan(0);
        expect(URLS.content.news.length).toBeGreaterThan(0);
        expect(URLS.external.discord.length).toBeGreaterThan(0);
    });
});

describe('DOM Element Caching', () => {
    let elementCache;

    beforeEach(() => {
        elementCache = null;
        document.body.innerHTML = `
            <button id="launch-game-btn">Launch</button>
            <div id="game-status">Status</div>
            <div id="download-progress">Progress</div>
        `;
    });

    function getCachedElements() {
        if (!elementCache) {
            elementCache = {
                launchBtn: document.querySelector('#launch-game-btn'),
                statusEl: document.querySelector('#game-status'),
                progressEl: document.querySelector('#download-progress'),
            };
        }
        return elementCache;
    }

    function invalidateElementCache() {
        elementCache = null;
    }

    it('caches elements on first call', () => {
        const elements = getCachedElements();
        expect(elements).toBeDefined();
        expect(elements.launchBtn).not.toBeNull();
        expect(elements.statusEl).not.toBeNull();
        expect(elements.progressEl).not.toBeNull();
    });

    it('returns same cached instance on subsequent calls', () => {
        const elements1 = getCachedElements();
        const elements2 = getCachedElements();
        expect(elements1).toBe(elements2);
    });

    it('caches prevent redundant DOM queries', () => {
        const querySelectorSpy = vi.spyOn(document, 'querySelector');

        getCachedElements();
        const firstCallCount = querySelectorSpy.mock.calls.length;

        getCachedElements();
        const secondCallCount = querySelectorSpy.mock.calls.length;

        expect(secondCallCount).toBe(firstCallCount);

        querySelectorSpy.mockRestore();
    });

    it('invalidateElementCache clears the cache', () => {
        getCachedElements();
        expect(elementCache).not.toBeNull();

        invalidateElementCache();
        expect(elementCache).toBeNull();
    });

    it('rebuilds cache after invalidation', () => {
        const elements1 = getCachedElements();
        invalidateElementCache();
        const elements2 = getCachedElements();

        expect(elements1).not.toBe(elements2);
        expect(elements2.launchBtn).not.toBeNull();
    });

    it('handles missing elements gracefully', () => {
        document.body.innerHTML = '';
        const elements = getCachedElements();
        expect(elements.launchBtn).toBeNull();
        expect(elements.statusEl).toBeNull();
    });
});

describe('Event Listener Setup Flag', () => {
    let _homeListenersSetup;
    let listenerCallCount;

    beforeEach(() => {
        _homeListenersSetup = false;
        listenerCallCount = 0;
        document.body.innerHTML = '<button id="test-btn">Test</button>';
    });

    function setupHomePageEventListeners() {
        if (_homeListenersSetup) {
            return;
        }

        const btn = document.getElementById('test-btn');
        if (btn) {
            btn.addEventListener('click', () => {
                listenerCallCount++;
            });
        }

        _homeListenersSetup = true;
    }

    it('sets up listeners on first call', () => {
        expect(_homeListenersSetup).toBe(false);
        setupHomePageEventListeners();
        expect(_homeListenersSetup).toBe(true);
    });

    it('prevents duplicate listener setup', () => {
        setupHomePageEventListeners();
        setupHomePageEventListeners();
        setupHomePageEventListeners();

        const btn = document.getElementById('test-btn');
        btn.click();

        expect(listenerCallCount).toBe(1);
    });

    it('early returns if already setup', () => {
        setupHomePageEventListeners();
        const firstSetup = _homeListenersSetup;

        setupHomePageEventListeners();
        const secondSetup = _homeListenersSetup;

        expect(firstSetup).toBe(true);
        expect(secondSetup).toBe(true);
    });

    it('flag persists across multiple calls', () => {
        setupHomePageEventListeners();

        for (let i = 0; i < 5; i++) {
            setupHomePageEventListeners();
            expect(_homeListenersSetup).toBe(true);
        }
    });
});

describe('State Reset on Path Change', () => {
    it('resets all download-related state', () => {
        const reset = getPathChangeResetState();

        expect(reset.downloadedSize).toBe(0);
        expect(reset.totalSize).toBe(0);
        expect(reset.currentProgress).toBe(0);
        expect(reset.currentSpeed).toBe(0);
        expect(reset.timeRemaining).toBe(0);
    });

    it('resets file tracking state', () => {
        const reset = getPathChangeResetState();

        expect(reset.currentFileName).toBe('');
        expect(reset.currentFileIndex).toBe(0);
        expect(reset.totalFiles).toBe(0);
    });

    it('resets flags to initial values', () => {
        const reset = getPathChangeResetState();

        expect(reset.isFileCheckComplete).toBe(false);
        expect(reset.isUpdateAvailable).toBe(false);
        expect(reset.isDownloadComplete).toBe(false);
        expect(reset.updateError).toBe(false);
        expect(reset.isPauseRequested).toBe(false);
    });

    it('sets currentUpdateMode to file_check', () => {
        const reset = getPathChangeResetState();
        expect(reset.currentUpdateMode).toBe('file_check');
    });

    it('resets time tracking', () => {
        const reset = getPathChangeResetState();

        expect(reset.lastProgressUpdate).toBe(null);
        expect(reset.lastDownloadedBytes).toBe(0);
        expect(reset.downloadStartTime).toBe(null);
    });

    it('maintains consistency with INITIAL_STATE', () => {
        const reset = getPathChangeResetState();

        const keysToCheck = [
            'isFileCheckComplete',
            'isUpdateAvailable',
            'isDownloadComplete',
            'downloadedSize',
            'totalSize',
            'currentProgress',
            'currentFileName',
            'currentFileIndex',
            'totalFiles',
            'currentSpeed',
            'timeRemaining',
            'updateError',
            'isPauseRequested',
        ];

        keysToCheck.forEach((key) => {
            if (key !== 'currentUpdateMode') {
                expect(reset[key]).toBe(INITIAL_STATE[key]);
            }
        });
    });
});
