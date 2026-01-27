//! Authentication-related Tauri commands
//!
//! This module contains commands for user authentication including:
//! - Login
//! - Registration
//! - Logout
//! - Auth info management

use log::info;
use serde_json::json;

use crate::domain::{CONNECT_TIMEOUT_SECS, DOWNLOAD_TIMEOUT_SECS};
use crate::infrastructure::{HttpClient, ReqwestClient};
use crate::services::auth_service;
use crate::state::{clear_auth_info, set_auth_info as set_auth_state};
use crate::GameState;
use teralib::config::get_config_value;

/// Inner testable login function that accepts an HttpClient implementation.
///
/// Performs the full login flow:
/// 1. Sends credentials to login endpoint
/// 2. Retrieves account info
/// 3. Gets auth key for game launch
/// 4. Gets character count
///
/// # Arguments
/// * `client` - The HTTP client to use for requests
/// * `username` - The user's account name
/// * `password` - The user's password
/// * `login_url` - URL for login endpoint
/// * `account_info_url` - URL for account info endpoint
/// * `auth_key_url` - URL for auth key endpoint
/// * `character_count_url` - URL for character count endpoint
///
/// # Returns
/// JSON string containing auth info on success, or error message on failure
async fn login_with_client<H: HttpClient>(
    client: &H,
    username: String,
    password: String,
    login_url: &str,
    account_info_url: &str,
    auth_key_url: &str,
    character_count_url: &str,
) -> Result<String, String> {
    if auth_service::validate_credentials(&username, &password).is_err() {
        return Err("Username and password cannot be empty".to_string());
    }

    // Prepare login payload
    let form_data = vec![
        ("login".to_string(), username.clone()),
        ("password".to_string(), password),
    ];

    // Send login request
    let login_res = client.post_form(login_url, &form_data).await?;

    if login_res.status == 401 || login_res.status == 403 {
        return Err("INVALID_CREDENTIALS".to_string());
    }

    if !login_res.is_success() {
        return Err(format!(
            "Login request failed with status {}",
            login_res.status
        ));
    }

    let login_text = login_res.text().map_err(|e| e.to_string())?;
    let login_status =
        auth_service::parse_login_response(&login_text).map_err(|e| e.to_string())?;

    if !auth_service::is_login_successful(&login_status) {
        return Err(login_status);
    }

    let account_info_res = client.get(account_info_url).await?;
    let account_info_text = account_info_res.text().map_err(|e| e.to_string())?;
    let (user_no, permission, user_name) =
        auth_service::parse_account_info(&account_info_text).map_err(|e| e.to_string())?;
    let (privilege, region, banned) =
        auth_service::parse_account_extras(&account_info_text).map_err(|e| e.to_string())?;

    let auth_key_res = client.get(auth_key_url).await?;
    let auth_key_text = auth_key_res.text().map_err(|e| e.to_string())?;
    let auth_key = auth_service::parse_auth_key(&auth_key_text).map_err(|e| e.to_string())?;

    let character_count_res = client.get(character_count_url).await?;
    let character_count_text = character_count_res.text().map_err(|e| e.to_string())?;
    let character_count =
        auth_service::parse_character_count(&character_count_text).map_err(|e| e.to_string())?;

    let result = auth_service::build_login_result(
        &login_status,
        user_no,
        permission,
        user_name,
        auth_key,
        character_count,
        privilege,
        region,
        banned,
    )
    .map_err(|e| e.to_string())?;

    Ok(auth_service::serialize_login_result(&result))
}

/// Authenticates a user with the game server.
///
/// Performs the full login flow:
/// 1. Sends credentials to login endpoint
/// 2. Retrieves account info
/// 3. Gets auth key for game launch
/// 4. Gets character count
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
    let client = ReqwestClient::with_defaults(DOWNLOAD_TIMEOUT_SECS, CONNECT_TIMEOUT_SECS)?;

    // Get URLs from configuration
    let login_url = get_config_value("LOGIN_ACTION_URL");
    let account_info_url = get_config_value("GET_ACCOUNT_INFO_URL");
    let auth_key_url = get_config_value("GET_AUTH_KEY_URL");
    let character_count_url = get_config_value("GET_CHARACTER_COUNT_URL");

    login_with_client(
        &client,
        username,
        password,
        &login_url,
        &account_info_url,
        &auth_key_url,
        &character_count_url,
    )
    .await
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
    if auth_service::validate_registration(&login, &email, &password).is_err() {
        return Err("All fields must be provided".to_string());
    }

    // Build JSON body for registration
    let payload = json!({
        "login": login,
        "email": email,
        "password": password
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
    let client = ReqwestClient::with_defaults(DOWNLOAD_TIMEOUT_SECS, CONNECT_TIMEOUT_SECS)?;
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

    // Log auth info received from frontend
    info!("Auth info set from frontend:");
    info!("User Name: {}", user_name);
    info!("User No: {}", user_no);
    info!("Character Count: {}", character_count);
    info!("Auth Key: {}", auth_key);
}

/// Logs out the current user.
///
/// Resets the launch state and clears authentication information.
///
/// # Arguments
/// * `state` - The game state containing launch flag
#[cfg(not(tarpaulin_include))]
#[tauri::command]
pub async fn handle_logout(state: tauri::State<'_, GameState>) -> Result<(), String> {
    let mut is_launching = state.is_launching.lock().await;
    *is_launching = false;

    // Reset global authentication information
    clear_auth_info();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::{HttpResponse, MockHttpClient};
    use serde_json::Value;

    // Test URL constants for mocking
    const TEST_LOGIN_URL: &str = "http://test.server/login";
    const TEST_ACCOUNT_INFO_URL: &str = "http://test.server/account";
    const TEST_AUTH_KEY_URL: &str = "http://test.server/authkey";
    const TEST_CHARACTER_COUNT_URL: &str = "http://test.server/charcount";
    const TEST_REGISTER_URL: &str = "http://test.server/register";

    // === Login Validation Tests ===

    #[tokio::test]
    async fn test_login_with_empty_username() {
        let mock = MockHttpClient::new();
        let result = login_with_client(
            &mock,
            "".to_string(),
            "pass".to_string(),
            TEST_LOGIN_URL,
            TEST_ACCOUNT_INFO_URL,
            TEST_AUTH_KEY_URL,
            TEST_CHARACTER_COUNT_URL,
        )
        .await;
        assert_eq!(result.unwrap_err(), "Username and password cannot be empty");
    }

    #[tokio::test]
    async fn test_login_with_empty_password() {
        let mock = MockHttpClient::new();
        let result = login_with_client(
            &mock,
            "user".to_string(),
            "".to_string(),
            TEST_LOGIN_URL,
            TEST_ACCOUNT_INFO_URL,
            TEST_AUTH_KEY_URL,
            TEST_CHARACTER_COUNT_URL,
        )
        .await;
        assert_eq!(result.unwrap_err(), "Username and password cannot be empty");
    }

    #[tokio::test]
    async fn test_login_with_both_empty() {
        let mock = MockHttpClient::new();
        let result = login_with_client(
            &mock,
            "".to_string(),
            "".to_string(),
            TEST_LOGIN_URL,
            TEST_ACCOUNT_INFO_URL,
            TEST_AUTH_KEY_URL,
            TEST_CHARACTER_COUNT_URL,
        )
        .await;
        assert_eq!(result.unwrap_err(), "Username and password cannot be empty");
    }

    // === Login Success Tests ===

    #[tokio::test]
    async fn test_login_success() {
        let mock = MockHttpClient::new();

        // Setup login response
        mock.add_response(
            TEST_LOGIN_URL,
            HttpResponse {
                status: 200,
                body: br#"{"Msg": "Success"}"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        // Setup account info response
        mock.add_response(
            TEST_ACCOUNT_INFO_URL,
            HttpResponse {
                status: 200,
                body: br#"{"UserNo": 12345, "Permission": 1, "UserName": "TestUser", "Privilege": 0, "Region": "EU", "Banned": false}"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        // Setup auth key response
        mock.add_response(
            TEST_AUTH_KEY_URL,
            HttpResponse {
                status: 200,
                body: br#"{"AuthKey": "test-auth-key-123"}"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        // Setup character count response
        mock.add_response(
            TEST_CHARACTER_COUNT_URL,
            HttpResponse {
                status: 200,
                body: br#"{"CharacterCount": "5"}"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let result = login_with_client(
            &mock,
            "testuser".to_string(),
            "testpass".to_string(),
            TEST_LOGIN_URL,
            TEST_ACCOUNT_INFO_URL,
            TEST_AUTH_KEY_URL,
            TEST_CHARACTER_COUNT_URL,
        )
        .await;

        assert!(result.is_ok());
        let json_result: Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(json_result["Return"]["UserName"], "TestUser");
        assert_eq!(json_result["Return"]["UserNo"], 12345);
        assert_eq!(json_result["Return"]["AuthKey"], "test-auth-key-123");
        assert_eq!(json_result["Return"]["CharacterCount"], "5");
        assert_eq!(json_result["Msg"], "success");
    }

    // === Login Error Handling Tests ===

    #[tokio::test]
    async fn test_login_invalid_credentials_401() {
        let mock = MockHttpClient::new();
        mock.add_response(
            TEST_LOGIN_URL,
            HttpResponse {
                status: 401,
                body: br#"{"Msg": "Unauthorized"}"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let result = login_with_client(
            &mock,
            "baduser".to_string(),
            "badpass".to_string(),
            TEST_LOGIN_URL,
            TEST_ACCOUNT_INFO_URL,
            TEST_AUTH_KEY_URL,
            TEST_CHARACTER_COUNT_URL,
        )
        .await;

        assert_eq!(result.unwrap_err(), "INVALID_CREDENTIALS");
    }

    #[tokio::test]
    async fn test_login_invalid_credentials_403() {
        let mock = MockHttpClient::new();
        mock.add_response(
            TEST_LOGIN_URL,
            HttpResponse {
                status: 403,
                body: br#"{"Msg": "Forbidden"}"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let result = login_with_client(
            &mock,
            "baduser".to_string(),
            "badpass".to_string(),
            TEST_LOGIN_URL,
            TEST_ACCOUNT_INFO_URL,
            TEST_AUTH_KEY_URL,
            TEST_CHARACTER_COUNT_URL,
        )
        .await;

        assert_eq!(result.unwrap_err(), "INVALID_CREDENTIALS");
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
            TEST_ACCOUNT_INFO_URL,
            TEST_AUTH_KEY_URL,
            TEST_CHARACTER_COUNT_URL,
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
            TEST_ACCOUNT_INFO_URL,
            TEST_AUTH_KEY_URL,
            TEST_CHARACTER_COUNT_URL,
        )
        .await;

        assert_eq!(result.unwrap_err(), "Connection refused");
    }

    #[tokio::test]
    async fn test_login_failed_status_message() {
        let mock = MockHttpClient::new();
        mock.add_response(
            TEST_LOGIN_URL,
            HttpResponse {
                status: 200,
                body: br#"{"Msg": "Account suspended"}"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let result = login_with_client(
            &mock,
            "user".to_string(),
            "pass".to_string(),
            TEST_LOGIN_URL,
            TEST_ACCOUNT_INFO_URL,
            TEST_AUTH_KEY_URL,
            TEST_CHARACTER_COUNT_URL,
        )
        .await;

        assert_eq!(result.unwrap_err(), "Account suspended");
    }

    #[tokio::test]
    async fn test_login_missing_account_info_field() {
        let mock = MockHttpClient::new();

        mock.add_response(
            TEST_LOGIN_URL,
            HttpResponse {
                status: 200,
                body: br#"{"Msg": "Success"}"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        // Missing UserNo field
        mock.add_response(
            TEST_ACCOUNT_INFO_URL,
            HttpResponse {
                status: 200,
                body: br#"{"Permission": 1, "UserName": "TestUser"}"#.to_vec(),
                content_length: None,
                supports_range: false,
            },
        );

        let result = login_with_client(
            &mock,
            "user".to_string(),
            "pass".to_string(),
            TEST_LOGIN_URL,
            TEST_ACCOUNT_INFO_URL,
            TEST_AUTH_KEY_URL,
            TEST_CHARACTER_COUNT_URL,
        )
        .await;

        assert_eq!(result.unwrap_err(), "Failed to retrieve UserNo");
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
                body: br#"{"Msg": "Registration successful"}"#.to_vec(),
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
        assert!(result.unwrap().contains("Registration successful"));
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
