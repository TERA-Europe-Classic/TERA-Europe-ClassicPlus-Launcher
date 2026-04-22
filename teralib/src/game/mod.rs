// External crate imports
use crate::{
    config,
    global_credentials::{set_credentials, GLOBAL_CREDENTIALS},
};
use lazy_static::lazy_static;
use log::{error, info, Level, Metadata, Record};
use once_cell::sync::Lazy;
use reqwest;
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::{
    ffi::OsStr,
    os::windows::ffi::OsStrExt,
    process::{Command, ExitStatus},
    ptr::null_mut,
    slice,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{mpsc as other_mpsc, watch, Notify};
use winapi::{
    shared::{
        minwindef::{BOOL, DWORD, LPARAM, LRESULT, TRUE, UINT, WPARAM},
        windef::HWND,
    },
    um::{
        libloaderapi::GetModuleHandleW,
        winuser::{GetClassInfoExW, GetWindowThreadProcessId, *},
    },
};

// Constants
const WM_GAME_EXITED: u32 = WM_USER + 1;

/// Module for handling server list functionality.
///
/// This module includes the generated code from the `_serverlist_proto.rs` file,
/// which likely contains protobuf-generated structures and functions for
/// managing server list data.
mod serverlist {
    // Include the generated server list protobuf definitions. Using a
    // forward slash makes the path platform agnostic so that builds on
    // non-Windows hosts succeed as well.
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/_serverlist_proto.rs"
    ));
}
use serverlist::{server_list::ServerInfo, ServerList};

static LOG_FILE: Lazy<Mutex<Option<File>>> = Lazy::new(|| Mutex::new(None));
static LOGGING_ENABLED: AtomicBool = AtomicBool::new(false);

// Global static variables
lazy_static! {
    /// Channel sender for server list requests.
    /// Uses tokio's unbounded channel so send() is non-blocking and can be
    /// called from the synchronous wnd_proc Windows callback.
    static ref SERVER_LIST_SENDER: Mutex<Option<tokio::sync::mpsc::UnboundedSender<(WPARAM, usize)>>> = Mutex::new(None);
}

/// Handle to the game window.
///
/// This static variable holds a mutex-protected optional `SafeHWND`,
/// which represents the handle to the game window.
static WINDOW_HANDLE: Lazy<Mutex<Option<SafeHWND>>> = Lazy::new(|| Mutex::new(None));

/// Counter for generating unique window instance IDs for logging (multi-client support).
static WINDOW_INSTANCE_COUNTER: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));

/// Tracks whether the shared IPC window has been created (multi-client support).
/// The window is created once on first game launch and reused for all subsequent launches.
static IPC_WINDOW_CREATED: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

/// Sender for game status updates.
///
/// This channel sender is used to broadcast changes in the game's running state
/// to any interested receivers.
static GAME_STATUS_SENDER: Lazy<watch::Sender<bool>> = Lazy::new(|| {
    let (tx, _) = watch::channel(false);
    tx
});

/// Tracks whether an exit/crash event (1020/1021) has already been signaled for the current run.
static EXIT_EVENT_SENT: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

// Mirror integration removed: no event broadcast or PID helpers.

// Struct definitions
/// A thread-safe wrapper around a Windows window handle (HWND).
///
/// # Safety
///
/// This type implements `Send` and `Sync` despite wrapping a raw pointer because:
///
/// 1. The HWND is only used with `PostMessageW`, which is documented by Microsoft
///    as safe to call from any thread (it posts to the window's message queue
///    asynchronously without requiring thread affinity).
///
/// 2. The handle is stored in `WINDOW_HANDLE` (a `Mutex<Option<SafeHWND>>`) and
///    only accessed to post `WM_GAME_EXITED` messages in `launch_game()`. All access
///    is protected by the mutex, ensuring no data races.
///
/// 3. The window itself is created and its message loop runs on a dedicated
///    `spawn_blocking` thread in `create_and_run_game_window()`. The window handle
///    becomes valid only after the window is fully created in that thread.
///
/// 4. No thread-local state or TLS (thread-local storage) is associated with this handle.
///
/// # Non-Safe Usage
///
/// **WARNING**: Do not use this HWND for operations that require thread affinity
/// (e.g., `SendMessage`, `GetWindowText`, `IsWindow`, or any synchronous window
/// operations). These functions can only be safely called from the thread that
/// owns the window's message queue.
///
/// Only async/fire-and-forget operations like `PostMessageW` are guaranteed safe
/// across threads.
#[derive(Clone, Copy)]
struct SafeHWND(HWND);

// Implementations
/// SAFETY: See SafeHWND documentation. This is safe because we exclusively use
/// PostMessageW, which is documented by Microsoft as thread-safe and asynchronous.
/// The handle is protected by a Mutex<Option<SafeHWND>> in WINDOW_HANDLE.
unsafe impl Send for SafeHWND {}

/// SAFETY: See SafeHWND documentation.
unsafe impl Sync for SafeHWND {}

impl SafeHWND {
    /// Creates a new `SafeHWND` instance.
    ///
    /// This function wraps a raw `HWND` into a `SafeHWND` struct, providing a safer interface
    /// for handling window handles.
    ///
    /// # Arguments
    ///
    /// * `hwnd` - A raw window handle of type `HWND`.
    ///
    /// # Returns
    ///
    /// A new `SafeHWND` instance containing the provided window handle.
    fn new(hwnd: HWND) -> Self {
        SafeHWND(hwnd)
    }

    /// Retrieves the raw window handle.
    ///
    /// This method provides access to the underlying `HWND` stored in the `SafeHWND` instance.
    ///
    /// # Returns
    ///
    /// The raw `HWND` window handle.
    fn get(&self) -> HWND {
        self.0
    }
}

/// A custom logger for the Tera application.
///
/// This struct implements the `log::Log` trait and provides a way to send log messages
/// through a channel, allowing for asynchronous logging.
pub struct TeraLogger {
    /// The sender half of a channel for log messages.
    sender: other_mpsc::Sender<String>,
}

impl log::Log for TeraLogger {
    /// Checks if a log message with the given metadata should be recorded.
    ///
    /// This method filters log messages based on the target and log level.
    ///
    /// # Arguments
    ///
    /// * `metadata` - The metadata associated with the log record.
    ///
    /// # Returns
    ///
    /// `true` if the log message should be recorded, `false` otherwise.
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.target().starts_with("teralib") && metadata.level() <= Level::Info
    }

    /// Records a log message.
    ///
    /// If the log message is enabled based on its metadata, this method formats the message
    /// and sends it through the channel.
    ///
    /// # Arguments
    ///
    /// * `record` - The log record to be processed.
    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let log_message = format!("{} - {}", record.level(), record.args());
            let _ = self.sender.try_send(log_message.clone());
            if LOGGING_ENABLED.load(Ordering::SeqCst) {
                if let Ok(mut file_opt) = LOG_FILE.lock() {
                    if let Some(ref mut file) = *file_opt {
                        let _ = writeln!(file, "{}", log_message);
                    }
                }
            }
        }
    }

    /// Flushes any buffered records.
    ///
    /// This implementation does nothing as there is no buffering.
    fn flush(&self) {}
}

/// Sets up logging for the application.
///
/// This function initializes the global logger with an Info level filter.
/// It uses a lazy initialization pattern to ensure the logger is only set up once.
pub fn setup_logging() -> (TeraLogger, other_mpsc::Receiver<String>) {
    let (sender, receiver) = other_mpsc::channel(100);
    (TeraLogger { sender }, receiver)
}

/// Enables or disables logging to a file named `log.txt` in the application directory.
pub fn enable_file_logging(enabled: bool) -> Result<(), String> {
    LOGGING_ENABLED.store(enabled, Ordering::SeqCst);
    if enabled {
        let mut path = std::env::current_exe().map_err(|e| e.to_string())?;
        path.pop();
        let file_path = path.join("log.txt");
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)
            .map_err(|e| e.to_string())?;
        *LOG_FILE.lock().map_err(|e| e.to_string())? = Some(file);
    } else {
        *LOG_FILE.lock().map_err(|e| e.to_string())? = None;
    }
    Ok(())
}

/// Runs the game with the provided credentials and language.
///
/// This function sets the credentials, checks if the game is already running,
/// and launches the game asynchronously.
///
/// # Arguments
///
/// * `account_name` - The account name as a &str.
/// * `ticket` - The session ticket as a &str.
/// * `game_lang` - The game language as a &str.
///
/// # Returns
///
/// A Result containing the exit status of the game process or an error.
pub async fn run_game(
    account_name: &str,
    characters_count: &str,
    ticket: &str,
    game_lang: &str,
    game_path: &str,
) -> Result<ExitStatus, Box<dyn std::error::Error>> {
    info!("Starting run_game function");

    // Multi-client support: Don't block if a game is already running.
    // Per-account tracking is handled by the frontend.
    let current_count = get_running_game_count();
    if current_count > 0 {
        info!(
            "Multi-client: {} game(s) already running, launching another",
            current_count
        );
    }

    set_credentials(account_name, characters_count, ticket, game_lang, game_path);

    info!(
        "User Info - Characters_count: {}, Lang: {}, Game Path: {}",
        GLOBAL_CREDENTIALS.get_characters_count(),
        GLOBAL_CREDENTIALS.get_game_lang(),
        GLOBAL_CREDENTIALS.get_game_path()
    );
    /* ORIGINAL
    info!(
        "Set credentials - Account: {}, Characters_count: {}, Ticket: {}, Lang: {}, Game Path: {}",
        GLOBAL_CREDENTIALS.get_account_name(),
        GLOBAL_CREDENTIALS.get_characters_count(),
        GLOBAL_CREDENTIALS.get_ticket(),
        GLOBAL_CREDENTIALS.get_game_lang(),
        GLOBAL_CREDENTIALS.get_game_path()
    );
    */
    launch_game().await
}

/// Launches the game and handles the game process lifecycle.
///
/// This function spawns the game process, manages the game window, and handles
/// server list requests asynchronously.
///
/// # Returns
///
/// A Result containing the exit status of the game process or an error.
async fn launch_game() -> Result<ExitStatus, Box<dyn std::error::Error>> {
    // CRITICAL: Capture credentials IMMEDIATELY at function entry to prevent race conditions.
    // Another thread might call set_credentials() during async operations below.
    // These captured values will be used for both launching and per-PID storage.
    let creds_account_name = GLOBAL_CREDENTIALS.get_account_name();
    let creds_characters_count = GLOBAL_CREDENTIALS.get_characters_count();
    let creds_ticket = GLOBAL_CREDENTIALS.get_ticket();
    let creds_game_lang = GLOBAL_CREDENTIALS.get_game_lang();
    let creds_game_path = GLOBAL_CREDENTIALS.get_game_path();

    // Signal game is running (PID-based tracking happens after spawn)
    GAME_STATUS_SENDER.send(true).unwrap();
    let current_count = get_running_game_count();
    info!(
        "Game instance starting (currently {} running)",
        current_count
    );

    // Reset exit/crash signaling state for this run
    EXIT_EVENT_SENT.store(false, Ordering::SeqCst);

    info!("Launching game for account: {}", creds_account_name);

    // Multi-client: Create IPC window only once, reuse for all subsequent game launches.
    // This ensures all games share one window for WM_COPYDATA communication.
    let mut is_first_launch = !IPC_WINDOW_CREATED.swap(true, Ordering::SeqCst);

    // Validate that window handle exists if flag says it was created
    // This handles edge case where launcher crashed/restarted while games were running
    if !is_first_launch {
        let window_valid = WINDOW_HANDLE.lock().map(|h| h.is_some()).unwrap_or(false);
        if !window_valid {
            info!("IPC window flag was set but no valid handle found - recreating window");
            is_first_launch = true;
        }
    }

    if is_first_launch {
        // Use tokio's unbounded channel for server list requests.
        // UnboundedSender::send() is synchronous and non-blocking, making it safe
        // to call from the Windows message callback (wnd_proc).
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(WPARAM, usize)>();
        *SERVER_LIST_SENDER.lock().unwrap() = Some(tx);

        let tcs = Arc::new(tokio::sync::Notify::new());
        let tcs_clone = Arc::clone(&tcs);

        // Spawn the IPC window task - it will run until all games exit.
        // We don't await it here; it runs in background serving all game instances.
        tokio::task::spawn_blocking(move || unsafe { create_and_run_game_window(tcs_clone) });

        // Spawn async task to handle server list requests off the message pump thread.
        // This ensures the Windows message loop is never blocked by network calls.
        tokio::spawn(async move {
            while let Some((w_param, sender)) = rx.recv().await {
                // Fetch server list asynchronously - no blocking, no new runtime needed
                let server_list_data = match get_server_list().await {
                    Ok(data) => {
                        info!("Server list request successful, {} bytes", data.len());
                        data
                    }
                    Err(e) => {
                        error!("Failed to get server list: {}", e);
                        continue; // Don't crash, just skip this request
                    }
                };
                // Send response back to the game client
                unsafe {
                    send_response_message(w_param, sender as HWND, 6, &server_list_data);
                }
            }
        });

        tcs.notified().await;
        info!("IPC window created (first game launch)");
    } else {
        // IPC window already exists - just verify it's ready
        info!("Reusing existing IPC window (multi-client launch)");
    }
    crate::av::ensure_av_exclusion_before_launch();

    let mut child = Command::new(&creds_game_path)
        .arg(format!("-LANGUAGEEXT={}", creds_game_lang))
        .spawn()?;

    let pid = child.id();
    info!("Game process spawned with PID: {}", pid);

    // Store captured credentials for this specific PID (multi-client support)
    // Note: This happens after spawn, but the race window is tiny (<1ms) since spawn()
    // returns immediately after process creation. The game takes much longer to
    // initialize before sending WM_COPYDATA requests (loading DLLs, creating windows).
    // Using captured values prevents race conditions with concurrent launches.
    crate::global_credentials::store_credentials_for_pid(
        pid,
        &creds_account_name,
        &creds_characters_count,
        &creds_ticket,
        &creds_game_lang,
        &creds_game_path,
    );

    let status = child.wait()?;
    info!("Game process exited with status: {:?}", status);

    // Clean up credentials for this PID (this is our source of truth for running games)
    crate::global_credentials::remove_credentials_for_pid(pid);

    // Fallback: if no WM_COPYDATA-based exit/crash (1020/1021) was received,
    // synthesize one based on process exit status
    // behave consistently on force-close/kill.
    if !EXIT_EVENT_SENT.load(Ordering::SeqCst) {
        if status.success() {
            info!("Synthesized GameExit (1020) event after process exit");
        } else {
            error!("Synthesized GameCrash (1021) event after abnormal process exit");
        }
        EXIT_EVENT_SENT.store(true, Ordering::SeqCst);
    }

    // Multi-client: Check PID map to see if all games have exited
    let remaining = get_running_game_count();
    if remaining == 0 {
        GAME_STATUS_SENDER.send(false).unwrap();
        info!("All game instances exited, status set to not running");
    } else {
        info!("Game instance exited, {} still running", remaining);
    }

    // Multi-client: Only signal window to exit when ALL games have closed.
    // If other games are still running, they still need the IPC window.
    if remaining == 0 {
        if let Ok(handle) = WINDOW_HANDLE.lock() {
            if let Some(safe_hwnd) = *handle {
                let hwnd = safe_hwnd.get();
                unsafe {
                    PostMessageW(hwnd, WM_GAME_EXITED, 0, 0);
                }
            } else {
                error!("Window handle not found when trying to post WM_GAME_EXITED message");
            }
        } else {
            error!("Failed to acquire lock on WINDOW_HANDLE");
        }
        // Reset IPC window flag so next launcher session creates a new window
        IPC_WINDOW_CREATED.store(false, Ordering::SeqCst);
    } else {
        info!(
            "Skipping WM_GAME_EXITED - {} other game(s) still using IPC window",
            remaining
        );
    }
    // Note: We no longer await the window task here. The IPC window runs in the
    // background serving all game instances until the last one exits.

    Ok(status)
}

/// Converts a Rust string slice to a null-terminated wide string (UTF-16).
///
/// This function is useful for interoperability with Windows API functions
/// that expect wide string parameters.
///
/// # Arguments
///
/// * `s` - The input string slice to convert.
///
/// # Returns
///
/// A vector of u16 values representing the wide string, including a null terminator.
fn to_wstring(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

/// Gets the PID of the process that owns a window (for multi-client credential lookup).
///
/// # Safety
/// This function calls Windows API.
unsafe fn get_pid_from_hwnd(hwnd: HWND) -> u32 {
    let mut pid: DWORD = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    pid
}

/// Returns a receiver for game status updates.
///
/// This function provides a way to subscribe to game status changes.
///
/// # Returns
///
/// A `watch::Receiver<bool>` that can be used to receive game status updates.
pub fn get_game_status_receiver() -> watch::Receiver<bool> {
    GAME_STATUS_SENDER.subscribe()
}

/// Checks if any game instance is currently running.
/// Uses the PID-based credential map as the source of truth.
///
/// # Returns
///
/// A boolean indicating whether any game is running (true) or not (false).
pub fn is_game_running() -> bool {
    crate::global_credentials::has_running_games()
}

/// Returns the number of currently running game instances.
/// Uses the PID-based credential map as the source of truth.
pub fn get_running_game_count() -> usize {
    crate::global_credentials::running_game_count()
}

/// Resets the global state of the application.
///
/// This function performs the following actions:
/// 1. Clears all per-PID game credentials (source of truth for running games).
/// 2. Sends a game status update (not running).
/// 3. Clears the stored window handle.
///
/// It's typically called when cleaning up or restarting the application state.
pub fn reset_global_state() {
    crate::global_credentials::clear_all_game_credentials();
    if let Err(e) = GAME_STATUS_SENDER.send(false) {
        error!("Failed to send game status: {:?}", e);
    }
    if let Ok(mut handle) = WINDOW_HANDLE.lock() {
        *handle = None;
    }
    info!("Global state reset completed");
}

/// Window procedure for handling Windows messages.
///
/// This function is called by the Windows operating system to process messages
/// for the application's window.
///
/// # Safety
///
/// This function is unsafe because it deals directly with raw pointers and
/// Windows API calls.
///
/// # Arguments
///
/// * `h_wnd` - The handle to the window.
/// * `msg` - The message identifier.
/// * `w_param` - Additional message information (depends on the message).
/// * `l_param` - Additional message information (depends on the message).
///
/// # Returns
///
/// The result of the message processing.
unsafe extern "system" fn wnd_proc(
    h_wnd: HWND,
    msg: UINT,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    // Note: Removed verbose message logging as it was causing file I/O
    // on every Windows message, which could delay the message pump.
    match msg {
        WM_COPYDATA => {
            let copy_data = &*(l_param as *const COPYDATASTRUCT);
            info!("Received WM_COPYDATA message");
            let event_id = copy_data.dwData;
            info!("Event ID: {}", event_id);
            let payload = if copy_data.cbData > 0 {
                slice::from_raw_parts(copy_data.lpData as *const u8, copy_data.cbData as usize)
            } else {
                &[]
            };
            let hex_payload: Vec<String> = payload.iter().map(|b| format!("{:02X}", b)).collect();
            info!("Payload (hex): {}", hex_payload.join(" "));

            match event_id {
                1 => handle_account_name_request(w_param, h_wnd),
                3 => handle_session_ticket_request(w_param, h_wnd),
                5 => {
                    // Send server list request to async handler via channel.
                    // This prevents blocking the Windows message pump during the
                    // network call (which can take up to 10 seconds).
                    if let Ok(guard) = SERVER_LIST_SENDER.lock() {
                        if let Some(ref sender) = *guard {
                            let _ = sender.send((w_param, h_wnd as usize));
                        }
                    }
                }
                7 => handle_enter_lobby_or_world(w_param, h_wnd, payload),
                // Notify subscribers of raw event 7 with payload as-is
                // (handlers below will also emit more specific events)
                1000 => handle_game_start(w_param, h_wnd, payload),
                1001..=1016 => handle_game_event(w_param, h_wnd, event_id, payload),
                1020 => handle_game_exit(w_param, h_wnd, payload),
                1021 => handle_game_crash(w_param, h_wnd, payload),
                _ => {
                    info!("Unhandled event ID: {}", event_id);
                }
            }
            1
        }
        WM_GAME_EXITED => {
            info!("Received WM_GAME_EXITED in wnd_proc");
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(h_wnd, msg, w_param, l_param),
    }
}

/// Creates and runs the game window.
///
/// This function sets up the window class, creates the window, and enters
/// the message loop for processing window messages. It also handles cleanup
/// when the window is closed.
///
/// # Safety
///
/// This function is unsafe due to its use of raw pointers and Windows API calls.
///
/// # Arguments
///
/// * `tcs` - An `Arc<Notify>` used to signal when the window has been created.
unsafe fn create_and_run_game_window(tcs: Arc<Notify>) {
    // Track this instance for multi-client support (for logging only)
    let instance_id = WINDOW_INSTANCE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let launcher_class_name = "LAUNCHER_CLASS";
    let launcher_window_title = "LAUNCHER_WINDOW";
    let class_name = to_wstring(launcher_class_name);
    let window_name = to_wstring(launcher_window_title);

    // Check if class is already registered (for multi-client support)
    let mut existing_class: WNDCLASSEXW = std::mem::zeroed();
    existing_class.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    let class_exists = GetClassInfoExW(
        GetModuleHandleW(null_mut()),
        class_name.as_ptr(),
        &mut existing_class,
    ) != 0;

    if !class_exists {
        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: GetModuleHandleW(null_mut()),
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null_mut(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: null_mut(),
        };

        let atom = RegisterClassExW(&wnd_class);
        if atom == 0 {
            error!("Failed to register window class");
            return;
        }
        info!("Registered window class LAUNCHER_CLASS");
    } else {
        info!(
            "Window class LAUNCHER_CLASS already registered, reusing (instance {})",
            instance_id
        );
    }

    // WS_EX_NOACTIVATE (0x08000000): Prevents this window from becoming the
    // foreground window or receiving activation. This is important because:
    // 1. The window is invisible and only used for WM_COPYDATA IPC with the game
    // 2. If the message pump is slow (e.g., during server list fetch), Windows
    //    might otherwise try to activate this window during focus changes
    // 3. This ensures the window never interferes with foreground window management
    let hwnd = CreateWindowExW(
        WS_EX_NOACTIVATE,
        class_name.as_ptr(),
        window_name.as_ptr(),
        0,
        0,
        0,
        0,
        0,
        null_mut(),
        null_mut(),
        GetModuleHandleW(null_mut()),
        null_mut(),
    );

    if hwnd.is_null() {
        error!("Failed to create window");
        UnregisterClassW(class_name.as_ptr(), GetModuleHandleW(null_mut()));
        return;
    }

    info!("Window created with HWND: {:?}", hwnd);

    if let Ok(mut handle) = WINDOW_HANDLE.lock() {
        handle.replace(SafeHWND::new(hwnd));
    } else {
        error!("Failed to acquire lock on WINDOW_HANDLE");
    }

    tcs.notify_one();

    let mut msg = std::mem::zeroed();
    info!("Entering message loop");
    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
        if msg.message == WM_GAME_EXITED {
            info!("Received WM_GAME_EXITED message");
            break;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    info!("Exiting message loop");

    DestroyWindow(hwnd);
    // Don't unregister class - it may be in use by other game instances (multi-client)
    // UnregisterClassW(class_name.as_ptr(), GetModuleHandleW(null_mut()));

    // Don't call reset_global_state() here - launch_game() already handles counter decrement.
    // Calling it here would reset counter to 0 even if other games are still running.
    info!("Window cleanup complete for this instance");

    // Multi-client: Don't clean up window class or enumerate/destroy windows here.
    // Other game instances may still be using the shared window class.
    // The class will be cleaned up when the launcher process exits.
}

/// Callback function for enumerating windows.
///
/// This function is called for each top-level window on the screen.
/// It checks if the window's class name matches the given class name,
/// and if so, destroys the window.
///
/// # Safety
///
/// This function is unsafe because it deals with raw window handles and
/// destroys windows, which can have system-wide effects.
///
/// # Arguments
///
/// * `hwnd` - Handle to a top-level window.
/// * `lparam` - Application-defined value given in EnumWindows.
///
/// # Returns
///
/// Returns TRUE to continue enumeration, FALSE to stop.
#[allow(dead_code)]
unsafe extern "system" fn enum_window_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let mut class_name: [u16; 256] = [0; 256];
    let len = GetClassNameW(hwnd, class_name.as_mut_ptr(), 256) as usize;
    let class_name = &class_name[..len];

    let search_class = slice::from_raw_parts(lparam as *const u16, 256);
    let search_len = search_class.iter().position(|&c| c == 0).unwrap_or(256);
    let search_class = &search_class[..search_len];

    if class_name.starts_with(search_class) {
        DestroyWindow(hwnd);
    }
    TRUE
}

/// Sends a response message to a specified recipient.
///
/// This function constructs a COPYDATASTRUCT and sends it using the SendMessageW Windows API function.
///
/// # Safety
///
/// This function is unsafe due to its use of raw pointers and Windows API calls.
///
/// # Arguments
///
/// * `recipient` - The HWND of the recipient window as a WPARAM.
/// * `sender` - The sender's window handle as a HWND.
/// * `game_event` - The event identifier as a usize.
/// * `payload` - The data payload to be sent as a slice of bytes.
unsafe fn send_response_message(
    recipient: WPARAM,
    sender: HWND,
    game_event: usize,
    payload: &[u8],
) {
    info!(
        "Sending response message - Event: {}, Payload length: {}",
        game_event,
        payload.len()
    );
    let copy_data = COPYDATASTRUCT {
        dwData: game_event,
        cbData: payload.len() as u32,
        lpData: payload.as_ptr() as *mut _,
    };
    let result = SendMessageW(
        recipient as HWND,
        WM_COPYDATA,
        sender as WPARAM,
        &copy_data as *const _ as LPARAM,
    );
    info!("SendMessageW result: {}", result);
}

/// Handles the account name request from the game client.
///
/// This function retrieves the account name and sends it back to the game client.
///
/// # Safety
///
/// This function is unsafe due to its use of raw pointers and Windows API calls.
///
/// # Arguments
///
/// * `recipient` - The HWND of the recipient window as a WPARAM.
/// * `sender` - The sender's window handle as a HWND.
unsafe fn handle_account_name_request(recipient: WPARAM, sender: HWND) {
    // Multi-client support: look up credentials by game PID
    // IMPORTANT: Do NOT fall back to GLOBAL_CREDENTIALS - it may have wrong account's data
    // due to concurrent launches or user switching accounts in the launcher UI.
    let game_pid = get_pid_from_hwnd(recipient as HWND);
    let account_name =
        if let Some(creds) = crate::global_credentials::get_credentials_for_pid(game_pid) {
            info!(
                "Account Name Request from PID {} - found per-PID credentials",
                game_pid
            );
            creds.account_name
        } else {
            error!(
                "Account Name Request from PID {} - NO credentials found! This is a bug.",
                game_pid
            );
            // Return empty string rather than potentially wrong GLOBAL_CREDENTIALS data
            String::new()
        };
    let account_name_utf16: Vec<u8> = account_name
        .encode_utf16()
        .flat_map(|c| c.to_le_bytes().to_vec())
        .collect();
    send_response_message(recipient, sender, 2, &account_name_utf16);
}

/// Handles the session ticket request from the game client.
///
/// This function retrieves the session ticket and sends it back to the game client.
///
/// # Safety
///
/// This function is unsafe due to its use of raw pointers and Windows API calls.
///
/// # Arguments
///
/// * `recipient` - The HWND of the recipient window as a WPARAM.
/// * `sender` - The sender's window handle as a HWND.
unsafe fn handle_session_ticket_request(recipient: WPARAM, sender: HWND) {
    // Multi-client support: look up credentials by game PID
    // IMPORTANT: Do NOT fall back to GLOBAL_CREDENTIALS - it may have wrong account's data
    // due to concurrent launches or user switching accounts in the launcher UI.
    let game_pid = get_pid_from_hwnd(recipient as HWND);
    let session_ticket =
        if let Some(creds) = crate::global_credentials::get_credentials_for_pid(game_pid) {
            info!(
                "Session Ticket Request from PID {} - found per-PID credentials",
                game_pid
            );
            creds.ticket
        } else {
            error!(
                "Session Ticket Request from PID {} - NO credentials found! This is a bug.",
                game_pid
            );
            // Return empty string rather than potentially wrong GLOBAL_CREDENTIALS data
            String::new()
        };
    send_response_message(recipient, sender, 4, session_ticket.as_bytes());
}

// NOTE: handle_server_list_request was removed - server list fetching is now
// handled asynchronously in the tokio::spawn task in run_game() to prevent
// blocking the Windows message pump.

/// Handles the event of entering a lobby or world.
///
/// This function processes the payload to determine if the player is entering a lobby or a specific world,
/// and sends an appropriate response.
///
/// # Safety
///
/// This function is unsafe due to its use of raw pointers and Windows API calls.
///
/// # Arguments
///
/// * `recipient` - The HWND of the recipient window as a WPARAM.
/// * `sender` - The HWND of the sender window.
/// * `payload` - The payload containing world information, if any.
unsafe fn handle_enter_lobby_or_world(recipient: WPARAM, sender: HWND, payload: &[u8]) {
    if payload.is_empty() {
        on_lobby_entered();
        send_response_message(recipient, sender, 8, &[]);
        info!("EnteredLobby (1004)");
    } else {
        let world_name = String::from_utf8_lossy(payload);
        on_world_entered(&world_name);
        send_response_message(recipient, sender, 8, payload);
    }
}

/// Handles the game start event.
///
/// This function is called when the game starts. Currently, it only logs the event.
///
/// # Safety
///
/// This function is unsafe due to its use of raw pointers, but it doesn't perform any unsafe operations.
///
/// # Arguments
///
/// * `_recipient` - The HWND of the recipient window as a WPARAM (unused).
/// * `_sender` - The HWND of the sender window (unused).
/// * `_payload` - The payload associated with the game start event (unused).
unsafe fn handle_game_start(_recipient: WPARAM, _sender: HWND, _payload: &[u8]) {
    info!("Game started");
}

/// Handles various game events.
///
/// This function is called for various game events identified by the event_id.
/// Currently, it only logs the event.
///
/// # Safety
///
/// This function is unsafe due to its use of raw pointers, but it doesn't perform any unsafe operations.
///
/// # Arguments
///
/// * `_recipient` - The HWND of the recipient window as a WPARAM (unused).
/// * `_sender` - The HWND of the sender window (unused).
/// * `event_id` - The identifier of the game event.
/// * `_payload` - The payload associated with the game event (unused).
unsafe fn handle_game_event(_recipient: WPARAM, _sender: HWND, event_id: usize, _payload: &[u8]) {
    info!("Game event {} received", event_id);
    // Don't send 1004 events here - they're already sent by handle_enter_lobby_or_world
    let _ = event_id;
}

/// Handles the game exit event.
///
/// This function is called when the game exits normally. Currently, it only logs the event.
///
/// # Safety
///
/// This function is unsafe due to its use of raw pointers, but it doesn't perform any unsafe operations.
///
/// # Arguments
///
/// * `_recipient` - The HWND of the recipient window as a WPARAM (unused).
/// * `_sender` - The HWND of the sender window (unused).
/// * `_payload` - The payload associated with the game exit event (unused).
unsafe fn handle_game_exit(_recipient: WPARAM, _sender: HWND, _payload: &[u8]) {
    info!("Game ended");
    EXIT_EVENT_SENT.store(true, Ordering::SeqCst);
}

/// Handles the game crash event.
///
/// This function is called when the game crashes. Currently, it only logs the event as an error.
///
/// # Safety
///
/// This function is unsafe due to its use of raw pointers, but it doesn't perform any unsafe operations.
///
/// # Arguments
///
/// * `_recipient` - The HWND of the recipient window as a WPARAM (unused).
/// * `_sender` - The HWND of the sender window (unused).
/// * `_payload` - The payload associated with the game crash event (unused).
unsafe fn handle_game_crash(_recipient: WPARAM, _sender: HWND, _payload: &[u8]) {
    error!("Game crash detected");
    EXIT_EVENT_SENT.store(true, Ordering::SeqCst);
}

/// Logs the event of entering the lobby.
fn on_lobby_entered() {
    info!("Entered the lobby");
}

/// Logs the event of entering a world.
///
/// # Arguments
///
/// * `world_name` - The name of the world being entered.
fn on_world_entered(world_name: &str) {
    info!("Entered the world: {}", world_name);
}

/// Asynchronously retrieves the server list.
///
/// Fetches from SERVER_LIST_URL and auto-detects format: XML (v100 API) or JSON (hosted file).
///
/// # Returns
///
/// A Result containing a Vec<u8> of the encoded server list on success, or an error on failure.
async fn get_server_list() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let url = config::get_config_value("SERVER_LIST_URL");
    info!("Fetching server list from: {}", url);
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .timeout(Duration::from_secs(10))
        .send()
        .await?;

    if !response.status().is_success() {
        error!("Unsuccessful HTTP response: {}", response.status());
        return Err(format!("Unsuccessful HTTP response: {}", response.status()).into());
    }

    let body = response.text().await?;
    let trimmed = body.trim_start();

    let server_list = if trimmed.starts_with("<?xml") || trimmed.starts_with("<serverlist") {
        info!("Server list XML received, parsing...");
        parse_server_list_xml(&body)?
    } else {
        info!("Server list JSON received, parsing...");
        let json: Value = serde_json::from_str(&body)?;
        parse_server_list_json(&json)?
    };

    info!(
        "Server list parsed successfully, {} servers total",
        server_list.servers.len()
    );

    Ok(encode_server_list_wire_compatible(&server_list))
}

fn utf16_host_bytes(address: &str, port: u32) -> Vec<u8> {
    utf16_to_bytes(&format!("{}:{}", address, port))
}

fn encode_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push(((value as u8) & 0x7F) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn encode_tag(out: &mut Vec<u8>, field_num: u32, wire_type: u8) {
    encode_varint(out, ((field_num << 3) | wire_type as u32) as u64);
}

fn encode_fixed32_field(out: &mut Vec<u8>, field_num: u32, value: u32) {
    encode_tag(out, field_num, 5);
    out.extend_from_slice(&value.to_le_bytes());
}

fn encode_bytes_field(out: &mut Vec<u8>, field_num: u32, data: &[u8]) {
    encode_tag(out, field_num, 2);
    encode_varint(out, data.len() as u64);
    out.extend_from_slice(data);
}

fn encode_server_info_wire_compatible(server: &ServerInfo) -> Vec<u8> {
    let mut out = Vec::new();
    encode_fixed32_field(&mut out, 1, server.id);
    encode_bytes_field(&mut out, 2, &server.name);
    encode_bytes_field(&mut out, 3, &server.category);
    encode_bytes_field(&mut out, 4, &server.title);
    encode_bytes_field(&mut out, 5, &server.queue);
    encode_bytes_field(&mut out, 6, &server.population);
    encode_fixed32_field(&mut out, 7, server.address);
    encode_fixed32_field(&mut out, 8, server.port);
    encode_fixed32_field(&mut out, 9, server.available);
    encode_bytes_field(&mut out, 10, &server.unavailable_message);
    encode_bytes_field(&mut out, 11, &server.host);
    out
}

fn encode_server_list_wire_compatible(server_list: &ServerList) -> Vec<u8> {
    let mut out = Vec::new();

    for server in &server_list.servers {
        let server_bytes = encode_server_info_wire_compatible(server);
        encode_tag(&mut out, 1, 2);
        encode_varint(&mut out, server_bytes.len() as u64);
        out.extend_from_slice(&server_bytes);
    }

    encode_fixed32_field(&mut out, 2, server_list.last_server_id);
    encode_fixed32_field(&mut out, 3, server_list.sort_criterion);
    out
}

/// Strips HTML tags from a string. Used to extract plain text from CDATA like
/// `<font color="#00ff00">Low</font>` → `"Low"`.
fn strip_html_tags(s: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }
    result.trim().to_string()
}

/// Parses v100 XML server list into a ServerList protobuf struct.
///
/// The v100 API (`/tera/ServerList`) returns XML. A server is considered
/// unavailable when its `server_stat` has bit 31 set (`0x80000000`).
fn parse_server_list_xml(xml: &str) -> Result<ServerList, Box<dyn std::error::Error>> {
    let doc = roxmltree::Document::parse(xml)?;

    let mut server_list = ServerList {
        servers: vec![],
        last_server_id: 0,
        sort_criterion: 2,
    };

    let credentials = GLOBAL_CREDENTIALS.get_characters_count();
    info!("Raw credentials string: {}", credentials);

    let parts: Vec<&str> = credentials.split('|').collect();
    let character_counts: std::collections::HashMap<u32, u32> = if parts.len() > 1 {
        parts[1]
            .split(',')
            .collect::<Vec<&str>>()
            .chunks(2)
            .filter_map(|chunk| {
                if chunk.len() == 2 {
                    Some((chunk[0].parse::<u32>().ok()?, chunk[1].parse::<u32>().ok()?))
                } else {
                    None
                }
            })
            .collect()
    } else {
        std::collections::HashMap::new()
    };

    let root = doc.root_element();
    for server_node in root.children().filter(|n| n.has_tag_name("server")) {
        let get_text = |tag: &str| -> String {
            server_node
                .children()
                .find(|n| n.has_tag_name(tag))
                .and_then(|n| n.text())
                .unwrap_or("")
                .trim()
                .to_string()
        };

        let id_str = get_text("id");
        let server_id: u32 = match id_str.parse() {
            Ok(v) if v != 0 => v,
            _ => {
                error!("Missing or invalid <id> for server");
                continue;
            }
        };

        let address_str = get_text("ip");
        if address_str.parse::<std::net::Ipv4Addr>().is_err() {
            error!("Invalid <ip> for server {}: {}", server_id, address_str);
            continue;
        }
        let address = ipv4_to_u32(&address_str);

        let port_str = get_text("port");
        let port: u32 = match port_str.parse::<u64>() {
            Ok(p) if p > 0 && p <= 65535 => p as u32,
            _ => {
                error!("Invalid <port> for server {}: {}", server_id, port_str);
                continue;
            }
        };

        // CDATA inside <name> — roxmltree exposes this as text()
        let name = get_text("name");
        if name.is_empty() {
            error!("Missing <name> for server {}", server_id);
            continue;
        }

        let category = get_text("category");
        let queue = get_text("crowdness");

        // <open> CDATA contains HTML like `<font color="#00ff00">Low</font>`
        let population_raw = get_text("open");
        let population = strip_html_tags(&population_raw);

        // <server_stat> is hex: 0x80000000 = offline, anything else = available
        let server_stat_str = get_text("server_stat");
        let server_stat_val =
            u64::from_str_radix(server_stat_str.trim_start_matches("0x"), 16).unwrap_or(0x80000000);
        let is_available = (server_stat_val & 0x80000000) == 0;

        let popup = get_text("popup");

        let char_count = character_counts.get(&server_id).copied().unwrap_or(0);
        let formatted_name = if char_count > 0 {
            format!("{} ({})", name, char_count)
        } else {
            name.clone()
        };

        info!(
            "XML server: id={}, name={}, ip={}, port={}, available={}",
            server_id, name, address_str, port, is_available
        );

        let server_info = ServerInfo {
            id: server_id,
            name: utf16_to_bytes(&formatted_name),
            category: utf16_to_bytes(&category),
            title: utf16_to_bytes(&name),
            queue: utf16_to_bytes(&queue),
            population: utf16_to_bytes(&population),
            address,
            port,
            available: if is_available { 1 } else { 0 },
            unavailable_message: utf16_to_bytes(&popup),
            host: utf16_host_bytes(&address_str, port),
        };
        server_list.servers.push(server_info);
    }

    // Add relay servers from config (same as JSON path)
    let relay_servers = config::get_relay_servers();
    for relay in relay_servers {
        let relay_id = relay["id"].as_u64().unwrap_or(9999) as u32;
        let relay_name = relay["name"].as_str().unwrap_or("Relay Server");
        let relay_address_str = relay["address"].as_str().unwrap_or("127.0.0.1");
        if relay_address_str.parse::<std::net::Ipv4Addr>().is_err() {
            continue;
        }
        let relay_port_raw = relay["port"].as_u64().unwrap_or(7801);
        if relay_port_raw == 0 || relay_port_raw > 65535 {
            continue;
        }
        let relay_port = relay_port_raw as u32;
        let relay_category = relay["category"].as_str().unwrap_or("Relay");
        let relay_title = relay["title"].as_str().unwrap_or("Relay Server");
        let relay_queue = relay["queue"].as_str().unwrap_or("no");
        let relay_population = relay["population"].as_str().unwrap_or("Online");
        let relay_available = relay["available"].as_u64().unwrap_or(1) != 0;
        let relay_address = ipv4_to_u32(relay_address_str);
        let relay_unavailable_message = relay["unavailable_message"].as_str().unwrap_or("");

        server_list.servers.push(ServerInfo {
            id: relay_id,
            name: utf16_to_bytes(relay_name),
            category: utf16_to_bytes(relay_category),
            title: utf16_to_bytes(relay_title),
            queue: utf16_to_bytes(relay_queue),
            population: utf16_to_bytes(relay_population),
            address: relay_address,
            port: relay_port,
            available: if relay_available { 1 } else { 0 },
            unavailable_message: utf16_to_bytes(relay_unavailable_message),
            host: utf16_host_bytes(relay_address_str, relay_port),
        });
    }

    Ok(server_list)
}

/// Parses JSON into ServerList struct.
///
/// Converts server list JSON to ServerList with error checking.
///
/// # Arguments
///
/// * `json` - Reference to serde_json::Value with server list data.
///
/// # Returns
///
/// Result<ServerList, Box<dyn std::error::Error>>:
/// - Ok(ServerList): Populated ServerList struct
/// - Err: Parsing error description
fn parse_server_list_json(json: &Value) -> Result<ServerList, Box<dyn std::error::Error>> {
    let mut server_list = ServerList {
        servers: vec![],
        last_server_id: 0,
        sort_criterion: 2,
    };

    // Parse GLOBAL_CREDENTIALS.get_characters_count()
    let credentials = GLOBAL_CREDENTIALS.get_characters_count();
    info!("Raw credentials string: {}", credentials);

    let parts: Vec<&str> = credentials.split('|').collect();

    // Parse character counts for each server
    let character_counts: std::collections::HashMap<u32, u32> = if parts.len() > 1 {
        parts[1]
            .split(',')
            .collect::<Vec<&str>>()
            .chunks(2)
            .filter_map(|chunk| {
                if chunk.len() == 2 {
                    Some((chunk[0].parse::<u32>().ok()?, chunk[1].parse::<u32>().ok()?))
                } else {
                    None
                }
            })
            .collect()
    } else {
        std::collections::HashMap::new()
    };

    info!("Parsed character counts: {:?}", character_counts);

    let servers = json["servers"]
        .as_array()
        .ok_or("No Servers found in JSON")?;
    for server in servers {
        let server_id = match server["id"].as_u64() {
            Some(id) => id as u32,
            None => {
                error!("Missing or invalid 'id' field");
                continue; // Skip invalid server
            }
        };
        if server_id == 0 {
            continue; // Skip server with ID 0
        }

        let is_available = server["available"].as_u64().unwrap_or(0) != 0;

        info!(
            "Processing server: id={}, name={}, is_available={}",
            server_id, server["name"], is_available
        );

        let name = match server["name"].as_str() {
            Some(n) => n.to_string(),
            None => {
                error!("Missing or invalid 'name' field for server {}", server_id);
                continue; // Skip invalid server
            }
        };
        let _title = server["title"].as_str().unwrap_or("").to_string();
        let category = server["category"].as_str().unwrap_or("");
        let queue = server["queue"].as_str().unwrap_or("");
        let population = server["population"].as_str().unwrap_or("");

        // Validate server address is a valid IP
        let address_str = match server["address"].as_str() {
            Some(addr) => addr,
            None => {
                error!(
                    "Missing or invalid 'address' field for server {}",
                    server_id
                );
                continue; // Skip invalid server
            }
        };
        if address_str.parse::<std::net::Ipv4Addr>().is_err() {
            error!(
                "Invalid server address format: {} for server {}",
                address_str, server_id
            );
            continue; // Skip invalid server
        }
        let address = ipv4_to_u32(address_str);

        // Validate port is in valid range
        let port_raw = match server["port"].as_u64() {
            Some(p) => p,
            None => {
                error!("Missing or invalid 'port' field for server {}", server_id);
                continue; // Skip invalid server
            }
        };
        if port_raw == 0 || port_raw > 65535 {
            error!("Invalid port number: {} for server {}", port_raw, server_id);
            continue; // Skip invalid server
        }
        let port = port_raw as u32;

        let unavailable_message = server["unavailable_message"].as_str().unwrap_or("");

        // Do not add character count suffix; keep original names exactly as provided
        let formatted_name = name.clone();
        let formatted_title = name.clone();

        let server_info = ServerInfo {
            id: server_id,
            name: utf16_to_bytes(&formatted_name),
            category: utf16_to_bytes(category),
            title: utf16_to_bytes(&formatted_title),
            queue: utf16_to_bytes(queue),
            population: utf16_to_bytes(population),
            address,
            port,
            available: if is_available { 1 } else { 0 },
            unavailable_message: utf16_to_bytes(unavailable_message),
            host: utf16_host_bytes(address_str, port),
        };
        server_list.servers.push(server_info);
    }

    // Force last_server_id = 0 so the TERA client shows the server-select
    // screen instead of auto-joining the previously used server.
    server_list.last_server_id = 0;
    server_list.sort_criterion = json["sort_criterion"].as_u64().unwrap_or(3) as u32;
    // Add configured relay servers to the server list
    let relay_servers = config::get_relay_servers();
    for relay in relay_servers {
        let relay_id = relay["id"].as_u64().unwrap_or(9999) as u32;
        let relay_name = relay["name"].as_str().unwrap_or("Relay Server");

        // Validate relay server address
        let relay_address_str = relay["address"].as_str().unwrap_or("127.0.0.1");
        if relay_address_str.parse::<std::net::Ipv4Addr>().is_err() {
            error!("Invalid relay server address format: {}", relay_address_str);
            continue; // Skip invalid relay
        }

        // Validate relay port
        let relay_port_raw = relay["port"].as_u64().unwrap_or(7801);
        if relay_port_raw == 0 || relay_port_raw > 65535 {
            error!("Invalid relay port number: {}", relay_port_raw);
            continue; // Skip invalid relay
        }
        let relay_port = relay_port_raw as u32;

        let relay_category = relay["category"].as_str().unwrap_or("Relay");
        let relay_title = relay["title"].as_str().unwrap_or("Relay Server");
        let relay_queue = relay["queue"].as_str().unwrap_or("no");
        let relay_population = relay["population"].as_str().unwrap_or("Online");
        let relay_available = relay["available"].as_u64().unwrap_or(1) != 0;
        let relay_unavailable_msg = relay["unavailable_message"].as_str().unwrap_or("");

        let relay_address = ipv4_to_u32(relay_address_str);

        let relay_server = ServerInfo {
            id: relay_id,
            name: utf16_to_bytes(relay_name),
            category: utf16_to_bytes(relay_category),
            title: utf16_to_bytes(relay_title),
            queue: utf16_to_bytes(relay_queue),
            population: utf16_to_bytes(relay_population),
            address: relay_address,
            port: relay_port,
            available: if relay_available { 1 } else { 0 },
            unavailable_message: utf16_to_bytes(relay_unavailable_msg),
            host: utf16_host_bytes(relay_address_str, relay_port),
        };

        server_list.servers.push(relay_server);
        info!(
            "Added relay server: {} ({}:{})",
            relay_name, relay_address_str, relay_port
        );
    }

    // Keep original last_server_id from JSON for proper display
    // server_list.last_server_id = json["last_server_id"].as_u64().unwrap_or(0) as u32;
    // server_list.sort_criterion = 2;

    Ok(server_list)
}

/// Converts a Rust string to UTF-16 little-endian bytes.
///
/// This function is useful for preparing strings for Windows API calls that expect UTF-16.
///
/// # Arguments
///
/// * `s` - A string slice that holds the text to be converted.
///
/// # Returns
///
/// A vector of bytes representing the UTF-16 little-endian encoded string.
fn utf16_to_bytes(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .flat_map(|c| c.to_le_bytes().to_vec())
        .collect()
}

/// Converts an IPv4 address string to a u32 representation.
///
/// # Arguments
///
/// * `ip` - A string slice that holds the IPv4 address.
///
/// # Returns
///
/// A u32 representation of the IP address, or 0 if parsing fails.
fn ipv4_to_u32(ip: &str) -> u32 {
    ip.parse::<std::net::Ipv4Addr>()
        .map(|addr| u32::from_be_bytes(addr.octets()))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_varint_for_test(data: &[u8], start: usize) -> Option<(u64, usize)> {
        let mut value = 0u64;
        let mut shift = 0u32;
        let mut i = start;

        while i < data.len() {
            let byte = data[i];
            i += 1;
            value |= ((byte & 0x7F) as u64) << shift;

            if (byte & 0x80) == 0 {
                return Some((value, i));
            }

            shift += 7;
            if shift >= 64 {
                return None;
            }
        }

        None
    }

    fn first_nested_server_bytes(data: &[u8]) -> Option<&[u8]> {
        let mut i = 0usize;

        while i < data.len() {
            let (tag, next) = decode_varint_for_test(data, i)?;
            i = next;

            let wire_type = (tag & 0x07) as u8;
            let field = (tag >> 3) as u32;

            match wire_type {
                2 => {
                    let (len, next) = decode_varint_for_test(data, i)?;
                    let len = len as usize;
                    i = next;
                    if i + len > data.len() {
                        return None;
                    }
                    if field == 1 {
                        return Some(&data[i..i + len]);
                    }
                    i += len;
                }
                5 => {
                    i += 4;
                }
                _ => return None,
            }
        }

        None
    }

    fn contains_len_field(data: &[u8], field_num: u32, expected: &[u8]) -> bool {
        let mut i = 0usize;
        let expected_tag = ((field_num << 3) | 2) as u64;

        while i < data.len() {
            let Some((tag, next)) = decode_varint_for_test(data, i) else {
                return false;
            };
            i = next;

            let wire_type = (tag & 0x07) as u8;

            match wire_type {
                2 => {
                    let Some((len, next)) = decode_varint_for_test(data, i) else {
                        return false;
                    };
                    let len = len as usize;
                    i = next;
                    if i + len > data.len() {
                        return false;
                    }
                    if tag == expected_tag && &data[i..i + len] == expected {
                        return true;
                    }
                    i += len;
                }
                5 => {
                    i += 4;
                }
                _ => return false,
            }
        }

        false
    }

    #[test]
    fn test_server_list_encoder_emits_empty_required_style_fields() {
        let server = ServerInfo {
            id: 1,
            name: utf16_to_bytes("Classic+"),
            category: Vec::new(),
            title: utf16_to_bytes("Classic+"),
            queue: Vec::new(),
            population: Vec::new(),
            address: ipv4_to_u32("127.0.0.1"),
            port: 7801,
            available: 1,
            unavailable_message: Vec::new(),
            host: utf16_to_bytes("127.0.0.1:7801"),
        };
        let list = ServerList {
            servers: vec![server],
            last_server_id: 0,
            sort_criterion: 2,
        };

        let encoded = encode_server_list_wire_compatible(&list);
        let server_bytes = first_nested_server_bytes(&encoded).expect("server entry must be present");

        assert!(contains_len_field(server_bytes, 3, &[]), "category must be emitted even when empty");
        assert!(contains_len_field(server_bytes, 5, &[]), "queue must be emitted even when empty");
        assert!(contains_len_field(server_bytes, 6, &[]), "population must be emitted even when empty");
        assert!(contains_len_field(server_bytes, 10, &[]), "unavailable_message must be emitted even when empty");
    }

    #[test]
    fn test_server_list_encoder_emits_host_as_ip_port_utf16() {
        let host = utf16_to_bytes("127.0.0.1:7801");
        let server = ServerInfo {
            id: 1,
            name: utf16_to_bytes("Classic+"),
            category: utf16_to_bytes("PvE"),
            title: utf16_to_bytes(&format!("Classic+{}", "x".repeat(80))),
            queue: utf16_to_bytes(""),
            population: utf16_to_bytes("Online"),
            address: ipv4_to_u32("127.0.0.1"),
            port: 7801,
            available: 1,
            unavailable_message: Vec::new(),
            host: host.clone(),
        };
        let list = ServerList {
            servers: vec![server],
            last_server_id: 0,
            sort_criterion: 2,
        };

        let encoded = encode_server_list_wire_compatible(&list);
        let server_bytes = first_nested_server_bytes(&encoded).expect("server entry must be present");

        assert!(contains_len_field(server_bytes, 11, &host), "host must be encoded as UTF-16 ip:port bytes");
    }

    #[test]
    fn test_to_wstring_empty() {
        let result = to_wstring("");
        assert_eq!(
            result,
            vec![0u16],
            "Empty string should produce only null terminator"
        );
    }

    #[test]
    fn test_to_wstring_ascii() {
        let result = to_wstring("Hello");
        let expected: Vec<u16> = vec![
            'H' as u16, 'e' as u16, 'l' as u16, 'l' as u16, 'o' as u16,
            0u16, // null terminator
        ];
        assert_eq!(
            result, expected,
            "ASCII string should be correctly converted to wide string"
        );
    }

    #[test]
    fn test_to_wstring_unicode() {
        let result = to_wstring("Hello世界");
        // '世' = U+4E16 and '界' = U+754C
        let expected: Vec<u16> = vec![
            'H' as u16, 'e' as u16, 'l' as u16, 'l' as u16, 'o' as u16, 0x4E16, // 世
            0x754C, // 界
            0u16,   // null terminator
        ];
        assert_eq!(
            result, expected,
            "Unicode string should be correctly converted to wide string"
        );
    }

    #[test]
    fn test_to_wstring_special_chars() {
        let result = to_wstring("Test\n\t\r");
        let expected: Vec<u16> = vec![
            'T' as u16,
            'e' as u16,
            's' as u16,
            't' as u16,
            '\n' as u16,
            '\t' as u16,
            '\r' as u16,
            0u16, // null terminator
        ];
        assert_eq!(
            result, expected,
            "Special characters should be correctly converted"
        );
    }

    #[test]
    fn test_utf16_to_bytes_empty() {
        let result = utf16_to_bytes("");
        assert!(
            result.is_empty(),
            "Empty string should produce empty byte vector"
        );
    }

    #[test]
    fn test_utf16_to_bytes_ascii() {
        let result = utf16_to_bytes("Hi");
        // 'H' = 0x0048, 'i' = 0x0069 in little-endian
        let expected: Vec<u8> = vec![0x48, 0x00, 0x69, 0x00];
        assert_eq!(
            result, expected,
            "ASCII string should be correctly converted to UTF-16 LE bytes"
        );
    }

    #[test]
    fn test_utf16_to_bytes_unicode() {
        let result = utf16_to_bytes("世界");
        // '世' = U+4E16, '界' = U+754C in little-endian
        let expected: Vec<u8> = vec![0x16, 0x4E, 0x4C, 0x75];
        assert_eq!(
            result, expected,
            "Unicode string should be correctly converted to UTF-16 LE bytes"
        );
    }

    #[test]
    fn test_utf16_to_bytes_mixed() {
        let result = utf16_to_bytes("A世");
        // 'A' = 0x0041, '世' = U+4E16 in little-endian
        let expected: Vec<u8> = vec![0x41, 0x00, 0x16, 0x4E];
        assert_eq!(
            result, expected,
            "Mixed ASCII and Unicode should be correctly converted"
        );
    }

    #[test]
    fn test_ipv4_to_u32_valid() {
        // 192.168.1.1 = 0xC0A80101 in network byte order
        let result = ipv4_to_u32("192.168.1.1");
        assert_eq!(result, 0xC0A80101, "Valid IP should be correctly converted");
    }

    #[test]
    fn test_ipv4_to_u32_localhost() {
        // 127.0.0.1 = 0x7F000001 in network byte order
        let result = ipv4_to_u32("127.0.0.1");
        assert_eq!(
            result, 0x7F000001,
            "Localhost IP should be correctly converted"
        );
    }

    #[test]
    fn test_ipv4_to_u32_zero() {
        // 0.0.0.0 = 0x00000000 in network byte order
        let result = ipv4_to_u32("0.0.0.0");
        assert_eq!(result, 0x00000000, "Zero IP should be correctly converted");
    }

    #[test]
    fn test_ipv4_to_u32_max() {
        // 255.255.255.255 = 0xFFFFFFFF in network byte order
        let result = ipv4_to_u32("255.255.255.255");
        assert_eq!(result, 0xFFFFFFFF, "Max IP should be correctly converted");
    }

    #[test]
    fn test_ipv4_to_u32_invalid_format() {
        let result = ipv4_to_u32("not.an.ip.address");
        assert_eq!(result, 0, "Invalid IP format should return 0");
    }

    #[test]
    fn test_ipv4_to_u32_invalid_octets() {
        let result = ipv4_to_u32("256.256.256.256");
        assert_eq!(result, 0, "Invalid octets should return 0");
    }

    #[test]
    fn test_ipv4_to_u32_incomplete() {
        let result = ipv4_to_u32("192.168.1");
        assert_eq!(result, 0, "Incomplete IP address should return 0");
    }

    #[test]
    fn test_ipv4_to_u32_extra_octets() {
        let result = ipv4_to_u32("192.168.1.1.1");
        assert_eq!(result, 0, "IP with extra octets should return 0");
    }

    #[test]
    fn test_ipv4_to_u32_negative() {
        let result = ipv4_to_u32("-1.0.0.1");
        assert_eq!(result, 0, "IP with negative octets should return 0");
    }

    #[test]
    fn test_ipv4_to_u32_empty() {
        let result = ipv4_to_u32("");
        assert_eq!(result, 0, "Empty string should return 0");
    }

    #[test]
    fn test_ipv4_to_u32_whitespace() {
        let result = ipv4_to_u32("192.168.1.1 ");
        assert_eq!(result, 0, "IP with trailing whitespace should return 0");
    }

    #[test]
    fn test_ipv4_to_u32_various_valid() {
        let test_cases = vec![
            ("10.0.0.1", 0x0A000001),
            ("172.16.0.1", 0xAC100001),
            ("8.8.8.8", 0x08080808),
            ("1.2.3.4", 0x01020304),
        ];

        for (ip, expected) in test_cases {
            let result = ipv4_to_u32(ip);
            assert_eq!(
                result, expected,
                "IP {} should convert to 0x{:08X}",
                ip, expected
            );
        }
    }
}
