use serde::Serialize;

/// Payload for download progress events
#[derive(Clone, Serialize)]
pub struct ProgressPayload {
    pub file_name: String,
    pub progress: f64,
    pub speed: f64,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub base_downloaded: u64,
    pub total_files: usize,
    pub elapsed_time: f64,
    pub current_file_index: usize,
}

/// Payload for file check/hash verification progress events
#[derive(Clone, Serialize)]
pub struct FileCheckProgress {
    pub current_file: String,
    pub progress: f64,
    pub current_count: usize,
    pub total_files: usize,
    pub elapsed_time: f64,
    pub files_to_update: usize,
}
