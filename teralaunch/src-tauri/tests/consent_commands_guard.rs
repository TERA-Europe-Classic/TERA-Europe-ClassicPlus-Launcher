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
    assert!(source.contains("/api/tester/auth/settings/consent"));
    assert!(source.contains("/api/tester/auth/login/start"));
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
