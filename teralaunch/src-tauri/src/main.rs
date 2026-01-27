#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]


// Standard library imports
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, remove_file, File};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime};

// Third-party imports
use dotenvy::dotenv;
use log::{error, info, LevelFilter};

use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use ini::Ini;
use lazy_static::lazy_static;
use rayon::iter::{
    IndexedParallelIterator, IntoParallelRefIterator, ParallelBridge, ParallelIterator,
};
use reqwest::header::{RANGE, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::Manager;
use teralib::config::get_config_value;
use teralib::{get_game_status_receiver, reset_global_state, run_game};
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufWriter;
use tokio::sync::{watch, Mutex, Semaphore};
use tokio::task::JoinSet;
use walkdir::WalkDir;

// ============================================================================
// Configuration Constants
// ============================================================================

/// Buffer size for file I/O operations (64 KB)
const BUFFER_SIZE: usize = 65_536;

/// HTTP request timeout for downloads (5 minutes)
const DOWNLOAD_TIMEOUT_SECS: u64 = 300;

/// HTTP connection timeout (30 seconds)
const CONNECT_TIMEOUT_SECS: u64 = 30;

/// Timeout before considering a download stalled (2 minutes)
const STALL_TIMEOUT_SECS: u64 = 120;

/// Progress update emission interval (500ms)
const PROGRESS_UPDATE_MS: u64 = 500;

/// Minimum chunk size for parallel downloads (16 MB)
const CHUNK_MIN_SIZE: u64 = 16 * 1024 * 1024;

/// Part size for chunked downloads (32 MB)
const PART_SIZE: u64 = 32 * 1024 * 1024;

/// Maximum number of parallel download parts
const MAX_PARTS: usize = 32;

/// Maximum concurrent file downloads
const MAX_CONCURRENT_DOWNLOADS: usize = 16;

/// BufWriter capacity for file downloads (1 MB)
const BUFWRITER_CAPACITY: usize = 1024 * 1024;

/// Part assembly buffer size (64 KB)
const PART_ASSEMBLY_BUFFER_SIZE: usize = 64 * 1024;

/// Maximum retry attempts for transient download errors
const MAX_RETRIES: u8 = 2;

/// Retry delay base multiplier (500ms per attempt)
const RETRY_DELAY_BASE_MS: u64 = 500;

/// HTTP client max idle connections per host
const HTTP_POOL_MAX_IDLE_PER_HOST: usize = 10;

// ============================================================================
// Helper Functions
// ============================================================================

/// Validates that a resolved path is safely within the base directory.
/// Prevents path traversal attacks using ".." or absolute paths.
fn validate_path_within_base(base: &Path, file_path: &Path) -> Result<PathBuf, String> {
    // Canonicalize both paths to resolve symlinks and ".." components
    let canonical_base = base.canonicalize()
        .map_err(|e| format!("Failed to canonicalize base path: {}", e))?;

    // For the file path, if it doesn't exist yet, canonicalize the parent
    let canonical_path = if file_path.exists() {
        file_path.canonicalize()
            .map_err(|e| format!("Failed to canonicalize file path: {}", e))?
    } else {
        // For new files, ensure parent exists and check that
        let parent = file_path.parent()
            .ok_or_else(|| "File path has no parent".to_string())?;
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent directory: {}", e))?;
        }
        let canonical_parent = parent.canonicalize()
            .map_err(|e| format!("Failed to canonicalize parent: {}", e))?;
        canonical_parent.join(file_path.file_name().ok_or("No file name")?)
    };

    // Check that the canonical path starts with the canonical base
    if !canonical_path.starts_with(&canonical_base) {
        return Err(format!(
            "Path traversal detected: {} is outside {}",
            canonical_path.display(),
            canonical_base.display()
        ));
    }

    Ok(canonical_path)
}

// Struct definitions
#[derive(Serialize, Deserialize)]
struct LoginResponse {
    #[serde(rename = "Return")]
    return_value: bool,
    #[serde(rename = "ReturnCode")]
    return_code: i32,
    #[serde(rename = "Msg")]
    msg: String,
    #[serde(rename = "CharacterCount")]
    character_count: String,
    #[serde(rename = "Permission")]
    permission: i32,
    #[serde(rename = "Privilege")]
    privilege: i32,
    #[serde(rename = "UserNo")]
    user_no: i32,
    #[serde(rename = "UserName")]
    user_name: String,
    #[serde(rename = "AuthKey")]
    auth_key: String,
}

#[tauri::command]
fn is_debug() -> bool {
    // True in dev builds (cargo tauri dev). False in release/installer builds.
    cfg!(debug_assertions)
}

struct GlobalAuthInfo {
    character_count: String,
    user_no: i32,
    user_name: String,
    auth_key: String,
}

lazy_static! {
    static ref GLOBAL_AUTH_INFO: RwLock<GlobalAuthInfo> = RwLock::new(GlobalAuthInfo {
        character_count: String::new(),
        user_no: 0,
        user_name: String::new(),
        auth_key: String::new(),
    });
    static ref HTTP_CLIENT: reqwest::Client = reqwest::Client::builder()
        .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .pool_max_idle_per_host(HTTP_POOL_MAX_IDLE_PER_HOST)
        .build()
        .expect("Failed to create HTTP client");
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FileInfo {
    path: String,
    hash: String,
    size: u64,
    url: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    existing_size: u64,
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

fn resume_offset(existing_size: u64, total_size: u64) -> u64 {
    if existing_size == 0 || total_size == 0 || existing_size >= total_size {
        0
    } else {
        existing_size
    }
}

fn compute_initial_downloaded(files: &[FileInfo], resume_override: Option<u64>) -> u64 {
    let sum_existing: u64 = files
        .iter()
        .map(|f| resume_offset(f.existing_size, f.size))
        .sum();
    let total_size: u64 = files.iter().map(|f| f.size).sum();
    let mut base = sum_existing;
    if let Some(override_bytes) = resume_override {
        if override_bytes > base {
            base = override_bytes;
        }
    }
    if total_size > 0 && base > total_size {
        total_size
    } else {
        base
    }
}

fn stall_exceeded(
    last_bytes: u64,
    current_bytes: u64,
    idle_secs: u64,
    threshold_secs: u64,
) -> bool {
    if current_bytes != last_bytes {
        return false;
    }
    idle_secs >= threshold_secs
}

#[derive(Clone, Serialize)]
struct ProgressPayload {
    file_name: String,
    progress: f64,
    speed: f64,
    downloaded_bytes: u64,
    total_bytes: u64,
    base_downloaded: u64,
    total_files: usize,
    elapsed_time: f64,
    current_file_index: usize,
}

#[derive(Clone, Serialize)]
struct FileCheckProgress {
    current_file: String,
    progress: f64,
    current_count: usize,
    total_files: usize,
    elapsed_time: f64,
    files_to_update: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct CachedFileInfo {
    hash: String,
    last_modified: SystemTime,
}

struct GameState {
    status_receiver: Arc<Mutex<watch::Receiver<bool>>>,
    is_launching: Arc<Mutex<bool>>,
}

lazy_static! {
    static ref HASH_CACHE: Mutex<HashMap<String, CachedFileInfo>> = Mutex::new(HashMap::new());
    static ref CANCEL_DOWNLOAD: AtomicBool = AtomicBool::new(false);
}

static GLOBAL_DOWNLOADED_BYTES: AtomicU64 = AtomicU64::new(0);
static CURRENT_FILE_NAME: RwLock<String> = RwLock::new(String::new());

fn is_ignored(path: &Path, game_path: &Path, ignored_paths: &HashSet<&str>) -> bool {
    let relative_path = match path.strip_prefix(game_path) {
        Ok(p) => match p.to_str() {
            Some(s) => s.replace("\\", "/"),
            None => return false, // Non-UTF8 path, don't ignore
        },
        Err(_) => return false, // Path not under game_path, don't ignore
    };

    // Ignore files at the root
    if relative_path.chars().filter(|&c| c == '/').count() == 0 {
        return true;
    }

    // Check if the path is in the list of ignored paths
    for ignored_path in ignored_paths {
        if relative_path.starts_with(ignored_path) {
            return true;
        }
    }

    false
}

async fn get_server_hash_file() -> Result<serde_json::Value, String> {
    let client = HTTP_CLIENT.clone();
    let res = client
        .get(get_config_value("HASH_FILE_URL"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(json)
}

fn calculate_file_hash<P: AsRef<Path>>(path: P) -> Result<String, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let mut reader = BufReader::with_capacity(BUFFER_SIZE, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; BUFFER_SIZE];

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|e| format!("Failed to read file: {}", e))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

fn get_cache_file_path() -> Result<PathBuf, String> {
    let mut path = std::env::current_exe().map_err(|e| e.to_string())?;
    path.pop();
    path.push("file_cache.json");
    Ok(path)
}

async fn save_cache_to_disk(cache: &HashMap<String, CachedFileInfo>) -> Result<(), String> {
    let cache_path = get_cache_file_path()?;
    let serialized = serde_json::to_string(cache).map_err(|e| e.to_string())?;
    tokio::task::spawn_blocking(move || {
        std::fs::write(&cache_path, serialized)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

async fn load_cache_from_disk() -> Result<HashMap<String, CachedFileInfo>, String> {
    let cache_path = get_cache_file_path()?;
    let contents = tokio::task::spawn_blocking(move || {
        std::fs::read_to_string(&cache_path)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    let cache: HashMap<String, CachedFileInfo> =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    Ok(cache)
}

//REPAIR CLIENT, DELETE CACHE AND RESTART HASH CHECK
#[tauri::command]
fn clear_cache() -> Result<(), String> {
    // Clear the in-memory hash cache to prevent stale entries from old directory
    if let Ok(mut cache) = HASH_CACHE.try_lock() {
        cache.clear();
    }
    // Remove the disk cache file
    let cache_path = get_cache_file_path()?;
    if cache_path.exists() {
        remove_file(cache_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn cancel_downloads() {
    CANCEL_DOWNLOAD.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn get_downloaded_bytes() -> u64 {
    GLOBAL_DOWNLOADED_BYTES.load(Ordering::SeqCst)
}

#[tauri::command]
fn set_logging(enabled: bool) -> Result<(), String> {
    teralib::enable_file_logging(enabled)
}

#[tauri::command]
async fn update_launcher(download_url: String) -> Result<(), String> {
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = current_exe.parent().ok_or("exe dir not found")?;
    let new_path = exe_dir.join("launcher_update.exe");

    let client = HTTP_CLIENT.clone();

    let bytes = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    tokio::fs::write(&new_path, &bytes)
        .await
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let cmd = format!(
            "ping 127.0.0.1 -n 2 > NUL && move /Y \"{}\" \"{}\" && start \"\" \"{}\"",
            new_path.display(),
            current_exe.display(),
            current_exe.display()
        );
        Command::new("cmd")
            .args(["/C", &cmd])
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::fs::rename(&new_path, &current_exe).map_err(|e| e.to_string())?;
        let _ = Command::new(&current_exe).spawn();
    }

    std::process::exit(0);
}

fn find_config_file() -> Option<PathBuf> {
    use dirs_next::config_dir;

    let dir = config_dir()?.join("Crazy-eSports.com");
    let file_path = dir.join("tera_config.ini");

    if file_path.exists() {
        return Some(file_path);
    }

    let mut legacy_paths = Vec::new();
    if let Ok(current_dir) = env::current_dir() {
        legacy_paths.push(current_dir.join("src/tera_config.ini"));
        if let Some(parent) = current_dir.parent() {
            legacy_paths.push(parent.join("src/tera_config.ini"));
        }
    }
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            legacy_paths.push(exe_dir.join("src/tera_config.ini"));
        }
    }
    let legacy_config = legacy_paths.into_iter().find(|p| p.exists());

    if fs::create_dir_all(&dir).is_err() {
        return None;
    }

    if let Some(old) = legacy_config {
        if fs::copy(&old, &file_path).is_ok() {
            return Some(file_path);
        }
    }

    if fs::write(&file_path, include_str!("tera_config.ini")).is_ok() {
        return Some(file_path);
    }

    None
}

fn load_config() -> Result<(PathBuf, String), String> {
    let config_path = find_config_file().ok_or("Config file not found")?;
    let conf =
        Ini::load_from_file(&config_path).map_err(|e| format!("Failed to load config: {}", e))?;

    let section = conf
        .section(Some("game"))
        .ok_or("Game section not found in config")?;

    let game_path = section.get("path").ok_or("Game path not found in config")?;

    let game_path = PathBuf::from(game_path);

    let game_lang = section
        .get("lang")
        .ok_or("Game language not found in config")?
        .to_string();

    Ok((game_path, game_lang))
}

#[tauri::command]
async fn generate_hash_file(window: tauri::Window) -> Result<String, String> {
    let start_time = Instant::now();

    let game_path = get_game_path().map_err(|e| e.to_string())?;
    info!("Game path: {:?}", game_path);
    let output_path = game_path.join("hash-file.json");
    info!("Output path: {:?}", output_path);

    // List of files and directories to ignore
    let ignored_paths: HashSet<&str> = [
        "$Patch",
        "Binaries/cookies.dat",
        "S1Game/GuildFlagUpload",
        "S1Game/GuildLogoUpload",
        "S1Game/ImageCache",
        "S1Game/Logs",
        "S1Game/Screenshots",
        "S1Game/Config/S1Engine.ini",
        "S1Game/Config/S1Game.ini",
        "S1Game/Config/S1Input.ini",
        "S1Game/Config/S1Lightmass.ini",
        "S1Game/Config/S1Option.ini",
        "S1Game/Config/S1SystemSettings.ini",
        "S1Game/Config/S1TBASettings.ini",
        "S1Game/Config/S1UI.ini",
        "Launcher.exe",
        "local.db",
        "version.ini",
        "unins000.dat",
        "unins000.exe",
    ]
    .iter()
    .cloned()
    .collect();

    let total_files = WalkDir::new(&game_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| !is_ignored(e.path(), &game_path, &ignored_paths))
        .count();
    info!("Total files to process: {}", total_files);

    let progress_bar = ProgressBar::new(total_files as u64);
    let progress_style = ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
        .map_err(|e| e.to_string())?
        .progress_chars("##-");
    progress_bar.set_style(progress_style);

    let processed_files = AtomicU64::new(0);
    let total_size = AtomicU64::new(0);
    let files = Arc::new(Mutex::new(Vec::new()));

    let result: Result<(), String> = WalkDir::new(&game_path)
        .into_iter()
        .par_bridge()
        .try_for_each(|entry| -> Result<(), String> {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_file() && !is_ignored(path, &game_path, &ignored_paths) {
                let relative_path = match path.strip_prefix(&game_path) {
                    Ok(p) => match p.to_str() {
                        Some(s) => s.replace("\\", "/"),
                        None => return Ok(()), // Non-UTF8 path, skip
                    },
                    Err(_) => return Ok(()), // Path not under game_path, skip
                };
                info!("Processing file: {}", relative_path);

                let file = File::open(path).map_err(|e| e.to_string())?;
                let mut reader = BufReader::with_capacity(BUFFER_SIZE, file);
                let mut hasher = Sha256::new();
                let mut buffer = [0u8; BUFFER_SIZE];
                let mut size: u64 = 0;

                loop {
                    let bytes_read = reader.read(&mut buffer).map_err(|e| e.to_string())?;
                    if bytes_read == 0 {
                        break;
                    }
                    hasher.update(&buffer[..bytes_read]);
                    size += bytes_read as u64;
                }
                let hash = format!("{:x}", hasher.finalize());
                let file_server_url = get_config_value("FILE_SERVER_URL");
                let url = format!("{}/{}", file_server_url, relative_path);

                files.blocking_lock().push(FileInfo {
                    path: relative_path.clone(),
                    hash,
                    size,
                    url,
                    existing_size: 0,
                });

                total_size.fetch_add(size, Ordering::Relaxed);
                let current_processed = processed_files.fetch_add(1, Ordering::Relaxed) + 1;
                progress_bar.set_position(current_processed);

                let progress = (current_processed as f64 / total_files as f64) * 100.0;
                window
                    .emit(
                        "hash_file_progress",
                        json!({
                            "current_file": relative_path,
                            "progress": progress,
                            "processed_files": current_processed,
                            "total_files": total_files,
                            "total_size": total_size.load(Ordering::Relaxed)
                        }),
                    )
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        });

    if let Err(e) = result {
        error!("Error during file processing: {:?}", e);
        return Err(e);
    }

    progress_bar.finish_with_message("File processing completed");

    info!("Generating JSON");
    let json = serde_json::to_string(&json!({
        "files": files.lock().await.clone()
    }))
    .map_err(|e| e.to_string())?;

    info!("Writing hash file");
    let mut file = File::create(&output_path).map_err(|e| e.to_string())?;
    file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;

    let duration = start_time.elapsed();
    let total_processed = processed_files.load(Ordering::Relaxed);
    let total_size = total_size.load(Ordering::Relaxed);
    info!("Hash file generation completed in {:?}", duration);
    info!("Total files processed: {}", total_processed);
    info!("Total size: {} bytes", total_size);

    Ok(format!("Hash file generated successfully. Processed {} files with a total size of {} bytes in {:?}", total_processed, total_size, duration))
}

#[tauri::command]
async fn select_game_folder() -> Result<String, String> {
    use tauri::api::dialog::blocking::FileDialogBuilder;

    let folder = FileDialogBuilder::new()
        .set_title("Select Tera Game Folder")
        .set_directory("/")
        .pick_folder();

    match folder {
        Some(path) => Ok(path.to_string_lossy().into_owned()),
        None => Err("Folder selection cancelled or failed".into()),
    }
}

fn get_game_path() -> Result<PathBuf, String> {
    let (game_path, _) = load_config()?;
    Ok(game_path)
}

#[tauri::command]
async fn save_game_path_to_config(
    path: String,
    window: tauri::Window,
    _app_handle: tauri::AppHandle,
) -> Result<(), String> {
    // Validate that the path is a real directory
    let path_buf = PathBuf::from(&path);
    if !path_buf.exists() {
        return Err("The specified path does not exist".to_string());
    }
    if !path_buf.is_dir() {
        return Err("The specified path is not a directory".to_string());
    }

    // Capture previous path before writing, so we can detect actual changes
    let prev_path_string = get_game_path()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()));

    let config_path = find_config_file().ok_or("Config file not found")?;
    let mut conf =
        Ini::load_from_file(&config_path).map_err(|e| format!("Failed to load config: {}", e))?;

    conf.with_section(Some("game")).set("path", &path);

    conf.write_to_file(&config_path)
        .map_err(|e| format!("Failed to write config: {}", e))?;

    // Only interrupt/recheck when path actually changed
    let should_refresh = game_path_changed(prev_path_string.as_deref(), &path);

    if should_refresh {
        // Interrupt any ongoing downloads
        CANCEL_DOWNLOAD.store(true, Ordering::SeqCst);
        // Clear stale hash cache from old directory and reset download progress
        clear_cache().ok();
        GLOBAL_DOWNLOADED_BYTES.store(0, Ordering::SeqCst);
        let _ = window.emit("game_path_changed", &path);
    }

    Ok(())
}

#[tauri::command]
fn get_game_path_from_config() -> Result<String, String> {
    match get_game_path() {
        Ok(game_path) => game_path
            .to_str()
            .ok_or_else(|| "Invalid UTF-8 in game path".to_string())
            .map(|s| s.to_string()),
        Err(e) => {
            if e.contains("Config file not found") {
                Err("tera_config.ini is missing".to_string())
            } else {
                Err(e)
            }
        }
    }
}

#[tauri::command]
async fn check_update_required(window: tauri::Window) -> Result<bool, String> {
    match get_files_to_update(window).await {
        Ok(files) => Ok(!files.is_empty()),
        Err(e) => Err(e),
    }
}

#[tauri::command]
async fn update_file(
    _app_handle: tauri::AppHandle,
    _window: tauri::Window,
    file_info: FileInfo,
    _total_files: usize,
    _current_file_index: usize,
    _total_size: u64,
) -> Result<u64, String> {
    // Update the current file name for global progress tracking
    {
        let mut current_file = CURRENT_FILE_NAME.write().unwrap_or_else(|e| e.into_inner());
        *current_file = file_info.path.clone();
    }

    let game_path = get_game_path()?;
    let file_path = game_path.join(&file_info.path);

    // Validate that the file path is within the game directory (prevent path traversal)
    let file_path = validate_path_within_base(&game_path, &file_path)?;

    if let Some(parent) = file_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }

    let client = HTTP_CLIENT.clone();

    let mut corrected_url = file_info.url.clone();
    if let Some(pos) = corrected_url.find("/files/") {
        corrected_url = format!("{}{}", &corrected_url[..pos], &corrected_url[(pos + 7)..]);
    }

    let file_size = file_info.size;
    let mut resume_from = resume_offset(file_info.existing_size, file_size);

    if resume_from > 0 && !file_path.exists() {
        GLOBAL_DOWNLOADED_BYTES.fetch_sub(resume_from, Ordering::SeqCst);
        resume_from = 0;
    }

    if resume_from == 0 && file_path.exists() {
        tokio::fs::remove_file(&file_path)
            .await
            .map_err(|e| e.to_string())?;
    }

    let file_handle = if resume_from > 0 {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&file_path)
            .await
            .map_err(|e| e.to_string())?
    } else {
        tokio::fs::File::create(&file_path)
            .await
            .map_err(|e| e.to_string())?
    };
    let mut file = BufWriter::with_capacity(BUFWRITER_CAPACITY, file_handle);

    // Keep downloads resumable with contiguous files.
    let allow_parallel = false;

    info!("Downloading file: {} ({} bytes)", file_info.path, file_size);
    let bytes_written: u64;

    if resume_from > 0 {
        let range_probe = client
            .get(&corrected_url)
            .header(RANGE, "bytes=0-0")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let supports_range = range_probe.status() == reqwest::StatusCode::PARTIAL_CONTENT
            || range_probe.headers().get("content-range").is_some();

        if supports_range {
            let range_header = format!("bytes={}-", resume_from);
            let res = client
                .get(&corrected_url)
                .header(RANGE, range_header)
                .header("Accept-Encoding", "gzip, deflate, br")
                .header("Connection", "keep-alive")
                .header("Cache-Control", "no-cache")
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;

            let status = res.status();
            let has_content_range = res.headers().get("content-range").is_some();
            if status != reqwest::StatusCode::PARTIAL_CONTENT && !has_content_range {
                GLOBAL_DOWNLOADED_BYTES.fetch_sub(resume_from, Ordering::SeqCst);
                resume_from = 0;
                drop(file);
                tokio::fs::remove_file(&file_path).await.ok();
                let file_handle = tokio::fs::File::create(&file_path)
                    .await
                    .map_err(|e| e.to_string())?;
                file = BufWriter::with_capacity(BUFWRITER_CAPACITY, file_handle);
            } else {
                let mut downloaded: u64 = 0;
                let mut stream = res.bytes_stream();

                while let Some(chunk_result) = stream.next().await {
                    if CANCEL_DOWNLOAD.load(Ordering::SeqCst) {
                        return Err("cancelled".into());
                    }
                    let chunk = chunk_result.map_err(|e| e.to_string())?;
                    file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                    let len = chunk.len() as u64;
                    downloaded += len;
                    GLOBAL_DOWNLOADED_BYTES.fetch_add(len, Ordering::SeqCst);
                }
                bytes_written = resume_from + downloaded;
                return Ok(bytes_written);
            }
        }

        GLOBAL_DOWNLOADED_BYTES.fetch_sub(resume_from, Ordering::SeqCst);
        drop(file);
        tokio::fs::remove_file(&file_path).await.ok();
        let file_handle = tokio::fs::File::create(&file_path)
            .await
            .map_err(|e| e.to_string())?;
        file = BufWriter::with_capacity(1024 * 1024, file_handle);
    }

    if allow_parallel && file_size >= CHUNK_MIN_SIZE {
        // Check if server supports range requests; fallback if not
        let range_probe = client
            .get(&corrected_url)
            .header(RANGE, "bytes=0-0")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let supports_range = range_probe.status() == reqwest::StatusCode::PARTIAL_CONTENT
            || range_probe.headers().get("content-range").is_some();

        if !supports_range {
            info!(
                "Server does not support range requests. Falling back to single-stream for {}",
                file_info.path
            );
            // Single-stream fallback
            let res = client
                .get(&corrected_url)
                .header("Accept-Encoding", "gzip, deflate, br")
                .header("Connection", "keep-alive")
                .header("Cache-Control", "no-cache")
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;

            let mut downloaded: u64 = 0;
            let mut stream = res.bytes_stream();

            while let Some(chunk_result) = stream.next().await {
                if CANCEL_DOWNLOAD.load(Ordering::SeqCst) {
                    return Err("cancelled".into());
                }
                let chunk = chunk_result.map_err(|e| e.to_string())?;
                file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                let len = chunk.len() as u64;
                downloaded += len;
                GLOBAL_DOWNLOADED_BYTES.fetch_add(len, Ordering::SeqCst);
            }
            bytes_written = downloaded;
        } else {
            // Perform chunked parallel download using HTTP ranges into temp parts
            let num_parts = std::cmp::max(
                1,
                std::cmp::min(MAX_PARTS as u64, (file_size + PART_SIZE - 1) / PART_SIZE) as usize,
            );
            let mut join_set: JoinSet<Result<(), String>> = JoinSet::new();

            for part_idx in 0..num_parts {
                if CANCEL_DOWNLOAD.load(Ordering::SeqCst) {
                    return Err("cancelled".into());
                }

                let start = (part_idx as u64) * PART_SIZE;
                let mut end = ((part_idx as u64 + 1) * PART_SIZE).saturating_sub(1);
                if end >= file_size {
                    end = file_size - 1;
                }

                let part_url = corrected_url.clone();
                let part_path = file_path.with_extension(format!("part{}", part_idx));

                join_set.spawn(async move {
                let client = HTTP_CLIENT.clone();
                let range_header = format!("bytes={}-{}", start, end);
                let res = client
                    .get(&part_url)
                    .header(RANGE, range_header)
                    .header("Accept-Encoding", "gzip, deflate, br")
                    .header("Connection", "keep-alive")
                    .header("Cache-Control", "no-cache")
                    .send()
                    .await
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?;

                let mut stream = res.bytes_stream();
                let mut part_file = BufWriter::with_capacity(BUFWRITER_CAPACITY,
                    tokio::fs::File::create(&part_path).await.map_err(|e| e.to_string())?
                );
                while let Some(chunk_result) = stream.next().await {
                    if CANCEL_DOWNLOAD.load(Ordering::SeqCst) { return Err("cancelled".into()); }
                    let chunk = chunk_result.map_err(|e| e.to_string())?;
                    part_file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                    let len = chunk.len() as u64;
                    GLOBAL_DOWNLOADED_BYTES.fetch_add(len, Ordering::SeqCst);
                }

                part_file.flush().await.map_err(|e| e.to_string())?;
                Ok(())
            });
            }

            while let Some(res) = join_set.join_next().await {
                match res {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => return Err(e),
                    Err(e) => return Err(format!("Join error: {}", e)),
                }
            }

            // Assemble parts
            for part_idx in 0..num_parts {
                let part_path = file_path.with_extension(format!("part{}", part_idx));
                let mut part_f = tokio::fs::File::open(&part_path)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut buf = vec![0u8; PART_ASSEMBLY_BUFFER_SIZE];
                loop {
                    let n = part_f.read(&mut buf).await.map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    file.write_all(&buf[..n]).await.map_err(|e| e.to_string())?;
                }
                tokio::fs::remove_file(&part_path).await.ok();
            }
            bytes_written = file_size;
        }
    } else {
        // Single-stream download
        let req = client.get(&corrected_url);
        let res = req
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;

        let mut downloaded: u64 = 0;
        let mut stream = res.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            if CANCEL_DOWNLOAD.load(Ordering::SeqCst) {
                return Err("cancelled".into());
            }
            let chunk = chunk_result.map_err(|e| e.to_string())?;
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            let len = chunk.len() as u64;
            downloaded += len;
            GLOBAL_DOWNLOADED_BYTES.fetch_add(len, Ordering::SeqCst);
        }
        bytes_written = downloaded;
    }

    file.flush().await.map_err(|e| e.to_string())?;

    info!("File download completed: {}", file_info.path);

    Ok(bytes_written)
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_index])
}

fn should_auto_install_updater() -> bool {
    match std::env::var("TERA_LAUNCHER_AUTO_UPDATE").ok().as_deref() {
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES") => true,
        _ => false,
    }
}

fn normalize_path_for_compare(value: &str) -> String {
    let mut path = value.replace('\\', "/");
    while path.ends_with('/') {
        path.pop();
    }
    path.to_lowercase()
}

fn game_path_changed(previous: Option<&str>, next: &str) -> bool {
    match previous {
        Some(prev) => normalize_path_for_compare(prev) != normalize_path_for_compare(next),
        None => true,
    }
}

fn is_transient_download_error(message: &str) -> bool {
    let msg = message.to_lowercase();
    msg.contains("timed out")
        || msg.contains("timeout")
        || msg.contains("connection reset")
        || msg.contains("connection closed")
        || msg.contains("broken pipe")
        || msg.contains("temporarily")
        || msg.contains("network")
        || msg.contains("dns")
        || msg.contains("503")
        || msg.contains("502")
        || msg.contains("504")
}

fn retry_delay_ms(attempt: u8) -> u64 {
    RETRY_DELAY_BASE_MS.saturating_mul(attempt as u64)
}

#[tauri::command]
async fn download_all_files(
    app_handle: tauri::AppHandle,
    window: tauri::Window,
    files_to_update: Vec<FileInfo>,
    resume_downloaded: Option<u64>,
) -> Result<Vec<u64>, String> {
    let total_files = files_to_update.len();
    let total_size: u64 = files_to_update.iter().map(|f| f.size).sum();
    let initial_downloaded = compute_initial_downloaded(&files_to_update, resume_downloaded);

    if total_files == 0 {
        info!("No files to download");
        if let Err(e) = window.emit("download_complete", ()) {
            error!("Failed to emit download_complete event: {}", e);
        }
        return Ok(vec![]);
    }

    let mut results: Vec<Option<u64>> = vec![None; total_files];
    let mut files_by_index: Vec<Option<FileInfo>> = vec![None; total_files];
    GLOBAL_DOWNLOADED_BYTES.store(initial_downloaded, Ordering::SeqCst);
    CANCEL_DOWNLOAD.store(false, Ordering::SeqCst);

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_DOWNLOADS));
    let global_start = Instant::now();

    // Emit a smooth global progress tick to stabilize UI speed/ETA
    {
        let window_tick = window.clone();
        let total_bytes_tick = total_size;
        let total_files_tick = total_files;
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(PROGRESS_UPDATE_MS));
            let mut last_bytes = initial_downloaded;
            let mut last_change = Instant::now();
            loop {
                interval.tick().await;
                let d = GLOBAL_DOWNLOADED_BYTES.load(Ordering::SeqCst);
                let elapsed = global_start.elapsed();
                let speed = if elapsed.as_secs() > 0 {
                    d.saturating_sub(initial_downloaded) / elapsed.as_secs()
                } else {
                    0
                };
                let current_file_name = {
                    let current_file = CURRENT_FILE_NAME.read().unwrap_or_else(|e| e.into_inner());
                    current_file.clone()
                };

                if d != last_bytes {
                    last_bytes = d;
                    last_change = Instant::now();
                } else if stall_exceeded(
                    last_bytes,
                    d,
                    last_change.elapsed().as_secs(),
                    STALL_TIMEOUT_SECS,
                ) {
                    let _ = window_tick.emit(
                        "download_error",
                        json!({
                            "message": "Download stalled. Please retry.",
                            "file": current_file_name,
                        }),
                    );
                    CANCEL_DOWNLOAD.store(true, Ordering::SeqCst);
                    break;
                }

                let payload = ProgressPayload {
                    file_name: current_file_name,
                    progress: if total_bytes_tick > 0 {
                        (d as f64 / total_bytes_tick as f64) * 100.0
                    } else {
                        0.0
                    },
                    speed: speed as f64,
                    downloaded_bytes: d,
                    total_bytes: total_bytes_tick,
                    base_downloaded: initial_downloaded,
                    total_files: total_files_tick,
                    elapsed_time: elapsed.as_secs_f64(),
                    current_file_index: 0,
                };
                let _ = window_tick.emit("global_download_progress", &payload);
                if d >= total_bytes_tick || CANCEL_DOWNLOAD.load(Ordering::SeqCst) {
                    break;
                }
            }
        });
    }
    let mut join_set: JoinSet<(usize, Result<u64, String>)> = JoinSet::new();

    for (index, file_info) in files_to_update.into_iter().enumerate() {
        if CANCEL_DOWNLOAD.load(Ordering::SeqCst) {
            let _ = window.emit("download_cancelled", ());
            break;
        }
        files_by_index[index] = Some(file_info.clone());
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        let app_handle_cl = app_handle.clone();
        let window_cl = window.clone();
        let file_info_cl = file_info.clone();
        join_set.spawn(async move {
            let _permit = permit;
            let mut attempt: u8 = 0;
            let res = loop {
                let result = update_file(
                    app_handle_cl.clone(),
                    window_cl.clone(),
                    file_info_cl.clone(),
                    total_files,
                    index + 1,
                    total_size,
                )
                .await;

                match result {
                    Ok(size) => break Ok(size),
                    Err(e) if e == "cancelled" => break Err(e),
                    Err(e) => {
                        attempt = attempt.saturating_add(1);
                        if attempt > MAX_RETRIES || !is_transient_download_error(&e) {
                            break Err(format!("{}: {}", file_info_cl.path, e));
                        }
                        tokio::time::sleep(Duration::from_millis(retry_delay_ms(attempt))).await;
                    }
                }
            };
            (index, res)
        });
    }

    while let Some(jr) = join_set.join_next().await {
        match jr {
            Ok((idx, Ok(sz))) => {
                results[idx] = Some(sz);
            }
            Ok((_idx, Err(e))) => {
                if e == "cancelled" {
                    let _ = window.emit("download_cancelled", ());
                    CANCEL_DOWNLOAD.store(true, Ordering::SeqCst);
                    break;
                } else {
                    let message = e.clone();
                    let _ = window.emit(
                        "download_error",
                        json!({
                            "message": message,
                            "file": ""
                        }),
                    );
                    return Err(message);
                }
            }
            Err(e) => return Err(format!("Join error: {}", e)),
        }
    }

    let downloaded_sizes: Vec<u64> = results.into_iter().filter_map(|x| x).collect();

    if !CANCEL_DOWNLOAD.load(Ordering::SeqCst) {
        // Post-download verification of files that completed
        for (_idx, maybe_file) in files_by_index.into_iter().enumerate() {
            // Only verify files that finished successfully
            // (we used results before moving it; so check via downloaded_sizes length wouldn't align by idx)
            // Instead, recompute: if handler returned Some for this idx we already moved results, so we can't check here.
            // To avoid confusion, re-run a cheap metadata check: if the file exists, verify hash.
            if CANCEL_DOWNLOAD.load(Ordering::SeqCst) {
                break;
            }
            let file_info = match maybe_file {
                Some(f) => f,
                None => continue,
            };
            let game_path = get_game_path()?;
            let file_path = game_path.join(&file_info.path);
            if !file_path.exists() {
                continue;
            }
            let expected_hash = file_info.hash.clone();
            let calc = tokio::task::spawn_blocking(move || calculate_file_hash(&file_path))
                .await
                .map_err(|e| e.to_string())??;
            if calc != expected_hash {
                let message = format!("Hash mismatch after download for file: {}", file_info.path);
                let _ = window.emit(
                    "download_error",
                    json!({
                        "message": message,
                        "file": file_info.path
                    }),
                );
                return Err(message);
            }
        }
    }

    if CANCEL_DOWNLOAD.load(Ordering::SeqCst) {
        info!("Download cancelled");
    } else {
        info!("Download complete for {} file(s)", total_files);
        if let Err(e) = window.emit("download_complete", ()) {
            error!("Failed to emit download_complete event: {}", e);
        }
    }

    Ok(downloaded_sizes)
}

#[tauri::command]
async fn get_files_to_update(window: tauri::Window) -> Result<Vec<FileInfo>, String> {
    info!("Starting get_files_to_update");

    let start_time = Instant::now();
    let server_hash_file = get_server_hash_file().await?;

    // Get the path to the game folder, which is the folder that contains the Tera game
    // files. This is the folder that we will be comparing with the server hash file
    // to determine which files need to be updated.
    let local_game_path = get_game_path()?;
    info!("Local game path: {:?}", local_game_path);

    info!("Attempting to read server hash file");
    let files = server_hash_file["files"]
        .as_array()
        .ok_or("Invalid server hash file format")?;
    info!("Server hash file parsed, {} files found", files.len());

    info!("Starting file comparison");
    let loaded_cache = load_cache_from_disk().await.unwrap_or_else(|_| HashMap::new());
    let cache = Arc::new(RwLock::new(loaded_cache));

    let progress_bar = ProgressBar::new(files.len() as u64);
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .expect("Invalid progress bar template - this is a bug")
            .progress_chars("##-"),
    );

    let processed_count = Arc::new(AtomicUsize::new(0));
    let files_to_update_count = Arc::new(AtomicUsize::new(0));
    let total_size = Arc::new(AtomicU64::new(0));

    let files_to_update: Vec<FileInfo> = files
        .par_iter()
        .enumerate()
        .filter_map(|(_index, file_info)| {
            let path = file_info["path"].as_str().unwrap_or("");
            let server_hash = file_info["hash"].as_str().unwrap_or("");
            let size = file_info["size"].as_u64().unwrap_or(0);
            let url = file_info["url"].as_str().unwrap_or("").to_string();

            let local_file_path = local_game_path.join(path);

            // Validate path to prevent path traversal attacks
            let local_file_path = match validate_path_within_base(&local_game_path, &local_file_path) {
                Ok(p) => p,
                Err(e) => {
                    error!("Path validation failed for {}: {}", path, e);
                    return None; // Skip this file
                }
            };

            let current_count = processed_count.fetch_add(1, Ordering::SeqCst) + 1;
            if current_count % 100 == 0 || current_count == files.len() {
                let progress_payload = FileCheckProgress {
                    current_file: path.to_string(),
                    progress: (current_count as f64 / files.len() as f64) * 100.0,
                    current_count,
                    total_files: files.len(),
                    elapsed_time: start_time.elapsed().as_secs_f64(),
                    files_to_update: files_to_update_count.load(Ordering::SeqCst),
                };

                let _ = window
                    .emit("file_check_progress", progress_payload)
                    .map_err(|e| {
                        error!("Error emitting file_check_progress event: {}", e);
                        e.to_string()
                    });
            }

            progress_bar.inc(1);

            if !local_file_path.exists() {
                files_to_update_count.fetch_add(1, Ordering::SeqCst);
                total_size.fetch_add(size, Ordering::SeqCst);
                return Some(FileInfo {
                    path: path.to_string(),
                    hash: server_hash.to_string(),
                    size,
                    url,
                    existing_size: 0,
                });
            }

            let metadata = match fs::metadata(&local_file_path) {
                Ok(m) => m,
                Err(_) => {
                    files_to_update_count.fetch_add(1, Ordering::SeqCst);
                    total_size.fetch_add(size, Ordering::SeqCst);
                    return Some(FileInfo {
                        path: path.to_string(),
                        hash: server_hash.to_string(),
                        size,
                        url,
                        existing_size: 0,
                    });
                }
            };

            let last_modified = metadata.modified().ok();

            let cache_read = cache.read().unwrap_or_else(|e| e.into_inner());
            if let Some(cached_info) = cache_read.get(path) {
                if let Some(lm) = last_modified {
                    if cached_info.last_modified == lm && cached_info.hash == server_hash {
                        return None;
                    }
                }
            }
            drop(cache_read);

            if metadata.len() != size {
                files_to_update_count.fetch_add(1, Ordering::SeqCst);
                total_size.fetch_add(size, Ordering::SeqCst);
                return Some(FileInfo {
                    path: path.to_string(),
                    hash: server_hash.to_string(),
                    size,
                    url,
                    existing_size: resume_offset(metadata.len(), size),
                });
            }

            let local_hash = match calculate_file_hash(&local_file_path) {
                Ok(hash) => hash,
                Err(_) => {
                    files_to_update_count.fetch_add(1, Ordering::SeqCst);
                    total_size.fetch_add(size, Ordering::SeqCst);
                    return Some(FileInfo {
                        path: path.to_string(),
                        hash: server_hash.to_string(),
                        size,
                        url,
                        existing_size: resume_offset(metadata.len(), size),
                    });
                }
            };

            let mut cache_write = cache.write().unwrap_or_else(|e| e.into_inner());
            cache_write.insert(
                path.to_string(),
                CachedFileInfo {
                    hash: local_hash.clone(),
                    last_modified: last_modified.unwrap_or_else(SystemTime::now),
                },
            );
            drop(cache_write);

            if local_hash != server_hash {
                files_to_update_count.fetch_add(1, Ordering::SeqCst);
                total_size.fetch_add(size, Ordering::SeqCst);
                Some(FileInfo {
                    path: path.to_string(),
                    hash: server_hash.to_string(),
                    size,
                    url,
                    existing_size: resume_offset(metadata.len(), size),
                })
            } else {
                None
            }
        })
        .collect();

    progress_bar.finish_with_message("File comparison completed");

    // Ensure the UI receives a final 100% progress update
    let final_progress = FileCheckProgress {
        current_file: String::new(),
        progress: 100.0,
        current_count: files.len(),
        total_files: files.len(),
        elapsed_time: start_time.elapsed().as_secs_f64(),
        files_to_update: files_to_update_count.load(Ordering::SeqCst),
    };

    let _ = window
        .emit("file_check_progress", final_progress)
        .map_err(|e| {
            error!("Error emitting final file_check_progress event: {}", e);
            e.to_string()
        });

    // Save the updated cache to disk
    let final_cache = cache.read().unwrap_or_else(|e| e.into_inner()).clone();
    if let Err(e) = save_cache_to_disk(&final_cache).await {
        error!("Failed to save cache to disk: {}", e);
    }

    let total_time = start_time.elapsed();
    info!(
        "File comparison completed. Files to update: {}",
        files_to_update.len()
    );

    // Emit a final event with complete statistics
    let _ = window.emit(
        "file_check_completed",
        json!({
            "total_files": files.len(),
            "files_to_update": files_to_update.len(),
            "total_size": total_size.load(Ordering::SeqCst),
            "total_time_seconds": total_time.as_secs(),
            "average_time_per_file_ms": (total_time.as_millis() as f64) / (files.len() as f64)
        }),
    );

    Ok(files_to_update)
}

#[tauri::command]
async fn get_game_status(state: tauri::State<'_, GameState>) -> Result<bool, String> {
    let status = state.status_receiver.lock().await.borrow().clone();
    let is_launching = *state.is_launching.lock().await;
    Ok(status || is_launching)
}

#[tauri::command]
async fn handle_launch_game(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, GameState>,
) -> Result<String, String> {
    info!("Total time: {:?}", 3);
    let mut is_launching = state.is_launching.lock().await;
    if *is_launching {
        return Err("Game is already launching".to_string());
    }
    *is_launching = true;

    let is_running = *state.status_receiver.lock().await.borrow();

    if is_running {
        *is_launching = false;
        return Err("Game is already running".to_string());
    }

    let auth_info = GLOBAL_AUTH_INFO.read().unwrap_or_else(|e| e.into_inner());
    let account_name = auth_info.user_no.to_string();
    let characters_count = auth_info.character_count.clone();
    let ticket = auth_info.auth_key.clone();
    let (game_path, game_lang) = load_config()?;

    let full_game_path = game_path.join("Binaries").join("TERA.exe");

    if !full_game_path.exists() {
        *is_launching = false;
        return Err(format!(
            "Game executable not found at: {:?}",
            full_game_path
        ));
    }

    let full_game_path_str = full_game_path
        .to_str()
        .ok_or("Invalid path to game executable")?
        .to_string();

    let app_handle_clone = app_handle.clone();
    let is_launching_clone = Arc::clone(&state.is_launching);

    tokio::task::spawn(async move {
        // Emit the game_status_changed event at the start of the launch
        if let Err(e) = app_handle_clone.emit_all("game_status_changed", true) {
            error!("Failed to emit game_status_changed event: {:?}", e);
        }

        info!("run_game reached");

        match run_game(
            &account_name,
            &characters_count,
            &ticket,
            &game_lang,
            &full_game_path_str,
        )
        .await
        {
            Ok(exit_status) => {
                let result = format!("Game exited with status: {:?}", exit_status);
                if let Err(e) = app_handle_clone.emit_all("game_status", &result) {
                    error!("Failed to emit game_status event: {:?}", e);
                }
                info!("{}", result);
            }
            Err(e) => {
                let error = format!("Error launching game: {:?}", e);
                if let Err(emit_err) = app_handle_clone.emit_all("game_status", &error) {
                    error!("Failed to emit game_status event: {:?}", emit_err);
                }
                error!("{}", error);
            }
        }

        info!("Emitting game_ended event");
        if let Err(e) = app_handle_clone.emit_all("game_ended", ()) {
            error!("Failed to emit game_ended event: {:?}", e);
        }

        let mut is_launching = is_launching_clone.lock().await;
        *is_launching = false;
        if let Err(e) = app_handle_clone.emit_all("game_status_changed", false) {
            error!("Failed to emit game_status_changed event: {:?}", e);
        }

        reset_global_state();

        info!("Game launch state reset");
    });

    Ok("Game launch initiated".to_string())
}

#[tauri::command]
fn get_language_from_config() -> Result<String, String> {
    info!("Attempting to read language from config file");
    let (_, game_lang) = load_config()?;
    info!("Language read from config: {}", game_lang);
    Ok(game_lang)
}

#[tauri::command]
fn save_language_to_config(language: String) -> Result<(), String> {
    info!("Attempting to save language {} to config file", language);
    let config_path = find_config_file().ok_or("Config file not found")?;
    let mut conf =
        Ini::load_from_file(&config_path).map_err(|e| format!("Failed to load config: {}", e))?;

    conf.with_section(Some("game")).set("lang", &language);

    conf.write_to_file(&config_path)
        .map_err(|e| format!("Failed to write config: {}", e))?;

    info!("Language successfully saved to config");
    Ok(())
}

#[tauri::command]
async fn reset_launch_state(state: tauri::State<'_, GameState>) -> Result<(), String> {
    let mut is_launching = state.is_launching.lock().await;
    *is_launching = false;
    Ok(())
}

#[tauri::command]
fn set_auth_info(auth_key: String, user_name: String, user_no: i32, character_count: String) {
    let mut auth_info = GLOBAL_AUTH_INFO.write().unwrap_or_else(|e| e.into_inner());
    auth_info.auth_key = auth_key;
    auth_info.user_name = user_name;
    auth_info.user_no = user_no;
    auth_info.character_count = character_count;

    // AC: log auth info received from frontend

    info!("Auth info set from frontend:");
    info!("User Name: {}", auth_info.user_name);
    info!("User No: {}", auth_info.user_no);
    info!("Character Count: {}", auth_info.character_count);
    info!("Auth Key: {}", auth_info.auth_key);
}

#[tauri::command]
async fn login(username: String, password: String) -> Result<String, String> {
    // AC: early validation to avoid unnecessary network calls
    if username.is_empty() || password.is_empty() {
        return Err("Username and password cannot be empty".to_string());
    }

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .pool_max_idle_per_host(HTTP_POOL_MAX_IDLE_PER_HOST)
        .build()
        .map_err(|e| e.to_string())?;

    // URLs aus der Konfiguration abrufen
    let login_url = get_config_value("LOGIN_ACTION_URL");
    let account_info_url = get_config_value("GET_ACCOUNT_INFO_URL");
    let gcc_url = get_config_value("GET_CHARACTER_COUNT_URL");
    let gotp_url = get_config_value("GET_AUTH_KEY_URL");

    // Login-Payload vorbereiten
    let mut payload = HashMap::new();
    payload.insert("login", username.clone());
    payload.insert("password", password);

    // Login-Anfrage senden
    let login_res = client
        .post(&login_url)
        .header(USER_AGENT, "Tera Game Launcher")
        .form(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = login_res.status();
    let text = login_res.text().await.map_err(|e| e.to_string())?;

    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err("INVALID_CREDENTIALS".to_string());
    }

    if !status.is_success() {
        return Err(format!("Login request failed with status {}", status));
    }

    // Login-Daten extrahieren
    let login_data: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let this_status = login_data["Msg"]
        .as_str()
        .ok_or("Failed to retrieve status message")?
        .to_string();

    if this_status.to_lowercase() != "success" {
        return Err(this_status);
    }

    // Zusätzliche Benutzerinformationen abrufen
    let account_info_res = client
        .get(&account_info_url)
        .header(USER_AGENT, "Tera Game Launcher")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let account_info_data: Value = account_info_res.json().await.map_err(|e| e.to_string())?;
    let this_user_id = account_info_data["UserNo"]
        .as_i64()
        .ok_or("Failed to retrieve UserNo")?;
    let this_permission = account_info_data["Permission"]
        .as_i64()
        .ok_or("Failed to retrieve Permission")?;
    let this_username = account_info_data["UserName"]
        .as_str()
        .ok_or("Failed to retrieve UserName")?
        .to_string();

    // AuthKey abrufen
    let auth_key_res = client
        .get(&gotp_url)
        .header(USER_AGENT, "Tera Game Launcher")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let auth_key_data: Value = auth_key_res.json().await.map_err(|e| e.to_string())?;
    let this_auth_key = auth_key_data["AuthKey"]
        .as_str()
        .ok_or("Failed to retrieve AuthKey")?
        .to_string();

    // CharacterCount abrufen
    let character_count_res = client
        .get(&gcc_url)
        .header(USER_AGENT, "Tera Game Launcher")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let character_count_data: Value = character_count_res
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let this_character_count = character_count_data["CharacterCount"]
        .as_str()
        .ok_or("Failed to retrieve CharacterCount")?
        .to_string();

    // Ergebnis im gewünschten Format zurückgeben
    let result_json = json!({
        "Return": {
            "AuthKey": this_auth_key,
            "UserName": this_username,
            "UserNo": this_user_id,
            "CharacterCount": this_character_count,
            "Permission": this_permission,
            "Privilege": account_info_data["Privilege"].as_i64().unwrap_or(0),
            "Region": account_info_data["Region"].as_str().unwrap_or("Unknown"),
            "Banned": account_info_data["Banned"].as_bool().unwrap_or(false)
        },
        "Msg": this_status
    });

    Ok(result_json.to_string())
}

#[tauri::command]
async fn register_new_account(
    login: String,
    email: String,
    password: String,
) -> Result<String, String> {
    if login.is_empty() || email.is_empty() || password.is_empty() {
        return Err("All fields must be provided".to_string());
    }

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .pool_max_idle_per_host(HTTP_POOL_MAX_IDLE_PER_HOST)
        .build()
        .map_err(|e| e.to_string())?;

    let register_url = get_config_value("REGISTER_ACTION_URL");

    let mut payload = HashMap::new();
    payload.insert("login", login);
    payload.insert("email", email);
    payload.insert("password", password);

    let res = client
        .post(&register_url)
        .header(USER_AGENT, "Tera Game Launcher")
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    // `reqwest::Response::text` consumes the response, so capture the status
    // first and evaluate it after reading the body.
    let status_success = res.status().is_success();
    let text = res.text().await.map_err(|e| e.to_string())?;

    if status_success {
        Ok(text)
    } else {
        Err(text)
    }
}

#[tauri::command]
async fn handle_logout(state: tauri::State<'_, GameState>) -> Result<(), String> {
    let mut is_launching = state.is_launching.lock().await;
    *is_launching = false;

    // Reset global authentication information
    let mut auth_info = GLOBAL_AUTH_INFO.write().unwrap_or_else(|e| e.into_inner());
    auth_info.auth_key = String::new();
    auth_info.user_name = String::new();
    auth_info.user_no = 0;
    auth_info.character_count = String::new();

    Ok(())
}

#[tauri::command]
async fn check_server_connection() -> Result<bool, String> {
    let client = HTTP_CLIENT.clone();

    match client.get(get_config_value("FILE_SERVER_URL")).send().await {
        Ok(response) => Ok(response.status().is_success()),
        Err(e) => Err(e.to_string()),
    }
}

fn main() {
    dotenv().ok();

    // Windows: relaunch elevated via UAC using ShellExecute with "runas" verb.
    // This shows proper UAC dialog and admin shield icon without command prompt flash.
    #[cfg(target_os = "windows")]
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
                let parameters = CString::new(args_str)
                    .expect("Arguments contain null bytes");
                let verb = CString::new("runas")
                    .expect("runas verb contains null bytes - this is a bug");

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

    let game_status_receiver = get_game_status_receiver();
    let game_state = GameState {
        status_receiver: Arc::new(Mutex::new(game_status_receiver)),
        is_launching: Arc::new(Mutex::new(false)),
    };

    tauri::Builder::default()
        .manage(game_state)
        .setup(|app| {
            let window = app.get_window("main")
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
                                        // On success the process may exit/restart depending on platform
                                        // so we avoid showing the window here.
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

            // No log forwarding to frontend.

            // No mirror/broadcast behavior.

            info!("Tauri setup completed");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            handle_launch_game,
            get_game_status,
            select_game_folder,
            get_game_path_from_config,
            save_game_path_to_config,
            reset_launch_state,
            clear_cache,
            login,
            register_new_account,
            set_auth_info,
            get_language_from_config,
            save_language_to_config,
            get_files_to_update,
            update_file,
            handle_logout,
            generate_hash_file,
            check_server_connection,
            check_update_required,
            download_all_files,
            cancel_downloads,
            get_downloaded_bytes,
            set_logging,
            update_launcher,
            // mirror commands removed
            is_debug,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ... (rest of the code remains the same)
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[tokio::test]
    async fn test_login_with_empty_username() {
        let result = login("".to_string(), "pass".to_string()).await;
        assert_eq!(result.unwrap_err(), "Username and password cannot be empty");
    }

    #[test]
    fn resume_offset_returns_existing_when_partial() {
        assert_eq!(resume_offset(1024, 4096), 1024);
    }

    #[test]
    fn resume_offset_returns_zero_when_missing_or_full() {
        assert_eq!(resume_offset(0, 4096), 0);
        assert_eq!(resume_offset(4096, 4096), 0);
    }

    #[test]
    fn resume_offset_returns_zero_when_existing_exceeds_total() {
        assert_eq!(resume_offset(8192, 4096), 0);
    }

    #[test]
    fn compute_initial_downloaded_prefers_resume_override() {
        let files = vec![FileInfo {
            path: "a".to_string(),
            hash: "h".to_string(),
            size: 100,
            url: "u".to_string(),
            existing_size: 20,
        }];
        assert_eq!(compute_initial_downloaded(&files, Some(80)), 80);
    }

    #[test]
    fn compute_initial_downloaded_clamps_to_total_size() {
        let files = vec![FileInfo {
            path: "a".to_string(),
            hash: "h".to_string(),
            size: 100,
            url: "u".to_string(),
            existing_size: 0,
        }];
        assert_eq!(compute_initial_downloaded(&files, Some(150)), 100);
    }

    #[test]
    fn stall_exceeded_detects_no_progress() {
        assert!(stall_exceeded(100, 100, 61, 60));
        assert!(!stall_exceeded(100, 100, 30, 60));
        assert!(!stall_exceeded(100, 120, 61, 60));
    }

    #[test]
    fn get_downloaded_bytes_reads_global_state() {
        GLOBAL_DOWNLOADED_BYTES.store(1234, Ordering::SeqCst);
        assert_eq!(get_downloaded_bytes(), 1234);
    }

    #[test]
    fn game_path_changed_detects_same_path_with_slashes() {
        let prev = Some("C:\\Games\\TERA\\");
        let next = "c:/games/tera";
        assert!(!game_path_changed(prev, next));
    }

    #[test]
    fn game_path_changed_detects_real_change() {
        let prev = Some("C:/Games/TERA");
        let next = "C:/Games/TERA2";
        assert!(game_path_changed(prev, next));
        assert!(game_path_changed(None, next));
    }

    #[test]
    fn transient_error_detection() {
        assert!(is_transient_download_error("request timed out"));
        assert!(is_transient_download_error("connection reset by peer"));
        assert!(is_transient_download_error("HTTP 503"));
        assert!(!is_transient_download_error("hash mismatch"));
    }

    #[test]
    fn retry_delay_grows_by_attempt() {
        assert_eq!(retry_delay_ms(0), 0);
        assert_eq!(retry_delay_ms(1), 500);
        assert_eq!(retry_delay_ms(2), 1000);
    }

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

    // ========================================================================
    // Tests for is_zero
    // ========================================================================

    #[test]
    fn is_zero_returns_true_for_zero() {
        assert!(is_zero(&0));
    }

    #[test]
    fn is_zero_returns_false_for_nonzero() {
        assert!(!is_zero(&1));
        assert!(!is_zero(&100));
        assert!(!is_zero(&u64::MAX));
    }

    // ========================================================================
    // Tests for format_bytes
    // ========================================================================

    #[test]
    fn format_bytes_zero() {
        assert_eq!(format_bytes(0), "0.00 B");
    }

    #[test]
    fn format_bytes_bytes_range() {
        assert_eq!(format_bytes(1), "1.00 B");
        assert_eq!(format_bytes(512), "512.00 B");
        assert_eq!(format_bytes(1023), "1023.00 B");
    }

    #[test]
    fn format_bytes_kb_range() {
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048575), "1024.00 KB");
    }

    #[test]
    fn format_bytes_mb_range() {
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1572864), "1.50 MB");
        assert_eq!(format_bytes(1073741823), "1024.00 MB");
    }

    #[test]
    fn format_bytes_gb_range() {
        assert_eq!(format_bytes(1073741824), "1.00 GB");
        assert_eq!(format_bytes(1610612736), "1.50 GB");
        // Very large value stays in GB
        assert_eq!(format_bytes(10737418240), "10.00 GB");
    }

    #[test]
    fn format_bytes_boundary_cases() {
        // Exactly at KB boundary
        assert_eq!(format_bytes(1024), "1.00 KB");
        // Exactly at MB boundary
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        // Exactly at GB boundary
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    // ========================================================================
    // Tests for normalize_path_for_compare
    // ========================================================================

    #[test]
    fn normalize_path_forward_slashes() {
        assert_eq!(normalize_path_for_compare("c:/games/tera"), "c:/games/tera");
    }

    #[test]
    fn normalize_path_back_slashes_to_forward() {
        assert_eq!(normalize_path_for_compare("c:\\games\\tera"), "c:/games/tera");
    }

    #[test]
    fn normalize_path_mixed_slashes() {
        assert_eq!(normalize_path_for_compare("c:\\games/tera\\sub"), "c:/games/tera/sub");
    }

    #[test]
    fn normalize_path_lowercase() {
        assert_eq!(normalize_path_for_compare("C:/GAMES/TERA"), "c:/games/tera");
        assert_eq!(normalize_path_for_compare("C:\\Games\\Tera"), "c:/games/tera");
    }

    #[test]
    fn normalize_path_removes_trailing_slashes() {
        assert_eq!(normalize_path_for_compare("c:/games/tera/"), "c:/games/tera");
        assert_eq!(normalize_path_for_compare("c:/games/tera//"), "c:/games/tera");
        assert_eq!(normalize_path_for_compare("c:\\games\\tera\\"), "c:/games/tera");
    }

    #[test]
    fn normalize_path_empty_string() {
        assert_eq!(normalize_path_for_compare(""), "");
    }

    #[test]
    fn normalize_path_only_slashes() {
        assert_eq!(normalize_path_for_compare("/"), "");
        assert_eq!(normalize_path_for_compare("//"), "");
        assert_eq!(normalize_path_for_compare("\\"), "");
    }

    // ========================================================================
    // Tests for is_ignored
    // ========================================================================

    #[test]
    fn is_ignored_root_files_ignored() {
        let game_path = Path::new("/games/tera");
        let ignored: HashSet<&str> = HashSet::new();

        // File at root level (no subdirectory) should be ignored
        let root_file = Path::new("/games/tera/somefile.txt");
        assert!(is_ignored(root_file, game_path, &ignored));
    }

    #[test]
    fn is_ignored_exact_match() {
        let game_path = Path::new("/games/tera");
        let mut ignored: HashSet<&str> = HashSet::new();
        ignored.insert("$Patch");

        let patch_dir = Path::new("/games/tera/$Patch/file.txt");
        assert!(is_ignored(patch_dir, game_path, &ignored));
    }

    #[test]
    fn is_ignored_prefix_match() {
        let game_path = Path::new("/games/tera");
        let mut ignored: HashSet<&str> = HashSet::new();
        ignored.insert("S1Game/Logs");

        let log_file = Path::new("/games/tera/S1Game/Logs/game.log");
        assert!(is_ignored(log_file, game_path, &ignored));
    }

    #[test]
    fn is_ignored_non_ignored_path() {
        let game_path = Path::new("/games/tera");
        let mut ignored: HashSet<&str> = HashSet::new();
        ignored.insert("$Patch");
        ignored.insert("S1Game/Logs");

        // A legitimate game file should not be ignored
        let game_file = Path::new("/games/tera/Binaries/TERA.exe");
        assert!(!is_ignored(game_file, game_path, &ignored));
    }

    #[test]
    fn is_ignored_path_outside_game_dir() {
        let game_path = Path::new("/games/tera");
        let ignored: HashSet<&str> = HashSet::new();

        // Path not under game_path returns false
        let outside_path = Path::new("/other/path/file.txt");
        assert!(!is_ignored(outside_path, game_path, &ignored));
    }

    #[test]
    fn is_ignored_handles_backslash_paths() {
        let game_path = Path::new("/games/tera");
        let mut ignored: HashSet<&str> = HashSet::new();
        ignored.insert("S1Game/Config/S1Engine.ini");

        // The function normalizes backslashes to forward slashes
        let config_file = Path::new("/games/tera/S1Game/Config/S1Engine.ini");
        assert!(is_ignored(config_file, game_path, &ignored));
    }

    // ========================================================================
    // Tests for validate_path_within_base (requires temp directory)
    // ========================================================================

    #[test]
    fn validate_path_within_base_valid_path() {
        let temp_dir = std::env::temp_dir().join("test_validate_path");
        let _ = std::fs::create_dir_all(&temp_dir);

        let file_path = temp_dir.join("subdir").join("file.txt");
        let result = validate_path_within_base(&temp_dir, &file_path);

        assert!(result.is_ok());
        let validated = result.unwrap();
        assert!(validated.starts_with(temp_dir.canonicalize().unwrap()));

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn validate_path_within_base_traversal_attempt() {
        let temp_dir = std::env::temp_dir().join("test_validate_traversal");
        let _ = std::fs::create_dir_all(&temp_dir);

        // Attempt path traversal with ..
        let malicious_path = temp_dir.join("..").join("..").join("etc").join("passwd");
        let result = validate_path_within_base(&temp_dir, &malicious_path);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Path traversal detected") || err.contains("outside"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn validate_path_within_base_absolute_outside_path() {
        let temp_dir = std::env::temp_dir().join("test_validate_outside");
        let _ = std::fs::create_dir_all(&temp_dir);

        // Try to validate a path completely outside the base
        let outside_path = std::env::temp_dir().join("completely_different_dir").join("file.txt");
        let _ = std::fs::create_dir_all(outside_path.parent().unwrap());

        let result = validate_path_within_base(&temp_dir, &outside_path);

        assert!(result.is_err());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_dir_all(outside_path.parent().unwrap());
    }

    #[test]
    fn validate_path_within_base_nonexistent_base() {
        let nonexistent_base = Path::new("/nonexistent/base/path/that/does/not/exist");
        let file_path = nonexistent_base.join("file.txt");

        let result = validate_path_within_base(nonexistent_base, &file_path);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to canonicalize base path"));
    }

    #[test]
    fn validate_path_within_base_creates_parent_dirs() {
        let temp_dir = std::env::temp_dir().join("test_validate_create_parent");
        let _ = std::fs::create_dir_all(&temp_dir);

        // Nested path that doesn't exist yet
        let nested_path = temp_dir.join("new").join("nested").join("dir").join("file.txt");
        let result = validate_path_within_base(&temp_dir, &nested_path);

        assert!(result.is_ok());
        // Parent directories should have been created
        assert!(nested_path.parent().unwrap().exists());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn validate_path_within_base_existing_file() {
        let temp_dir = std::env::temp_dir().join("test_validate_existing");
        let _ = std::fs::create_dir_all(&temp_dir);

        // Create an actual file
        let file_path = temp_dir.join("existing_file.txt");
        let _ = std::fs::write(&file_path, "test content");

        let result = validate_path_within_base(&temp_dir, &file_path);

        assert!(result.is_ok());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // ========================================================================
    // Additional tests for compute_initial_downloaded
    // ========================================================================

    #[test]
    fn compute_initial_downloaded_empty_files_array() {
        let files: Vec<FileInfo> = vec![];
        assert_eq!(compute_initial_downloaded(&files, None), 0);
        // With empty files, total_size=0, so override is returned unclamped
        assert_eq!(compute_initial_downloaded(&files, Some(100)), 100);
    }

    #[test]
    fn compute_initial_downloaded_none_resume_override() {
        let files = vec![
            FileInfo {
                path: "a".to_string(),
                hash: "h".to_string(),
                size: 100,
                url: "u".to_string(),
                existing_size: 50,
            },
            FileInfo {
                path: "b".to_string(),
                hash: "h".to_string(),
                size: 200,
                url: "u".to_string(),
                existing_size: 100,
            },
        ];
        // Sum of resume_offsets: 50 + 100 = 150
        assert_eq!(compute_initial_downloaded(&files, None), 150);
    }

    #[test]
    fn compute_initial_downloaded_override_less_than_existing() {
        let files = vec![FileInfo {
            path: "a".to_string(),
            hash: "h".to_string(),
            size: 100,
            url: "u".to_string(),
            existing_size: 80,
        }];
        // Existing is 80, override is 50, so we use existing (80)
        assert_eq!(compute_initial_downloaded(&files, Some(50)), 80);
    }

    #[test]
    fn compute_initial_downloaded_files_with_zero_size() {
        let files = vec![FileInfo {
            path: "a".to_string(),
            hash: "h".to_string(),
            size: 0,
            url: "u".to_string(),
            existing_size: 0,
        }];
        assert_eq!(compute_initial_downloaded(&files, None), 0);
        // With total_size=0, the clamping condition (total_size > 0) is false,
        // so the override value is returned as-is
        assert_eq!(compute_initial_downloaded(&files, Some(100)), 100);
    }

    #[test]
    fn compute_initial_downloaded_clamping_with_positive_size() {
        let files = vec![FileInfo {
            path: "a".to_string(),
            hash: "h".to_string(),
            size: 50,
            url: "u".to_string(),
            existing_size: 0,
        }];
        // Override 100 exceeds total_size 50, should be clamped
        assert_eq!(compute_initial_downloaded(&files, Some(100)), 50);
    }

    // ========================================================================
    // Additional tests for stall_exceeded
    // ========================================================================

    #[test]
    fn stall_exceeded_exactly_at_threshold() {
        // idle_secs == threshold_secs should return true
        assert!(stall_exceeded(100, 100, 60, 60));
    }

    #[test]
    fn stall_exceeded_just_below_threshold() {
        assert!(!stall_exceeded(100, 100, 59, 60));
    }

    #[test]
    fn stall_exceeded_zero_threshold() {
        // With threshold of 0, any idle time >= 0 triggers stall
        assert!(stall_exceeded(100, 100, 0, 0));
    }

    #[test]
    fn stall_exceeded_progress_made_resets() {
        // Even with high idle time, if progress was made, no stall
        assert!(!stall_exceeded(100, 101, 1000, 60));
    }

    // ========================================================================
    // Additional tests for is_transient_download_error
    // ========================================================================

    #[test]
    fn is_transient_all_patterns() {
        // Test all transient patterns
        assert!(is_transient_download_error("request TIMED OUT"));
        assert!(is_transient_download_error("Connection Timeout occurred"));
        assert!(is_transient_download_error("connection reset by peer"));
        assert!(is_transient_download_error("connection closed unexpectedly"));
        assert!(is_transient_download_error("broken pipe error"));
        assert!(is_transient_download_error("service temporarily unavailable"));
        assert!(is_transient_download_error("network error"));
        assert!(is_transient_download_error("DNS resolution failed"));
        assert!(is_transient_download_error("HTTP error 503"));
        assert!(is_transient_download_error("error 502 bad gateway"));
        assert!(is_transient_download_error("gateway timeout 504"));
    }

    #[test]
    fn is_transient_non_transient_errors() {
        assert!(!is_transient_download_error("file not found"));
        assert!(!is_transient_download_error("permission denied"));
        assert!(!is_transient_download_error("hash mismatch"));
        assert!(!is_transient_download_error("invalid response format"));
        assert!(!is_transient_download_error("HTTP 404 not found"));
        assert!(!is_transient_download_error("HTTP 401 unauthorized"));
        assert!(!is_transient_download_error("HTTP 500 internal server error"));
    }

    #[test]
    fn is_transient_case_insensitive() {
        assert!(is_transient_download_error("TIMED OUT"));
        assert!(is_transient_download_error("Timed Out"));
        assert!(is_transient_download_error("CONNECTION RESET"));
        assert!(is_transient_download_error("NETWORK ERROR"));
    }

    #[test]
    fn is_transient_empty_string() {
        assert!(!is_transient_download_error(""));
    }

    // ========================================================================
    // Additional tests for game_path_changed
    // ========================================================================

    #[test]
    fn game_path_changed_same_path_different_case() {
        assert!(!game_path_changed(Some("C:/Games/TERA"), "c:/games/tera"));
    }

    #[test]
    fn game_path_changed_same_with_trailing_slash() {
        assert!(!game_path_changed(Some("C:/Games/TERA"), "C:/Games/TERA/"));
        assert!(!game_path_changed(Some("C:/Games/TERA/"), "C:/Games/TERA"));
    }

    #[test]
    fn game_path_changed_none_previous() {
        assert!(game_path_changed(None, "C:/Games/TERA"));
    }

    // ========================================================================
    // Additional tests for retry_delay_ms
    // ========================================================================

    #[test]
    fn retry_delay_high_attempts() {
        // Test that it doesn't overflow
        assert_eq!(retry_delay_ms(10), 5000);
        assert_eq!(retry_delay_ms(255), 127500);
    }
}
