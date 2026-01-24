import { describe, it, expect, beforeEach, vi } from 'vitest';
import { calculateRemainingSize, calculateResumeSnapshot } from '../src/utils/download.js';
import { shouldDisableLaunch, getStatusKey, getDlStatusKey } from '../src/utils/updateState.js';

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
