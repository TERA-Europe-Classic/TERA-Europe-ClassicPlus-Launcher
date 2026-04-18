#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! TERA Launcher - Main Entry Point
//!
//! This is the entry point for the TERA launcher application.
//! All Tauri commands are organized in the `commands` module.

use std::sync::Arc;

use dotenvy::dotenv;
use log::{error, info, LevelFilter};
use tauri::Manager;
use tokio::sync::Mutex;

use state::set_pending_deep_link;

// Local modules
mod commands;
mod domain;
mod infrastructure;
mod services;
mod state;
mod utils;

// Re-export GameState for use by command modules
pub use game_state::GameState;

mod game_state {
    use std::sync::Arc;
    use tokio::sync::{watch, Mutex};

    /// Holds game running state for Tauri managed state.
    /// Game running status is tracked by teralib via PID-based credentials map.
    pub struct GameState {
        pub status_receiver: Arc<Mutex<watch::Receiver<bool>>>,
    }
}

/// Registers the `teraclassicplus://` custom URL protocol handler on Windows.
///
/// Writes registry keys under `HKCU\Software\Classes\teraclassicplus` so that
/// when the OS encounters a `teraclassicplus://` URL, it launches this executable
/// with the URL as a command-line argument.
///
/// This is idempotent — safe to call on every startup.
#[cfg(target_os = "windows")]
fn register_deep_link_protocol() {
    use winreg::enums::*;
    use winreg::RegKey;

    let exe_path = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => {
            error!(
                "Failed to get current exe path for deep link registration: {}",
                e
            );
            return;
        }
    };

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // Create or open HKCU\Software\Classes\teraclassicplus
    let (key, _) = match hkcu.create_subkey("Software\\Classes\\teraclassicplus") {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to create registry key for deep link: {}", e);
            return;
        }
    };

    // Set the default value and URL Protocol marker
    let _ = key.set_value("", &"URL:TERA Classic+ Launcher");
    let _ = key.set_value("URL Protocol", &"");

    // Create shell\open\command subkey with the exe path
    match key.create_subkey("shell\\open\\command") {
        Ok((cmd_key, _)) => {
            let command = format!("\"{}\" \"%1\"", exe_path);
            let _ = cmd_key.set_value("", &command);
            info!("Registered teraclassicplus:// protocol handler");
        }
        Err(e) => {
            error!("Failed to create command registry key: {}", e);
        }
    }
}

/// Checks if auto-update is enabled via environment variable.
fn should_auto_install_updater() -> bool {
    matches!(
        std::env::var("TERA_LAUNCHER_AUTO_UPDATE").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

#[cfg(not(tarpaulin_include))]
fn main() {
    dotenv().ok();

    // Windows: relaunch elevated via UAC using ShellExecute with "runas" verb.
    // This shows proper UAC dialog and admin shield icon without command prompt flash.
    #[cfg(all(target_os = "windows", not(debug_assertions)))]
    {
        use std::ffi::CString;
        use std::ptr;
        use winapi::um::shellapi::ShellExecuteA;
        use winapi::um::winuser::SW_SHOWNORMAL;

        // If the special flag is not present, relaunch self elevated and append it.
        let is_guard_present = std::env::args().any(|a| a == "--elevated");
        if !is_guard_present {
            if let Ok(current_exe) = std::env::current_exe() {
                // Preserve original args and append our guard flag
                let mut args: Vec<String> = std::env::args().skip(1).collect();
                args.push("--elevated".to_string());
                let args_str = args.join(" ");

                // Convert to CString for Windows API
                let exe_path = CString::new(current_exe.to_string_lossy().as_ref())
                    .expect("Executable path contains null bytes");
                let parameters = CString::new(args_str).expect("Arguments contain null bytes");
                let verb =
                    CString::new("runas").expect("runas verb contains null bytes - this is a bug");

                unsafe {
                    let result = ShellExecuteA(
                        ptr::null_mut(),
                        verb.as_ptr(),
                        exe_path.as_ptr(),
                        parameters.as_ptr(),
                        ptr::null(),
                        SW_SHOWNORMAL,
                    );

                    // ShellExecute returns > 32 on success
                    if result as i32 > 32 {
                        std::process::exit(0);
                    }
                }
            }
        }
    }

    let (tera_logger, _tera_log_receiver) = teralib::setup_logging();

    // Configure only the teralib logger
    log::set_boxed_logger(Box::new(tera_logger)).expect("Failed to set logger");
    log::set_max_level(LevelFilter::Info);

    let game_status_receiver = teralib::get_game_status_receiver();
    let game_state = GameState {
        status_receiver: Arc::new(Mutex::new(game_status_receiver)),
    };

    // Register teraclassicplus:// protocol handler on Windows (idempotent).
    #[cfg(target_os = "windows")]
    register_deep_link_protocol();

    // Check CLI args for deep link URL (Windows passes deep link as argument).
    // When the OS opens `teraclassicplus://auth?token=...`, it launches the exe
    // with the URL as a command-line argument.
    for arg in std::env::args().skip(1) {
        if arg.starts_with("teraclassicplus://") {
            info!("Deep link received via CLI arg: teraclassicplus://...");
            set_pending_deep_link(arg);
            break;
        }
    }

    tauri::Builder::default()
        .manage(game_state)
        .setup(|app| {
            let window = app
                .get_window("main")
                .expect("Main window not found - check tauri.conf.json");
            info!("Tauri setup started");

            // Keep window hidden until updater check completes (when auto-install is enabled).
            let _ = window.hide();

            let app_handle_for_update = app.handle();
            tauri::async_runtime::spawn(async move {
                if should_auto_install_updater() {
                    let mut should_show_window = true;
                    match app_handle_for_update.updater().check().await {
                        Ok(update) => {
                            if update.is_update_available() {
                                match update.download_and_install().await {
                                    Ok(_status) => {
                                        // On success the process may exit/restart
                                        should_show_window = false;
                                    }
                                    Err(e) => {
                                        error!("Updater failed: {}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to check updates: {}", e);
                        }
                    }

                    if should_show_window {
                        if let Some(win) = app_handle_for_update.get_window("main") {
                            let _ = win.show();
                        }
                    }
                } else if let Some(win) = app_handle_for_update.get_window("main") {
                    let _ = win.show();
                }
            });

            info!("Tauri setup completed");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Auth commands
            commands::auth::login,
            commands::auth::register_new_account,
            commands::auth::set_auth_info,
            commands::auth::handle_logout,
            commands::auth::has_auth_session,
            // Config commands
            commands::config::select_game_folder,
            commands::config::get_game_path_from_config,
            commands::config::save_game_path_to_config,
            commands::config::get_language_from_config,
            commands::config::save_language_to_config,
            commands::config::get_game_folder_state,
            // Download commands
            commands::download::download_all_files,
            commands::download::cancel_downloads,
            commands::download::get_downloaded_bytes,
            commands::download::reset_download_state,
            // Game commands
            commands::game::handle_launch_game,
            commands::game::get_game_status,
            commands::game::get_running_game_count,
            commands::game::reset_launch_state,
            // Hash commands
            commands::hash::get_files_to_update,
            commands::hash::check_update_required,
            commands::hash::generate_hash_file,
            commands::hash::clear_cache,
            // Util commands
            commands::util::is_debug,
            commands::util::set_logging,
            commands::util::update_launcher,
            commands::util::check_server_connection,
            commands::util::fetch_player_count,
            commands::util::fetch_news_feed,
            // Mods commands
            commands::mods::list_installed_mods,
            commands::mods::get_mods_catalog,
            commands::mods::install_mod,
            commands::mods::uninstall_mod,
            commands::mods::enable_mod,
            commands::mods::disable_mod,
            commands::mods::launch_external_app,
            commands::mods::stop_external_app,
            commands::mods::open_mods_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn auto_install_updater_disabled_by_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("TERA_LAUNCHER_AUTO_UPDATE");
        assert!(!should_auto_install_updater());
    }

    #[test]
    fn auto_install_updater_enabled_with_env_var() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("TERA_LAUNCHER_AUTO_UPDATE", "true");
        assert!(should_auto_install_updater());
        std::env::remove_var("TERA_LAUNCHER_AUTO_UPDATE");
    }

    #[test]
    fn refactor_wiring_map_exists() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let map_path = manifest_dir
            .join("..")
            .join("..")
            .join("docs")
            .join("plans")
            .join("2026-01-27-refactor-wiring-map.md");
        assert!(
            map_path.exists(),
            "Refactor wiring map missing at {}",
            map_path.display()
        );
    }
}
