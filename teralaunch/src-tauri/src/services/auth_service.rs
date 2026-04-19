//! Authentication service for user login and session management.
//!
//! This module provides pure functions for authentication operations:
//! - Credential validation
//! - Login response parsing
//! - Session management

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Maximum username length
const MAX_USERNAME_LENGTH: usize = 100;
/// Maximum password length
const MAX_PASSWORD_LENGTH: usize = 256;

/// Result of a login attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoginResult {
    pub auth_key: String,
    pub user_name: String,
    pub user_no: i64,
    pub character_count: String,
    pub permission: i64,
    pub privilege: i64,
    pub region: String,
    pub banned: bool,
    /// Leaderboard consent status: true (agreed), false (disagreed), None (not set)
    pub leaderboard_consent: Option<bool>,
}

/// Error types for authentication operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// Credentials are empty or invalid format
    InvalidCredentials(String),
    /// Server returned an error
    ServerError(String),
    /// Failed to parse response
    ParseError(String),
    /// Network error
    NetworkError(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::InvalidCredentials(msg) => write!(f, "Invalid credentials: {}", msg),
            AuthError::ServerError(msg) => write!(f, "Server error: {}", msg),
            AuthError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            AuthError::NetworkError(msg) => write!(f, "Network error: {}", msg),
        }
    }
}

/// Validates login credentials before sending to server.
///
/// Checks that username and password are not empty and meet basic requirements.
///
/// # Arguments
/// * `username` - The username to validate
/// * `password` - The password to validate
///
/// # Returns
/// * `Ok(())` - If credentials are valid
/// * `Err(AuthError)` - If validation fails
///
/// # Examples
/// ```ignore
/// assert!(validate_credentials("user", "pass").is_ok());
/// assert!(validate_credentials("", "pass").is_err());
/// assert!(validate_credentials("user", "").is_err());
/// ```
pub fn validate_credentials(username: &str, password: &str) -> Result<(), AuthError> {
    // Trim whitespace
    let username = username.trim();
    let password = password.trim();

    if username.is_empty() {
        return Err(AuthError::InvalidCredentials(
            "Username cannot be empty".to_string(),
        ));
    }
    if password.is_empty() {
        return Err(AuthError::InvalidCredentials(
            "Password cannot be empty".to_string(),
        ));
    }
    if username.len() > MAX_USERNAME_LENGTH {
        return Err(AuthError::InvalidCredentials(format!(
            "Username too long (max {} characters)",
            MAX_USERNAME_LENGTH
        )));
    }
    if password.len() > MAX_PASSWORD_LENGTH {
        return Err(AuthError::InvalidCredentials(format!(
            "Password too long (max {} characters)",
            MAX_PASSWORD_LENGTH
        )));
    }
    Ok(())
}

/// Validates email format (basic validation, server will do full validation).
///
/// Checks for:
/// - Exactly one @ symbol
/// - Non-empty local part (before @)
/// - Domain with at least one dot
/// - Domain doesn't start or end with dot
/// - No consecutive dots in domain
///
/// # Arguments
/// * `email` - The email address to validate
///
/// # Returns
/// `true` if the email format is valid, `false` otherwise
fn validate_email_format(email: &str) -> bool {
    let email = email.trim();

    // Must have exactly one @
    let at_count = email.chars().filter(|c| *c == '@').count();
    if at_count != 1 {
        return false;
    }

    // Split at @
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 {
        return false;
    }

    let local = parts[0];
    let domain = parts[1];

    // Local part must not be empty
    if local.is_empty() {
        return false;
    }

    // Domain must have at least one dot, not start/end with dot
    if !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.') {
        return false;
    }

    // Domain parts must not be empty (no consecutive dots)
    if domain.split('.').any(|p| p.is_empty()) {
        return false;
    }

    true
}

/// Validates registration fields.
///
/// # Arguments
/// * `login` - The login/username
/// * `email` - The email address
/// * `password` - The password
///
/// # Returns
/// * `Ok(())` - If all fields are valid
/// * `Err(AuthError)` - If validation fails
pub fn validate_registration(login: &str, email: &str, password: &str) -> Result<(), AuthError> {
    if login.is_empty() {
        return Err(AuthError::InvalidCredentials(
            "Login cannot be empty".to_string(),
        ));
    }
    if email.is_empty() {
        return Err(AuthError::InvalidCredentials(
            "Email cannot be empty".to_string(),
        ));
    }
    if password.is_empty() {
        return Err(AuthError::InvalidCredentials(
            "Password cannot be empty".to_string(),
        ));
    }
    // Improved email format check
    if !validate_email_format(email) {
        return Err(AuthError::InvalidCredentials(
            "Invalid email format".to_string(),
        ));
    }
    Ok(())
}

/// Parses a v100 API login response into a LoginResult.
///
/// The v100 API returns all fields in a single JSON response:
/// `{"Return":true,"ReturnCode":0,"Msg":"success","CharacterCount":"0||",
///   "Permission":0,"Privilege":0,"UserNo":19,"UserName":"user","AuthKey":"uuid"}`
///
/// On failure: `{"Return":false,"ReturnCode":50000,"Msg":"account not exist"}`
///
/// # Arguments
/// * `response` - The raw JSON response string from the v100 login endpoint
///
/// # Returns
/// * `Ok(LoginResult)` - Parsed login result with all fields populated
/// * `Err(AuthError)` - If the response indicates failure or cannot be parsed
pub fn parse_v100_login_response(response: &str) -> Result<LoginResult, AuthError> {
    let json: Value =
        serde_json::from_str(response).map_err(|e| AuthError::ParseError(e.to_string()))?;

    let success = json["Return"]
        .as_bool()
        .ok_or_else(|| AuthError::ParseError("Missing 'Return' boolean field".to_string()))?;

    if !success {
        let msg = json["Msg"].as_str().unwrap_or("Unknown error");
        return Err(AuthError::ServerError(msg.to_string()));
    }

    let user_no = json["UserNo"]
        .as_i64()
        .ok_or_else(|| AuthError::ParseError("Missing 'UserNo' field".to_string()))?;

    let user_name = json["UserName"]
        .as_str()
        .ok_or_else(|| AuthError::ParseError("Missing 'UserName' field".to_string()))?
        .to_string();

    let auth_key = json["AuthKey"]
        .as_str()
        .ok_or_else(|| AuthError::ParseError("Missing 'AuthKey' field".to_string()))?
        .to_string();

    let character_count = json["CharacterCount"].as_str().unwrap_or("0").to_string();

    let permission = json["Permission"].as_i64().unwrap_or(0);
    let privilege = json["Privilege"].as_i64().unwrap_or(0);

    Ok(LoginResult {
        auth_key,
        user_name,
        user_no,
        character_count,
        permission,
        privilege,
        region: "EU".to_string(),
        banned: false,
        leaderboard_consent: None,
    })
}

/// Serializes a login result to JSON string for frontend consumption.
///
/// # Arguments
/// * `result` - The login result to serialize
///
/// # Returns
/// JSON string in the expected format for the frontend
pub fn serialize_login_result(result: &LoginResult) -> String {
    let json = serde_json::json!({
        "Return": {
            "AuthKey": result.auth_key,
            "UserName": result.user_name,
            "UserNo": result.user_no,
            "CharacterCount": result.character_count,
            "Permission": result.permission,
            "Privilege": result.privilege,
            "Region": result.region,
            "Banned": result.banned,
            "LeaderboardConsent": result.leaderboard_consent
        },
        "Msg": "success"
    });
    json.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Tests for validate_credentials
    // ========================================================================

    #[test]
    fn validate_credentials_valid() {
        assert!(validate_credentials("user", "pass").is_ok());
    }

    #[test]
    fn validate_credentials_empty_username() {
        let result = validate_credentials("", "pass");
        assert!(matches!(result, Err(AuthError::InvalidCredentials(_))));
        assert!(result.unwrap_err().to_string().contains("Username"));
    }

    #[test]
    fn validate_credentials_empty_password() {
        let result = validate_credentials("user", "");
        assert!(matches!(result, Err(AuthError::InvalidCredentials(_))));
        assert!(result.unwrap_err().to_string().contains("Password"));
    }

    #[test]
    fn validate_credentials_both_empty() {
        // Should fail on username first
        let result = validate_credentials("", "");
        assert!(matches!(result, Err(AuthError::InvalidCredentials(_))));
    }

    #[test]
    fn validate_credentials_whitespace_username() {
        // Whitespace-only should fail after trimming
        let result = validate_credentials("  ", "pass");
        assert!(matches!(result, Err(AuthError::InvalidCredentials(_))));
        assert!(result.unwrap_err().to_string().contains("Username"));
    }

    #[test]
    fn validate_credentials_whitespace_password() {
        // Whitespace-only password should fail after trimming
        let result = validate_credentials("user", "  ");
        assert!(matches!(result, Err(AuthError::InvalidCredentials(_))));
        assert!(result.unwrap_err().to_string().contains("Password"));
    }

    #[test]
    fn validate_credentials_username_too_long() {
        let long_username = "a".repeat(MAX_USERNAME_LENGTH + 1);
        let result = validate_credentials(&long_username, "pass");
        assert!(matches!(result, Err(AuthError::InvalidCredentials(_))));
        assert!(result.unwrap_err().to_string().contains("too long"));
    }

    #[test]
    fn validate_credentials_password_too_long() {
        let long_password = "p".repeat(MAX_PASSWORD_LENGTH + 1);
        let result = validate_credentials("user", &long_password);
        assert!(matches!(result, Err(AuthError::InvalidCredentials(_))));
        assert!(result.unwrap_err().to_string().contains("too long"));
    }

    #[test]
    fn validate_credentials_max_length_valid() {
        let max_username = "u".repeat(MAX_USERNAME_LENGTH);
        let max_password = "p".repeat(MAX_PASSWORD_LENGTH);
        assert!(validate_credentials(&max_username, &max_password).is_ok());
    }

    #[test]
    fn validate_credentials_trimming() {
        // Should trim leading/trailing whitespace and accept valid credentials
        assert!(validate_credentials("  user  ", "  pass  ").is_ok());
    }

    // ========================================================================
    // Tests for validate_registration
    // ========================================================================

    #[test]
    fn validate_registration_valid() {
        assert!(validate_registration("user", "user@example.com", "pass123").is_ok());
    }

    #[test]
    fn validate_registration_empty_login() {
        let result = validate_registration("", "user@example.com", "pass");
        assert!(matches!(result, Err(AuthError::InvalidCredentials(_))));
    }

    #[test]
    fn validate_registration_empty_email() {
        let result = validate_registration("user", "", "pass");
        assert!(matches!(result, Err(AuthError::InvalidCredentials(_))));
    }

    #[test]
    fn validate_registration_empty_password() {
        let result = validate_registration("user", "user@example.com", "");
        assert!(matches!(result, Err(AuthError::InvalidCredentials(_))));
    }

    #[test]
    fn validate_registration_invalid_email_no_at() {
        let result = validate_registration("user", "userexample.com", "pass");
        assert!(matches!(result, Err(AuthError::InvalidCredentials(_))));
        assert!(result.unwrap_err().to_string().contains("email"));
    }

    #[test]
    fn validate_registration_invalid_email_no_dot() {
        let result = validate_registration("user", "user@example", "pass");
        assert!(matches!(result, Err(AuthError::InvalidCredentials(_))));
    }

    // ========================================================================
    // Tests for validate_email_format
    // ========================================================================

    #[test]
    fn validate_email_format_valid_simple() {
        assert!(validate_email_format("user@example.com"));
    }

    #[test]
    fn validate_email_format_valid_with_numbers() {
        assert!(validate_email_format("user123@example456.com"));
    }

    #[test]
    fn validate_email_format_valid_with_plus() {
        assert!(validate_email_format("user+tag@example.com"));
    }

    #[test]
    fn validate_email_format_valid_with_dots() {
        assert!(validate_email_format("first.last@example.com"));
    }

    #[test]
    fn validate_email_format_valid_subdomain() {
        assert!(validate_email_format("user@mail.example.com"));
    }

    #[test]
    fn validate_email_format_valid_multiple_subdomains() {
        assert!(validate_email_format("user@mail.internal.example.co.uk"));
    }

    #[test]
    fn validate_email_format_valid_with_whitespace() {
        // Should trim before validation
        assert!(validate_email_format("  user@example.com  "));
    }

    #[test]
    fn validate_email_format_invalid_no_at() {
        assert!(!validate_email_format("userexample.com"));
    }

    #[test]
    fn validate_email_format_invalid_multiple_at() {
        assert!(!validate_email_format("user@@example.com"));
        assert!(!validate_email_format("user@exam@ple.com"));
    }

    #[test]
    fn validate_email_format_invalid_no_dot_in_domain() {
        assert!(!validate_email_format("user@example"));
    }

    #[test]
    fn validate_email_format_invalid_empty_local() {
        assert!(!validate_email_format("@example.com"));
    }

    #[test]
    fn validate_email_format_invalid_domain_starts_with_dot() {
        assert!(!validate_email_format("user@.example.com"));
    }

    #[test]
    fn validate_email_format_invalid_domain_ends_with_dot() {
        assert!(!validate_email_format("user@example.com."));
    }

    #[test]
    fn validate_email_format_invalid_consecutive_dots() {
        assert!(!validate_email_format("user@example..com"));
    }

    #[test]
    fn validate_email_format_invalid_consecutive_dots_multiple() {
        assert!(!validate_email_format("user@ex..am..ple.com"));
    }

    #[test]
    fn validate_email_format_invalid_domain_only_dot() {
        assert!(!validate_email_format("user@."));
    }

    #[test]
    fn validate_email_format_invalid_empty_email() {
        assert!(!validate_email_format(""));
    }

    #[test]
    fn validate_email_format_invalid_only_at() {
        assert!(!validate_email_format("@"));
    }

    #[test]
    fn validate_email_format_invalid_old_style_cases() {
        // Cases that old validation would incorrectly accept
        assert!(!validate_email_format("a@b.")); // Old validation would accept
        assert!(!validate_email_format(".@.")); // Old validation would accept
        assert!(!validate_email_format("@.com")); // Old validation would accept
    }

    #[test]
    fn validate_email_format_invalid_single_char_domain_part() {
        // Actually valid - single character domain parts are technically OK
        assert!(validate_email_format("user@a.b"));
    }

    #[test]
    fn validate_email_format_valid_special_chars_in_local() {
        assert!(validate_email_format("user+tag@example.com"));
        assert!(validate_email_format("user_name@example.com"));
        assert!(validate_email_format("user-name@example.com"));
    }

    // ========================================================================
    // Tests for serialize_login_result
    // ========================================================================

    #[test]
    fn serialize_login_result_contains_expected_fields() {
        let result = LoginResult {
            auth_key: "testkey".to_string(),
            user_name: "TestUser".to_string(),
            user_no: 42,
            character_count: "5".to_string(),
            permission: 10,
            privilege: 20,
            region: "EU".to_string(),
            banned: false,
            leaderboard_consent: Some(true),
        };
        let json = serialize_login_result(&result);

        // Parse back to verify
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["Return"]["AuthKey"], "testkey");
        assert_eq!(parsed["Return"]["UserName"], "TestUser");
        assert_eq!(parsed["Return"]["UserNo"], 42);
        assert_eq!(parsed["Return"]["CharacterCount"], "5");
        assert_eq!(parsed["Return"]["Permission"], 10);
        assert_eq!(parsed["Return"]["Privilege"], 20);
        assert_eq!(parsed["Return"]["Region"], "EU");
        assert_eq!(parsed["Return"]["Banned"], false);
        assert_eq!(parsed["Msg"], "success");
    }

    #[test]
    fn serialize_login_result_banned_user() {
        let result = LoginResult {
            auth_key: "key".to_string(),
            user_name: "BannedUser".to_string(),
            user_no: 1,
            character_count: "0".to_string(),
            permission: 0,
            privilege: 0,
            region: "EU".to_string(),
            banned: true,
            leaderboard_consent: None,
        };
        let json = serialize_login_result(&result);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["Return"]["Banned"], true);
    }

    // ========================================================================
    // Tests for parse_v100_login_response
    // ========================================================================

    #[test]
    fn parse_v100_login_response_success() {
        let response = r#"{"Return":true,"ReturnCode":0,"Msg":"success","CharacterCount":"0||","Permission":0,"Privilege":0,"UserNo":19,"UserName":"testclaude01","AuthKey":"550e8400-e29b-41d4-a716-446655440000"}"#;
        let result = parse_v100_login_response(response);
        assert!(result.is_ok());
        let login = result.unwrap();
        assert_eq!(login.user_no, 19);
        assert_eq!(login.user_name, "testclaude01");
        assert_eq!(login.auth_key, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(login.character_count, "0||");
        assert_eq!(login.permission, 0);
        assert_eq!(login.privilege, 0);
        assert_eq!(login.region, "EU");
        assert!(!login.banned);
        assert_eq!(login.leaderboard_consent, None);
    }

    #[test]
    fn parse_v100_login_response_failure_account_not_exist() {
        let response = r#"{"Return":false,"ReturnCode":50000,"Msg":"account not exist"}"#;
        let result = parse_v100_login_response(response);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AuthError::ServerError(_)));
        assert!(err.to_string().contains("account not exist"));
    }

    #[test]
    fn parse_v100_login_response_failure_wrong_password() {
        let response = r#"{"Return":false,"ReturnCode":50001,"Msg":"wrong password"}"#;
        let result = parse_v100_login_response(response);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("wrong password"));
    }

    #[test]
    fn parse_v100_login_response_missing_return_field() {
        let response = r#"{"ReturnCode":0,"Msg":"success","UserNo":19}"#;
        let result = parse_v100_login_response(response);
        assert!(matches!(result, Err(AuthError::ParseError(_))));
    }

    #[test]
    fn parse_v100_login_response_missing_user_no() {
        let response =
            r#"{"Return":true,"ReturnCode":0,"Msg":"success","UserName":"test","AuthKey":"key"}"#;
        let result = parse_v100_login_response(response);
        assert!(matches!(result, Err(AuthError::ParseError(_))));
    }

    #[test]
    fn parse_v100_login_response_missing_auth_key() {
        let response =
            r#"{"Return":true,"ReturnCode":0,"Msg":"success","UserNo":19,"UserName":"test"}"#;
        let result = parse_v100_login_response(response);
        assert!(matches!(result, Err(AuthError::ParseError(_))));
    }

    #[test]
    fn parse_v100_login_response_missing_user_name() {
        let response =
            r#"{"Return":true,"ReturnCode":0,"Msg":"success","UserNo":19,"AuthKey":"key"}"#;
        let result = parse_v100_login_response(response);
        assert!(matches!(result, Err(AuthError::ParseError(_))));
    }

    #[test]
    fn parse_v100_login_response_invalid_json() {
        let response = "not valid json";
        let result = parse_v100_login_response(response);
        assert!(matches!(result, Err(AuthError::ParseError(_))));
    }

    #[test]
    fn parse_v100_login_response_empty_string() {
        let response = "";
        let result = parse_v100_login_response(response);
        assert!(matches!(result, Err(AuthError::ParseError(_))));
    }

    #[test]
    fn parse_v100_login_response_defaults_for_optional_fields() {
        // CharacterCount, Permission, Privilege missing — should use defaults
        let response = r#"{"Return":true,"ReturnCode":0,"Msg":"success","UserNo":1,"UserName":"u","AuthKey":"k"}"#;
        let result = parse_v100_login_response(response).unwrap();
        assert_eq!(result.character_count, "0");
        assert_eq!(result.permission, 0);
        assert_eq!(result.privilege, 0);
    }

    #[test]
    fn parse_v100_login_response_with_nonzero_permission() {
        let response = r#"{"Return":true,"ReturnCode":0,"Msg":"success","CharacterCount":"3||","Permission":5,"Privilege":10,"UserNo":42,"UserName":"admin","AuthKey":"abc-123"}"#;
        let result = parse_v100_login_response(response).unwrap();
        assert_eq!(result.permission, 5);
        assert_eq!(result.privilege, 10);
        assert_eq!(result.character_count, "3||");
    }

    #[test]
    fn parse_v100_login_response_false_without_msg_uses_unknown() {
        let response = r#"{"Return":false,"ReturnCode":99999}"#;
        let result = parse_v100_login_response(response);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown error"));
    }

    // ========================================================================
    // Tests for AuthError Display
    // ========================================================================

    #[test]
    fn auth_error_display() {
        let err = AuthError::InvalidCredentials("test".to_string());
        assert!(err.to_string().contains("Invalid credentials"));

        let err = AuthError::ServerError("test".to_string());
        assert!(err.to_string().contains("Server error"));

        let err = AuthError::ParseError("test".to_string());
        assert!(err.to_string().contains("Parse error"));

        let err = AuthError::NetworkError("test".to_string());
        assert!(err.to_string().contains("Network error"));
    }
}
