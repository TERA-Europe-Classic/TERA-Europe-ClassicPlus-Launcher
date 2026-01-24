export function calculateRemainingSize(files) {
  if (!Array.isArray(files)) return 0;
  return files.reduce((sum, file) => {
    const total = file?.size || 0;
    const existing = file?.existing_size || 0;
    return sum + Math.max(0, total - existing);
  }, 0);
}

export function calculateResumeSnapshot(previousTotal, previousDownloaded, files) {
  const remainingSize = calculateRemainingSize(files);
  const newTotalSize = previousTotal > 0 ? previousTotal : remainingSize;
  let alreadyDownloaded = 0;
  if (previousTotal > 0 && remainingSize > 0 && remainingSize < previousTotal) {
    alreadyDownloaded = previousTotal - remainingSize;
  } else {
    alreadyDownloaded = previousDownloaded || 0;
  }
  const clampedDownloaded = Math.min(
    Math.max(0, alreadyDownloaded),
    newTotalSize,
  );
  const stabilizedDownloaded = Math.max(previousDownloaded || 0, clampedDownloaded);
  return { remainingSize, newTotalSize, clampedDownloaded: stabilizedDownloaded };
}
