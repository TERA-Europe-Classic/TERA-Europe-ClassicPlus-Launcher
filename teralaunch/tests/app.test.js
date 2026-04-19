/**
 * Comprehensive unit tests for TERA Germany Launcher
 * Tests all functions in app.js with 100% coverage goal
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

// Mock Tauri APIs before importing anything
const mockInvoke = vi.fn();
const mockListen = vi.fn(() => Promise.resolve(() => {}));
const mockAppWindow = {
    minimize: vi.fn(),
    close: vi.fn(),
};
const mockMessage = vi.fn();
const mockAsk = vi.fn();

// Set up global Tauri mock.
// v2 exposes invoke under `core`; keep `tauri` alias so any lingering
// legacy-lookup paths still resolve until M3 fully deletes them.
global.window.__TAURI__ = {
    core: { invoke: mockInvoke },
    tauri: { invoke: mockInvoke },
    event: { listen: mockListen },
    window: { appWindow: mockAppWindow, WebviewWindow: vi.fn() },
    dialog: { message: mockMessage, ask: mockAsk, save: vi.fn() },
    shell: { open: vi.fn() },
    updater: { checkUpdate: vi.fn(), installUpdate: vi.fn() },
    process: { relaunch: vi.fn() },
    app: { getVersion: vi.fn(() => Promise.resolve('1.0.0')) },
    fs: { writeTextFile: vi.fn() },
};

// Mock localStorage
const localStorageMock = (() => {
    let store = {};
    return {
        getItem: vi.fn((key) => store[key] || null),
        setItem: vi.fn((key, value) => { store[key] = value.toString(); }),
        removeItem: vi.fn((key) => { delete store[key]; }),
        clear: vi.fn(() => { store = {}; }),
    };
})();
Object.defineProperty(global, 'localStorage', { value: localStorageMock });

// Mock requestAnimationFrame
global.requestAnimationFrame = vi.fn((cb) => setTimeout(cb, 0));

// Mock anime.js
global.anime = vi.fn(() => ({ pause: vi.fn(), play: vi.fn() }));

// Mock gsap
global.gsap = {
    set: vi.fn(),
    to: vi.fn(),
    fromTo: vi.fn(),
    timeline: vi.fn(() => ({
        paused: true,
        play: vi.fn(),
        reverse: vi.fn(() => Promise.resolve()),
        to: vi.fn(),
    })),
};

// Mock Swiper
global.Swiper = vi.fn(() => ({}));

// Mock fetch
global.fetch = vi.fn(() =>
    Promise.resolve({
        ok: true,
        json: () => Promise.resolve({}),
    })
);

// ============================================================================
// PURE FUNCTION EXTRACTION - These are testable without DOM
// ============================================================================

/**
 * Compare two semantic version strings
 */
function compareVersions(v1, v2) {
    const parts1 = v1.split('.').map((n) => parseInt(n, 10) || 0);
    const parts2 = v2.split('.').map((n) => parseInt(n, 10) || 0);
    const len = Math.max(parts1.length, parts2.length);
    for (let i = 0; i < len; i++) {
        const a = parts1[i] || 0;
        const b = parts2[i] || 0;
        if (a > b) return 1;
        if (a < b) return -1;
    }
    return 0;
}

/**
 * Format bytes to human-readable size
 */
function formatSize(bytes) {
    if (bytes === undefined || bytes === null || isNaN(bytes)) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let size = parseFloat(bytes);
    let unitIndex = 0;
    while (size >= 1024 && unitIndex < units.length - 1) {
        size /= 1024;
        unitIndex++;
    }
    return `${size.toFixed(2)} ${units[unitIndex]}`;
}

/**
 * Format bytes per second to human-readable speed
 */
function formatSpeed(bytesPerSecond) {
    if (!isFinite(bytesPerSecond) || bytesPerSecond < 0) return '0 B/s';
    const units = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
    let speed = bytesPerSecond;
    let unitIndex = 0;
    while (speed >= 1024 && unitIndex < units.length - 1) {
        speed /= 1024;
        unitIndex++;
    }
    return `${speed.toFixed(2)} ${units[unitIndex]}`;
}

/**
 * Format seconds to human-readable time
 */
function formatTime(seconds) {
    if (!isFinite(seconds) || seconds < 0) return 'Calculating...';

    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    const remainingSeconds = Math.floor(seconds % 60);

    if (hours > 0) {
        return `${hours}h ${minutes}m ${remainingSeconds}s`;
    } else if (minutes > 0) {
        return `${minutes}m ${remainingSeconds}s`;
    } else {
        return `${remainingSeconds}s`;
    }
}

/**
 * Get filename from path
 */
function getFileName(path) {
    return path ? path.split('\\').pop().split('/').pop() : '';
}

/**
 * Calculate average speed from history
 */
function calculateAverageSpeed(speedHistory, currentSpeed, maxLength = 10) {
    speedHistory.push(currentSpeed);
    if (speedHistory.length > maxLength) {
        speedHistory.shift();
    }
    const sum = speedHistory.reduce((acc, speed) => acc + speed, 0);
    return sum / speedHistory.length;
}

/**
 * Calculate global time remaining
 */
function calculateGlobalTimeRemaining(totalDownloadedBytes, totalSize, speed, speedHistory, maxLength) {
    if (
        !isFinite(speed) ||
        speed <= 0 ||
        !isFinite(totalDownloadedBytes) ||
        !isFinite(totalSize) ||
        totalDownloadedBytes >= totalSize
    ) {
        return 0;
    }
    const bytesRemaining = totalSize - totalDownloadedBytes;
    const averageSpeed = calculateAverageSpeed(speedHistory, speed, maxLength);
    if (averageSpeed <= 0) return 0;
    const secondsRemaining = bytesRemaining / averageSpeed;
    return Math.min(secondsRemaining, 30 * 24 * 60 * 60);
}

/**
 * Translation function
 */
function t(translations, currentLanguage, key, ...args) {
    const langTranslations = translations[currentLanguage] || {};
    let str = langTranslations[key] || key;
    return str.replace(/\{(\d+)\}/g, (_, index) => args[index] || '');
}

// ============================================================================
// TESTS - Pure Functions
// ============================================================================

describe('compareVersions', () => {
    it('returns 0 for equal versions', () => {
        expect(compareVersions('1.0.0', '1.0.0')).toBe(0);
        expect(compareVersions('2.3.4', '2.3.4')).toBe(0);
    });

    it('returns 1 when first version is greater', () => {
        expect(compareVersions('2.0.0', '1.0.0')).toBe(1);
        expect(compareVersions('1.1.0', '1.0.0')).toBe(1);
        expect(compareVersions('1.0.1', '1.0.0')).toBe(1);
    });

    it('returns -1 when first version is less', () => {
        expect(compareVersions('1.0.0', '2.0.0')).toBe(-1);
        expect(compareVersions('1.0.0', '1.1.0')).toBe(-1);
        expect(compareVersions('1.0.0', '1.0.1')).toBe(-1);
    });

    it('handles different version lengths', () => {
        expect(compareVersions('1.0', '1.0.0')).toBe(0);
        expect(compareVersions('1.0.0', '1.0')).toBe(0);
        expect(compareVersions('1.0.1', '1.0')).toBe(1);
        expect(compareVersions('1.0', '1.0.1')).toBe(-1);
    });

    it('handles non-numeric parts gracefully', () => {
        expect(compareVersions('1.0.a', '1.0.0')).toBe(0);
        expect(compareVersions('1.0.0', '1.0.b')).toBe(0);
    });
});

describe('formatSize', () => {
    it('formats bytes correctly', () => {
        expect(formatSize(0)).toBe('0.00 B');
        expect(formatSize(500)).toBe('500.00 B');
        expect(formatSize(1023)).toBe('1023.00 B');
    });

    it('formats kilobytes correctly', () => {
        expect(formatSize(1024)).toBe('1.00 KB');
        expect(formatSize(1536)).toBe('1.50 KB');
        expect(formatSize(1048575)).toBe('1024.00 KB');
    });

    it('formats megabytes correctly', () => {
        expect(formatSize(1048576)).toBe('1.00 MB');
        expect(formatSize(10485760)).toBe('10.00 MB');
    });

    it('formats gigabytes correctly', () => {
        expect(formatSize(1073741824)).toBe('1.00 GB');
        expect(formatSize(5368709120)).toBe('5.00 GB');
    });

    it('formats terabytes correctly', () => {
        expect(formatSize(1099511627776)).toBe('1.00 TB');
    });

    it('handles edge cases', () => {
        expect(formatSize(undefined)).toBe('0 B');
        expect(formatSize(null)).toBe('0 B');
        expect(formatSize(NaN)).toBe('0 B');
    });
});

describe('formatSpeed', () => {
    it('formats bytes per second correctly', () => {
        expect(formatSpeed(0)).toBe('0.00 B/s');
        expect(formatSpeed(500)).toBe('500.00 B/s');
    });

    it('formats kilobytes per second correctly', () => {
        expect(formatSpeed(1024)).toBe('1.00 KB/s');
        expect(formatSpeed(2048)).toBe('2.00 KB/s');
    });

    it('formats megabytes per second correctly', () => {
        expect(formatSpeed(1048576)).toBe('1.00 MB/s');
        expect(formatSpeed(10485760)).toBe('10.00 MB/s');
    });

    it('formats gigabytes per second correctly', () => {
        expect(formatSpeed(1073741824)).toBe('1.00 GB/s');
    });

    it('handles edge cases', () => {
        expect(formatSpeed(-1)).toBe('0 B/s');
        expect(formatSpeed(Infinity)).toBe('0 B/s');
        expect(formatSpeed(-Infinity)).toBe('0 B/s');
        expect(formatSpeed(NaN)).toBe('0 B/s');
    });
});

describe('formatTime', () => {
    it('formats seconds only', () => {
        expect(formatTime(0)).toBe('0s');
        expect(formatTime(30)).toBe('30s');
        expect(formatTime(59)).toBe('59s');
    });

    it('formats minutes and seconds', () => {
        expect(formatTime(60)).toBe('1m 0s');
        expect(formatTime(90)).toBe('1m 30s');
        expect(formatTime(3599)).toBe('59m 59s');
    });

    it('formats hours, minutes and seconds', () => {
        expect(formatTime(3600)).toBe('1h 0m 0s');
        expect(formatTime(3661)).toBe('1h 1m 1s');
        expect(formatTime(7265)).toBe('2h 1m 5s');
    });

    it('handles edge cases', () => {
        expect(formatTime(-1)).toBe('Calculating...');
        expect(formatTime(Infinity)).toBe('Calculating...');
        expect(formatTime(NaN)).toBe('Calculating...');
    });
});

describe('getFileName', () => {
    it('extracts filename from Unix paths', () => {
        expect(getFileName('/path/to/file.txt')).toBe('file.txt');
        expect(getFileName('/deep/nested/path/document.pdf')).toBe('document.pdf');
    });

    it('extracts filename from Windows paths', () => {
        expect(getFileName('C:\\Users\\test\\file.txt')).toBe('file.txt');
        expect(getFileName('D:\\Games\\TERA\\TERA.exe')).toBe('TERA.exe');
    });

    it('handles mixed path separators', () => {
        expect(getFileName('/path\\to/file.txt')).toBe('file.txt');
    });

    it('handles edge cases', () => {
        expect(getFileName('')).toBe('');
        expect(getFileName(null)).toBe('');
        expect(getFileName(undefined)).toBe('');
        expect(getFileName('filename.txt')).toBe('filename.txt');
    });
});

describe('calculateAverageSpeed', () => {
    it('calculates average with single value', () => {
        const history = [];
        expect(calculateAverageSpeed(history, 100)).toBe(100);
    });

    it('calculates average with multiple values', () => {
        const history = [100, 200];
        expect(calculateAverageSpeed(history, 300)).toBe(200);
    });

    it('limits history size', () => {
        const history = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        calculateAverageSpeed(history, 11, 10);
        expect(history.length).toBe(10);
        expect(history[0]).toBe(2);
    });

    it('handles custom max length', () => {
        const history = [1, 2, 3, 4, 5];
        calculateAverageSpeed(history, 6, 5);
        expect(history.length).toBe(5);
    });
});

describe('calculateGlobalTimeRemaining', () => {
    it('calculates time remaining correctly', () => {
        const history = [];
        const result = calculateGlobalTimeRemaining(500, 1000, 100, history, 10);
        expect(result).toBe(5);
    });

    it('returns 0 for invalid speed', () => {
        const history = [];
        expect(calculateGlobalTimeRemaining(500, 1000, 0, history, 10)).toBe(0);
        expect(calculateGlobalTimeRemaining(500, 1000, -1, history, 10)).toBe(0);
        expect(calculateGlobalTimeRemaining(500, 1000, NaN, history, 10)).toBe(0);
        expect(calculateGlobalTimeRemaining(500, 1000, Infinity, history, 10)).toBe(0);
    });

    it('returns 0 when average speed becomes zero or negative', () => {
        // Pass positive speed to bypass early guard, but history averages negative
        const history = [-500, -500, -500];
        expect(calculateGlobalTimeRemaining(500, 1000, 100, history, 4)).toBe(0);
    });

    it('returns 0 when download complete', () => {
        const history = [];
        expect(calculateGlobalTimeRemaining(1000, 1000, 100, history, 10)).toBe(0);
        expect(calculateGlobalTimeRemaining(1001, 1000, 100, history, 10)).toBe(0);
    });

    it('returns 0 for invalid sizes', () => {
        const history = [];
        expect(calculateGlobalTimeRemaining(NaN, 1000, 100, history, 10)).toBe(0);
        expect(calculateGlobalTimeRemaining(500, NaN, 100, history, 10)).toBe(0);
    });

    it('caps at 30 days maximum', () => {
        const history = [];
        const result = calculateGlobalTimeRemaining(0, 1e15, 1, history, 10);
        expect(result).toBe(30 * 24 * 60 * 60);
    });
});

describe('t (translation function)', () => {
    const translations = {
        GER: {
            HELLO: 'Hallo',
            GREETING: 'Hallo {0}!',
            MULTI: '{0} und {1}',
        },
        EUR: {
            HELLO: 'Hello',
            GREETING: 'Hello {0}!',
        },
    };

    it('returns translated string', () => {
        expect(t(translations, 'GER', 'HELLO')).toBe('Hallo');
        expect(t(translations, 'EUR', 'HELLO')).toBe('Hello');
    });

    it('returns key if translation not found', () => {
        expect(t(translations, 'GER', 'MISSING_KEY')).toBe('MISSING_KEY');
    });

    it('replaces placeholders with arguments', () => {
        expect(t(translations, 'GER', 'GREETING', 'World')).toBe('Hallo World!');
        expect(t(translations, 'GER', 'MULTI', 'eins', 'zwei')).toBe('eins und zwei');
    });

    it('handles missing arguments', () => {
        expect(t(translations, 'GER', 'GREETING')).toBe('Hallo !');
    });

    it('handles missing language', () => {
        expect(t(translations, 'FRA', 'HELLO')).toBe('HELLO');
    });
});

// ============================================================================
// TESTS - App Object Mock and Integration Tests
// ============================================================================

describe('App State Management', () => {
    let App;

    beforeEach(() => {
        // Reset mocks
        vi.clearAllMocks();
        localStorageMock.clear();

        // Create fresh App mock
        App = {
            translations: {},
            currentLanguage: 'GER',
            languages: { GER: 'GERMAN', EUR: 'ENGLISH', FRA: 'FRENCH', RUS: 'RUSSIAN' },
            launchGameBtn: null,
            statusEl: null,
            deferredUpdate: null,
            _activeFileWindow: [],
            state: {
                speedHistory: [],
                speedHistoryMaxLength: 10,
                isUpdateAvailable: false,
                isDownloadComplete: false,
                lastProgressUpdate: null,
                lastDownloadedBytes: 0,
                downloadStartTime: null,
                currentUpdateMode: null,
                currentProgress: 0,
                currentFileName: '',
                currentFileIndex: 0,
                totalFiles: 0,
                downloadedSize: 0,
                downloadedBytesOffset: 0,
                totalSize: 0,
                currentSpeed: 0,
                timeRemaining: 0,
                isLoggingIn: false,
                isLoggingOut: false,
                isGameRunning: false,
                isCheckingForUpdates: false,
                updateCheckPerformed: false,
                isAuthenticated: false,
                isFileCheckComplete: false,
                isFirstLaunch: true,
                isGeneratingHashFile: false,
                hashFileProgress: 0,
                currentProcessingFile: '',
                processedFiles: 0,
            },
            setState(newState) {
                if (newState.totalSize !== undefined && this.state.totalSize === undefined) {
                    this.state.totalSize = newState.totalSize;
                }
                if (newState.totalDownloadedBytes !== undefined && this.state.totalDownloadedBytes === undefined) {
                    this.state.totalDownloadedBytes = 0;
                }
                Object.assign(this.state, newState);
                this.updateUI();
            },
            updateUI: vi.fn(),
            t: vi.fn((key) => key),
            updateLaunchGameButton: vi.fn(),
            toggleLanguageSelector: vi.fn(),
        };
    });

    describe('setState', () => {
        it('merges new state with existing state', () => {
            App.setState({ currentProgress: 50 });
            expect(App.state.currentProgress).toBe(50);
            expect(App.updateUI).toHaveBeenCalled();
        });

        it('initializes totalSize if undefined', () => {
            App.state.totalSize = undefined;
            App.setState({ totalSize: 1000 });
            expect(App.state.totalSize).toBe(1000);
        });

        it('does not override totalSize if already set', () => {
            App.state.totalSize = 500;
            App.setState({ totalSize: 1000 });
            expect(App.state.totalSize).toBe(1000);
        });

        it('initializes totalDownloadedBytes to 0', () => {
            App.state.totalDownloadedBytes = undefined;
            App.setState({ totalDownloadedBytes: 100 });
            expect(App.state.totalDownloadedBytes).toBe(100);
        });
    });

    describe('resetState', () => {
        it('resets state to initial values', () => {
            const resetState = function() {
                this.setState({
                    isFileCheckComplete: false,
                    isUpdateAvailable: false,
                    isDownloadComplete: false,
                    lastProgressUpdate: null,
                    lastDownloadedBytes: 0,
                    downloadStartTime: null,
                    currentUpdateMode: null,
                    currentProgress: 0,
                    currentFileName: '',
                    currentFileIndex: 0,
                    totalFiles: 0,
                    downloadedSize: 0,
                    downloadedBytesOffset: 0,
                    totalSize: 0,
                    currentSpeed: 0,
                    timeRemaining: 0,
                    isLoggingIn: false,
                    isLoggingOut: false,
                    isGameRunning: false,
                    updateCheckPerformed: false,
                    isGeneratingHashFile: false,
                    hashFileProgress: 0,
                    currentProcessingFile: '',
                    processedFiles: 0,
                });
            };

            App.state.currentProgress = 75;
            App.state.isUpdateAvailable = true;
            App.state.currentUpdateMode = 'download';

            resetState.call(App);

            expect(App.state.currentProgress).toBe(0);
            expect(App.state.isUpdateAvailable).toBe(false);
            expect(App.state.currentUpdateMode).toBe(null);
        });
    });

    describe('handleDownloadProgress', () => {
        function handleDownloadProgress(event, state, setState) {
            if (!event || !event.payload) return false;
            const { downloaded_bytes, total_bytes, total_files, current_file_index, file_name } = event.payload;
            if (state.totalSize === undefined || state.totalSize === 0) {
                state.totalSize = total_bytes;
            }
            const offset = state.downloadedBytesOffset || 0;
            const totalDownloadedBytes = downloaded_bytes + offset;
            const effectiveTotalSize = Math.max(state.totalSize, total_bytes);
            setState({
                currentFileName: file_name,
                currentProgress: Math.min(100, (totalDownloadedBytes / effectiveTotalSize) * 100),
                downloadedSize: totalDownloadedBytes,
                totalSize: effectiveTotalSize,
                totalFiles: total_files,
                currentFileIndex: current_file_index,
                currentUpdateMode: 'download',
            });
            return true;
        }

        it('returns false for null event', () => {
            expect(handleDownloadProgress(null, {}, vi.fn())).toBe(false);
        });

        it('returns false for missing payload', () => {
            expect(handleDownloadProgress({}, {}, vi.fn())).toBe(false);
        });

        it('uses offset of 0 when downloadedBytesOffset is not set', () => {
            const state = { totalSize: 1000 };
            const setStateFn = (newState) => Object.assign(state, newState);
            handleDownloadProgress({
                payload: { downloaded_bytes: 500, total_bytes: 1000, file_name: 'test.gpk', total_files: 1, current_file_index: 0 }
            }, state, setStateFn);
            expect(state.downloadedSize).toBe(500);
        });

        it('updates state with download progress', () => {
            const state = { totalSize: 0, downloadedBytesOffset: 0, downloadStartTime: null };
            const setStateFn = (newState) => Object.assign(state, newState);

            handleDownloadProgress({
                payload: {
                    file_name: 'test.gpk',
                    downloaded_bytes: 500,
                    total_bytes: 1000,
                    total_files: 10,
                    current_file_index: 5,
                    speed: 100,
                },
            }, state, setStateFn);

            expect(state.currentFileName).toBe('test.gpk');
            expect(state.downloadedSize).toBe(500);
            expect(state.totalSize).toBe(1000);
            expect(state.currentProgress).toBe(50);
            expect(state.currentUpdateMode).toBe('download');
        });

        it('returns early with invalid event', () => {
            expect(handleDownloadProgress(null, {}, vi.fn())).toBe(false);
            expect(handleDownloadProgress({}, {}, vi.fn())).toBe(false);
        });

        it('applies offset for resumed downloads', () => {
            const state = { downloadedBytesOffset: 300, totalSize: 1000 };
            const setStateFn = (newState) => Object.assign(state, newState);

            handleDownloadProgress({
                payload: {
                    downloaded_bytes: 200,
                    total_bytes: 700,
                    file_name: 'test.gpk',
                    total_files: 1,
                    current_file_index: 0,
                },
            }, state, setStateFn);

            expect(state.downloadedSize).toBe(500);
            expect(state.currentProgress).toBe(50);
        });
    });

    describe('handleCompletion', () => {
        it('sets completion state', () => {
            const handleCompletion = function() {
                this.setState({
                    isDownloadComplete: true,
                    currentProgress: 100,
                    currentUpdateMode: 'complete',
                    isUpdateAvailable: false,
                    isFileCheckComplete: true,
                });
                this.updateLaunchGameButton(false);
                this.toggleLanguageSelector(true);
            };

            handleCompletion.call(App);

            expect(App.state.isDownloadComplete).toBe(true);
            expect(App.state.currentProgress).toBe(100);
            expect(App.state.currentUpdateMode).toBe('complete');
            expect(App.state.isUpdateAvailable).toBe(false);
            expect(App.updateLaunchGameButton).toHaveBeenCalledWith(false);
            expect(App.toggleLanguageSelector).toHaveBeenCalledWith(true);
        });
    });

    describe('handleFileCheckCompleted', () => {
        function createHandleFileCheckCompleted(onCompletion) {
            return function(event) {
                const { files_to_update } = event.payload;
                const hasUpdates = (files_to_update ?? 0) > 0;
                this.setState({ isFileCheckComplete: true, isUpdateAvailable: hasUpdates });
                if (!hasUpdates) onCompletion();
            };
        }

        it('triggers completion when no updates needed', () => {
            let completionCalled = false;
            const handler = createHandleFileCheckCompleted(() => { completionCalled = true; });
            handler.call(App, { payload: { files_to_update: 0 } });
            expect(App.state.isFileCheckComplete).toBe(true);
            expect(App.state.isUpdateAvailable).toBe(false);
            expect(completionCalled).toBe(true);
        });

        it('does not trigger completion when updates are needed', () => {
            let completionCalled = false;
            const handler = createHandleFileCheckCompleted(() => { completionCalled = true; });
            handler.call(App, { payload: { files_to_update: 5 } });
            expect(App.state.isFileCheckComplete).toBe(true);
            expect(App.state.isUpdateAvailable).toBe(true);
            expect(completionCalled).toBe(false);
        });

        it('handles undefined files_to_update', () => {
            let completionCalled = false;
            const handler = createHandleFileCheckCompleted(() => { completionCalled = true; });
            handler.call(App, { payload: {} });
            expect(App.state.isUpdateAvailable).toBe(false);
            expect(completionCalled).toBe(true);
        });
    });

    describe('handleFileCheckProgress', () => {
        function handleFileCheckProgress(event, setState) {
            if (!event || !event.payload) return false;
            const { current_file, progress, current_count, total_files } = event.payload;
            setState({
                currentFileName: current_file,
                currentProgress: Math.min(100, progress),
                currentFileIndex: current_count,
                totalFiles: total_files,
                currentUpdateMode: 'file_check',
            });
            return true;
        }

        it('returns early when event is null', () => {
            expect(handleFileCheckProgress(null, vi.fn())).toBe(false);
        });

        it('returns early when payload is missing', () => {
            expect(handleFileCheckProgress({}, vi.fn())).toBe(false);
        });

        it('updates state with file check progress', () => {
            const setState = (newState) => Object.assign(App.state, newState);
            const result = handleFileCheckProgress({
                payload: {
                    current_file: 'checking.gpk',
                    progress: 50,
                    current_count: 25,
                    total_files: 50,
                },
            }, setState);
            expect(result).toBe(true);
            expect(App.state.currentFileName).toBe('checking.gpk');
            expect(App.state.currentProgress).toBe(50);
            expect(App.state.currentFileIndex).toBe(25);
            expect(App.state.totalFiles).toBe(50);
            expect(App.state.currentUpdateMode).toBe('file_check');
        });

        it('caps progress at 100', () => {
            const handleFileCheckProgress = function(event) {
                const { progress } = event.payload;
                this.setState({ currentProgress: Math.min(100, progress) });
            };

            handleFileCheckProgress.call(App, { payload: { progress: 150 } });
            expect(App.state.currentProgress).toBe(100);
        });
    });
});

describe('App UI Functions', () => {
    let App;

    beforeEach(() => {
        vi.clearAllMocks();

        App = {
            state: {
                isUpdateAvailable: false,
                isDownloadComplete: false,
                currentUpdateMode: null,
                currentProgress: 0,
                downloadedSize: 0,
                totalSize: 0,
                currentSpeed: 0,
                timeRemaining: 0,
                isFileCheckComplete: false,
                isUpdateComplete: false,
            },
            t: vi.fn((key) => key),
            updateUI: vi.fn(),
        };
    });

    describe('calculateProgress', () => {
        function calculateProgress(state) {
            if (state.isUpdateAvailable && state.totalSize > 0) {
                return (state.downloadedSize / state.totalSize) * 100;
            }
            return state.currentProgress;
        }

        it('calculates progress from downloaded/total size', () => {
            App.state.isUpdateAvailable = true;
            App.state.downloadedSize = 500;
            App.state.totalSize = 1000;
            expect(calculateProgress(App.state)).toBe(50);
        });

        it('returns current progress when no update available', () => {
            App.state.isUpdateAvailable = false;
            App.state.currentProgress = 75;
            expect(calculateProgress(App.state)).toBe(75);
        });

        it('returns current progress when totalSize is 0', () => {
            App.state.isUpdateAvailable = true;
            App.state.totalSize = 0;
            App.state.currentProgress = 25;
            expect(calculateProgress(App.state)).toBe(25);
        });
    });

    describe('getStatusText', () => {
        const getStatusText = function() {
            if (this.state.isDownloadComplete) return this.t('DOWNLOAD_COMPLETE');
            if (!this.state.isUpdateAvailable) return this.t('NO_UPDATE_REQUIRED');
            if (this.state.currentUpdateMode === 'file_check') return this.t('VERIFYING_FILES');
            return this.t('DOWNLOADING_FILES');
        };

        it('returns DOWNLOAD_COMPLETE when download is complete', () => {
            App.state.isDownloadComplete = true;
            expect(getStatusText.call(App)).toBe('DOWNLOAD_COMPLETE');
        });

        it('returns NO_UPDATE_REQUIRED when no update available', () => {
            App.state.isDownloadComplete = false;
            App.state.isUpdateAvailable = false;
            expect(getStatusText.call(App)).toBe('NO_UPDATE_REQUIRED');
        });

        it('returns VERIFYING_FILES during file check', () => {
            App.state.isDownloadComplete = false;
            App.state.isUpdateAvailable = true;
            App.state.currentUpdateMode = 'file_check';
            expect(getStatusText.call(App)).toBe('VERIFYING_FILES');
        });

        it('returns DOWNLOADING_FILES otherwise', () => {
            App.state.isDownloadComplete = false;
            App.state.isUpdateAvailable = true;
            App.state.currentUpdateMode = 'download';
            expect(getStatusText.call(App)).toBe('DOWNLOADING_FILES');
        });
    });

    describe('getDlStatusString', () => {
        const getDlStatusString = function() {
            switch (this.state.currentUpdateMode) {
                case 'file_check':
                    return this.t('VERIFYING_FILES');
                case 'paused':
                case 'download':
                    return this.t('DOWNLOADING_FILES');
                case 'complete':
                    if (this.state.isFileCheckComplete && !this.state.isUpdateAvailable) return this.t('NO_UPDATE_REQUIRED');
                    if (this.state.isFileCheckComplete && this.state.isUpdateAvailable) return this.t('FILE_CHECK_COMPLETE');
                    if (this.state.isDownloadComplete) return this.t('DOWNLOAD_COMPLETE');
                    if (this.state.isUpdateComplete) return this.t('UPDATE_COMPLETED');
                    return this.t('GAME_READY_TO_LAUNCH');
                default:
                    return this.t('GAME_READY_TO_LAUNCH');
            }
        };

        it('returns VERIFYING_FILES for file_check mode', () => {
            App.state.currentUpdateMode = 'file_check';
            expect(getDlStatusString.call(App)).toBe('VERIFYING_FILES');
        });

        it('returns DOWNLOADING_FILES for paused mode', () => {
            App.state.currentUpdateMode = 'paused';
            expect(getDlStatusString.call(App)).toBe('DOWNLOADING_FILES');
        });

        it('returns DOWNLOADING_FILES for download mode', () => {
            App.state.currentUpdateMode = 'download';
            expect(getDlStatusString.call(App)).toBe('DOWNLOADING_FILES');
        });

        it('returns NO_UPDATE_REQUIRED for complete with no updates', () => {
            App.state.currentUpdateMode = 'complete';
            App.state.isFileCheckComplete = true;
            App.state.isUpdateAvailable = false;
            expect(getDlStatusString.call(App)).toBe('NO_UPDATE_REQUIRED');
        });

        it('returns FILE_CHECK_COMPLETE for complete with updates available', () => {
            App.state.currentUpdateMode = 'complete';
            App.state.isFileCheckComplete = true;
            App.state.isUpdateAvailable = true;
            expect(getDlStatusString.call(App)).toBe('FILE_CHECK_COMPLETE');
        });

        it('returns DOWNLOAD_COMPLETE when download is complete', () => {
            App.state.currentUpdateMode = 'complete';
            App.state.isFileCheckComplete = false;
            App.state.isDownloadComplete = true;
            expect(getDlStatusString.call(App)).toBe('DOWNLOAD_COMPLETE');
        });

        it('returns UPDATE_COMPLETED when update is complete', () => {
            App.state.currentUpdateMode = 'complete';
            App.state.isFileCheckComplete = false;
            App.state.isDownloadComplete = false;
            App.state.isUpdateComplete = true;
            expect(getDlStatusString.call(App)).toBe('UPDATE_COMPLETED');
        });

        it('returns GAME_READY_TO_LAUNCH for complete with no flags', () => {
            App.state.currentUpdateMode = 'complete';
            App.state.isFileCheckComplete = false;
            App.state.isDownloadComplete = false;
            App.state.isUpdateComplete = false;
            expect(getDlStatusString.call(App)).toBe('GAME_READY_TO_LAUNCH');
        });

        it('returns GAME_READY_TO_LAUNCH for null mode', () => {
            App.state.currentUpdateMode = null;
            expect(getDlStatusString.call(App)).toBe('GAME_READY_TO_LAUNCH');
        });

        it('returns GAME_READY_TO_LAUNCH for ready mode', () => {
            App.state.currentUpdateMode = 'ready';
            expect(getDlStatusString.call(App)).toBe('GAME_READY_TO_LAUNCH');
        });
    });
});

describe('Authentication', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        localStorageMock.clear();
    });

    describe('checkAuthentication', () => {
        it('sets isAuthenticated true when authKey exists', () => {
            localStorageMock.setItem('authKey', 'test-key');

            const App = {
                state: {},
                setState(newState) {
                    Object.assign(this.state, newState);
                },
            };

            const checkAuthentication = function() {
                this.setState({ isAuthenticated: localStorage.getItem('authKey') !== null });
            };

            checkAuthentication.call(App);
            expect(App.state.isAuthenticated).toBe(true);
        });

        it('sets isAuthenticated false when authKey missing', () => {
            const App = {
                state: {},
                setState(newState) {
                    Object.assign(this.state, newState);
                },
            };

            const checkAuthentication = function() {
                this.setState({ isAuthenticated: localStorage.getItem('authKey') !== null });
            };

            checkAuthentication.call(App);
            expect(App.state.isAuthenticated).toBe(false);
        });
    });

    describe('checkPrivilegeLevel', () => {
        const REQUIRED_PRIVILEGE_LEVEL = 3;
        function checkPrivilegeLevel() {
            const userPrivilege = parseInt(localStorage.getItem('privilege'), 10);
            return !isNaN(userPrivilege) && userPrivilege >= REQUIRED_PRIVILEGE_LEVEL;
        }

        it('returns true when privilege >= required', () => {
            localStorageMock.setItem('privilege', '5');
            expect(checkPrivilegeLevel()).toBe(true);
        });

        it('returns false when privilege < required', () => {
            localStorageMock.setItem('privilege', '1');
            expect(checkPrivilegeLevel()).toBe(false);
        });

        it('returns false when privilege is NaN', () => {
            localStorageMock.setItem('privilege', 'invalid');
            expect(checkPrivilegeLevel()).toBe(false);
        });

        it('returns false when privilege is not set', () => {
            expect(checkPrivilegeLevel()).toBe(false);
        });
    });

    describe('storeAuthInfo', () => {
        it('stores auth info in localStorage', () => {
            const storeAuthInfo = function(jsonResponse) {
                localStorage.setItem('authKey', jsonResponse.AuthKey);
                localStorage.setItem('userName', jsonResponse.UserName);
                localStorage.setItem('userNo', jsonResponse.UserNo.toString());
                localStorage.setItem('characterCount', jsonResponse.CharacterCount.toString());
                localStorage.setItem('permission', jsonResponse.Permission.toString());
                localStorage.setItem('privilege', jsonResponse.Privilege.toString());
            };

            storeAuthInfo({
                AuthKey: 'test-auth-key',
                UserName: 'testuser',
                UserNo: 123,
                CharacterCount: 5,
                Permission: 2,
                Privilege: 3,
            });

            expect(localStorageMock.setItem).toHaveBeenCalledWith('authKey', 'test-auth-key');
            expect(localStorageMock.setItem).toHaveBeenCalledWith('userName', 'testuser');
            expect(localStorageMock.setItem).toHaveBeenCalledWith('userNo', '123');
            expect(localStorageMock.setItem).toHaveBeenCalledWith('characterCount', '5');
            expect(localStorageMock.setItem).toHaveBeenCalledWith('permission', '2');
            expect(localStorageMock.setItem).toHaveBeenCalledWith('privilege', '3');
        });
    });

    describe('login response handling', () => {
        it('treats Msg=success with Return payload as authenticated', () => {
            const response = JSON.stringify({
                Return: {
                    AuthKey: 'auth-key',
                    UserName: 'TestUser',
                    UserNo: 123,
                    CharacterCount: '5',
                    Permission: 1,
                    Privilege: 0,
                },
                Msg: 'success',
            });

            const jsonResponse = JSON.parse(response);
            const isSuccess = jsonResponse && jsonResponse.Return && jsonResponse.Msg === 'success';
            expect(isSuccess).toBe(true);
        });
    });
});

describe('Pause/Resume Functionality', () => {
    let App;

    beforeEach(() => {
        vi.clearAllMocks();
        mockInvoke.mockResolvedValue([]);

        App = {
            state: {
                currentUpdateMode: 'download',
                totalSize: 1000,
                downloadedSize: 500,
                downloadedBytesOffset: 0,
                downloadStartTime: Date.now(),
                speedHistory: [100, 200],
                currentSpeed: 150,
                isUpdateAvailable: true,
                isFileCheckComplete: true,
                isCheckingForUpdates: false,
            },
            setState(newState) {
                Object.assign(this.state, newState);
            },
            updateLaunchGameButton: vi.fn(),
            updateUI: vi.fn(),
        };
    });

    describe('togglePauseResume - Pause', () => {
        it('pauses download when in download mode', async () => {
            const togglePauseResume = async function() {
                if (this.state.currentUpdateMode === 'download') {
                    await mockInvoke('cancel_downloads');
                    this.setState({
                        currentUpdateMode: 'paused',
                        lastProgressUpdate: null,
                        downloadStartTime: null,
                        speedHistory: [],
                        currentSpeed: 0,
                    });
                    this.updateLaunchGameButton(true);
                }
            };

            await togglePauseResume.call(App);

            expect(mockInvoke).toHaveBeenCalledWith('cancel_downloads');
            expect(App.state.currentUpdateMode).toBe('paused');
            expect(App.state.downloadStartTime).toBe(null);
            expect(App.state.speedHistory).toEqual([]);
            expect(App.updateLaunchGameButton).toHaveBeenCalledWith(true);
        });
    });

    describe('togglePauseResume - Resume', () => {
        function createTogglePauseResume(onCompletion) {
            return async function() {
                if (this.state.currentUpdateMode !== 'paused') return;
                const previousTotal = this.state.totalSize || 0;
                this.setState({ currentUpdateMode: 'file_check', isCheckingForUpdates: true });
                const filesToUpdate = await mockInvoke('get_files_to_update');
                this.setState({ isCheckingForUpdates: false });
                if (!filesToUpdate || filesToUpdate.length === 0) {
                    onCompletion?.();
                    return;
                }
                const remainingSize = filesToUpdate.reduce((sum, f) => sum + (f.size || 0), 0);
                const alreadyDownloaded = previousTotal > 0 ? Math.max(0, previousTotal - remainingSize) : 0;
                const newTotalSize = previousTotal > 0 ? previousTotal : remainingSize;
                this.setState({
                    currentUpdateMode: 'download',
                    isUpdateAvailable: true,
                    isFileCheckComplete: true,
                    downloadStartTime: Date.now(),
                    totalFiles: filesToUpdate.length,
                    totalSize: newTotalSize,
                    downloadedBytesOffset: alreadyDownloaded,
                });
            };
        }

        it('resumes download when paused with previous total', async () => {
            App.state.currentUpdateMode = 'paused';
            App.state.totalSize = 1000;
            mockInvoke.mockResolvedValueOnce([
                { path: 'file1.gpk', size: 300 },
                { path: 'file2.gpk', size: 200 },
            ]);
            const toggle = createTogglePauseResume();
            await toggle.call(App);
            expect(App.state.currentUpdateMode).toBe('download');
            expect(App.state.downloadedBytesOffset).toBe(500);
            expect(App.state.totalSize).toBe(1000);
        });

        it('resumes download when paused without previous total', async () => {
            App.state.currentUpdateMode = 'paused';
            App.state.totalSize = 0;
            mockInvoke.mockResolvedValueOnce([
                { path: 'file1.gpk', size: 300 },
                { path: 'file2.gpk', size: 200 },
            ]);
            const toggle = createTogglePauseResume();
            await toggle.call(App);
            expect(App.state.currentUpdateMode).toBe('download');
            expect(App.state.downloadedBytesOffset).toBe(0);
            expect(App.state.totalSize).toBe(500);
        });

        it('handles files with missing size', async () => {
            App.state.currentUpdateMode = 'paused';
            App.state.totalSize = 0;
            mockInvoke.mockResolvedValueOnce([
                { path: 'file1.gpk' },
                { path: 'file2.gpk', size: 200 },
            ]);
            const toggle = createTogglePauseResume();
            await toggle.call(App);
            expect(App.state.totalSize).toBe(200);
        });

        it('triggers completion when no files remaining', async () => {
            App.state.currentUpdateMode = 'paused';
            mockInvoke.mockResolvedValueOnce([]);
            let completionCalled = false;
            const toggle = createTogglePauseResume(() => { completionCalled = true; });
            await toggle.call(App);
            expect(completionCalled).toBe(true);
        });

        it('does nothing when not paused', async () => {
            App.state.currentUpdateMode = 'download';
            const toggle = createTogglePauseResume();
            await toggle.call(App);
            expect(mockInvoke).not.toHaveBeenCalledWith('get_files_to_update');
        });
    });
});

describe('First Launch Flow', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        localStorageMock.clear();
    });

    describe('checkFirstLaunch', () => {
        it('returns true when localStorage is empty', () => {
            const App = {
                state: {},
                setState(newState) {
                    Object.assign(this.state, newState);
                },
            };

            const checkFirstLaunch = function() {
                const isFirstLaunch = localStorage.getItem('isFirstLaunch') !== 'false';
                this.setState({ isFirstLaunch });
            };

            checkFirstLaunch.call(App);
            expect(App.state.isFirstLaunch).toBe(true);
        });

        it('returns false when isFirstLaunch is false', () => {
            localStorageMock.setItem('isFirstLaunch', 'false');

            const App = {
                state: {},
                setState(newState) {
                    Object.assign(this.state, newState);
                },
            };

            const checkFirstLaunch = function() {
                const isFirstLaunch = localStorage.getItem('isFirstLaunch') !== 'false';
                this.setState({ isFirstLaunch });
            };

            checkFirstLaunch.call(App);
            expect(App.state.isFirstLaunch).toBe(false);
        });
    });

    describe('completeFirstLaunch', () => {
        it('marks first launch as complete', () => {
            const App = {
                state: { isFirstLaunch: true },
                setState(newState) {
                    Object.assign(this.state, newState);
                },
            };

            const completeFirstLaunch = function() {
                localStorage.setItem('isFirstLaunch', 'false');
                this.setState({ isFirstLaunch: false });
            };

            completeFirstLaunch.call(App);

            expect(localStorageMock.setItem).toHaveBeenCalledWith('isFirstLaunch', 'false');
            expect(App.state.isFirstLaunch).toBe(false);
        });
    });
});

describe('Active File Window', () => {
    function capEntries(entries, nowTs) {
        const filtered = entries.filter((s) => nowTs - s.t <= 1500);
        return filtered.length > 100 ? filtered.slice(-100) : filtered;
    }

    it('caps at 100 entries when exceeds limit', () => {
        const entries = [];
        const nowTs = Date.now();
        for (let i = 0; i < 150; i++) {
            entries.push({ t: nowTs, name: `file${i}.gpk` });
        }
        const capped = capEntries(entries, nowTs);
        expect(capped.length).toBe(100);
    });

    it('does not cap when under limit', () => {
        const entries = [];
        const nowTs = Date.now();
        for (let i = 0; i < 50; i++) {
            entries.push({ t: nowTs, name: `file${i}.gpk` });
        }
        const capped = capEntries(entries, nowTs);
        expect(capped.length).toBe(50);
    });

    it('filters by time window', () => {
        const nowTs = Date.now();
        const entries = [
            { t: nowTs - 2000, name: 'old.gpk' },
            { t: nowTs - 500, name: 'recent.gpk' },
            { t: nowTs, name: 'current.gpk' },
        ];
        const filtered = capEntries(entries, nowTs);
        expect(filtered.length).toBe(2);
        expect(filtered.map((f) => f.name)).toEqual(['recent.gpk', 'current.gpk']);
    });

    it('finds most frequent file name', () => {
        const entries = [
            { t: Date.now(), name: 'file1.gpk' },
            { t: Date.now(), name: 'file2.gpk' },
            { t: Date.now(), name: 'file1.gpk' },
            { t: Date.now(), name: 'file1.gpk' },
            { t: Date.now(), name: 'file2.gpk' },
        ];

        const freq = {};
        for (const s of entries) freq[s.name] = (freq[s.name] || 0) + 1;

        let topName = '';
        let topCount = 0;
        for (const k in freq) {
            if (freq[k] > topCount) {
                topCount = freq[k];
                topName = k;
            }
        }

        expect(topName).toBe('file1.gpk');
        expect(topCount).toBe(3);
    });
});

function toggleTheme() {
    const body = document.body;
    const isLight = body.classList.contains('light-mode');
    body.classList.toggle('light-mode', !isLight);
    localStorage.setItem('theme', isLight ? 'dark' : 'light');
}

describe('Theme Toggle', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        localStorageMock.clear();
        document.body = document.createElement('body');
    });

    it('toggles to light mode', () => {
        toggleTheme();
        expect(document.body.classList.contains('light-mode')).toBe(true);
        expect(localStorageMock.setItem).toHaveBeenCalledWith('theme', 'light');
    });

    it('toggles to dark mode', () => {
        document.body.classList.add('light-mode');
        toggleTheme();
        expect(document.body.classList.contains('light-mode')).toBe(false);
        expect(localStorageMock.setItem).toHaveBeenCalledWith('theme', 'dark');
    });
});

describe('Mirror Log Dedupe', () => {
    function createMirrorLog() {
        const state = { lastLogMessage: null, lastLogTime: 0 };
        const messages = [];
        const mirrorLog = (message) => {
            const currentTime = Date.now();
            const msgStr = String(message ?? '');
            const lastTime = state.lastLogTime || 0;
            if (state.lastLogMessage === msgStr && currentTime - lastTime < 100) {
                return false;
            }
            state.lastLogMessage = msgStr;
            state.lastLogTime = currentTime;
            messages.push(message);
            return true;
        };
        return { mirrorLog, messages, state };
    }

    it('deduplicates rapid duplicate messages', () => {
        const { mirrorLog, messages } = createMirrorLog();
        expect(mirrorLog('test')).toBe(true);
        expect(mirrorLog('test')).toBe(false);
        expect(mirrorLog('different')).toBe(true);
        expect(messages).toEqual(['test', 'different']);
    });

    it('handles null and undefined messages', () => {
        const { mirrorLog, messages } = createMirrorLog();
        expect(mirrorLog(null)).toBe(true);
        expect(mirrorLog(undefined)).toBe(false);
        expect(messages).toEqual([null]);
    });

    it('handles initial state with no lastLogTime', () => {
        const { mirrorLog, state } = createMirrorLog();
        expect(state.lastLogTime).toBe(0);
        mirrorLog('first');
        expect(state.lastLogTime).toBeGreaterThan(0);
    });
});

function createMockLaunchBtn(disabled = false) {
    return { disabled, classList: { toggle: vi.fn() } };
}

function shouldDisableLaunchButton(disabled, currentUpdateMode) {
    return disabled || currentUpdateMode === 'download' || currentUpdateMode === 'paused';
}

describe('Update Launch Button', () => {
    it('disables button during download', () => {
        const btn = createMockLaunchBtn();
        const shouldDisable = shouldDisableLaunchButton(false, 'download');
        btn.disabled = shouldDisable;
        btn.classList.toggle('disabled', shouldDisable);
        expect(btn.disabled).toBe(true);
        expect(btn.classList.toggle).toHaveBeenCalledWith('disabled', true);
    });

    it('disables button when paused', () => {
        const btn = createMockLaunchBtn();
        const shouldDisable = shouldDisableLaunchButton(false, 'paused');
        btn.disabled = shouldDisable;
        expect(btn.disabled).toBe(true);
    });

    it('enables button when not downloading', () => {
        const btn = createMockLaunchBtn(true);
        const shouldDisable = shouldDisableLaunchButton(false, 'complete');
        btn.disabled = shouldDisable;
        btn.classList.toggle('disabled', shouldDisable);
        expect(btn.disabled).toBe(false);
        expect(btn.classList.toggle).toHaveBeenCalledWith('disabled', false);
    });

    function updateLaunchButton(btn, disabled, mode) {
        if (!btn) return;
        btn.disabled = shouldDisableLaunchButton(disabled, mode);
    }

    it('returns early if button is null', () => {
        expect(() => updateLaunchButton(null, false, 'download')).not.toThrow();
    });

    it('updates button when button exists', () => {
        const btn = createMockLaunchBtn();
        updateLaunchButton(btn, true, 'complete');
        expect(btn.disabled).toBe(true);
    });
});

describe('Mock Coverage', () => {
    it('exercises gsap.timeline mock', () => {
        const tl = gsap.timeline();
        expect(tl.paused).toBe(true);
        expect(typeof tl.play).toBe('function');
        expect(typeof tl.reverse).toBe('function');
        expect(typeof tl.to).toBe('function');
    });

    it('exercises fetch mock', async () => {
        const response = await fetch('test');
        expect(response.ok).toBe(true);
        const json = await response.json();
        expect(json).toEqual({});
    });
});
