// External crate imports
use crate::{
    config,
    global_credentials::{set_credentials, GLOBAL_CREDENTIALS},
};
use lazy_static::lazy_static;
use log::{error, info, Level, Metadata, Record};
use once_cell::sync::Lazy;
use std::fs::{File, OpenOptions};
use std::io::Write;
use prost::Message;
use reqwest;
use serde_json::Value;
use std::{
    ffi::OsStr,
    os::windows::ffi::OsStrExt,
    process::{Command, ExitStatus},
    ptr::null_mut,
    slice,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{mpsc as other_mpsc, watch, Notify};
use winapi::{
    shared::{
        minwindef::{BOOL, LPARAM, LRESULT, TRUE, UINT, WPARAM},
        windef::HWND,
    },
    um::{
        errhandlingapi::GetLastError,
        libloaderapi::GetModuleHandleW,
        winuser::{GetClassInfoExW, *},
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

/// Flag indicating whether the game is currently running.
///
/// This atomic boolean is used to track the running state of the game
/// across multiple threads.
static GAME_RUNNING: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

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

    if is_game_running() {
        return Err("Game is already running".into());
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
    if GAME_RUNNING.load(Ordering::SeqCst) {
        return Err("Game is already running".into());
    }

    GAME_RUNNING.store(true, Ordering::SeqCst);
    GAME_STATUS_SENDER.send(true).unwrap();
    info!("Game status set to running");

    // Reset exit/crash signaling state for this run
    EXIT_EVENT_SENT.store(false, Ordering::SeqCst);

    info!(
        "Launching game for account: {}",
        GLOBAL_CREDENTIALS.get_account_name()
    );

    // Use tokio's unbounded channel for server list requests.
    // UnboundedSender::send() is synchronous and non-blocking, making it safe
    // to call from the Windows message callback (wnd_proc).
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(WPARAM, usize)>();
    *SERVER_LIST_SENDER.lock().unwrap() = Some(tx);

    let tcs = Arc::new(tokio::sync::Notify::new());
    let tcs_clone = Arc::clone(&tcs);

    let handle =
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
    crate::av::ensure_av_exclusion_before_launch();

    let mut child = Command::new(GLOBAL_CREDENTIALS.get_game_path())
        .arg(format!(
            "-LANGUAGEEXT={}",
            GLOBAL_CREDENTIALS.get_game_lang()
        ))
        .spawn()?;

    let pid = child.id();
    info!("Game process spawned with PID: {}", pid);

    let status = child.wait()?;
    info!("Game process exited with status: {:?}", status);

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

    GAME_RUNNING.store(false, Ordering::SeqCst);
    GAME_STATUS_SENDER.send(false).unwrap();
    info!("Game status set to not running");

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
    handle.await?;

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

/// Checks if the game is currently running.
///
/// # Returns
///
/// A boolean indicating whether the game is running (true) or not (false).
pub fn is_game_running() -> bool {
    GAME_RUNNING.load(Ordering::SeqCst)
}

/// Resets the global state of the application.
///
/// This function performs the following actions:
/// 1. Sets the game running status to false.
/// 2. Sends a game status update.
/// 3. Clears the stored window handle.
///
/// It's typically called when cleaning up or restarting the application state.
pub fn reset_global_state() {
    GAME_RUNNING.store(false, Ordering::SeqCst);
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
    let launcher_class_name = "LAUNCHER_CLASS";
    let launcher_window_title = "LAUNCHER_WINDOW";
    let class_name = to_wstring(launcher_class_name);
    let window_name = to_wstring(launcher_window_title);
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
    UnregisterClassW(class_name.as_ptr(), GetModuleHandleW(null_mut()));

    reset_global_state();

    let mut wcex: WNDCLASSEXW = std::mem::zeroed();
    wcex.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;

    EnumWindows(Some(enum_window_proc), class_name.as_ptr() as LPARAM);

    if GetClassInfoExW(GetModuleHandleW(null_mut()), class_name.as_ptr(), &mut wcex) != 0 {
        if UnregisterClassW(class_name.as_ptr(), GetModuleHandleW(null_mut())) == 0 {
            let error = GetLastError();
            error!("Failed to unregister class. Error code: {}", error);
        } else {
            info!("Tera ClassName Unregistered successfully");
        }
    } else {
        info!("Tera ClassName does not exist or is already unregistered");
    }
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
    let account_name = GLOBAL_CREDENTIALS.get_account_name();
    info!("Account Name Request - Sending: [REDACTED]");
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
    let session_ticket = GLOBAL_CREDENTIALS.get_ticket();
    info!("Session Ticket Request - Sending: [REDACTED]");
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
/// This function sends a GET request to a local server to retrieve the server list,
/// then parses the JSON response into a ServerList struct.
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

    let json: Value = response.json().await?;
    info!("Server list JSON received, parsing...");
    let server_list = parse_server_list_json(&json)?;
    info!("Server list parsed successfully, {} servers total", server_list.servers.len());

    let mut buf = Vec::new();
    server_list.encode(&mut buf)?;
    Ok(buf)
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

    let player_last_server = "0";
    let player_last_server_id = 2800;

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

    info!(
        "Parsed values - Last server: {}, Last server ID: {}, Character counts: {:?}",
        player_last_server, player_last_server_id, character_counts
    );

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
            server_id,
            server["name"],
            is_available
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
                error!("Missing or invalid 'address' field for server {}", server_id);
                continue; // Skip invalid server
            }
        };
        if address_str.parse::<std::net::Ipv4Addr>().is_err() {
            error!("Invalid server address format: {} for server {}", address_str, server_id);
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
            host: Vec::new(),
        };
        server_list.servers.push(server_info);
    }

    // Use Enterance's approach: hardcoded last_server_id and format server names with (0) suffix
    server_list.last_server_id = 2800;
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
            host: Vec::new(),
        };
        
        server_list.servers.push(relay_server);
        info!("Added relay server: {} ({}:{})", relay_name, relay_address_str, relay_port);
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

    #[test]
    fn test_to_wstring_empty() {
        let result = to_wstring("");
        assert_eq!(result, vec![0u16], "Empty string should produce only null terminator");
    }

    #[test]
    fn test_to_wstring_ascii() {
        let result = to_wstring("Hello");
        let expected: Vec<u16> = vec![
            'H' as u16,
            'e' as u16,
            'l' as u16,
            'l' as u16,
            'o' as u16,
            0u16, // null terminator
        ];
        assert_eq!(result, expected, "ASCII string should be correctly converted to wide string");
    }

    #[test]
    fn test_to_wstring_unicode() {
        let result = to_wstring("Hello世界");
        // '世' = U+4E16 and '界' = U+754C
        let expected: Vec<u16> = vec![
            'H' as u16,
            'e' as u16,
            'l' as u16,
            'l' as u16,
            'o' as u16,
            0x4E16, // 世
            0x754C, // 界
            0u16,   // null terminator
        ];
        assert_eq!(result, expected, "Unicode string should be correctly converted to wide string");
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
        assert_eq!(result, expected, "Special characters should be correctly converted");
    }

    #[test]
    fn test_utf16_to_bytes_empty() {
        let result = utf16_to_bytes("");
        assert!(result.is_empty(), "Empty string should produce empty byte vector");
    }

    #[test]
    fn test_utf16_to_bytes_ascii() {
        let result = utf16_to_bytes("Hi");
        // 'H' = 0x0048, 'i' = 0x0069 in little-endian
        let expected: Vec<u8> = vec![0x48, 0x00, 0x69, 0x00];
        assert_eq!(result, expected, "ASCII string should be correctly converted to UTF-16 LE bytes");
    }

    #[test]
    fn test_utf16_to_bytes_unicode() {
        let result = utf16_to_bytes("世界");
        // '世' = U+4E16, '界' = U+754C in little-endian
        let expected: Vec<u8> = vec![0x16, 0x4E, 0x4C, 0x75];
        assert_eq!(result, expected, "Unicode string should be correctly converted to UTF-16 LE bytes");
    }

    #[test]
    fn test_utf16_to_bytes_mixed() {
        let result = utf16_to_bytes("A世");
        // 'A' = 0x0041, '世' = U+4E16 in little-endian
        let expected: Vec<u8> = vec![0x41, 0x00, 0x16, 0x4E];
        assert_eq!(result, expected, "Mixed ASCII and Unicode should be correctly converted");
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
        assert_eq!(result, 0x7F000001, "Localhost IP should be correctly converted");
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
            assert_eq!(result, expected, "IP {} should convert to 0x{:08X}", ip, expected);
        }
    }
}
