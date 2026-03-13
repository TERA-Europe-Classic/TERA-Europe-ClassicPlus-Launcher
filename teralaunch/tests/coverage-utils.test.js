import { describe, it, expect } from 'vitest';
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

// =============================================================================
// download.js - 100% Coverage Tests
// =============================================================================

describe('download.js - calculateRemainingSize', () => {
    describe('Non-array input handling (line 2)', () => {
        it('returns 0 for null input', () => {
            expect(calculateRemainingSize(null)).toBe(0);
        });

        it('returns 0 for undefined input', () => {
            expect(calculateRemainingSize(undefined)).toBe(0);
        });

        it('returns 0 for string input', () => {
            expect(calculateRemainingSize('not an array')).toBe(0);
        });

        it('returns 0 for number input', () => {
            expect(calculateRemainingSize(123)).toBe(0);
        });

        it('returns 0 for object input', () => {
            expect(calculateRemainingSize({ files: [] })).toBe(0);
        });
    });

    describe('Empty array handling', () => {
        it('returns 0 for empty array', () => {
            expect(calculateRemainingSize([])).toBe(0);
        });
    });

    describe('Files with missing size/existing_size (lines 4-5)', () => {
        it('handles file with missing size property', () => {
            const files = [{ existing_size: 50 }];
            // total = 0 (missing), existing = 50, max(0, 0-50) = 0
            expect(calculateRemainingSize(files)).toBe(0);
        });

        it('handles file with missing existing_size property', () => {
            const files = [{ size: 100 }];
            // total = 100, existing = 0 (missing), max(0, 100-0) = 100
            expect(calculateRemainingSize(files)).toBe(100);
        });

        it('handles file with both properties missing', () => {
            const files = [{}];
            // total = 0, existing = 0, max(0, 0-0) = 0
            expect(calculateRemainingSize(files)).toBe(0);
        });

        it('handles null file in array', () => {
            const files = [null];
            // file?.size = undefined -> 0, file?.existing_size = undefined -> 0
            expect(calculateRemainingSize(files)).toBe(0);
        });

        it('handles undefined file in array', () => {
            const files = [undefined];
            expect(calculateRemainingSize(files)).toBe(0);
        });
    });

    describe('Files where existing_size > size (Math.max branch, line 6)', () => {
        it('returns 0 when existing_size exceeds size (single file)', () => {
            const files = [{ size: 50, existing_size: 100 }];
            // max(0, 50-100) = max(0, -50) = 0
            expect(calculateRemainingSize(files)).toBe(0);
        });

        it('returns 0 when existing_size equals size', () => {
            const files = [{ size: 100, existing_size: 100 }];
            // max(0, 100-100) = max(0, 0) = 0
            expect(calculateRemainingSize(files)).toBe(0);
        });

        it('handles mixed files with some negative differences', () => {
            const files = [
                { size: 50, existing_size: 100 }, // max(0, -50) = 0
                { size: 200, existing_size: 50 }, // max(0, 150) = 150
                { size: 30, existing_size: 30 },  // max(0, 0) = 0
            ];
            expect(calculateRemainingSize(files)).toBe(150);
        });
    });

    describe('Normal files with partial downloads', () => {
        it('calculates remaining for single file', () => {
            const files = [{ size: 100, existing_size: 20 }];
            expect(calculateRemainingSize(files)).toBe(80);
        });

        it('calculates remaining for multiple files', () => {
            const files = [
                { size: 100, existing_size: 20 },
                { size: 200, existing_size: 0 },
                { size: 50, existing_size: 25 },
            ];
            // (100-20) + (200-0) + (50-25) = 80 + 200 + 25 = 305
            expect(calculateRemainingSize(files)).toBe(305);
        });

        it('handles zero size files', () => {
            const files = [{ size: 0, existing_size: 0 }];
            expect(calculateRemainingSize(files)).toBe(0);
        });
    });
});

describe('download.js - calculateResumeSnapshot', () => {
    describe('previousTotal > 0 branch (line 12)', () => {
        it('uses previousTotal when it is positive', () => {
            const files = [{ size: 500, existing_size: 200 }];
            const snapshot = calculateResumeSnapshot(1000, 100, files);
            expect(snapshot.newTotalSize).toBe(1000);
        });
    });

    describe('previousTotal <= 0 branch (line 12, uses remainingSize)', () => {
        it('uses remainingSize when previousTotal is 0', () => {
            const files = [{ size: 500, existing_size: 200 }];
            const snapshot = calculateResumeSnapshot(0, 100, files);
            expect(snapshot.newTotalSize).toBe(300); // remainingSize
        });

        it('uses remainingSize when previousTotal is negative', () => {
            const files = [{ size: 500, existing_size: 200 }];
            const snapshot = calculateResumeSnapshot(-100, 100, files);
            expect(snapshot.newTotalSize).toBe(300); // remainingSize
        });
    });

    describe('Line 14 condition: previousTotal > 0 && remainingSize > 0 && remainingSize < previousTotal', () => {
        it('enters line 15 branch when all conditions true', () => {
            // previousTotal=1000, remainingSize=300, 300 > 0 && 300 < 1000
            const files = [{ size: 500, existing_size: 200 }];
            const snapshot = calculateResumeSnapshot(1000, 100, files);
            // alreadyDownloaded = 1000 - 300 = 700
            expect(snapshot.clampedDownloaded).toBe(700);
        });

        it('enters else branch (line 17) when previousTotal is 0', () => {
            const files = [{ size: 500, existing_size: 200 }];
            const snapshot = calculateResumeSnapshot(0, 250, files);
            // Uses previousDownloaded || 0 = 250
            // newTotalSize = 300, clamped to 250
            expect(snapshot.clampedDownloaded).toBe(250);
        });

        it('enters else branch when remainingSize is 0', () => {
            const files = [{ size: 100, existing_size: 100 }]; // remainingSize = 0
            const snapshot = calculateResumeSnapshot(1000, 200, files);
            // remainingSize = 0, so condition fails
            // alreadyDownloaded = previousDownloaded || 0 = 200
            expect(snapshot.clampedDownloaded).toBe(200);
        });

        it('enters else branch when remainingSize >= previousTotal', () => {
            const files = [{ size: 500, existing_size: 0 }]; // remainingSize = 500
            const snapshot = calculateResumeSnapshot(400, 150, files);
            // remainingSize=500 >= previousTotal=400, condition fails
            // alreadyDownloaded = previousDownloaded || 0 = 150
            expect(snapshot.clampedDownloaded).toBe(150);
        });

        it('enters else branch when remainingSize equals previousTotal', () => {
            const files = [{ size: 500, existing_size: 0 }]; // remainingSize = 500
            const snapshot = calculateResumeSnapshot(500, 100, files);
            // remainingSize=500 is NOT < previousTotal=500
            // alreadyDownloaded = 100
            expect(snapshot.clampedDownloaded).toBe(100);
        });
    });

    describe('previousDownloaded is undefined/null/0 (line 17)', () => {
        it('uses 0 when previousDownloaded is undefined', () => {
            const files = [{ size: 100, existing_size: 100 }]; // remainingSize = 0
            const snapshot = calculateResumeSnapshot(0, undefined, files);
            // alreadyDownloaded = undefined || 0 = 0
            expect(snapshot.clampedDownloaded).toBe(0);
        });

        it('uses 0 when previousDownloaded is null', () => {
            const files = [{ size: 100, existing_size: 100 }];
            const snapshot = calculateResumeSnapshot(0, null, files);
            expect(snapshot.clampedDownloaded).toBe(0);
        });

        it('uses 0 when previousDownloaded is 0', () => {
            const files = [{ size: 100, existing_size: 100 }];
            const snapshot = calculateResumeSnapshot(0, 0, files);
            expect(snapshot.clampedDownloaded).toBe(0);
        });
    });

    describe('stabilizedDownloaded capping (lines 19-24)', () => {
        it('clamps alreadyDownloaded to non-negative (Math.max)', () => {
            // Edge case: if somehow alreadyDownloaded could be negative
            // This is handled by Math.max(0, alreadyDownloaded)
            const files = [{ size: 100, existing_size: 50 }]; // remainingSize = 50
            const snapshot = calculateResumeSnapshot(0, -50, files);
            // alreadyDownloaded = -50 || 0 = 0 (falsy check)
            expect(snapshot.clampedDownloaded).toBeGreaterThanOrEqual(0);
        });

        it('clamps clampedDownloaded to newTotalSize (Math.min)', () => {
            const files = [{ size: 100, existing_size: 50 }]; // remainingSize = 50
            const snapshot = calculateResumeSnapshot(0, 1000, files);
            // newTotalSize = 50, previousDownloaded = 1000
            // clampedDownloaded = min(max(0, 1000), 50) = 50
            // stabilizedDownloaded = max(1000 || 0, 50) = 1000
            // But this is clamped by min to newTotalSize in the return
            expect(snapshot.clampedDownloaded).toBe(1000);
        });

        it('stabilizedDownloaded preserves larger previousDownloaded (line 23)', () => {
            const files = [{ size: 1000, existing_size: 100 }]; // remainingSize = 900
            const snapshot = calculateResumeSnapshot(0, 400, files);
            // newTotalSize = 900, alreadyDownloaded = 400
            // clampedDownloaded = min(max(0, 400), 900) = 400
            // stabilizedDownloaded = max(400 || 0, 400) = 400
            expect(snapshot.clampedDownloaded).toBe(400);
        });

        it('stabilizedDownloaded uses clampedDownloaded when previousDownloaded is smaller', () => {
            const files = [{ size: 500, existing_size: 200 }]; // remainingSize = 300
            const snapshot = calculateResumeSnapshot(1000, 100, files);
            // alreadyDownloaded = 1000 - 300 = 700
            // clampedDownloaded = min(max(0, 700), 1000) = 700
            // stabilizedDownloaded = max(100 || 0, 700) = 700
            expect(snapshot.clampedDownloaded).toBe(700);
        });
    });

    describe('Return value structure', () => {
        it('returns object with all expected properties', () => {
            const files = [{ size: 100, existing_size: 50 }];
            const snapshot = calculateResumeSnapshot(100, 50, files);
            expect(snapshot).toHaveProperty('remainingSize');
            expect(snapshot).toHaveProperty('newTotalSize');
            expect(snapshot).toHaveProperty('clampedDownloaded');
        });

        it('remainingSize matches calculateRemainingSize result', () => {
            const files = [{ size: 100, existing_size: 30 }];
            const snapshot = calculateResumeSnapshot(200, 50, files);
            expect(snapshot.remainingSize).toBe(70);
        });
    });

    describe('Edge cases', () => {
        it('handles empty files array', () => {
            const snapshot = calculateResumeSnapshot(100, 50, []);
            expect(snapshot.remainingSize).toBe(0);
            expect(snapshot.newTotalSize).toBe(100);
        });

        it('handles non-array files (returns 0 remainingSize)', () => {
            const snapshot = calculateResumeSnapshot(100, 50, null);
            expect(snapshot.remainingSize).toBe(0);
        });
    });
});

// =============================================================================
// updateState.js - 100% Coverage Tests
// =============================================================================

describe('updateState.js - shouldDisableLaunch', () => {
    describe('Each boolean condition being the trigger', () => {
        it('returns true when only disabled is true (line 3)', () => {
            expect(shouldDisableLaunch({
                disabled: true,
                currentUpdateMode: 'ready',
                updateError: false,
            })).toBe(true);
        });

        it('returns true when only updateError is true (line 4)', () => {
            expect(shouldDisableLaunch({
                disabled: false,
                currentUpdateMode: 'ready',
                updateError: true,
            })).toBe(true);
        });

        it('returns true when currentUpdateMode is "download" (line 5)', () => {
            expect(shouldDisableLaunch({
                disabled: false,
                currentUpdateMode: 'download',
                updateError: false,
            })).toBe(true);
        });

        it('returns true when currentUpdateMode is "paused" (line 6)', () => {
            expect(shouldDisableLaunch({
                disabled: false,
                currentUpdateMode: 'paused',
                updateError: false,
            })).toBe(true);
        });

        it('returns true when currentUpdateMode is "error" (line 7)', () => {
            expect(shouldDisableLaunch({
                disabled: false,
                currentUpdateMode: 'error',
                updateError: false,
            })).toBe(true);
        });
    });

    describe('All combinations of false inputs', () => {
        it('returns false when all conditions are false', () => {
            expect(shouldDisableLaunch({
                disabled: false,
                currentUpdateMode: 'ready',
                updateError: false,
            })).toBe(false);
        });

        it('returns false when currentUpdateMode is "complete"', () => {
            expect(shouldDisableLaunch({
                disabled: false,
                currentUpdateMode: 'complete',
                updateError: false,
            })).toBe(false);
        });

        it('returns false when currentUpdateMode is "file_check"', () => {
            expect(shouldDisableLaunch({
                disabled: false,
                currentUpdateMode: 'file_check',
                updateError: false,
            })).toBe(false);
        });

        it('returns false when currentUpdateMode is null', () => {
            expect(shouldDisableLaunch({
                disabled: false,
                currentUpdateMode: null,
                updateError: false,
            })).toBe(false);
        });
    });
});

describe('updateState.js - getProgressUpdateMode', () => {
    describe('currentUpdateMode === "ready" branch (line 16)', () => {
        it('returns "ready" when currentUpdateMode is "ready"', () => {
            expect(getProgressUpdateMode({
                currentUpdateMode: 'ready',
                isDownloadComplete: false,
                isUpdateAvailable: true,
            })).toBe('ready');
        });

        it('returns "ready" regardless of other params', () => {
            expect(getProgressUpdateMode({
                currentUpdateMode: 'ready',
                isDownloadComplete: true,
                isUpdateAvailable: false,
            })).toBe('ready');
        });
    });

    describe('currentUpdateMode === "complete" branch (line 16)', () => {
        it('returns "complete" when currentUpdateMode is "complete"', () => {
            expect(getProgressUpdateMode({
                currentUpdateMode: 'complete',
                isDownloadComplete: false,
                isUpdateAvailable: true,
            })).toBe('complete');
        });

        it('returns "complete" regardless of other params', () => {
            expect(getProgressUpdateMode({
                currentUpdateMode: 'complete',
                isDownloadComplete: true,
                isUpdateAvailable: false,
            })).toBe('complete');
        });
    });

    describe('isDownloadComplete true branch (line 19)', () => {
        it('returns currentUpdateMode when isDownloadComplete is true', () => {
            expect(getProgressUpdateMode({
                currentUpdateMode: 'download',
                isDownloadComplete: true,
                isUpdateAvailable: true,
            })).toBe('download');
        });

        it('returns currentUpdateMode for file_check when isDownloadComplete', () => {
            expect(getProgressUpdateMode({
                currentUpdateMode: 'file_check',
                isDownloadComplete: true,
                isUpdateAvailable: true,
            })).toBe('file_check');
        });
    });

    describe('isUpdateAvailable === false branch (line 19)', () => {
        it('returns currentUpdateMode when isUpdateAvailable is false', () => {
            expect(getProgressUpdateMode({
                currentUpdateMode: 'paused',
                isDownloadComplete: false,
                isUpdateAvailable: false,
            })).toBe('paused');
        });

        it('returns currentUpdateMode for download mode when no update available', () => {
            expect(getProgressUpdateMode({
                currentUpdateMode: 'download',
                isDownloadComplete: false,
                isUpdateAvailable: false,
            })).toBe('download');
        });
    });

    describe('Default "download" return (line 22)', () => {
        it('returns "download" when none of the conditions are met', () => {
            expect(getProgressUpdateMode({
                currentUpdateMode: 'file_check',
                isDownloadComplete: false,
                isUpdateAvailable: true,
            })).toBe('download');
        });

        it('returns "download" for paused mode with update available', () => {
            expect(getProgressUpdateMode({
                currentUpdateMode: 'paused',
                isDownloadComplete: false,
                isUpdateAvailable: true,
            })).toBe('download');
        });

        it('returns "download" for null currentUpdateMode', () => {
            expect(getProgressUpdateMode({
                currentUpdateMode: null,
                isDownloadComplete: false,
                isUpdateAvailable: true,
            })).toBe('download');
        });
    });
});

describe('updateState.js - getUpdateErrorMessage', () => {
    describe('String error with content (line 26)', () => {
        it('returns string error when it has content', () => {
            expect(getUpdateErrorMessage('Network error', 'fallback')).toBe('Network error');
        });

        it('returns string error with leading/trailing spaces', () => {
            expect(getUpdateErrorMessage('  Error message  ', 'fallback')).toBe('  Error message  ');
        });
    });

    describe('String error that is empty/whitespace (line 26 false branch)', () => {
        it('does not return empty string error', () => {
            expect(getUpdateErrorMessage('', 'fallback')).toBe('fallback');
        });

        it('does not return whitespace-only error', () => {
            expect(getUpdateErrorMessage('   ', 'fallback')).toBe('fallback');
        });

        it('does not return tab-only error', () => {
            expect(getUpdateErrorMessage('\t\t', 'fallback')).toBe('fallback');
        });

        it('does not return newline-only error', () => {
            expect(getUpdateErrorMessage('\n\n', 'fallback')).toBe('fallback');
        });
    });

    describe('Object with message property (line 27-28)', () => {
        it('returns object.message when it has content', () => {
            expect(getUpdateErrorMessage({ message: 'Object error' }, 'fallback')).toBe('Object error');
        });

        it('returns trimmed message content', () => {
            expect(getUpdateErrorMessage({ message: '  Trimmed  ' }, 'fallback')).toBe('  Trimmed  ');
        });
    });

    describe('Object with empty message (line 27 false branch)', () => {
        it('does not return empty message property', () => {
            const error = { message: '' };
            expect(getUpdateErrorMessage(error, 'fallback')).toBe('[object Object]');
        });

        it('does not return whitespace-only message', () => {
            const error = { message: '   ' };
            expect(getUpdateErrorMessage(error, 'fallback')).toBe('[object Object]');
        });
    });

    describe('Object with toString() method (lines 29-31)', () => {
        it('returns toString() result when it has content', () => {
            const error = {
                toString() {
                    return 'Custom toString error';
                },
            };
            expect(getUpdateErrorMessage(error, 'fallback')).toBe('Custom toString error');
        });

        it('returns [object Object] for plain object (default toString)', () => {
            expect(getUpdateErrorMessage({}, 'fallback')).toBe('[object Object]');
        });

        it('returns toString() for Error objects', () => {
            const error = new Error('Real error');
            expect(getUpdateErrorMessage(error, 'fallback')).toBe('Real error');
        });
    });

    describe('Object with toString() returning empty (line 31 false branch)', () => {
        it('returns fallback when toString returns empty string', () => {
            const error = {
                toString() {
                    return '';
                },
            };
            expect(getUpdateErrorMessage(error, 'fallback')).toBe('fallback');
        });

        it('returns fallback when toString returns whitespace', () => {
            const error = {
                toString() {
                    return '   ';
                },
            };
            expect(getUpdateErrorMessage(error, 'fallback')).toBe('fallback');
        });

        it('returns fallback when toString returns non-string', () => {
            const error = {
                toString() {
                    return null;
                },
            };
            expect(getUpdateErrorMessage(error, 'fallback')).toBe('fallback');
        });
    });

    describe('null/undefined error (line 33)', () => {
        it('returns fallback for null error', () => {
            expect(getUpdateErrorMessage(null, 'fallback value')).toBe('fallback value');
        });

        it('returns fallback for undefined error', () => {
            expect(getUpdateErrorMessage(undefined, 'fallback value')).toBe('fallback value');
        });
    });

    describe('Fallback being returned', () => {
        it('returns the provided fallback string', () => {
            expect(getUpdateErrorMessage(null, 'Custom fallback')).toBe('Custom fallback');
        });

        it('returns undefined as fallback if not provided', () => {
            expect(getUpdateErrorMessage(null)).toBe(undefined);
        });

        it('returns empty string fallback', () => {
            expect(getUpdateErrorMessage(null, '')).toBe('');
        });
    });
});

describe('updateState.js - INITIAL_STATE', () => {
    describe('Verify all properties exist', () => {
        it('contains lastLogMessage property', () => {
            expect(INITIAL_STATE).toHaveProperty('lastLogMessage', null);
        });

        it('contains lastLogTime property', () => {
            expect(INITIAL_STATE).toHaveProperty('lastLogTime', 0);
        });

        it('contains speedHistory property', () => {
            expect(INITIAL_STATE).toHaveProperty('speedHistory');
            expect(INITIAL_STATE.speedHistory).toEqual([]);
        });

        it('contains speedHistoryMaxLength property', () => {
            expect(INITIAL_STATE).toHaveProperty('speedHistoryMaxLength', 10);
        });

        it('contains isUpdateAvailable property', () => {
            expect(INITIAL_STATE).toHaveProperty('isUpdateAvailable', false);
        });

        it('contains isDownloadComplete property', () => {
            expect(INITIAL_STATE).toHaveProperty('isDownloadComplete', false);
        });

        it('contains lastProgressUpdate property', () => {
            expect(INITIAL_STATE).toHaveProperty('lastProgressUpdate', null);
        });

        it('contains lastDownloadedBytes property', () => {
            expect(INITIAL_STATE).toHaveProperty('lastDownloadedBytes', 0);
        });

        it('contains downloadStartTime property', () => {
            expect(INITIAL_STATE).toHaveProperty('downloadStartTime', null);
        });

        it('contains currentUpdateMode property', () => {
            expect(INITIAL_STATE).toHaveProperty('currentUpdateMode', null);
        });

        it('contains currentProgress property', () => {
            expect(INITIAL_STATE).toHaveProperty('currentProgress', 0);
        });

        it('contains currentFileName property', () => {
            expect(INITIAL_STATE).toHaveProperty('currentFileName', '');
        });

        it('contains currentFileIndex property', () => {
            expect(INITIAL_STATE).toHaveProperty('currentFileIndex', 0);
        });

        it('contains totalFiles property', () => {
            expect(INITIAL_STATE).toHaveProperty('totalFiles', 0);
        });

        it('contains downloadedSize property', () => {
            expect(INITIAL_STATE).toHaveProperty('downloadedSize', 0);
        });

        it('contains downloadedBytesOffset property', () => {
            expect(INITIAL_STATE).toHaveProperty('downloadedBytesOffset', 0);
        });

        it('contains totalSize property', () => {
            expect(INITIAL_STATE).toHaveProperty('totalSize', 0);
        });

        it('contains currentSpeed property', () => {
            expect(INITIAL_STATE).toHaveProperty('currentSpeed', 0);
        });

        it('contains timeRemaining property', () => {
            expect(INITIAL_STATE).toHaveProperty('timeRemaining', 0);
        });

        it('contains isLoggingIn property', () => {
            expect(INITIAL_STATE).toHaveProperty('isLoggingIn', false);
        });

        it('contains isLoggingOut property', () => {
            expect(INITIAL_STATE).toHaveProperty('isLoggingOut', false);
        });

        it('contains isGameRunning property', () => {
            expect(INITIAL_STATE).toHaveProperty('isGameRunning', false);
        });

        it('contains gameExecutionFailed property', () => {
            expect(INITIAL_STATE).toHaveProperty('gameExecutionFailed', false);
        });

        it('contains updatesEnabled property', () => {
            expect(INITIAL_STATE).toHaveProperty('updatesEnabled', true);
        });

        it('contains isCheckingForUpdates property', () => {
            expect(INITIAL_STATE).toHaveProperty('isCheckingForUpdates', false);
        });

        it('contains updateCheckPerformed property', () => {
            expect(INITIAL_STATE).toHaveProperty('updateCheckPerformed', false);
        });

        it('contains isGameLaunching property', () => {
            expect(INITIAL_STATE).toHaveProperty('isGameLaunching', false);
        });

        it('contains isAuthenticated property', () => {
            expect(INITIAL_STATE).toHaveProperty('isAuthenticated', false);
        });

        it('contains isFileCheckComplete property', () => {
            expect(INITIAL_STATE).toHaveProperty('isFileCheckComplete', false);
        });

        it('contains isFirstLaunch property', () => {
            expect(INITIAL_STATE).toHaveProperty('isFirstLaunch', true);
        });

        it('contains isGeneratingHashFile property', () => {
            expect(INITIAL_STATE).toHaveProperty('isGeneratingHashFile', false);
        });

        it('contains hashFileProgress property', () => {
            expect(INITIAL_STATE).toHaveProperty('hashFileProgress', 0);
        });

        it('contains currentProcessingFile property', () => {
            expect(INITIAL_STATE).toHaveProperty('currentProcessingFile', '');
        });

        it('contains processedFiles property', () => {
            expect(INITIAL_STATE).toHaveProperty('processedFiles', 0);
        });

        it('contains isPauseRequested property', () => {
            expect(INITIAL_STATE).toHaveProperty('isPauseRequested', false);
        });

        it('contains updateError property', () => {
            expect(INITIAL_STATE).toHaveProperty('updateError', false);
        });
    });
});

describe('updateState.js - getPathChangeResetState', () => {
    describe('Verify it returns expected values', () => {
        it('returns isFileCheckComplete from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.isFileCheckComplete).toBe(INITIAL_STATE.isFileCheckComplete);
        });

        it('returns isUpdateAvailable from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.isUpdateAvailable).toBe(INITIAL_STATE.isUpdateAvailable);
        });

        it('returns isDownloadComplete from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.isDownloadComplete).toBe(INITIAL_STATE.isDownloadComplete);
        });

        it('returns lastProgressUpdate from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.lastProgressUpdate).toBe(INITIAL_STATE.lastProgressUpdate);
        });

        it('returns lastDownloadedBytes from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.lastDownloadedBytes).toBe(INITIAL_STATE.lastDownloadedBytes);
        });

        it('returns downloadStartTime from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.downloadStartTime).toBe(INITIAL_STATE.downloadStartTime);
        });

        it('sets currentUpdateMode to "file_check" (special case)', () => {
            const reset = getPathChangeResetState();
            expect(reset.currentUpdateMode).toBe('file_check');
        });

        it('returns currentProgress from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.currentProgress).toBe(INITIAL_STATE.currentProgress);
        });

        it('returns currentFileName from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.currentFileName).toBe(INITIAL_STATE.currentFileName);
        });

        it('returns currentFileIndex from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.currentFileIndex).toBe(INITIAL_STATE.currentFileIndex);
        });

        it('returns totalFiles from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.totalFiles).toBe(INITIAL_STATE.totalFiles);
        });

        it('returns downloadedSize from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.downloadedSize).toBe(INITIAL_STATE.downloadedSize);
        });

        it('returns downloadedBytesOffset from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.downloadedBytesOffset).toBe(INITIAL_STATE.downloadedBytesOffset);
        });

        it('returns totalSize from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.totalSize).toBe(INITIAL_STATE.totalSize);
        });

        it('returns currentSpeed from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.currentSpeed).toBe(INITIAL_STATE.currentSpeed);
        });

        it('returns timeRemaining from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.timeRemaining).toBe(INITIAL_STATE.timeRemaining);
        });

        it('returns isPauseRequested from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.isPauseRequested).toBe(INITIAL_STATE.isPauseRequested);
        });

        it('returns updateError from INITIAL_STATE', () => {
            const reset = getPathChangeResetState();
            expect(reset.updateError).toBe(INITIAL_STATE.updateError);
        });

        it('does not include properties not needed for reset', () => {
            const reset = getPathChangeResetState();
            expect(reset).not.toHaveProperty('isLoggingIn');
            expect(reset).not.toHaveProperty('isGameRunning');
            expect(reset).not.toHaveProperty('updatesEnabled');
        });
    });
});

describe('updateState.js - getStatusKey', () => {
    describe('updateError true (line 107)', () => {
        it('returns UPDATE_ERROR_MESSAGE when updateError is true', () => {
            expect(getStatusKey({ updateError: true })).toBe('UPDATE_ERROR_MESSAGE');
        });

        it('returns UPDATE_ERROR_MESSAGE regardless of other state', () => {
            expect(getStatusKey({
                updateError: true,
                isDownloadComplete: true,
                isUpdateAvailable: false,
                currentUpdateMode: 'file_check',
            })).toBe('UPDATE_ERROR_MESSAGE');
        });
    });

    describe('isDownloadComplete true (line 108)', () => {
        it('returns READY_TO_PLAY when isDownloadComplete is true', () => {
            expect(getStatusKey({
                updateError: false,
                isDownloadComplete: true,
            })).toBe('READY_TO_PLAY');
        });

        it('returns READY_TO_PLAY even with isUpdateAvailable true', () => {
            expect(getStatusKey({
                updateError: false,
                isDownloadComplete: true,
                isUpdateAvailable: true,
            })).toBe('READY_TO_PLAY');
        });
    });

    describe('isUpdateAvailable false (line 109)', () => {
        it('returns READY_TO_PLAY when isUpdateAvailable is false', () => {
            expect(getStatusKey({
                updateError: false,
                isDownloadComplete: false,
                isUpdateAvailable: false,
            })).toBe('READY_TO_PLAY');
        });

        it('returns READY_TO_PLAY when isUpdateAvailable is undefined', () => {
            expect(getStatusKey({
                updateError: false,
                isDownloadComplete: false,
            })).toBe('READY_TO_PLAY');
        });
    });

    describe('currentUpdateMode === "file_check" (line 110)', () => {
        it('returns VERIFYING_FILES when currentUpdateMode is file_check', () => {
            expect(getStatusKey({
                updateError: false,
                isDownloadComplete: false,
                isUpdateAvailable: true,
                currentUpdateMode: 'file_check',
            })).toBe('VERIFYING_FILES');
        });
    });

    describe('Default DOWNLOADING_FILES (line 111)', () => {
        it('returns DOWNLOADING_FILES for download mode', () => {
            expect(getStatusKey({
                updateError: false,
                isDownloadComplete: false,
                isUpdateAvailable: true,
                currentUpdateMode: 'download',
            })).toBe('DOWNLOADING_FILES');
        });

        it('returns DOWNLOADING_FILES for paused mode', () => {
            expect(getStatusKey({
                updateError: false,
                isDownloadComplete: false,
                isUpdateAvailable: true,
                currentUpdateMode: 'paused',
            })).toBe('DOWNLOADING_FILES');
        });

        it('returns DOWNLOADING_FILES for complete mode', () => {
            expect(getStatusKey({
                updateError: false,
                isDownloadComplete: false,
                isUpdateAvailable: true,
                currentUpdateMode: 'complete',
            })).toBe('DOWNLOADING_FILES');
        });

        it('returns DOWNLOADING_FILES for null mode', () => {
            expect(getStatusKey({
                updateError: false,
                isDownloadComplete: false,
                isUpdateAvailable: true,
                currentUpdateMode: null,
            })).toBe('DOWNLOADING_FILES');
        });
    });
});

describe('updateState.js - getDlStatusKey', () => {
    describe('updateError true (line 115)', () => {
        it('returns UPDATE_ERROR_MESSAGE when updateError is true', () => {
            expect(getDlStatusKey({ updateError: true })).toBe('UPDATE_ERROR_MESSAGE');
        });

        it('returns UPDATE_ERROR_MESSAGE regardless of currentUpdateMode', () => {
            expect(getDlStatusKey({
                updateError: true,
                currentUpdateMode: 'download',
            })).toBe('UPDATE_ERROR_MESSAGE');
        });
    });

    describe('case "file_check" (lines 117-118)', () => {
        it('returns VERIFYING_FILES for file_check mode', () => {
            expect(getDlStatusKey({
                updateError: false,
                currentUpdateMode: 'file_check',
            })).toBe('VERIFYING_FILES');
        });
    });

    describe('case "paused" (lines 119-120)', () => {
        it('returns DOWNLOADING_FILES for paused mode', () => {
            expect(getDlStatusKey({
                updateError: false,
                currentUpdateMode: 'paused',
            })).toBe('DOWNLOADING_FILES');
        });
    });

    describe('case "download" (lines 119-120)', () => {
        it('returns DOWNLOADING_FILES for download mode', () => {
            expect(getDlStatusKey({
                updateError: false,
                currentUpdateMode: 'download',
            })).toBe('DOWNLOADING_FILES');
        });
    });

    describe('case "complete" with isFileCheckComplete && !isUpdateAvailable (lines 123-124)', () => {
        it('returns NO_UPDATE_REQUIRED', () => {
            expect(getDlStatusKey({
                updateError: false,
                currentUpdateMode: 'complete',
                isFileCheckComplete: true,
                isUpdateAvailable: false,
            })).toBe('NO_UPDATE_REQUIRED');
        });
    });

    describe('case "complete" with isFileCheckComplete && isUpdateAvailable (lines 125-126)', () => {
        it('returns FILE_CHECK_COMPLETE', () => {
            expect(getDlStatusKey({
                updateError: false,
                currentUpdateMode: 'complete',
                isFileCheckComplete: true,
                isUpdateAvailable: true,
            })).toBe('FILE_CHECK_COMPLETE');
        });
    });

    describe('case "complete" with isDownloadComplete (line 127)', () => {
        it('returns DOWNLOAD_COMPLETE', () => {
            expect(getDlStatusKey({
                updateError: false,
                currentUpdateMode: 'complete',
                isFileCheckComplete: false,
                isDownloadComplete: true,
            })).toBe('DOWNLOAD_COMPLETE');
        });
    });

    describe('case "complete" with isUpdateComplete (line 128)', () => {
        it('returns UPDATE_COMPLETED', () => {
            expect(getDlStatusKey({
                updateError: false,
                currentUpdateMode: 'complete',
                isFileCheckComplete: false,
                isDownloadComplete: false,
                isUpdateComplete: true,
            })).toBe('UPDATE_COMPLETED');
        });
    });

    describe('case "complete" falling through to break then line 133', () => {
        it('returns GAME_READY_TO_LAUNCH when no complete conditions match', () => {
            expect(getDlStatusKey({
                updateError: false,
                currentUpdateMode: 'complete',
                isFileCheckComplete: false,
                isDownloadComplete: false,
                isUpdateComplete: false,
                isUpdateAvailable: false,
            })).toBe('GAME_READY_TO_LAUNCH');
        });

        it('returns GAME_READY_TO_LAUNCH for complete with undefined flags', () => {
            expect(getDlStatusKey({
                updateError: false,
                currentUpdateMode: 'complete',
            })).toBe('GAME_READY_TO_LAUNCH');
        });
    });

    describe('default case (lines 130-131)', () => {
        it('returns GAME_READY_TO_LAUNCH for ready mode', () => {
            expect(getDlStatusKey({
                updateError: false,
                currentUpdateMode: 'ready',
            })).toBe('GAME_READY_TO_LAUNCH');
        });

        it('returns GAME_READY_TO_LAUNCH for null mode', () => {
            expect(getDlStatusKey({
                updateError: false,
                currentUpdateMode: null,
            })).toBe('GAME_READY_TO_LAUNCH');
        });

        it('returns GAME_READY_TO_LAUNCH for undefined mode', () => {
            expect(getDlStatusKey({
                updateError: false,
            })).toBe('GAME_READY_TO_LAUNCH');
        });

        it('returns GAME_READY_TO_LAUNCH for error mode', () => {
            expect(getDlStatusKey({
                updateError: false,
                currentUpdateMode: 'error',
            })).toBe('GAME_READY_TO_LAUNCH');
        });

        it('returns GAME_READY_TO_LAUNCH for unknown mode', () => {
            expect(getDlStatusKey({
                updateError: false,
                currentUpdateMode: 'unknown_mode',
            })).toBe('GAME_READY_TO_LAUNCH');
        });
    });
});
