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

    /// Holds game running state for Tauri managed state
    pub struct GameState {
        pub status_receiver: Arc<Mutex<watch::Receiver<bool>>>,
        pub is_launching: Arc<Mutex<bool>>,
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
        is_launching: Arc::new(Mutex::new(false)),
    };

    tauri::Builder::default()
        .manage(game_state)
        .setup(|app| {
            let window = app
                .get_window("main")
                .expect("Main window not found - check tauri.conf.json");
            info!("Tauri setup started");

            // Ensure window stays hidden until updater check completes (if auto-install is enabled)
            let _ = window.hide();

            // Only auto-install updates when explicitly enabled via env var.
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
                            let _ = win.set_focus();
                        }
                    }
                } else if let Some(win) = app_handle_for_update.get_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
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
            // Config commands
            commands::config::select_game_folder,
            commands::config::get_game_path_from_config,
            commands::config::save_game_path_to_config,
            commands::config::get_language_from_config,
            commands::config::save_language_to_config,
            // Download commands
            commands::download::download_all_files,
            commands::download::cancel_downloads,
            commands::download::get_downloaded_bytes,
            commands::download::reset_download_state,
            // Game commands
            commands::game::handle_launch_game,
            commands::game::get_game_status,
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

    #[test]
    fn no_legacy_impls_in_main() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let main_path = manifest_dir.join("src").join("main.rs");
        let source =
            std::fs::read_to_string(&main_path).expect("Failed to read main.rs for legacy check");
        let forbidden = [
            "fn login(",
            "fn register_new_account(",
            "fn download_all_files(",
            "fn update_file(",
            "fn handle_launch_game(",
            "fn get_files_to_update(",
        ];
        for pattern in forbidden {
            assert!(
                !source.contains(pattern),
                "main.rs still contains legacy implementation: {}",
                pattern
            );
        }
    }
}
