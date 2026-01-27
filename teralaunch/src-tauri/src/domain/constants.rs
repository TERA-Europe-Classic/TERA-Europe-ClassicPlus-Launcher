// ============================================================================
// Configuration Constants
// ============================================================================

/// Buffer size for file I/O operations (64 KB)
pub const BUFFER_SIZE: usize = 65_536;

/// HTTP request timeout for downloads (5 minutes)
pub const DOWNLOAD_TIMEOUT_SECS: u64 = 300;

/// HTTP connection timeout (30 seconds)
pub const CONNECT_TIMEOUT_SECS: u64 = 30;

/// Timeout before considering a download stalled (2 minutes)
pub const STALL_TIMEOUT_SECS: u64 = 120;

/// Progress update emission interval (500ms)
pub const PROGRESS_UPDATE_MS: u64 = 500;

/// Minimum chunk size for parallel downloads (16 MB)
pub const CHUNK_MIN_SIZE: u64 = 16 * 1024 * 1024;

/// Part size for chunked downloads (32 MB)
pub const PART_SIZE: u64 = 32 * 1024 * 1024;

/// Maximum number of parallel download parts
pub const MAX_PARTS: usize = 32;

/// Maximum concurrent file downloads
pub const MAX_CONCURRENT_DOWNLOADS: usize = 16;

/// BufWriter capacity for file downloads (1 MB)
pub const BUFWRITER_CAPACITY: usize = 1024 * 1024;

/// Part assembly buffer size (64 KB)
pub const PART_ASSEMBLY_BUFFER_SIZE: usize = 64 * 1024;

/// Maximum retry attempts for transient download errors
pub const MAX_RETRIES: u8 = 2;

/// Retry delay base multiplier (500ms per attempt)
pub const RETRY_DELAY_BASE_MS: u64 = 500;

/// HTTP client max idle connections per host
pub const HTTP_POOL_MAX_IDLE_PER_HOST: usize = 10;
