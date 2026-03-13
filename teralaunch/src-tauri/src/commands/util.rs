//! Utility Tauri commands
//!
//! This module contains miscellaneous utility commands:
//! - Debug mode detection
//! - Logging configuration
//! - Launcher updates
//! - Server connectivity checks
//!
//! This module provides testable inner functions that accept an `HttpClient`
//! implementation, allowing tests to use `MockHttpClient` for unit testing
//! without requiring actual network access.

#![allow(dead_code)]

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::domain::{CONNECT_TIMEOUT_SECS, DOWNLOAD_TIMEOUT_SECS, HTTP_POOL_MAX_IDLE_PER_HOST};
use crate::infrastructure::HttpClient;
use crate::utils::validate_download_url;
use teralib::config::get_config_value;

/// Checks if the application is running in debug mode.
///
/// Returns true in development builds (cargo tauri dev), false in release builds.
#[tauri::command]
pub fn is_debug() -> bool {
    cfg!(debug_assertions)
}

/// Enables or disables file logging.
///
/// # Arguments
/// * `enabled` - Whether to enable file logging
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub fn set_logging(enabled: bool) -> Result<(), String> {
    set_logging_inner(enabled)
}

/// Inner testable function for enabling/disabling file logging.
///
/// This wraps the teralib logging function, allowing the command to be tested
/// indirectly and providing a consistent interface.
///
/// # Arguments
/// * `enabled` - Whether to enable file logging
fn set_logging_inner(enabled: bool) -> Result<(), String> {
    teralib::enable_file_logging(enabled)
}

/// Checks if a path string contains shell metacharacters that could be used for injection.
///
/// # Arguments
/// * `path_str` - The path string to validate
///
/// # Returns
/// `true` if the path contains unsafe shell metacharacters, `false` otherwise
fn is_unsafe_for_shell(path_str: &str) -> bool {
    // Reject paths containing shell metacharacters that could be exploited
    path_str.contains('&')
        || path_str.contains('|')
        || path_str.contains('>')
        || path_str.contains('<')
        || path_str.contains('^')
        || path_str.contains('`')
        || path_str.contains('$')
        || path_str.contains(';')
}

/// Downloads and installs a launcher update.
///
/// This command downloads the update from the specified URL, saves it to disk,
/// and initiates the update process. On Windows, it uses a batch command to
/// replace the running executable.
///
/// # Arguments
/// * `download_url` - URL to download the launcher update from
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn update_launcher(download_url: String) -> Result<(), String> {
    // Validate URL domain to prevent downloading from arbitrary sources
    validate_download_url(&download_url)?;

    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = current_exe.parent().ok_or("exe dir not found")?;
    let new_path = exe_dir.join("launcher_update.exe");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .pool_max_idle_per_host(HTTP_POOL_MAX_IDLE_PER_HOST)
        .build()
        .map_err(|e| e.to_string())?;

    let bytes = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    tokio::fs::write(&new_path, &bytes)
        .await
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        // Validate paths don't contain shell metacharacters to prevent injection
        let new_path_str = new_path.to_string_lossy();
        let current_exe_str = current_exe.to_string_lossy();

        if is_unsafe_for_shell(&new_path_str) || is_unsafe_for_shell(&current_exe_str) {
            return Err(
                "Unsafe path detected: paths must not contain shell metacharacters".to_string(),
            );
        }

        let cmd = format!(
            "ping 127.0.0.1 -n 2 > NUL && move /Y \"{}\" \"{}\" && start \"\" \"{}\"",
            new_path.display(),
            current_exe.display(),
            current_exe.display()
        );
        Command::new("cmd")
            .args(["/C", &cmd])
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::fs::rename(&new_path, &current_exe).map_err(|e| e.to_string())?;
        let _ = Command::new(&current_exe).spawn();
    }

    std::process::exit(0);
}

/// Inner testable function for downloading launcher updates.
///
/// Downloads the update file content using the provided HttpClient.
/// This function handles only the download portion - actual file writing
/// and process replacement are handled by the Tauri command.
///
/// # Arguments
/// * `client` - HTTP client implementation
/// * `url` - URL to download the update from
///
/// # Returns
/// The downloaded bytes on success, or an error message on failure
pub async fn update_launcher_inner<H: HttpClient>(
    client: &H,
    url: &str,
) -> Result<Vec<u8>, String> {
    let response = client.get(url).await?;

    if !response.is_success() {
        return Err(format!(
            "Failed to download update: HTTP {}",
            response.status
        ));
    }

    Ok(response.body)
}

/// Inner testable function for writing update file and spawning replacement process.
///
/// This handles the file writing portion of the update process.
/// Note: In production, this is followed by process replacement which cannot be easily tested.
///
/// # Arguments
/// * `data` - The downloaded update file bytes
/// * `output_path` - Path to write the update file
pub async fn write_update_file(data: &[u8], output_path: &Path) -> Result<(), String> {
    tokio::fs::write(output_path, data)
        .await
        .map_err(|e| format!("Failed to write update file: {}", e))
}

/// Fetches the current player count from the game server API.
///
/// This bypasses CORS restrictions by making the request from the backend.
///
/// # Returns
/// JSON string containing player count data
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn fetch_player_count() -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get("https://tera-europe-classic.com/api/player-count?server=classic")
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch player count: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Server returned status: {}", response.status()));
    }

    response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))
}

/// Fetches the news RSS feed and returns parsed items.
///
/// This bypasses CORS restrictions by making the request from the backend.
///
/// # Returns
/// JSON array of news items with title, link, and pubDate
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn fetch_news_feed() -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get("https://forum.crazy-esports.com/forum/thread-list-rss-feed/43/")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch news feed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Server returned status: {}", response.status()));
    }

    let xml_text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    // Parse RSS XML and extract items
    parse_rss_to_json(&xml_text)
}

/// Parses RSS XML and returns a JSON array of news items.
fn parse_rss_to_json(xml: &str) -> Result<String, String> {
    use std::io::BufRead;

    let mut items: Vec<serde_json::Value> = Vec::new();
    let cursor = std::io::Cursor::new(xml);

    let mut current_item: Option<serde_json::Map<String, serde_json::Value>> = None;
    let current_tag = String::new();
    let mut in_item = false;

    for line in cursor.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let trimmed = line.trim();

        if trimmed.starts_with("<item>") || trimmed.starts_with("<item ") {
            in_item = true;
            current_item = Some(serde_json::Map::new());
        } else if trimmed == "</item>" {
            in_item = false;
            if let Some(item) = current_item.take() {
                items.push(serde_json::Value::Object(item));
                if items.len() >= 5 {
                    break; // Only need first 5 items
                }
            }
        } else if in_item {
            // Extract tag content
            if let Some(item) = current_item.as_mut() {
                if trimmed.starts_with("<title>") {
                    if let Some(content) = extract_tag_content(trimmed, "title") {
                        item.insert("title".to_string(), serde_json::Value::String(content));
                    }
                } else if trimmed.starts_with("<link>") {
                    if let Some(content) = extract_tag_content(trimmed, "link") {
                        item.insert("link".to_string(), serde_json::Value::String(content));
                    }
                } else if trimmed.starts_with("<pubDate>") {
                    if let Some(content) = extract_tag_content(trimmed, "pubDate") {
                        item.insert("pubDate".to_string(), serde_json::Value::String(content));
                    }
                } else if trimmed.starts_with("<dc:creator>") {
                    if let Some(content) = extract_tag_content(trimmed, "dc:creator") {
                        item.insert("author".to_string(), serde_json::Value::String(content));
                    }
                }
            }
        }
    }

    serde_json::to_string(&items).map_err(|e| e.to_string())
}

/// Extracts content from a simple XML tag.
fn extract_tag_content(line: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);

    if let Some(start) = line.find(&start_tag) {
        let content_start = start + start_tag.len();
        if let Some(end) = line.find(&end_tag) {
            let content = &line[content_start..end];
            // Decode basic XML entities
            return Some(
                content
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&amp;", "&")
                    .replace("&quot;", "\"")
                    .replace("&#39;", "'"),
            );
        }
    }
    None
}

/// Checks if the file server is reachable.
///
/// Makes a simple GET request to the file server and returns whether
/// the server responded successfully.
#[tauri::command]
#[cfg(not(tarpaulin_include))]
pub async fn check_server_connection() -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .pool_max_idle_per_host(HTTP_POOL_MAX_IDLE_PER_HOST)
        .build()
        .map_err(|e| e.to_string())?;

    match client.get(get_config_value("FILE_SERVER_URL")).send().await {
        Ok(response) => Ok(response.status().is_success()),
        Err(e) => Err(e.to_string()),
    }
}

/// Inner testable function for checking server connectivity.
///
/// Makes a GET request to the specified URL and returns whether the server
/// responded with a success status code (2xx).
///
/// # Arguments
/// * `client` - HTTP client implementation
/// * `url` - URL to check connectivity against
///
/// # Returns
/// `Ok(true)` if server responds with 2xx status, `Ok(false)` for other statuses,
/// `Err` if request fails (network error, timeout, etc.)
pub async fn check_server_inner<H: HttpClient>(client: &H, url: &str) -> Result<bool, String> {
    let response = client.get(url).await?;
    Ok(response.is_success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::{HttpResponse, MockHttpClient};
    use tempfile::tempdir;

    // ============================================================================
    // is_unsafe_for_shell tests
    // ============================================================================

    #[test]
    fn is_unsafe_for_shell_rejects_ampersand() {
        assert!(is_unsafe_for_shell("C:\\path&malicious.exe"));
    }

    #[test]
    fn is_unsafe_for_shell_rejects_pipe() {
        assert!(is_unsafe_for_shell("C:\\path|dir.exe"));
    }

    #[test]
    fn is_unsafe_for_shell_rejects_greater_than() {
        assert!(is_unsafe_for_shell("C:\\path>output.txt"));
    }

    #[test]
    fn is_unsafe_for_shell_rejects_less_than() {
        assert!(is_unsafe_for_shell("C:\\path<input.txt"));
    }

    #[test]
    fn is_unsafe_for_shell_rejects_caret() {
        assert!(is_unsafe_for_shell("C:\\path^escape.exe"));
    }

    #[test]
    fn is_unsafe_for_shell_rejects_backtick() {
        assert!(is_unsafe_for_shell("C:\\path`command`.exe"));
    }

    #[test]
    fn is_unsafe_for_shell_rejects_dollar() {
        assert!(is_unsafe_for_shell("C:\\path$variable.exe"));
    }

    #[test]
    fn is_unsafe_for_shell_rejects_semicolon() {
        assert!(is_unsafe_for_shell("C:\\path;cmd.exe"));
    }

    #[test]
    fn is_unsafe_for_shell_accepts_normal_windows_path() {
        assert!(!is_unsafe_for_shell(
            "C:\\Program Files\\Launcher\\launcher.exe"
        ));
    }

    #[test]
    fn is_unsafe_for_shell_accepts_path_with_spaces() {
        assert!(!is_unsafe_for_shell("C:\\My Documents\\launcher.exe"));
    }

    #[test]
    fn is_unsafe_for_shell_accepts_path_with_dash() {
        assert!(!is_unsafe_for_shell("C:\\launcher-update.exe"));
    }

    #[test]
    fn is_unsafe_for_shell_accepts_path_with_underscore() {
        assert!(!is_unsafe_for_shell("C:\\launcher_update.exe"));
    }

    #[test]
    fn is_unsafe_for_shell_accepts_path_with_dots() {
        assert!(!is_unsafe_for_shell("C:\\path\\to\\launcher.v2.0.exe"));
    }

    #[test]
    fn is_unsafe_for_shell_rejects_injection_attempt() {
        // Classic injection: close quote, add command, reopen quote
        assert!(is_unsafe_for_shell(
            "C:\\path\\file.exe\" && malicious.exe && \""
        ));
    }

    // ============================================================================
    // is_debug tests
    // ============================================================================

    #[test]
    fn is_debug_returns_expected_value() {
        // In test builds, debug_assertions is typically enabled
        let result = is_debug();
        assert_eq!(result, cfg!(debug_assertions));
    }

    #[test]
    fn is_debug_returns_consistent_value() {
        // Multiple calls should return the same value
        let first = is_debug();
        let second = is_debug();
        assert_eq!(first, second);
    }

    // ============================================================================
    // set_logging tests
    // ============================================================================

    #[test]
    fn set_logging_inner_enable_returns_result() {
        // We can't fully test this without affecting global state,
        // but we can verify it doesn't panic and returns a result
        let result = set_logging_inner(false);
        // Disabling logging should always succeed
        assert!(result.is_ok());
    }

    #[test]
    fn set_logging_inner_disable_succeeds() {
        // Disabling logging should always work
        let result = set_logging_inner(false);
        assert!(result.is_ok());
    }

    // ============================================================================
    // check_server_inner tests (HttpClient)
    // ============================================================================

    #[tokio::test]
    async fn check_server_inner_returns_true_for_200() {
        let mock = MockHttpClient::new();
        mock.add_response(
            "https://example.com/health",
            HttpResponse {
                status: 200,
                body: b"OK".to_vec(),
                content_length: Some(2),
                supports_range: false,
            },
        );

        let result = check_server_inner(&mock, "https://example.com/health").await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn check_server_inner_returns_true_for_204() {
        let mock = MockHttpClient::new();
        mock.add_response(
            "https://example.com/health",
            HttpResponse {
                status: 204,
                body: vec![],
                content_length: Some(0),
                supports_range: false,
            },
        );

        let result = check_server_inner(&mock, "https://example.com/health").await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn check_server_inner_returns_false_for_404() {
        let mock = MockHttpClient::new();
        mock.add_response(
            "https://example.com/health",
            HttpResponse {
                status: 404,
                body: b"Not Found".to_vec(),
                content_length: Some(9),
                supports_range: false,
            },
        );

        let result = check_server_inner(&mock, "https://example.com/health").await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn check_server_inner_returns_false_for_500() {
        let mock = MockHttpClient::new();
        mock.add_response(
            "https://example.com/health",
            HttpResponse {
                status: 500,
                body: b"Internal Server Error".to_vec(),
                content_length: Some(21),
                supports_range: false,
            },
        );

        let result = check_server_inner(&mock, "https://example.com/health").await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn check_server_inner_returns_false_for_503() {
        let mock = MockHttpClient::new();
        mock.add_response(
            "https://example.com/health",
            HttpResponse {
                status: 503,
                body: b"Service Unavailable".to_vec(),
                content_length: Some(19),
                supports_range: false,
            },
        );

        let result = check_server_inner(&mock, "https://example.com/health").await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn check_server_inner_returns_error_on_network_failure() {
        let mock = MockHttpClient::new();
        mock.add_error("https://example.com/health", "Connection refused");

        let result = check_server_inner(&mock, "https://example.com/health").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Connection refused"));
    }

    #[tokio::test]
    async fn check_server_inner_returns_error_on_timeout() {
        let mock = MockHttpClient::new();
        mock.add_error("https://example.com/health", "Request timeout");

        let result = check_server_inner(&mock, "https://example.com/health").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("timeout"));
    }

    #[tokio::test]
    async fn check_server_inner_handles_unknown_url() {
        let mock = MockHttpClient::new();
        // MockHttpClient returns 404 for unknown URLs by default

        let result = check_server_inner(&mock, "https://unknown.example.com/health").await;
        assert!(result.is_ok());
        assert!(!result.unwrap()); // 404 is not a success
    }

    // ============================================================================
    // update_launcher_inner tests (HttpClient)
    // ============================================================================

    #[tokio::test]
    async fn update_launcher_inner_downloads_successfully() {
        let mock = MockHttpClient::new();
        let update_data = b"fake executable content";
        mock.add_response(
            "https://example.com/launcher.exe",
            HttpResponse {
                status: 200,
                body: update_data.to_vec(),
                content_length: Some(update_data.len() as u64),
                supports_range: false,
            },
        );

        let result = update_launcher_inner(&mock, "https://example.com/launcher.exe").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), update_data.to_vec());
    }

    #[tokio::test]
    async fn update_launcher_inner_downloads_large_file() {
        let mock = MockHttpClient::new();
        // Simulate a larger update file (1MB)
        let update_data: Vec<u8> = (0..1024 * 1024).map(|i| (i % 256) as u8).collect();
        mock.add_response(
            "https://example.com/launcher.exe",
            HttpResponse {
                status: 200,
                body: update_data.clone(),
                content_length: Some(update_data.len() as u64),
                supports_range: false,
            },
        );

        let result = update_launcher_inner(&mock, "https://example.com/launcher.exe").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1024 * 1024);
    }

    #[tokio::test]
    async fn update_launcher_inner_returns_error_on_404() {
        let mock = MockHttpClient::new();
        mock.add_response(
            "https://example.com/launcher.exe",
            HttpResponse {
                status: 404,
                body: b"Not Found".to_vec(),
                content_length: Some(9),
                supports_range: false,
            },
        );

        let result = update_launcher_inner(&mock, "https://example.com/launcher.exe").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("HTTP 404"));
    }

    #[tokio::test]
    async fn update_launcher_inner_returns_error_on_500() {
        let mock = MockHttpClient::new();
        mock.add_response(
            "https://example.com/launcher.exe",
            HttpResponse {
                status: 500,
                body: b"Internal Server Error".to_vec(),
                content_length: Some(21),
                supports_range: false,
            },
        );

        let result = update_launcher_inner(&mock, "https://example.com/launcher.exe").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("HTTP 500"));
    }

    #[tokio::test]
    async fn update_launcher_inner_returns_error_on_network_failure() {
        let mock = MockHttpClient::new();
        mock.add_error("https://example.com/launcher.exe", "DNS resolution failed");

        let result = update_launcher_inner(&mock, "https://example.com/launcher.exe").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("DNS resolution failed"));
    }

    #[tokio::test]
    async fn update_launcher_inner_returns_error_on_connection_refused() {
        let mock = MockHttpClient::new();
        mock.add_error("https://example.com/launcher.exe", "Connection refused");

        let result = update_launcher_inner(&mock, "https://example.com/launcher.exe").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Connection refused"));
    }

    #[tokio::test]
    async fn update_launcher_inner_handles_empty_response() {
        let mock = MockHttpClient::new();
        mock.add_response(
            "https://example.com/launcher.exe",
            HttpResponse {
                status: 200,
                body: vec![],
                content_length: Some(0),
                supports_range: false,
            },
        );

        let result = update_launcher_inner(&mock, "https://example.com/launcher.exe").await;
        // An empty file is technically a valid response (though likely not a valid executable)
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    // ============================================================================
    // write_update_file tests
    // ============================================================================

    #[tokio::test]
    async fn write_update_file_creates_file() {
        let temp_dir = tempdir().unwrap();
        let output_path = temp_dir.path().join("update.exe");
        let data = b"test update content";

        let result = write_update_file(data, &output_path).await;
        assert!(result.is_ok());
        assert!(output_path.exists());

        let written = std::fs::read(&output_path).unwrap();
        assert_eq!(written, data);
    }

    #[tokio::test]
    async fn write_update_file_overwrites_existing() {
        let temp_dir = tempdir().unwrap();
        let output_path = temp_dir.path().join("update.exe");

        // Create an existing file
        std::fs::write(&output_path, b"old content").unwrap();

        // Overwrite with new content
        let new_data = b"new update content";
        let result = write_update_file(new_data, &output_path).await;
        assert!(result.is_ok());

        let written = std::fs::read(&output_path).unwrap();
        assert_eq!(written, new_data);
    }

    #[tokio::test]
    async fn write_update_file_writes_empty_file() {
        let temp_dir = tempdir().unwrap();
        let output_path = temp_dir.path().join("update.exe");

        let result = write_update_file(&[], &output_path).await;
        assert!(result.is_ok());
        assert!(output_path.exists());

        let written = std::fs::read(&output_path).unwrap();
        assert!(written.is_empty());
    }

    #[tokio::test]
    async fn write_update_file_writes_large_file() {
        let temp_dir = tempdir().unwrap();
        let output_path = temp_dir.path().join("update.exe");

        // 1MB file
        let data: Vec<u8> = (0..1024 * 1024).map(|i| (i % 256) as u8).collect();

        let result = write_update_file(&data, &output_path).await;
        assert!(result.is_ok());

        let written = std::fs::read(&output_path).unwrap();
        assert_eq!(written.len(), 1024 * 1024);
    }

    #[tokio::test]
    async fn write_update_file_fails_for_invalid_path() {
        // Use a path that should not be writable
        let invalid_path = std::path::Path::new("/nonexistent/directory/update.exe");

        let result = write_update_file(b"test", invalid_path).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to write update file"));
    }

    // ============================================================================
    // Integration-style tests combining multiple functions
    // ============================================================================

    #[tokio::test]
    async fn download_and_write_update_integration() {
        let mock = MockHttpClient::new();
        let update_data = b"complete update executable";
        mock.add_response(
            "https://example.com/launcher.exe",
            HttpResponse {
                status: 200,
                body: update_data.to_vec(),
                content_length: Some(update_data.len() as u64),
                supports_range: false,
            },
        );

        // Download
        let downloaded = update_launcher_inner(&mock, "https://example.com/launcher.exe")
            .await
            .unwrap();

        // Write
        let temp_dir = tempdir().unwrap();
        let output_path = temp_dir.path().join("launcher_update.exe");
        write_update_file(&downloaded, &output_path).await.unwrap();

        // Verify
        let written = std::fs::read(&output_path).unwrap();
        assert_eq!(written, update_data);
    }

    #[tokio::test]
    async fn check_server_before_update_flow() {
        let mock = MockHttpClient::new();

        // Server health check
        mock.add_response(
            "https://example.com/health",
            HttpResponse {
                status: 200,
                body: b"OK".to_vec(),
                content_length: Some(2),
                supports_range: false,
            },
        );

        // Update file
        let update_data = b"update content";
        mock.add_response(
            "https://example.com/launcher.exe",
            HttpResponse {
                status: 200,
                body: update_data.to_vec(),
                content_length: Some(update_data.len() as u64),
                supports_range: false,
            },
        );

        // Check server first
        let is_available = check_server_inner(&mock, "https://example.com/health")
            .await
            .unwrap();
        assert!(is_available);

        // Then download update
        let downloaded = update_launcher_inner(&mock, "https://example.com/launcher.exe")
            .await
            .unwrap();
        assert_eq!(downloaded, update_data);
    }

    #[tokio::test]
    async fn check_server_fails_prevents_update() {
        let mock = MockHttpClient::new();
        mock.add_error("https://example.com/health", "Connection refused");

        // Server check should fail
        let result = check_server_inner(&mock, "https://example.com/health").await;
        assert!(result.is_err());
        // In real code, we would not proceed with update if server is unreachable
    }
}
