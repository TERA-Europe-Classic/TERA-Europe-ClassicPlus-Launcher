use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn auth_commands_define_leaderboard_consent_commands() {
    let source = fs::read_to_string(repo_root().join("src/commands/auth.rs"))
        .expect("auth commands source should be readable");

    assert!(source.contains("pub async fn get_leaderboard_consent"));
    assert!(source.contains("pub async fn set_leaderboard_consent"));
    assert!(source.contains("LOGIN_ACTION_URL"));
    assert!(source.contains("GET_ACCOUNT_INFO_URL"));
    assert!(source.contains("SetAccountInfoByUserNo"));
}

#[test]
fn consent_commands_do_not_use_classic_website_session() {
    let source = fs::read_to_string(repo_root().join("src/commands/auth.rs"))
        .expect("auth commands source should be readable");

    assert!(!source.contains("/api/tester/auth/settings/consent"));
    assert!(!source.contains("/api/tester/auth/login/start"));
    assert!(!source.contains("CLASSICPLUS_WEBSITE_BASE_URL"));
}

#[test]
fn consent_commands_use_v100_http_client() {
    let source = fs::read_to_string(repo_root().join("src/commands/auth.rs"))
        .expect("auth commands source should be readable");

    let get_section = source
        .split("pub async fn get_leaderboard_consent")
        .nth(1)
        .and_then(|section| section.split("pub async fn set_leaderboard_consent").next())
        .expect("get consent command section should exist");
    let set_section = source
        .split("pub async fn set_leaderboard_consent")
        .nth(1)
        .and_then(|section| section.split("#[cfg(test)]").next())
        .expect("set consent command section should exist");

    assert!(get_section.contains("ReqwestClient::with_http_allowed"));
    assert!(set_section.contains("ReqwestClient::with_http_allowed"));
}

#[test]
fn consent_commands_authenticate_against_v100_account_info() {
    let source = fs::read_to_string(repo_root().join("src/commands/auth.rs"))
        .expect("auth commands source should be readable");

    let get_section = source
        .split("pub async fn get_leaderboard_consent")
        .nth(1)
        .and_then(|section| section.split("pub async fn set_leaderboard_consent").next())
        .expect("get consent command section should exist");
    let set_section = source
        .split("pub async fn set_leaderboard_consent")
        .nth(1)
        .and_then(|section| section.split("#[cfg(test)]").next())
        .expect("set consent command section should exist");

    assert!(get_section.contains("login_for_account_info"));
    assert!(set_section.contains("login_for_account_info"));
    assert!(set_section.contains("set_account_info_url"));
    assert!(set_section.contains("leaderboardConsent"));
}

#[test]
fn main_registers_leaderboard_consent_commands() {
    let source = fs::read_to_string(repo_root().join("src/main.rs"))
        .expect("main source should be readable");

    assert!(source.contains("commands::auth::get_leaderboard_consent"));
    assert!(source.contains("commands::auth::set_leaderboard_consent"));
}

#[test]
fn command_catalog_lists_leaderboard_consent_commands() {
    let source = fs::read_to_string(repo_root().join("src/commands/mod.rs"))
        .expect("commands module source should be readable");

    assert!(source.contains("get_leaderboard_consent"));
    assert!(source.contains("set_leaderboard_consent"));
}
