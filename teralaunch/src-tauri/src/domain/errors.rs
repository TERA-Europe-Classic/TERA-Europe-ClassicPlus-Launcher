//! Download error types for resilient error handling.

#![allow(dead_code)]

use std::fmt;

/// Structured error types for download operations.
///
/// These errors enable intelligent retry decisions:
/// - Transient errors (Network, Http 5xx, HashMismatch) should be retried
/// - Permanent errors (Http 4xx, Cancelled, FileSystem) should not
/// - ServerUnreachable indicates all retry attempts exhausted
/// - HashMismatch means corruption during download - delete and retry
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadError {
    /// Network-level error (connection, DNS, timeout)
    Network(String),

    /// HTTP error with status code
    Http { status: u16, message: String },

    /// File system error (disk full, permissions, etc.)
    FileSystem(String),

    /// Hash verification failed
    HashMismatch { expected: String, actual: String },

    /// User cancelled the download
    Cancelled,

    /// Server is unreachable after all retry attempts
    ServerUnreachable { attempts: u8, last_error: String },

    /// Stream was interrupted mid-download
    StreamInterrupted { bytes_received: u64, error: String },
}

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DownloadError::Network(msg) => write!(f, "Network error: {}", msg),
            DownloadError::Http { status, message } => {
                write!(f, "HTTP error {}: {}", status, message)
            }
            DownloadError::FileSystem(msg) => write!(f, "File system error: {}", msg),
            DownloadError::HashMismatch { expected, actual } => {
                write!(
                    f,
                    "Hash verification failed: expected {}, got {}",
                    expected, actual
                )
            }
            DownloadError::Cancelled => write!(f, "Download cancelled by user"),
            DownloadError::ServerUnreachable {
                attempts,
                last_error,
            } => {
                write!(
                    f,
                    "Server unreachable after {} attempts: {}",
                    attempts, last_error
                )
            }
            DownloadError::StreamInterrupted {
                bytes_received,
                error,
            } => {
                write!(
                    f,
                    "Stream interrupted after {} bytes: {}",
                    bytes_received, error
                )
            }
        }
    }
}

impl std::error::Error for DownloadError {}

impl From<std::io::Error> for DownloadError {
    fn from(err: std::io::Error) -> Self {
        DownloadError::FileSystem(err.to_string())
    }
}

impl From<&str> for DownloadError {
    fn from(s: &str) -> Self {
        let lower = s.to_lowercase();

        // Check for HTTP status codes
        if let Some(status) = extract_http_status(&lower) {
            return DownloadError::Http {
                status,
                message: s.to_string(),
            };
        }

        // Check for network-related errors
        if lower.contains("timeout")
            || lower.contains("connection")
            || lower.contains("dns")
            || lower.contains("network")
            || lower.contains("timed out")
            || lower.contains("connection refused")
            || lower.contains("connection reset")
        {
            return DownloadError::Network(s.to_string());
        }

        // Check for cancellation
        if lower.contains("cancel") || lower.contains("abort") {
            return DownloadError::Cancelled;
        }

        // Check for hash mismatch
        if lower.contains("hash") || lower.contains("checksum") {
            return DownloadError::HashMismatch {
                expected: "unknown".to_string(),
                actual: "unknown".to_string(),
            };
        }

        // Check for stream interruption
        if lower.contains("stream") || lower.contains("interrupt") {
            return DownloadError::StreamInterrupted {
                bytes_received: 0,
                error: s.to_string(),
            };
        }

        // Default to network error for unknown strings
        DownloadError::Network(s.to_string())
    }
}

impl From<String> for DownloadError {
    fn from(s: String) -> Self {
        DownloadError::from(s.as_str())
    }
}

/// Extracts HTTP status code from error message
fn extract_http_status(msg: &str) -> Option<u16> {
    // Look for patterns like "500", "404", "HTTP 500", "status 404"
    for word in msg.split_whitespace() {
        if let Ok(status) = word.parse::<u16>() {
            if (400..=599).contains(&status) {
                return Some(status);
            }
        }
    }
    None
}

impl DownloadError {
    /// Returns true if this error is transient and should be retried
    pub fn is_transient(&self) -> bool {
        match self {
            DownloadError::Network(_) => true,
            DownloadError::Http { status, .. } => {
                // 5xx are server errors (transient)
                // 408 is Request Timeout (transient)
                // 416 is Range Not Satisfiable (transient - retry fresh)
                // 429 is Too Many Requests (transient)
                *status >= 500 || *status == 408 || *status == 416 || *status == 429
            }
            DownloadError::StreamInterrupted { .. } => true,
            // Hash mismatch = corruption, should delete file and retry download
            DownloadError::HashMismatch { .. } => true,
            // Some filesystem errors are transient and should be retried
            DownloadError::FileSystem(msg) => {
                let m = msg.to_lowercase();
                m.contains("temporarily")
                    || m.contains("locked")
                    || m.contains("in use")
                    || m.contains("eagain")
                    || m.contains("would block")
                    || m.contains("resource busy")
            }
            _ => false,
        }
    }

    /// Returns true if this error indicates server may be unreachable
    pub fn is_connectivity_error(&self) -> bool {
        matches!(
            self,
            DownloadError::Network(_) | DownloadError::ServerUnreachable { .. }
        )
    }

    /// Returns true if this is a permanent error that should not be retried
    pub fn is_permanent(&self) -> bool {
        match self {
            DownloadError::Http { status, .. } => {
                // 4xx except 408, 416, 429 are permanent
                *status >= 400
                    && *status < 500
                    && *status != 408
                    && *status != 416
                    && *status != 429
            }
            // Hash mismatch is NOT permanent - it means corruption during download
            // The file should be deleted and re-downloaded
            DownloadError::HashMismatch { .. } => false,
            DownloadError::Cancelled => true,
            // Some filesystem errors are transient (locked, busy, etc.)
            DownloadError::FileSystem(msg) => {
                let m = msg.to_lowercase();
                // These filesystem errors are transient and should be retried
                !(m.contains("temporarily")
                    || m.contains("locked")
                    || m.contains("in use")
                    || m.contains("eagain")
                    || m.contains("would block")
                    || m.contains("resource busy"))
            }
            _ => false,
        }
    }

    /// Returns the HTTP status code if this is an HTTP error
    pub fn http_status(&self) -> Option<u16> {
        match self {
            DownloadError::Http { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// Creates a Network error from a reqwest-style error message
    pub fn from_network_error(msg: impl Into<String>) -> Self {
        DownloadError::Network(msg.into())
    }

    /// Creates an Http error
    pub fn http(status: u16, msg: impl Into<String>) -> Self {
        DownloadError::Http {
            status,
            message: msg.into(),
        }
    }

    /// Creates a ServerUnreachable error after exhausting retries
    pub fn server_unreachable(attempts: u8, last_error: impl Into<String>) -> Self {
        DownloadError::ServerUnreachable {
            attempts,
            last_error: last_error.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_error_creation() {
        let err = DownloadError::Network("connection timeout".to_string());
        assert!(matches!(err, DownloadError::Network(_)));
        assert_eq!(err.to_string(), "Network error: connection timeout");
    }

    #[test]
    fn test_http_error_creation() {
        let err = DownloadError::http(404, "Not Found");
        assert_eq!(err.http_status(), Some(404));
        assert_eq!(err.to_string(), "HTTP error 404: Not Found");
    }

    #[test]
    fn test_filesystem_error_creation() {
        let err = DownloadError::FileSystem("disk full".to_string());
        assert_eq!(err.to_string(), "File system error: disk full");
    }

    #[test]
    fn test_hash_mismatch_creation() {
        let err = DownloadError::HashMismatch {
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Hash verification failed: expected abc123, got def456"
        );
    }

    #[test]
    fn test_cancelled_error() {
        let err = DownloadError::Cancelled;
        assert_eq!(err.to_string(), "Download cancelled by user");
    }

    #[test]
    fn test_server_unreachable_creation() {
        let err = DownloadError::server_unreachable(3, "timeout");
        assert_eq!(
            err.to_string(),
            "Server unreachable after 3 attempts: timeout"
        );
    }

    #[test]
    fn test_stream_interrupted_creation() {
        let err = DownloadError::StreamInterrupted {
            bytes_received: 1024,
            error: "connection reset".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Stream interrupted after 1024 bytes: connection reset"
        );
    }

    #[test]
    fn test_is_transient_network() {
        let err = DownloadError::Network("timeout".to_string());
        assert!(err.is_transient());
        assert!(!err.is_permanent());
    }

    #[test]
    fn test_is_transient_http_5xx() {
        let err = DownloadError::http(500, "Internal Server Error");
        assert!(err.is_transient());
        assert!(!err.is_permanent());

        let err = DownloadError::http(503, "Service Unavailable");
        assert!(err.is_transient());
    }

    #[test]
    fn test_is_permanent_http_4xx() {
        let err = DownloadError::http(404, "Not Found");
        assert!(!err.is_transient());
        assert!(err.is_permanent());

        let err = DownloadError::http(403, "Forbidden");
        assert!(err.is_permanent());
    }

    #[test]
    fn test_hash_mismatch_is_transient_not_permanent() {
        // Hash mismatch = corruption during download, should retry (delete + redownload)
        let err = DownloadError::HashMismatch {
            expected: "abc".to_string(),
            actual: "def".to_string(),
        };
        assert!(err.is_transient());
        assert!(!err.is_permanent());
    }

    #[test]
    fn test_is_permanent_cancelled() {
        let err = DownloadError::Cancelled;
        assert!(!err.is_transient());
        assert!(err.is_permanent());
    }

    #[test]
    fn test_is_transient_stream_interrupted() {
        let err = DownloadError::StreamInterrupted {
            bytes_received: 512,
            error: "reset".to_string(),
        };
        assert!(err.is_transient());
        assert!(!err.is_permanent());
    }

    #[test]
    fn test_server_unreachable_not_transient_not_permanent() {
        let err = DownloadError::server_unreachable(5, "failed");
        assert!(!err.is_transient());
        assert!(!err.is_permanent());
    }

    #[test]
    fn test_is_connectivity_error() {
        let network = DownloadError::Network("timeout".to_string());
        assert!(network.is_connectivity_error());

        let unreachable = DownloadError::server_unreachable(3, "timeout");
        assert!(unreachable.is_connectivity_error());

        let http = DownloadError::http(404, "Not Found");
        assert!(!http.is_connectivity_error());
    }

    #[test]
    fn test_http_status() {
        let err = DownloadError::http(403, "Forbidden");
        assert_eq!(err.http_status(), Some(403));

        let err = DownloadError::Network("timeout".to_string());
        assert_eq!(err.http_status(), None);
    }

    #[test]
    fn test_from_network_error() {
        let err = DownloadError::from_network_error("connection failed");
        assert!(matches!(err, DownloadError::Network(_)));
        assert_eq!(err.to_string(), "Network error: connection failed");
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err: DownloadError = io_err.into();
        assert!(matches!(err, DownloadError::FileSystem(_)));
        assert!(err.to_string().contains("access denied"));
    }

    #[test]
    fn test_from_str_timeout() {
        let err = DownloadError::from("connection timeout");
        assert!(matches!(err, DownloadError::Network(_)));
        assert!(err.is_transient());
    }

    #[test]
    fn test_from_str_connection_refused() {
        let err = DownloadError::from("connection refused");
        assert!(matches!(err, DownloadError::Network(_)));
    }

    #[test]
    fn test_from_str_dns() {
        let err = DownloadError::from("DNS resolution failed");
        assert!(matches!(err, DownloadError::Network(_)));
    }

    #[test]
    fn test_from_str_http_404() {
        let err = DownloadError::from("HTTP 404 not found");
        match err {
            DownloadError::Http { status, .. } => assert_eq!(status, 404),
            _ => panic!("Expected Http variant"),
        }
        assert!(err.is_permanent());
    }

    #[test]
    fn test_from_str_http_500() {
        let err = DownloadError::from("server returned 500");
        match err {
            DownloadError::Http { status, .. } => assert_eq!(status, 500),
            _ => panic!("Expected Http variant"),
        }
        assert!(err.is_transient());
    }

    #[test]
    fn test_from_str_cancelled() {
        let err = DownloadError::from("download cancelled by user");
        assert!(matches!(err, DownloadError::Cancelled));
        assert!(err.is_permanent());
    }

    #[test]
    fn test_from_str_hash() {
        // Hash mismatch = corruption during download, should retry (delete + redownload)
        let err = DownloadError::from("hash mismatch detected");
        assert!(matches!(err, DownloadError::HashMismatch { .. }));
        assert!(err.is_transient());
        assert!(!err.is_permanent());
    }

    #[test]
    fn test_from_str_stream() {
        let err = DownloadError::from("stream interrupted");
        assert!(matches!(err, DownloadError::StreamInterrupted { .. }));
        assert!(err.is_transient());
    }

    #[test]
    fn test_from_str_unknown_defaults_to_network() {
        let err = DownloadError::from("some random error");
        assert!(matches!(err, DownloadError::Network(_)));
    }

    #[test]
    fn test_from_string() {
        let err = DownloadError::from("timeout".to_string());
        assert!(matches!(err, DownloadError::Network(_)));
    }

    #[test]
    fn test_extract_http_status_various_formats() {
        assert_eq!(extract_http_status("error 404"), Some(404));
        assert_eq!(extract_http_status("HTTP 500 error"), Some(500));
        assert_eq!(extract_http_status("status code 403"), Some(403));
        assert_eq!(extract_http_status("503 service unavailable"), Some(503));
        assert_eq!(extract_http_status("no status here"), None);
        assert_eq!(extract_http_status("status 200 ok"), None); // 200 not in range
    }

    #[test]
    fn test_clone_and_equality() {
        let err1 = DownloadError::http(404, "Not Found");
        let err2 = err1.clone();
        assert_eq!(err1, err2);
    }

    #[test]
    fn test_error_trait() {
        let err: Box<dyn std::error::Error> = Box::new(DownloadError::Cancelled);
        assert_eq!(err.to_string(), "Download cancelled by user");
    }

    #[test]
    fn test_filesystem_error_transient_locked() {
        let err = DownloadError::FileSystem("file is locked by another process".to_string());
        assert!(err.is_transient());
        assert!(!err.is_permanent());
    }

    #[test]
    fn test_filesystem_error_transient_in_use() {
        let err = DownloadError::FileSystem("resource is in use".to_string());
        assert!(err.is_transient());
        assert!(!err.is_permanent());
    }

    #[test]
    fn test_filesystem_error_transient_temporarily() {
        let err = DownloadError::FileSystem("temporarily unavailable".to_string());
        assert!(err.is_transient());
        assert!(!err.is_permanent());
    }

    #[test]
    fn test_filesystem_error_transient_eagain() {
        let err = DownloadError::FileSystem("EAGAIN error occurred".to_string());
        assert!(err.is_transient());
        assert!(!err.is_permanent());
    }

    #[test]
    fn test_filesystem_error_transient_would_block() {
        let err = DownloadError::FileSystem("operation would block".to_string());
        assert!(err.is_transient());
        assert!(!err.is_permanent());
    }

    #[test]
    fn test_filesystem_error_transient_resource_busy() {
        let err = DownloadError::FileSystem("resource busy".to_string());
        assert!(err.is_transient());
        assert!(!err.is_permanent());
    }

    #[test]
    fn test_filesystem_error_permanent_disk_full() {
        let err = DownloadError::FileSystem("disk full".to_string());
        assert!(!err.is_transient());
        assert!(err.is_permanent());
    }

    #[test]
    fn test_filesystem_error_permanent_permission_denied() {
        let err = DownloadError::FileSystem("permission denied".to_string());
        assert!(!err.is_transient());
        assert!(err.is_permanent());
    }
}
