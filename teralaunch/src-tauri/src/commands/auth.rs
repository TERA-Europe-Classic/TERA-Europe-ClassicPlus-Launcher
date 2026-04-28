//! Authentication-related Tauri commands
//!
//! This module contains commands for user authentication including:
//! - Login (v100 single-POST flow)
//! - Registration
//! - Logout
//! - Auth info management

use log::info;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, ORIGIN};
use serde::Deserialize;
use serde_json::json;
use zeroize::Zeroizing;

use crate::domain::{CONNECT_TIMEOUT_SECS, DOWNLOAD_TIMEOUT_SECS};
use crate::infrastructure::{HttpClient, ReqwestClient};
use crate::services::auth_service;
use crate::state::{
    clear_auth_client, clear_auth_info, get_auth_client, set_auth_client,
    set_auth_info as set_auth_state,
};
use crate::GameState;
use teralib::config::get_config_value;

const CLASSICPLUS_WEBSITE_BASE_URL: &str = "https://tera-europe-classic.com";

#[derive(Debug, Deserialize)]
struct WebsiteSessionResponse {
    #[serde(rename = "csrfToken")]
    csrf_token: String,
}

#[derive(Debug, Deserialize)]
struct ConsentResponse {
    ok: bool,
    consent: Option<bool>,
}

fn classicplus_website_base_url() -> String {
    std::env::var("CLASSICPLUS_WEBSITE_BASE_URL")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| CLASSICPLUS_WEBSITE_BASE_URL.to_string())
}

async fn fetch_tester_csrf(client: &reqwest::Client, base_url: &str) -> Result<String, String> {
    let response = client
        .get(format!("{base_url}/api/tester/auth/session"))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch tester session: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Tester session request failed with status {}",
            response.status()
        ));
    }

    let session = response
        .json::<WebsiteSessionResponse>()
        .await
        .map_err(|e| format!("Failed to parse tester session response: {e}"))?;

    if session.csrf_token.trim().is_empty() {
        return Err("Tester session response did not include a CSRF token".to_string());
    }

    Ok(session.csrf_token)
}

fn csrf_headers(base_url: &str, csrf_token: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        ORIGIN,
        HeaderValue::from_str(base_url).map_err(|e| format!("Invalid origin header: {e}"))?,
    );
    headers.insert(
        "x-csrf-token",
        HeaderValue::from_str(csrf_token).map_err(|e| format!("Invalid CSRF header: {e}"))?,
    );
    Ok(headers)
}

async fn ensure_tester_website_session(
    client: &reqwest::Client,
    username: &str,
    password: &str,
    base_url: &str,
) -> Result<String, String> {
    let csrf_token = fetch_tester_csrf(client, base_url).await?;
    let password = Zeroizing::new(password.to_string());
    let payload = json!({
        "login": username,
        "password": password.as_str(),
    });

    let response = client
        .post(format!("{base_url}/api/tester/auth/login/start"))
        .headers(csrf_headers(base_url, &csrf_token)?)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to establish tester website session: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Tester website login failed with status {}",
            response.status()
        ));
    }

    fetch_tester_csrf(client, base_url).await
}

async fn fetch_leaderboard_consent_with_client(
    client: &reqwest::Client,
    base_url: &str,
) -> Result<ConsentResponse, String> {
    let response = client
        .get(format!("{base_url}/api/tester/auth/settings/consent"))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch leaderboard consent: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Leaderboard consent request failed with status {}",
            response.status()
        ));
    }

    response
        .json::<ConsentResponse>()
        .await
        .map_err(|e| format!("Failed to parse leaderboard consent response: {e}"))
}

async fn update_leaderboard_consent_with_client(
    client: &reqwest::Client,
    base_url: &str,
    csrf_token: &str,
    consent: bool,
) -> Result<ConsentResponse, String> {
    let response = client
        .post(format!("{base_url}/api/tester/auth/settings/consent"))
        .headers(csrf_headers(base_url, csrf_token)?)
        .json(&json!({ "consent": consent }))
        .send()
        .await
        .map_err(|e| format!("Failed to update leaderboard consent: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Leaderboard consent update failed with status {}",
            response.status()
        ));
    }

    response
        .json::<ConsentResponse>()
        .await
        .map_err(|e| format!("Failed to parse leaderboard consent update response: {e}"))
}

/// Inner testable login function using the v100 single-POST API.
///
/// Sends credentials as JSON to the login endpoint. The v100 API returns all
/// fields (AuthKey, UserNo, CharacterCount, Permission, Privilege, UserName)
/// in a single response, replacing the old 4-step cookie chain.
///
/// # Arguments
/// * `client` - The HTTP client to use for requests
/// * `username` - The user's account name
/// * `password` - The user's password
/// * `login_url` - URL for the v100 login endpoint
///
/// # Returns
/// JSON string containing auth info on success, or error message on failure
async fn login_with_client<H: HttpClient>(
    client: &H,
    username: String,
    password: String,
    login_url: &str,
) -> Result<String, String> {
    // Wrap the password the moment we cross into this fn so the buffer is
    // zeroed on drop — whether we return early on validation failure or fall
    // through to the HTTP request. (PRD 3.1.7.zeroize-audit)
    let password = Zeroizing::new(password);
    if auth_service::validate_credentials(&username, &password).is_err() {
        return Err("Username and password cannot be empty".to_string());
    }

    // Build JSON payload for v100 API
    let payload = json!({
        "login": username,
        "password": password.as_str()
    });

    // Single POST request to v100 login endpoint
    let login_res = client.post(login_url, &payload.to_string()).await?;

    if !login_res.is_success() {
        return Err(format!(
            "Login request failed with status {}",
            login_res.status
        ));
    }

    let response_text = login_res.text().map_err(|e| e.to_string())?;

    // Parse the v100 response which contains all fields in one payload
    let result =
        auth_service::parse_v100_login_response(&response_text).map_err(|e| e.to_string())?;

    Ok(auth_service::serialize_login_result(&result))
}

/// Authenticates a user with the game server using the v100 API.
///
/// Sends a single JSON POST with credentials and receives all auth fields
/// in one response.
///
/// # Arguments
/// * `username` - The user's account name
/// * `password` - The user's password
///
/// # Returns
/// JSON string containing auth info on success, or error message on failure
#[cfg(not(tarpaulin_include))]
#[tauri::command]
pub async fn login(username: String, password: String) -> Result<String, String> {
    // The v100 API runs over plain HTTP — use the http-allowed client here only.
    let client = ReqwestClient::with_http_allowed(DOWNLOAD_TIMEOUT_SECS, CONNECT_TIMEOUT_SECS)?;

    let login_url = get_config_value("LOGIN_ACTION_URL");

    let result = login_with_client(&client, username, password, &login_url).await;

    // Store the authenticated client for potential future session-based calls
    if result.is_ok() {
        set_auth_client(client.inner());
    }

    result
}

/// Inner testable registration function that accepts an HttpClient implementation.
///
/// # Arguments
/// * `client` - The HTTP client to use for requests
/// * `login` - The desired username
/// * `email` - The user's email address
/// * `password` - The desired password
/// * `register_url` - URL for registration endpoint
///
/// # Returns
/// Response from the registration endpoint
async fn register_with_client<H: HttpClient>(
    client: &H,
    login: String,
    email: String,
    password: String,
    register_url: &str,
) -> Result<String, String> {
    // PRD 3.1.7.zeroize-audit: zeroize password buffer on drop, regardless
    // of which branch we take.
    let password = Zeroizing::new(password);
    if auth_service::validate_registration(&login, &email, &password).is_err() {
        return Err("All fields must be provided".to_string());
    }

    // Build JSON body for registration
    let payload = json!({
        "login": login,
        "email": email,
        "password": password.as_str()
    });

    let res = client.post(register_url, &payload.to_string()).await?;

    let text = res.text().map_err(|e| e.to_string())?;

    if res.is_success() {
        Ok(text)
    } else {
        Err(text)
    }
}

/// Registers a new user account.
///
/// # Arguments
/// * `login` - The desired username
/// * `email` - The user's email address
/// * `password` - The desired password
///
/// # Returns
/// Response from the registration endpoint
#[cfg(not(tarpaulin_include))]
#[tauri::command]
pub async fn register_new_account(
    login: String,
    email: String,
    password: String,
) -> Result<String, String> {
    // Registration hits the same HTTP API endpoint.
    let client = ReqwestClient::with_http_allowed(DOWNLOAD_TIMEOUT_SECS, CONNECT_TIMEOUT_SECS)?;
    let register_url = get_config_value("REGISTER_ACTION_URL");

    register_with_client(&client, login, email, password, &register_url).await
}

/// Sets authentication info received from the frontend.
///
/// Stores the auth credentials in global state for later use during game launch.
///
/// # Arguments
/// * `auth_key` - The authentication key from login
/// * `user_name` - The user's account name
/// * `user_no` - The user's account ID
/// * `character_count` - The user's character count
#[cfg(not(tarpaulin_include))]
#[tauri::command]
pub fn set_auth_info(auth_key: String, user_name: String, user_no: i32, character_count: String) {
    set_auth_state(crate::domain::GlobalAuthInfo {
        auth_key: auth_key.clone(),
        user_name: user_name.clone(),
        user_no,
        character_count: character_count.clone(),
    });

    info!("Auth info set from frontend:");
    info!("User Name: {}", user_name);
    info!("User No: {}", user_no);
    info!("Character Count: {}", character_count);
    info!("Auth Key: [REDACTED]");
}

/// Logs out the current user.
///
/// Resets the launch state and clears authentication information.
///
/// # Arguments
/// * `state` - The game state containing launch flag
#[cfg(not(tarpaulin_include))]
#[tauri::command]
pub async fn handle_logout(_state: tauri::State<'_, GameState>) -> Result<(), String> {
    clear_auth_info();
    clear_auth_client();

    Ok(())
}

/// Checks if an authenticated session exists.
///
/// # Returns
/// true if there's an active session, false otherwise
#[cfg(not(tarpaulin_include))]
#[tauri::command]
pub fn has_auth_session() -> bool {
    get_auth_client().is_some()
}

#[cfg(not(tarpaulin_include))]
#[tauri::command]
pub async fn get_leaderboard_consent(username: String, password: String) -> Result<String, String> {
    if username.trim().is_empty() || password.trim().is_empty() {
        return Err("Stored launcher credentials are required for consent checks".to_string());
    }

    let client = ReqwestClient::with_defaults(DOWNLOAD_TIMEOUT_SECS, CONNECT_TIMEOUT_SECS)?;
    let client = client.inner();
    let base_url = classicplus_website_base_url();
    ensure_tester_website_session(&client, &username, &password, &base_url).await?;
    let response = fetch_leaderboard_consent_with_client(&client, &base_url).await?;

    Ok(json!({ "ok": response.ok, "consent": response.consent }).to_string())
}

#[cfg(not(tarpaulin_include))]
#[tauri::command]
pub async fn set_leaderboard_consent(
    username: String,
    password: String,
    consent: bool,
) -> Result<String, String> {
    if username.trim().is_empty() || password.trim().is_empty() {
        return Err("Stored launcher credentials are required for consent updates".to_string());
    }

    let client = ReqwestClient::with_defaults(DOWNLOAD_TIMEOUT_SECS, CONNECT_TIMEOUT_SECS)?;
    let client = client.inner();
    let base_url = classicplus_website_base_url();
    let csrf_token =
        ensure_tester_website_session(&client, &username, &password, &base_url).await?;
    let response =
        update_leaderboard_consent_with_client(&client, &base_url, &csrf_token, consent).await?;

    Ok(json!({ "ok": response.ok, "consent": response.consent }).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::{HttpResponse, MockHttpClient};
    use serde_json::Value;

    const TEST_LOGIN_URL: &str = "http://test.server/login";
    const TEST_REGISTER_URL: &str = "http://test.server/register";

    // === Login Validation Tests ===

    #[tokio::test]
    async fn test_login_with_empty_username() {
        let mock = MockHttpClient::new();
        let result =
            login_with_client(&mock, "".to_string(), "pass".to_string(), TEST_LOGIN_URL).await;
        assert_eq!(result.unwrap_err(), "Username and password cannot be empty");
    }

    #[tokio::test]
    async fn test_login_with_empty_password() {
        let mock = MockHttpClient::new();
        let result =
            login_with_client(&mock, "user".to_string(), "".to_string(), TEST_LOGIN_URL).await;
        assert_eq!(result.unwrap_err(), "Username and password cannot be empty");
    }

    #[tokio::test]
    async fn test_login_with_both_empty() {
        let mock = MockHttpClient::new();
        let result = login_with_client(&mock, "".to_string(), "".to_string(), TEST_LOGIN_URL).await;
        assert_eq!(result.unwrap_err(), "Username and password cannot be empty");
    }

    // === Login Success Tests ===

    #[tokio::test]
    async fn test_login_success() {
        let mock = MockHttpClient::new();

        // v100 API returns all fields in a single response
        mock.add_response(
            TEST_LOGIN_URL,
            HttpResponse {
                status: 200,
                body: br#"{"Return":true,"ReturnCode":0,"Msg":"success","CharacterCount":"5","Permission":1,"Privilege":0,"UserNo":12345,"UserName":"TestUser","AuthKey":"test-auth-key-123"}"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let result = login_with_client(
            &mock,
            "testuser".to_string(),
            "testpass".to_string(),
            TEST_LOGIN_URL,
        )
        .await;

        assert!(result.is_ok());
        let json_result: Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(json_result["Return"]["UserName"], "TestUser");
        assert_eq!(json_result["Return"]["UserNo"], 12345);
        assert_eq!(json_result["Return"]["AuthKey"], "test-auth-key-123");
        assert_eq!(json_result["Return"]["CharacterCount"], "5");
        assert_eq!(json_result["Return"]["Permission"], 1);
        assert_eq!(json_result["Return"]["Privilege"], 0);
        assert_eq!(json_result["Return"]["Region"], "EU");
        assert_eq!(json_result["Return"]["Banned"], false);
        assert_eq!(json_result["Msg"], "success");
    }

    #[tokio::test]
    async fn test_login_success_with_character_count_format() {
        let mock = MockHttpClient::new();

        // v100 returns CharacterCount in "0||" format
        mock.add_response(
            TEST_LOGIN_URL,
            HttpResponse {
                status: 200,
                body: br#"{"Return":true,"ReturnCode":0,"Msg":"success","CharacterCount":"0||","Permission":0,"Privilege":0,"UserNo":19,"UserName":"testclaude01","AuthKey":"550e8400-uuid"}"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let result = login_with_client(
            &mock,
            "testclaude01".to_string(),
            "Pass123!".to_string(),
            TEST_LOGIN_URL,
        )
        .await;

        assert!(result.is_ok());
        let json_result: Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(json_result["Return"]["CharacterCount"], "0||");
        assert_eq!(json_result["Return"]["UserNo"], 19);
    }

    // === Login Error Handling Tests ===

    #[tokio::test]
    async fn test_login_v100_account_not_exist() {
        let mock = MockHttpClient::new();
        mock.add_response(
            TEST_LOGIN_URL,
            HttpResponse {
                status: 200,
                body: br#"{"Return":false,"ReturnCode":50000,"Msg":"account not exist"}"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let result = login_with_client(
            &mock,
            "baduser".to_string(),
            "badpass".to_string(),
            TEST_LOGIN_URL,
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("account not exist"));
    }

    #[tokio::test]
    async fn test_login_v100_wrong_password() {
        let mock = MockHttpClient::new();
        mock.add_response(
            TEST_LOGIN_URL,
            HttpResponse {
                status: 200,
                body: br#"{"Return":false,"ReturnCode":50001,"Msg":"wrong password"}"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let result = login_with_client(
            &mock,
            "user".to_string(),
            "wrongpass".to_string(),
            TEST_LOGIN_URL,
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("wrong password"));
    }

    #[tokio::test]
    async fn test_login_server_error() {
        let mock = MockHttpClient::new();
        mock.add_response(
            TEST_LOGIN_URL,
            HttpResponse {
                status: 500,
                body: br#"Internal Server Error"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let result = login_with_client(
            &mock,
            "user".to_string(),
            "pass".to_string(),
            TEST_LOGIN_URL,
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("500"));
    }

    #[tokio::test]
    async fn test_login_network_error() {
        let mock = MockHttpClient::new();
        mock.add_error(TEST_LOGIN_URL, "Connection refused");

        let result = login_with_client(
            &mock,
            "user".to_string(),
            "pass".to_string(),
            TEST_LOGIN_URL,
        )
        .await;

        assert_eq!(result.unwrap_err(), "Connection refused");
    }

    #[tokio::test]
    async fn test_login_invalid_json_response() {
        let mock = MockHttpClient::new();
        mock.add_response(
            TEST_LOGIN_URL,
            HttpResponse {
                status: 200,
                body: br#"not valid json"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let result = login_with_client(
            &mock,
            "user".to_string(),
            "pass".to_string(),
            TEST_LOGIN_URL,
        )
        .await;

        assert!(result.is_err());
    }

    // === Registration Validation Tests ===

    #[tokio::test]
    async fn test_register_with_empty_login() {
        let mock = MockHttpClient::new();
        let result = register_with_client(
            &mock,
            "".to_string(),
            "email@test.com".to_string(),
            "pass".to_string(),
            TEST_REGISTER_URL,
        )
        .await;
        assert_eq!(result.unwrap_err(), "All fields must be provided");
    }

    #[tokio::test]
    async fn test_register_with_empty_email() {
        let mock = MockHttpClient::new();
        let result = register_with_client(
            &mock,
            "user".to_string(),
            "".to_string(),
            "pass".to_string(),
            TEST_REGISTER_URL,
        )
        .await;
        assert_eq!(result.unwrap_err(), "All fields must be provided");
    }

    #[tokio::test]
    async fn test_register_with_empty_password() {
        let mock = MockHttpClient::new();
        let result = register_with_client(
            &mock,
            "user".to_string(),
            "email@test.com".to_string(),
            "".to_string(),
            TEST_REGISTER_URL,
        )
        .await;
        assert_eq!(result.unwrap_err(), "All fields must be provided");
    }

    // === Registration Success Tests ===

    #[tokio::test]
    async fn test_register_success() {
        let mock = MockHttpClient::new();
        mock.add_response(
            TEST_REGISTER_URL,
            HttpResponse {
                status: 200,
                body: br#"{"Return":true,"ReturnCode":0,"Msg":"success","UserNo":19,"AuthKey":"uuid"}"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let result = register_with_client(
            &mock,
            "newuser".to_string(),
            "newuser@test.com".to_string(),
            "securepass".to_string(),
            TEST_REGISTER_URL,
        )
        .await;

        assert!(result.is_ok());
        assert!(result.unwrap().contains("success"));
    }

    // === Registration Error Handling Tests ===

    #[tokio::test]
    async fn test_register_user_exists() {
        let mock = MockHttpClient::new();
        mock.add_response(
            TEST_REGISTER_URL,
            HttpResponse {
                status: 409,
                body: br#"{"Msg": "Username already exists"}"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let result = register_with_client(
            &mock,
            "existinguser".to_string(),
            "existing@test.com".to_string(),
            "pass".to_string(),
            TEST_REGISTER_URL,
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Username already exists"));
    }

    #[tokio::test]
    async fn test_register_network_error() {
        let mock = MockHttpClient::new();
        mock.add_error(TEST_REGISTER_URL, "Network timeout");

        let result = register_with_client(
            &mock,
            "user".to_string(),
            "email@test.com".to_string(),
            "pass".to_string(),
            TEST_REGISTER_URL,
        )
        .await;

        assert_eq!(result.unwrap_err(), "Network timeout");
    }

    #[tokio::test]
    async fn test_register_server_error() {
        let mock = MockHttpClient::new();
        mock.add_response(
            TEST_REGISTER_URL,
            HttpResponse {
                status: 500,
                body: br#"Internal server error"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let result = register_with_client(
            &mock,
            "user".to_string(),
            "email@test.com".to_string(),
            "pass".to_string(),
            TEST_REGISTER_URL,
        )
        .await;

        assert!(result.is_err());
    }
}
