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
