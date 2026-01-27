//! HTTP client abstraction for testability.
//!
//! This module provides a trait for HTTP operations, allowing the application
//! to use mock implementations in tests while using reqwest in production.

use std::future::Future;

const DEFAULT_USER_AGENT: &str = "Tera Game Launcher";

/// Response from an HTTP request.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// HTTP status code
    pub status: u16,
    /// Response body as bytes
    pub body: Vec<u8>,
    /// Content-Length header value if present
    pub content_length: Option<u64>,
    /// Whether the server supports range requests
    pub supports_range: bool,
}

impl HttpResponse {
    /// Check if the response status indicates success (2xx)
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Check if the response is a partial content response (206)
    pub fn is_partial(&self) -> bool {
        self.status == 206
    }

    /// Get the body as a string (UTF-8)
    pub fn text(&self) -> Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.body.clone())
    }

    /// Parse the body as JSON
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }
}

/// Trait for HTTP operations, allowing mocking in tests.
///
/// All methods return `impl Future` to support async operations without
/// requiring the `async_trait` crate, leveraging Rust's native async trait support.
pub trait HttpClient: Send + Sync {
    /// Perform a GET request to the specified URL.
    fn get(&self, url: &str) -> impl Future<Output = Result<HttpResponse, String>> + Send;

    /// Perform a GET request with a Range header for partial content.
    fn get_range(
        &self,
        url: &str,
        start: u64,
        end: Option<u64>,
    ) -> impl Future<Output = Result<HttpResponse, String>> + Send;

    /// Perform a POST request with a JSON body.
    fn post(
        &self,
        url: &str,
        body: &str,
    ) -> impl Future<Output = Result<HttpResponse, String>> + Send;

    /// Perform a POST request with form data.
    fn post_form(
        &self,
        url: &str,
        form: &[(String, String)],
    ) -> impl Future<Output = Result<HttpResponse, String>> + Send;
}

/// Default HTTP client implementation using reqwest.
pub struct ReqwestClient {
    client: reqwest::Client,
}

impl ReqwestClient {
    /// Create a new ReqwestClient with the provided reqwest::Client.
    #[cfg(not(tarpaulin_include))]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// Create a new ReqwestClient with default configuration.
    #[cfg(not(tarpaulin_include))]
    pub fn with_defaults(timeout_secs: u64, connect_timeout_secs: u64) -> Result<Self, String> {
        use std::time::Duration;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .connect_timeout(Duration::from_secs(connect_timeout_secs))
            .cookie_store(true)
            .user_agent(DEFAULT_USER_AGENT)
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
        Ok(Self { client })
    }
}

impl HttpClient for ReqwestClient {
    #[cfg(not(tarpaulin_include))]
    async fn get(&self, url: &str) -> Result<HttpResponse, String> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let status = response.status().as_u16();
        let content_length = response.content_length();
        let supports_range = response
            .headers()
            .get("accept-ranges")
            .map(|v| v.to_str().unwrap_or("").contains("bytes"))
            .unwrap_or(false);

        let body = response.bytes().await.map_err(|e| e.to_string())?.to_vec();

        Ok(HttpResponse {
            status,
            body,
            content_length,
            supports_range,
        })
    }

    #[cfg(not(tarpaulin_include))]
    async fn get_range(
        &self,
        url: &str,
        start: u64,
        end: Option<u64>,
    ) -> Result<HttpResponse, String> {
        use reqwest::header::RANGE;

        let range_header = match end {
            Some(e) => format!("bytes={}-{}", start, e),
            None => format!("bytes={}-", start),
        };

        let response = self
            .client
            .get(url)
            .header(RANGE, range_header)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let status = response.status().as_u16();
        let content_length = response.content_length();
        let supports_range = response.headers().get("content-range").is_some() || status == 206;

        let body = response.bytes().await.map_err(|e| e.to_string())?.to_vec();

        Ok(HttpResponse {
            status,
            body,
            content_length,
            supports_range,
        })
    }

    #[cfg(not(tarpaulin_include))]
    async fn post(&self, url: &str, body: &str) -> Result<HttpResponse, String> {
        let response = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let status = response.status().as_u16();
        let content_length = response.content_length();

        let body_bytes = response.bytes().await.map_err(|e| e.to_string())?.to_vec();

        Ok(HttpResponse {
            status,
            body: body_bytes,
            content_length,
            supports_range: false,
        })
    }

    #[cfg(not(tarpaulin_include))]
    async fn post_form(
        &self,
        url: &str,
        form: &[(String, String)],
    ) -> Result<HttpResponse, String> {
        let response = self
            .client
            .post(url)
            .form(form)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let status = response.status().as_u16();
        let content_length = response.content_length();

        let body = response.bytes().await.map_err(|e| e.to_string())?.to_vec();

        Ok(HttpResponse {
            status,
            body,
            content_length,
            supports_range: false,
        })
    }
}

/// Mock HTTP client for testing.
///
/// Allows setting up predetermined responses for specific URLs.
/// This is available in test builds for use by other modules' tests.
#[cfg(test)]
pub struct MockHttpClient {
    responses: std::sync::RwLock<std::collections::HashMap<String, Result<HttpResponse, String>>>,
    range_responses:
        std::sync::RwLock<std::collections::HashMap<String, Result<HttpResponse, String>>>,
}

#[cfg(test)]
impl MockHttpClient {
    /// Create a new empty mock client.
    pub fn new() -> Self {
        Self {
            responses: std::sync::RwLock::new(std::collections::HashMap::new()),
            range_responses: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Add a successful response for a URL.
    pub fn add_response(&self, url: &str, response: HttpResponse) {
        self.responses
            .write()
            .unwrap()
            .insert(url.to_string(), Ok(response));
    }

    /// Add an error response for a URL.
    pub fn add_error(&self, url: &str, error: &str) {
        self.responses
            .write()
            .unwrap()
            .insert(url.to_string(), Err(error.to_string()));
    }

    /// Add a response specifically for range requests (get_range).
    pub fn add_range_response(&self, url: &str, response: HttpResponse) {
        self.range_responses
            .write()
            .unwrap()
            .insert(url.to_string(), Ok(response));
    }

    /// Add an error response specifically for range requests.
    pub fn add_range_error(&self, url: &str, error: &str) {
        self.range_responses
            .write()
            .unwrap()
            .insert(url.to_string(), Err(error.to_string()));
    }

    /// Get the response for a URL, or return a 404 error.
    fn get_response(&self, url: &str) -> Result<HttpResponse, String> {
        self.responses
            .read()
            .unwrap()
            .get(url)
            .cloned()
            .unwrap_or_else(|| {
                Ok(HttpResponse {
                    status: 404,
                    body: b"Not Found".to_vec(),
                    content_length: None,
                    supports_range: false,
                })
            })
    }
}

#[cfg(test)]
impl Default for MockHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl HttpClient for MockHttpClient {
    async fn get(&self, url: &str) -> Result<HttpResponse, String> {
        self.get_response(url)
    }

    async fn get_range(
        &self,
        url: &str,
        _start: u64,
        _end: Option<u64>,
    ) -> Result<HttpResponse, String> {
        // Check range-specific responses first
        if let Some(response) = self.range_responses.read().unwrap().get(url).cloned() {
            return response;
        }
        // Fall back to regular responses
        self.get_response(url)
    }

    async fn post(&self, url: &str, _body: &str) -> Result<HttpResponse, String> {
        self.get_response(url)
    }

    async fn post_form(
        &self,
        url: &str,
        _form: &[(String, String)],
    ) -> Result<HttpResponse, String> {
        self.get_response(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_response_is_success_for_2xx() {
        let response = HttpResponse {
            status: 200,
            body: vec![],
            content_length: None,
            supports_range: false,
        };
        assert!(response.is_success());

        let response = HttpResponse {
            status: 299,
            body: vec![],
            content_length: None,
            supports_range: false,
        };
        assert!(response.is_success());

        let response = HttpResponse {
            status: 404,
            body: vec![],
            content_length: None,
            supports_range: false,
        };
        assert!(!response.is_success());
    }

    #[test]
    fn http_response_is_partial_for_206() {
        let response = HttpResponse {
            status: 206,
            body: vec![],
            content_length: None,
            supports_range: true,
        };
        assert!(response.is_partial());

        let response = HttpResponse {
            status: 200,
            body: vec![],
            content_length: None,
            supports_range: false,
        };
        assert!(!response.is_partial());
    }

    #[test]
    fn http_response_text_parses_utf8() {
        let response = HttpResponse {
            status: 200,
            body: b"Hello, World!".to_vec(),
            content_length: None,
            supports_range: false,
        };
        assert_eq!(response.text().unwrap(), "Hello, World!");
    }

    #[test]
    fn http_response_json_parses_valid_json() {
        let response = HttpResponse {
            status: 200,
            body: br#"{"key": "value"}"#.to_vec(),
            content_length: None,
            supports_range: false,
        };
        let parsed: serde_json::Value = response.json().unwrap();
        assert_eq!(parsed["key"], "value");
    }

    // Edge case tests for is_success()
    #[test]
    fn http_response_is_success_boundary_values() {
        // 199 is not success (before 2xx range)
        let response = HttpResponse {
            status: 199,
            body: vec![],
            content_length: None,
            supports_range: false,
        };
        assert!(!response.is_success());

        // 200 is success (start of 2xx range)
        let response = HttpResponse {
            status: 200,
            body: vec![],
            content_length: None,
            supports_range: false,
        };
        assert!(response.is_success());

        // 299 is success (end of 2xx range)
        let response = HttpResponse {
            status: 299,
            body: vec![],
            content_length: None,
            supports_range: false,
        };
        assert!(response.is_success());

        // 300 is not success (after 2xx range)
        let response = HttpResponse {
            status: 300,
            body: vec![],
            content_length: None,
            supports_range: false,
        };
        assert!(!response.is_success());
    }

    // Test text() with invalid UTF-8 bytes
    #[test]
    fn http_response_text_with_invalid_utf8() {
        let response = HttpResponse {
            status: 200,
            body: vec![0xFF, 0xFE, 0xFD], // Invalid UTF-8 sequence
            content_length: None,
            supports_range: false,
        };
        assert!(response.text().is_err());
    }

    // Test json() with invalid JSON
    #[test]
    fn http_response_json_with_invalid_json() {
        let response = HttpResponse {
            status: 200,
            body: b"not valid json {".to_vec(),
            content_length: None,
            supports_range: false,
        };
        let result: Result<serde_json::Value, _> = response.json();
        assert!(result.is_err());
    }

    // Test json() with various JSON types
    #[test]
    fn http_response_json_with_various_types() {
        // Array
        let response = HttpResponse {
            status: 200,
            body: br#"[1, 2, 3]"#.to_vec(),
            content_length: None,
            supports_range: false,
        };
        let parsed: Vec<i32> = response.json().unwrap();
        assert_eq!(parsed, vec![1, 2, 3]);

        // Nested object
        let response = HttpResponse {
            status: 200,
            body: br#"{"outer": {"inner": "value"}}"#.to_vec(),
            content_length: None,
            supports_range: false,
        };
        let parsed: serde_json::Value = response.json().unwrap();
        assert_eq!(parsed["outer"]["inner"], "value");

        // Number
        let response = HttpResponse {
            status: 200,
            body: b"42".to_vec(),
            content_length: None,
            supports_range: false,
        };
        let parsed: i32 = response.json().unwrap();
        assert_eq!(parsed, 42);

        // Null
        let response = HttpResponse {
            status: 200,
            body: b"null".to_vec(),
            content_length: None,
            supports_range: false,
        };
        let parsed: Option<String> = response.json().unwrap();
        assert_eq!(parsed, None);
    }

    // Test HttpResponse Clone trait
    #[test]
    fn http_response_clone() {
        let original = HttpResponse {
            status: 200,
            body: b"test data".to_vec(),
            content_length: Some(9),
            supports_range: true,
        };
        let cloned = original.clone();

        assert_eq!(cloned.status, original.status);
        assert_eq!(cloned.body, original.body);
        assert_eq!(cloned.content_length, original.content_length);
        assert_eq!(cloned.supports_range, original.supports_range);
    }

    // Test HttpResponse Debug trait
    #[test]
    fn http_response_debug() {
        let response = HttpResponse {
            status: 200,
            body: b"test".to_vec(),
            content_length: Some(4),
            supports_range: true,
        };
        let debug_str = format!("{:?}", response);
        assert!(debug_str.contains("status: 200"));
        assert!(debug_str.contains("body:"));
        assert!(debug_str.contains("content_length:"));
        assert!(debug_str.contains("supports_range:"));
    }

    // Test is_partial() for non-206 status codes
    #[test]
    fn http_response_is_partial_for_non_206() {
        // 201 Created - not partial
        let response = HttpResponse {
            status: 201,
            body: vec![],
            content_length: None,
            supports_range: false,
        };
        assert!(!response.is_partial());

        // 200 OK - not partial
        let response = HttpResponse {
            status: 200,
            body: vec![],
            content_length: None,
            supports_range: true,
        };
        assert!(!response.is_partial());

        // 404 Not Found - not partial
        let response = HttpResponse {
            status: 404,
            body: vec![],
            content_length: None,
            supports_range: false,
        };
        assert!(!response.is_partial());

        // Only 206 should be partial
        let response = HttpResponse {
            status: 206,
            body: vec![],
            content_length: None,
            supports_range: true,
        };
        assert!(response.is_partial());
    }

    // Test content_length field variants
    #[test]
    fn http_response_content_length_variants() {
        // content_length Some
        let response = HttpResponse {
            status: 200,
            body: b"test".to_vec(),
            content_length: Some(1024),
            supports_range: false,
        };
        assert_eq!(response.content_length, Some(1024));

        // content_length None
        let response = HttpResponse {
            status: 200,
            body: b"test".to_vec(),
            content_length: None,
            supports_range: false,
        };
        assert_eq!(response.content_length, None);

        // content_length zero
        let response = HttpResponse {
            status: 204,
            body: vec![],
            content_length: Some(0),
            supports_range: false,
        };
        assert_eq!(response.content_length, Some(0));
    }

    // Test supports_range field in various responses
    #[test]
    fn http_response_supports_range_field() {
        // supports_range true
        let response = HttpResponse {
            status: 200,
            body: vec![],
            content_length: Some(1000),
            supports_range: true,
        };
        assert!(response.supports_range);

        // supports_range false
        let response = HttpResponse {
            status: 200,
            body: vec![],
            content_length: Some(1000),
            supports_range: false,
        };
        assert!(!response.supports_range);

        // supports_range true with 206 status
        let response = HttpResponse {
            status: 206,
            body: vec![],
            content_length: Some(500),
            supports_range: true,
        };
        assert!(response.supports_range);
        assert!(response.is_partial());
    }

    // MockHttpClient tests

    // Test Default trait implementation
    #[test]
    fn mock_http_client_default() {
        let client = MockHttpClient::default();
        let responses = client.responses.read().unwrap();
        assert_eq!(responses.len(), 0);
    }

    // Test get_response for URL not in responses map (returns 404)
    #[tokio::test]
    async fn mock_http_client_get_unknown_url() {
        let client = MockHttpClient::new();
        let response = client.get("http://unknown.com/test").await.unwrap();

        assert_eq!(response.status, 404);
        assert_eq!(response.body, b"Not Found");
        assert_eq!(response.content_length, None);
        assert!(!response.supports_range);
    }

    // Test overwriting a response for the same URL
    #[tokio::test]
    async fn mock_http_client_overwrite_response() {
        let client = MockHttpClient::new();

        // Add initial response
        client.add_response(
            "http://test.com/api",
            HttpResponse {
                status: 200,
                body: b"first response".to_vec(),
                content_length: Some(14),
                supports_range: false,
            },
        );

        // Verify initial response
        let response = client.get("http://test.com/api").await.unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"first response");

        // Overwrite with new response
        client.add_response(
            "http://test.com/api",
            HttpResponse {
                status: 201,
                body: b"second response".to_vec(),
                content_length: Some(15),
                supports_range: true,
            },
        );

        // Verify overwritten response
        let response = client.get("http://test.com/api").await.unwrap();
        assert_eq!(response.status, 201);
        assert_eq!(response.body, b"second response");
        assert_eq!(response.content_length, Some(15));
        assert!(response.supports_range);
    }

    // Test add_error functionality
    #[tokio::test]
    async fn mock_http_client_add_error() {
        let client = MockHttpClient::new();

        client.add_error("http://error.com/fail", "Connection timeout");

        let result = client.get("http://error.com/fail").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Connection timeout");
    }

    // Test all HttpClient trait methods use get_response internally
    #[tokio::test]
    async fn mock_http_client_all_methods_use_get_response() {
        let client = MockHttpClient::new();
        let test_url = "http://test.com/endpoint";

        client.add_response(
            test_url,
            HttpResponse {
                status: 200,
                body: b"mock response".to_vec(),
                content_length: Some(13),
                supports_range: true,
            },
        );

        // Test get
        let response = client.get(test_url).await.unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"mock response");

        // Test get_range
        let response = client.get_range(test_url, 0, Some(100)).await.unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"mock response");

        // Test post
        let response = client.post(test_url, "{}").await.unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"mock response");

        // Test post_form
        let form_data = vec![("key".to_string(), "value".to_string())];
        let response = client.post_form(test_url, &form_data).await.unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"mock response");
    }
}
