export function shouldDisableLaunch({ disabled, currentUpdateMode, updateError }) {
  return (
    disabled ||
    updateError ||
    currentUpdateMode === "download" ||
    currentUpdateMode === "paused" ||
    currentUpdateMode === "error"
  );
}

export function getProgressUpdateMode({
  currentUpdateMode,
  isDownloadComplete,
  isUpdateAvailable,
}) {
  if (currentUpdateMode === "ready" || currentUpdateMode === "complete") {
    return currentUpdateMode;
  }
  if (isDownloadComplete || isUpdateAvailable === false) {
    return currentUpdateMode;
  }
  return "download";
}

export function getUpdateErrorMessage(error, fallback) {
  if (typeof error === "string" && error.trim()) return error;
  if (error && typeof error.message === "string" && error.message.trim())
    return error.message;
  if (error && typeof error.toString === "function") {
    const text = error.toString();
    if (typeof text === "string" && text.trim()) return text;
  }
  return fallback;
}

/**
 * Default initial state values for the application.
 * Single source of truth for state initialization.
 */
export const INITIAL_STATE = {
  lastLogMessage: null,
  lastLogTime: 0,
  speedHistory: [],
  speedHistoryMaxLength: 10,
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
  isLoggingIn: false,
  isLoggingOut: false,
  isGameRunning: false,
  gameExecutionFailed: false,
  updatesEnabled: true,
  isCheckingForUpdates: false,
  updateCheckPerformed: false,
  isGameLaunching: false,
  isAuthenticated: false,
  isFileCheckComplete: false,
  isFirstLaunch: true,
  isGeneratingHashFile: false,
  hashFileProgress: 0,
  currentProcessingFile: "",
  processedFiles: 0,
  isPauseRequested: false,
  updateError: false,
};

/**
 * Returns state values to reset when game path changes.
 * Uses INITIAL_STATE as the source of truth.
 */
export function getPathChangeResetState() {
  return {
    isFileCheckComplete: INITIAL_STATE.isFileCheckComplete,
    isUpdateAvailable: INITIAL_STATE.isUpdateAvailable,
    isDownloadComplete: INITIAL_STATE.isDownloadComplete,
    lastProgressUpdate: INITIAL_STATE.lastProgressUpdate,
    lastDownloadedBytes: INITIAL_STATE.lastDownloadedBytes,
    downloadStartTime: INITIAL_STATE.downloadStartTime,
    currentUpdateMode: "file_check", // Special case: triggers file check
    currentProgress: INITIAL_STATE.currentProgress,
    currentFileName: INITIAL_STATE.currentFileName,
    currentFileIndex: INITIAL_STATE.currentFileIndex,
    totalFiles: INITIAL_STATE.totalFiles,
    downloadedSize: INITIAL_STATE.downloadedSize,
    downloadedBytesOffset: INITIAL_STATE.downloadedBytesOffset,
    totalSize: INITIAL_STATE.totalSize,
    currentSpeed: INITIAL_STATE.currentSpeed,
    timeRemaining: INITIAL_STATE.timeRemaining,
    isPauseRequested: INITIAL_STATE.isPauseRequested,
    updateError: INITIAL_STATE.updateError,
  };
}

export function getStatusKey(state) {
  if (state.updateError) return "UPDATE_ERROR_MESSAGE";
  if (state.isDownloadComplete) return "DOWNLOAD_COMPLETE";
  if (!state.isUpdateAvailable) return "NO_UPDATE_REQUIRED";
  if (state.currentUpdateMode === "file_check") return "VERIFYING_FILES";
  return "DOWNLOADING_FILES";
}

export function getDlStatusKey(state) {
  if (state.updateError) return "UPDATE_ERROR_MESSAGE";
  switch (state.currentUpdateMode) {
    case "file_check":
      return "VERIFYING_FILES";
    case "paused":
    case "download":
      return "DOWNLOADING_FILES";
    case "complete":
      if (state.isFileCheckComplete && !state.isUpdateAvailable)
        return "NO_UPDATE_REQUIRED";
      if (state.isFileCheckComplete && state.isUpdateAvailable)
        return "FILE_CHECK_COMPLETE";
      if (state.isDownloadComplete) return "DOWNLOAD_COMPLETE";
      if (state.isUpdateComplete) return "UPDATE_COMPLETED";
      break;
    default:
      return "GAME_READY_TO_LAUNCH";
  }
  return "GAME_READY_TO_LAUNCH";
}
