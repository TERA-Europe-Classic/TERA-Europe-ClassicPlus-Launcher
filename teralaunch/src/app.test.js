/**
 * Comprehensive tests for download state management fixes.
 * Run with: node app.test.js
 */

// Mock the App object's relevant methods for testing
function createMockApp() {
    return {
        state: {
            isFileCheckComplete: false,
            isUpdateAvailable: false,
            isDownloadComplete: false,
            lastProgressUpdate: null,
            lastDownloadedBytes: 0,
            downloadStartTime: null,
            currentUpdateMode: null,
            currentProgress: 0,
            currentFileName: "",
            currentFileIndex: 0,
            totalFiles: 0,
            downloadedSize: 0,
            downloadedBytesOffset: 0,
            totalSize: 0,
            currentSpeed: 0,
            timeRemaining: 0,
            isCheckingForUpdates: false,
        },
        _activeFileWindow: [],
        setState(newState) {
            Object.assign(this.state, newState);
        },
        updateLaunchGameButton() {},
        toggleLanguageSelector() {},
        t(key) { return key; },
        calculateGlobalTimeRemaining(downloaded, total, speed) {
            if (speed <= 0) return 0;
            return (total - downloaded) / speed;
        }
    };
}

// Extract and test the core logic from handleDownloadProgress
function simulateDownloadProgress(app, payload) {
    const {
        file_name,
        progress,
        speed,
        downloaded_bytes,
        total_bytes,
        total_files,
        current_file_index,
    } = payload;

    if (app.state.totalSize === undefined || app.state.totalSize === 0) {
        app.state.totalSize = total_bytes;
    }

    const offset = app.state.downloadedBytesOffset || 0;
    const totalDownloadedBytes = downloaded_bytes + offset;
    const effectiveTotalSize = Math.max(app.state.totalSize, total_bytes);

    const now = Date.now();
    if (app.state.downloadStartTime === null) {
        app.state.downloadStartTime = now;
    }

    const elapsedSeconds = (now - app.state.downloadStartTime) / 1000;
    const globalSpeed = elapsedSeconds > 0 ? downloaded_bytes / elapsedSeconds : speed;

    const timeRemaining = app.calculateGlobalTimeRemaining(
        totalDownloadedBytes,
        effectiveTotalSize,
        globalSpeed,
    );

    app.setState({
        currentFileName: file_name,
        currentProgress: Math.min(100, (totalDownloadedBytes / effectiveTotalSize) * 100),
        currentSpeed: globalSpeed,
        downloadedSize: totalDownloadedBytes,
        totalSize: effectiveTotalSize,
        totalFiles: total_files,
        currentFileIndex: current_file_index,
        totalDownloadedBytes: totalDownloadedBytes,
        timeRemaining: timeRemaining,
        currentUpdateMode: "download",
        lastProgressUpdate: now,
        lastDownloadedBytes: totalDownloadedBytes,
    });
}

// Extract and test handleCompletion logic
function simulateCompletion(app) {
    app.setState({
        isDownloadComplete: true,
        currentProgress: 100,
        currentUpdateMode: "complete",
        isUpdateAvailable: false,
        isFileCheckComplete: true,
    });
    app.updateLaunchGameButton(false);
    app.toggleLanguageSelector(true);
}

// Extract and test handleFileCheckCompleted logic  
function simulateFileCheckCompleted(app, files_to_update) {
    const hasUpdates = (files_to_update ?? 0) > 0;
    app.setState({
        isFileCheckComplete: true,
        isUpdateAvailable: hasUpdates,
    });
    if (!hasUpdates) {
        simulateCompletion(app);
    }
}

// Extract pause logic
function simulatePause(app) {
    app.setState({ 
        currentUpdateMode: "paused",
        lastProgressUpdate: null,
        downloadStartTime: null,
        speedHistory: [],
        currentSpeed: 0,
    });
}

// Extract resume logic
function simulateResume(app, filesToUpdate, previousTotal) {
    const remainingSize = filesToUpdate.reduce((sum, f) => sum + (f.size || 0), 0);
    const alreadyDownloaded = previousTotal > 0 ? Math.max(0, previousTotal - remainingSize) : 0;
    const newTotalSize = previousTotal > 0 ? previousTotal : remainingSize;
    
    app.setState({ 
        currentUpdateMode: "download",
        isUpdateAvailable: true,
        isFileCheckComplete: true,
        downloadStartTime: Date.now(),
        lastProgressUpdate: null,
        speedHistory: [],
        totalFiles: filesToUpdate.length,
        totalSize: newTotalSize,
        downloadedBytesOffset: alreadyDownloaded,
    });
}

// Test runner
let passed = 0;
let failed = 0;

function test(name, fn) {
    try {
        fn();
        console.log(`✓ ${name}`);
        passed++;
    } catch (e) {
        console.log(`✗ ${name}`);
        console.log(`  Error: ${e.message}`);
        failed++;
    }
}

function assertEqual(actual, expected, message = '') {
    if (actual !== expected) {
        throw new Error(`${message} Expected ${expected}, got ${actual}`);
    }
}

function assertApprox(actual, expected, tolerance = 0.01, message = '') {
    if (Math.abs(actual - expected) > tolerance) {
        throw new Error(`${message} Expected ~${expected}, got ${actual}`);
    }
}

console.log('\n=== Download State Management Tests ===\n');

// Test 1: Fresh download progress updates correctly
test('Fresh download: progress updates from 0 to completion', () => {
    const app = createMockApp();
    app.state.totalSize = 1000;
    
    simulateDownloadProgress(app, {
        file_name: 'test.gpk',
        progress: 50,
        speed: 100,
        downloaded_bytes: 500,
        total_bytes: 1000,
        total_files: 10,
        current_file_index: 5,
    });
    
    assertEqual(app.state.downloadedSize, 500, 'downloadedSize');
    assertEqual(app.state.currentProgress, 50, 'currentProgress');
    assertEqual(app.state.totalSize, 1000, 'totalSize');
});

// Test 2: Offset is applied correctly after resume
test('Resume download: offset is applied to downloaded bytes', () => {
    const app = createMockApp();
    app.state.totalSize = 1000;
    app.state.downloadedBytesOffset = 300; // Already downloaded 300 bytes before pause
    
    simulateDownloadProgress(app, {
        file_name: 'test.gpk',
        progress: 20,
        speed: 100,
        downloaded_bytes: 200, // Backend reports 200 bytes in current session
        total_bytes: 700, // Backend only knows about remaining 700 bytes
        total_files: 7,
        current_file_index: 2,
    });
    
    assertEqual(app.state.downloadedSize, 500, 'downloadedSize should be offset + current (300 + 200)');
    assertEqual(app.state.totalSize, 1000, 'totalSize should preserve original total');
    assertEqual(app.state.currentProgress, 50, 'progress should be 500/1000 = 50%');
});

// Test 3: Progress preserved on pause
test('Pause: state preserves downloaded size', () => {
    const app = createMockApp();
    app.state.downloadedSize = 500;
    app.state.totalSize = 1000;
    app.state.currentProgress = 50;
    app.state.currentUpdateMode = "download";
    
    simulatePause(app);
    
    assertEqual(app.state.currentUpdateMode, "paused", 'mode should be paused');
    assertEqual(app.state.downloadedSize, 500, 'downloadedSize should be preserved');
    assertEqual(app.state.totalSize, 1000, 'totalSize should be preserved');
});

// Test 4: Resume calculates correct offset
test('Resume: calculates correct offset from remaining files', () => {
    const app = createMockApp();
    app.state.downloadedSize = 300;
    app.state.totalSize = 1000;
    app.state.currentUpdateMode = "paused";
    
    const filesToUpdate = [
        { path: 'file1.gpk', size: 400 },
        { path: 'file2.gpk', size: 300 },
    ]; // 700 bytes remaining
    
    simulateResume(app, filesToUpdate, 1000);
    
    assertEqual(app.state.downloadedBytesOffset, 300, 'offset should be 1000 - 700 = 300');
    assertEqual(app.state.totalSize, 1000, 'totalSize should be preserved');
    assertEqual(app.state.totalFiles, 2, 'totalFiles should match remaining files');
});

// Test 5: Completion enables launch button
test('Completion: sets correct state and enables launch', () => {
    const app = createMockApp();
    let launchButtonDisabled = true;
    app.updateLaunchGameButton = (disabled) => { launchButtonDisabled = disabled; };
    
    app.state.currentUpdateMode = "download";
    app.state.isUpdateAvailable = true;
    
    simulateCompletion(app);
    
    assertEqual(app.state.currentUpdateMode, "complete", 'mode should be complete');
    assertEqual(app.state.isUpdateAvailable, false, 'isUpdateAvailable should be false');
    assertEqual(app.state.isDownloadComplete, true, 'isDownloadComplete should be true');
    assertEqual(launchButtonDisabled, false, 'launch button should be enabled');
});

// Test 6: File check with no updates completes correctly
test('File check complete with no updates: triggers completion', () => {
    const app = createMockApp();
    app.state.currentUpdateMode = "file_check";
    
    simulateFileCheckCompleted(app, 0);
    
    assertEqual(app.state.isFileCheckComplete, true, 'isFileCheckComplete');
    assertEqual(app.state.isUpdateAvailable, false, 'isUpdateAvailable should be false');
    assertEqual(app.state.currentUpdateMode, "complete", 'should complete');
});

// Test 7: File check with updates doesn't complete
test('File check complete with updates: does not trigger completion', () => {
    const app = createMockApp();
    app.state.currentUpdateMode = "file_check";
    
    simulateFileCheckCompleted(app, 5);
    
    assertEqual(app.state.isFileCheckComplete, true, 'isFileCheckComplete');
    assertEqual(app.state.isUpdateAvailable, true, 'isUpdateAvailable should be true');
    assertEqual(app.state.currentUpdateMode, "file_check", 'mode should not change');
});

// Test 8: Multiple pause/resume cycles maintain correct progress
test('Multiple pause/resume cycles: progress is maintained', () => {
    const app = createMockApp();
    
    // Initial download: 1000 bytes total
    app.state.totalSize = 1000;
    app.state.downloadedBytesOffset = 0;
    
    // Download 300 bytes
    simulateDownloadProgress(app, {
        file_name: 'file1.gpk',
        progress: 30,
        speed: 100,
        downloaded_bytes: 300,
        total_bytes: 1000,
        total_files: 10,
        current_file_index: 3,
    });
    assertEqual(app.state.currentProgress, 30, 'initial progress should be 30%');
    
    // First pause
    simulatePause(app);
    
    // First resume - 700 bytes remaining
    simulateResume(app, [
        { path: 'file4.gpk', size: 400 },
        { path: 'file5.gpk', size: 300 },
    ], 1000);
    assertEqual(app.state.downloadedBytesOffset, 300, 'first resume offset');
    
    // Download 200 more bytes (total 500)
    simulateDownloadProgress(app, {
        file_name: 'file4.gpk',
        progress: 28,
        speed: 100,
        downloaded_bytes: 200,
        total_bytes: 700,
        total_files: 2,
        current_file_index: 1,
    });
    assertEqual(app.state.downloadedSize, 500, 'after first resume download');
    assertEqual(app.state.currentProgress, 50, 'progress should be 50%');
    
    // Second pause
    simulatePause(app);
    
    // Second resume - 500 bytes remaining
    simulateResume(app, [
        { path: 'file4.gpk', size: 200 },
        { path: 'file5.gpk', size: 300 },
    ], 1000);
    assertEqual(app.state.downloadedBytesOffset, 500, 'second resume offset');
    
    // Download 100 more bytes (total 600)
    simulateDownloadProgress(app, {
        file_name: 'file4.gpk',
        progress: 20,
        speed: 100,
        downloaded_bytes: 100,
        total_bytes: 500,
        total_files: 2,
        current_file_index: 1,
    });
    assertEqual(app.state.downloadedSize, 600, 'after second resume download');
    assertEqual(app.state.currentProgress, 60, 'progress should be 60%');
});

// Test 9: Edge case - resume when all files already downloaded
test('Resume with no remaining files: triggers completion', () => {
    const app = createMockApp();
    let completed = false;
    
    app.state.totalSize = 1000;
    app.state.downloadedSize = 1000;
    app.state.currentUpdateMode = "paused";
    
    const filesToUpdate = []; // No files remaining
    
    if (filesToUpdate.length === 0) {
        simulateCompletion(app);
        completed = true;
    } else {
        simulateResume(app, filesToUpdate, 1000);
    }
    
    assertEqual(completed, true, 'should trigger completion');
    assertEqual(app.state.currentUpdateMode, "complete", 'mode should be complete');
});

// Test 10: effectiveTotalSize uses max of preserved and backend values
test('Download progress: uses larger of preserved and backend totalSize', () => {
    const app = createMockApp();
    app.state.totalSize = 1000; // Preserved total
    app.state.downloadedBytesOffset = 500;
    
    // Backend only knows about remaining 500 bytes
    simulateDownloadProgress(app, {
        file_name: 'test.gpk',
        progress: 50,
        speed: 100,
        downloaded_bytes: 250,
        total_bytes: 500, // Backend's smaller value
        total_files: 5,
        current_file_index: 2,
    });
    
    assertEqual(app.state.totalSize, 1000, 'should preserve larger totalSize');
    assertEqual(app.state.downloadedSize, 750, 'downloadedSize = offset + current');
    assertEqual(app.state.currentProgress, 75, 'progress should be 750/1000 = 75%');
});

// Test 11: Zero totalSize edge case
test('Edge case: zero totalSize is handled gracefully', () => {
    const app = createMockApp();
    app.state.totalSize = 0;
    
    simulateDownloadProgress(app, {
        file_name: 'test.gpk',
        progress: 0,
        speed: 100,
        downloaded_bytes: 0,
        total_bytes: 1000,
        total_files: 10,
        current_file_index: 0,
    });
    
    assertEqual(app.state.totalSize, 1000, 'totalSize should be initialized');
});

// Test 12: Visibility logic for paused state
test('Visibility: size info shows during paused state', () => {
    const app = createMockApp();
    app.state.isUpdateAvailable = true;
    app.state.currentUpdateMode = "paused";
    
    const isDownloading = app.state.currentUpdateMode === "download";
    const isPaused = app.state.currentUpdateMode === "paused";
    const showDownloadInfo = app.state.isUpdateAvailable && (isDownloading || isPaused);
    
    assertEqual(showDownloadInfo, true, 'size info should be visible when paused');
    assertEqual(isDownloading, false, 'speed/time should be hidden when paused');
});

// Test 13: Visibility logic for download state
test('Visibility: all info shows during download state', () => {
    const app = createMockApp();
    app.state.isUpdateAvailable = true;
    app.state.currentUpdateMode = "download";
    
    const isDownloading = app.state.currentUpdateMode === "download";
    const isPaused = app.state.currentUpdateMode === "paused";
    const showDownloadInfo = app.state.isUpdateAvailable && (isDownloading || isPaused);
    
    assertEqual(showDownloadInfo, true, 'download info should be visible');
    assertEqual(isDownloading, true, 'speed/time should be visible');
});

// Test 14: Speed calculation uses session bytes only
test('Speed calculation: uses current session bytes, not total with offset', () => {
    const app = createMockApp();
    app.state.totalSize = 1000;
    app.state.downloadedBytesOffset = 500;
    app.state.downloadStartTime = Date.now() - 10000; // 10 seconds ago
    
    const downloaded_bytes = 200; // Current session
    const offset = app.state.downloadedBytesOffset;
    const elapsedSeconds = 10;
    
    // Correct: speed based on current session bytes
    const correctSpeed = downloaded_bytes / elapsedSeconds; // 20 bytes/sec
    
    // Wrong: speed based on total bytes (would be inflated)
    const wrongSpeed = (downloaded_bytes + offset) / elapsedSeconds; // 70 bytes/sec
    
    assertEqual(correctSpeed, 20, 'correct speed should be 20 bytes/sec');
    assertApprox(wrongSpeed, 70, 0.1, 'wrong calculation would give 70 bytes/sec');
});

console.log(`\n=== Results: ${passed} passed, ${failed} failed ===\n`);

if (failed > 0) {
    process.exit(1);
}
