#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod mirror;
mod s1_events;
mod detect;

// Standard library imports
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File, remove_file};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use std::sync::atomic::{AtomicU64, AtomicUsize, AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};
use std::process::Command;

// Third-party imports
use dotenvy::dotenv;
use log::{LevelFilter, error, info};

use tokio::sync::{watch, Mutex, mpsc, Semaphore};
use tokio::task::JoinSet;
use tokio::io::AsyncWriteExt;
use tokio::io::AsyncReadExt;
use tokio::io::BufWriter;
use tokio::runtime::Runtime;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::Manager;
use tauri::WindowEvent;
use rayon::iter::{ParallelBridge, IntoParallelRefIterator, ParallelIterator, IndexedParallelIterator};
use teralib::{get_game_status_receiver, run_game, reset_global_state, subscribe_game_events, get_last_spawned_pid};
use teralib::config::get_config_value;
use reqwest::Client;
use lazy_static::lazy_static;
use ini::Ini;
use sha2::{Sha256, Digest};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use walkdir::WalkDir;
use reqwest::header::{USER_AGENT, RANGE};

use crate::mirror::client::{start_mirror_client, stop_mirror_client};
use crate::mirror::broadcaster::start_broadcast_server;
use crate::s1_events::S1Event;
use crate::detect::detect_remote_by_pid;

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

// Set the mirror connection target (host, port) from the UI before game/lobby entry
#[tauri::command]
async fn set_mirror_target(host: String, port: u16) -> Result<(), String> {
    let mut target = crate::mirror::MIRROR_TARGET.lock().await;
    *target = Some((host, port));
    Ok(())
}

#[tauri::command]
fn is_debug() -> bool {
    // True in dev builds (cargo tauri dev). False in release/installer builds.
    cfg!(debug_assertions)
}

#[derive(Serialize)]
struct AuthInfo {
    character_count: String,
    permission: i32,
    privilege: i32,
    user_no: i32,
    user_name: String,
    auth_key: String,
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
}



/* const CONFIG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/config/config.json"));

lazy_static::lazy_static! {
    static ref CONFIG_JSON: Value = serde_json::from_str(CONFIG).expect("Failed to parse config");
} */


#[derive(Debug, Serialize, Deserialize, Clone)]
struct FileInfo {
    path: String,
    hash: String,
    size: u64,
    url: String,
}

#[derive(Clone, Serialize)]
struct ProgressPayload {
    file_name: String,
    progress: f64,
    speed: f64,
    downloaded_bytes: u64,
    total_bytes: u64,
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


//static INIT: Once = Once::new();


lazy_static! {
    static ref HASH_CACHE: Mutex<HashMap<String, CachedFileInfo>> = Mutex::new(HashMap::new());
    static ref CANCEL_DOWNLOAD: AtomicBool = AtomicBool::new(false);
}

static GLOBAL_DOWNLOADED_BYTES: AtomicU64 = AtomicU64::new(0);
static CURRENT_FILE_NAME: RwLock<String> = RwLock::new(String::new());


/* fn get_config_value(key: &str) -> String {
    CONFIG_JSON[key].as_str().expect(&format!("{} must be set in config.json", key)).to_string()
} */

fn is_ignored(path: &Path, game_path: &Path, ignored_paths: &HashSet<&str>) -> bool {
    let relative_path = path.strip_prefix(game_path).unwrap().to_str().unwrap().replace("\\", "/");

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
    let client = reqwest::Client::new();
    let res = client
        .get(get_hash_file_url())
        .send().await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(json)
}


fn calculate_file_hash<P: AsRef<Path>>(path: P) -> Result<String, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let mut reader = BufReader::with_capacity(65_536, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65_536];

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

fn save_cache_to_disk(cache: &HashMap<String, CachedFileInfo>) -> Result<(), String> {
    let cache_path = get_cache_file_path()?;
    let serialized = serde_json::to_string(cache).map_err(|e| e.to_string())?;
    let mut file = File::create(cache_path).map_err(|e| e.to_string())?;
    file.write_all(serialized.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}


fn load_cache_from_disk() -> Result<HashMap<String, CachedFileInfo>, String> {
    let cache_path = get_cache_file_path()?;
    let mut file = File::open(cache_path).map_err(|e| e.to_string())?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).map_err(|e| e.to_string())?;
    let cache: HashMap<String, CachedFileInfo> = serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    Ok(cache)
}

//REPAIR CLIENT, DELETE CACHE AND RESTART HASH CHECK
#[tauri::command]
fn clear_cache() -> Result<(), String> {
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
fn set_logging(enabled: bool) -> Result<(), String> {
    teralib::enable_file_logging(enabled)
}

#[tauri::command]
async fn update_launcher(download_url: String) -> Result<(), String> {
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = current_exe.parent().ok_or("exe dir not found")?;
    let new_path = exe_dir.join("launcher_update.exe");

    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())?;

    let bytes = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    tokio::fs::write(&new_path, &bytes).await.map_err(|e| e.to_string())?;

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

fn get_hash_file_url() -> String {
    get_config_value("HASH_FILE_URL")
}

fn get_files_server_url() -> String {
    get_config_value("FILE_SERVER_URL")
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
    let conf = Ini::load_from_file(&config_path).map_err(|e|
        format!("Failed to load config: {}", e)
    )?;

    let section = conf.section(Some("game")).ok_or("Game section not found in config")?;

    let game_path = section.get("path").ok_or("Game path not found in config")?;

    let game_path = PathBuf::from(game_path);

    let game_lang = section.get("lang").ok_or("Game language not found in config")?.to_string();

    Ok((game_path, game_lang))
}

/* fn save_config(game_path: &Path, game_lang: &str) -> Result<(), String> {
    let config_path = find_config_file().ok_or("Config file not found")?;
    let mut conf = Ini::new();

    conf.with_section(Some("game")).set("path", game_path.to_str().ok_or("Invalid game path")?);
    conf.with_section(Some("game")).set("lang", game_lang);

    let mut file = std::fs::File
        ::create(&config_path)
        .map_err(|e| format!("Failed to create config file: {}", e))?;

    conf.write_to(&mut file).map_err(|e| format!("Failed to write config: {}", e))?;

    Ok(())
} */




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
    ].iter().cloned().collect();

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
                let relative_path = path.strip_prefix(&game_path).unwrap().to_str().unwrap().replace("\\", "/");
                info!("Processing file: {}", relative_path);

                let contents = std::fs::read(path).map_err(|e| e.to_string())?;
                let mut hasher = Sha256::new();
                hasher.update(&contents);
                let hash = format!("{:x}", hasher.finalize());
                let size = contents.len() as u64;
                let file_server_url = get_config_value("FILE_SERVER_URL");
                let url = format!("{}/{}", file_server_url, relative_path);

                files.blocking_lock().push(FileInfo {
                    path: relative_path.clone(),
                    hash,
                    size,
                    url,
                });

                total_size.fetch_add(size, Ordering::Relaxed);
                let current_processed = processed_files.fetch_add(1, Ordering::Relaxed) + 1;
                progress_bar.set_position(current_processed);

                let progress = (current_processed as f64 / total_files as f64) * 100.0;
                window.emit("hash_file_progress", json!({
                    "current_file": relative_path,
                    "progress": progress,
                    "processed_files": current_processed,
                    "total_files": total_files,
                    "total_size": total_size.load(Ordering::Relaxed)
                })).map_err(|e| e.to_string())?;
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
    })).map_err(|e| e.to_string())?;

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
async fn save_game_path_to_config(path: String, window: tauri::Window, app_handle: tauri::AppHandle) -> Result<(), String> {
    // Capture previous path before writing, so we can detect actual changes
    let prev_path_string = get_game_path()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()));

    // Simple path normalizer to compare semantically equal paths
    let normalize = |s: &str| -> String {
        let mut t = s.replace('\\', "/");
        while t.ends_with('/') { t.pop(); }
        t.to_lowercase()
    };
    let config_path = find_config_file().ok_or("Config file not found")?;
    let mut conf = Ini::load_from_file(&config_path).map_err(|e|
        format!("Failed to load config: {}", e)
    )?;

    conf.with_section(Some("game")).set("path", &path);

    conf.write_to_file(&config_path).map_err(|e| format!("Failed to write config: {}", e))?;

    // Only interrupt/recheck when path actually changed
    let should_refresh = match &prev_path_string {
        Some(prev) => normalize(prev) != normalize(&path),
        None => true,
    };

    if should_refresh {
        // Interrupt any ongoing downloads
        CANCEL_DOWNLOAD.store(true, Ordering::SeqCst);
        let window_clone = window.clone();
        let app_handle_clone = app_handle.clone();

        // Run file check and download (if needed) in background so UI stays responsive
        tauri::async_runtime::spawn(async move {
            // Give any in-flight tasks a brief moment to observe cancellation
            tokio::time::sleep(Duration::from_millis(50)).await;
            // Perform file check for the new path and download missing files
            match get_files_to_update(window_clone.clone()).await {
                Ok(files) => {
                    if !files.is_empty() {
                        let _ = download_all_files(app_handle_clone, window_clone, files).await;
                    }
                }
                Err(e) => {
                    let _ = window_clone.emit("error", e);
                }
            }
        });
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
    window: tauri::Window,
    file_info: FileInfo,
    total_files: usize,
    current_file_index: usize,
    total_size: u64,
) -> Result<u64, String> {
    // Update the current file name for global progress tracking
    {
        let mut current_file = CURRENT_FILE_NAME.write().unwrap();
        *current_file = file_info.path.clone();
    }
    
    let game_path = get_game_path()?;
    let file_path = game_path.join(&file_info.path);

    //println!("Game Path: {}", game_path.display());
    //println!("File Path: {}", file_path.display());
    //println!("FileInfo: {}", file_info.url);

    if let Some(parent) = file_path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    }

    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(300)) // 5 minute timeout
        .connect_timeout(Duration::from_secs(30)) // 30 second connect timeout
        .tcp_keepalive(Duration::from_secs(60)) // Keep connections alive
        .pool_max_idle_per_host(20) // More connection pooling
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| e.to_string())?;

    let mut corrected_url = file_info.url.clone();
    if let Some(pos) = corrected_url.find("/files/") {
        corrected_url = format!("{}{}", &corrected_url[..pos], &corrected_url[(pos + 7)..]);
    }

    // When a file needs to be updated we always download it from scratch.
    // Resuming downloads was causing issues if the server file changed because
    // the launcher would append the new data to the old file.
    if file_path.exists() {
        tokio::fs::remove_file(&file_path).await.map_err(|e| e.to_string())?;
    }
    let file_handle = tokio::fs::File::create(&file_path).await.map_err(|e| e.to_string())?;
    let mut file = BufWriter::with_capacity(1024 * 1024, file_handle); // 1MB buffer

    let file_size = file_info.size;

    // Thresholds for chunked parallelism
    const CHUNK_MIN_SIZE: u64 = 16 * 1024 * 1024; // 16 MB
    const PART_SIZE: u64 = 32 * 1024 * 1024; // 32 MB
    const MAX_PARTS: usize = 32;

    info!("Downloading file: {} ({} bytes)", file_info.path, file_size);
    let download_start = Instant::now();
    let mut bytes_written: u64 = 0;

    if file_size >= CHUNK_MIN_SIZE {
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
            info!("Server does not support range requests. Falling back to single-stream for {}", file_info.path);
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
            let start_time = Instant::now();
            let mut last_update = Instant::now();

            while let Some(chunk_result) = stream.next().await {
                if CANCEL_DOWNLOAD.load(Ordering::SeqCst) { return Err("cancelled".into()); }
                let chunk = chunk_result.map_err(|e| e.to_string())?;
                file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                let len = chunk.len() as u64;
                downloaded += len;
                GLOBAL_DOWNLOADED_BYTES.fetch_add(len, Ordering::SeqCst);

                let now = Instant::now();
            }
            bytes_written = downloaded;
        } else {
        // Perform chunked parallel download using HTTP ranges into temp parts
        let num_parts = std::cmp::max(1, std::cmp::min(MAX_PARTS as u64, (file_size + PART_SIZE - 1) / PART_SIZE) as usize);
        let mut join_set: JoinSet<Result<(), String>> = JoinSet::new();

        for part_idx in 0..num_parts {
            if CANCEL_DOWNLOAD.load(Ordering::SeqCst) { return Err("cancelled".into()); }

            let start = (part_idx as u64) * PART_SIZE;
            let mut end = ((part_idx as u64 + 1) * PART_SIZE).saturating_sub(1);
            if end >= file_size { end = file_size - 1; }

            let part_url = corrected_url.clone();
            let window_cl = window.clone();
            let part_path = file_path.with_extension(format!("part{}", part_idx));
            let file_name = file_info.path.clone();

            join_set.spawn(async move {
                let client = reqwest::Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(300))
                    .connect_timeout(Duration::from_secs(30))
                    .tcp_keepalive(Duration::from_secs(60))
                    .pool_max_idle_per_host(20)
                    .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                    .build()
                    .map_err(|e| e.to_string())?;
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
                let mut part_file = BufWriter::with_capacity(1024 * 1024,
                    tokio::fs::File::create(&part_path).await.map_err(|e| e.to_string())?
                );
                let mut last_update = Instant::now();
                let mut part_downloaded: u64 = 0;
                let start_time = Instant::now();

                while let Some(chunk_result) = stream.next().await {
                    if CANCEL_DOWNLOAD.load(Ordering::SeqCst) { return Err("cancelled".into()); }
                    let chunk = chunk_result.map_err(|e| e.to_string())?;
                    part_file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                    let len = chunk.len() as u64;
                    part_downloaded += len;
                    GLOBAL_DOWNLOADED_BYTES.fetch_add(len, Ordering::SeqCst);

                    let now = Instant::now();
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
            let mut part_f = tokio::fs::File::open(&part_path).await.map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let n = part_f.read(&mut buf).await.map_err(|e| e.to_string())?;
                if n == 0 { break; }
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
        let start_time = Instant::now();
        let mut last_update = Instant::now();

        while let Some(chunk_result) = stream.next().await {
            if CANCEL_DOWNLOAD.load(Ordering::SeqCst) { return Err("cancelled".into()); }
            let chunk = chunk_result.map_err(|e| e.to_string())?;
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            let len = chunk.len() as u64;
            downloaded += len;
            GLOBAL_DOWNLOADED_BYTES.fetch_add(len, Ordering::SeqCst);

            let now = Instant::now();
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

#[tauri::command]
async fn download_all_files(
    app_handle: tauri::AppHandle,
    window: tauri::Window,
    files_to_update: Vec<FileInfo>
) -> Result<Vec<u64>, String> {
    let total_files = files_to_update.len();
    let total_size: u64 = files_to_update.iter().map(|f| f.size).sum();

    if total_files == 0 {
        info!("No files to download");
        if let Err(e) = window.emit("download_complete", ()) {
            error!("Failed to emit download_complete event: {}", e);
        }
        return Ok(vec![]);
    }

    let mut results: Vec<Option<u64>> = vec![None; total_files];
    let mut files_by_index: Vec<Option<FileInfo>> = vec![None; total_files];
    GLOBAL_DOWNLOADED_BYTES.store(0, Ordering::SeqCst);
    CANCEL_DOWNLOAD.store(false, Ordering::SeqCst);

    const MAX_CONCURRENT_DOWNLOADS: usize = 16;
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_DOWNLOADS));
    let global_start = Instant::now();

    // Emit a smooth global progress tick to stabilize UI speed/ETA
    {
        let window_tick = window.clone();
        let total_bytes_tick = total_size;
        let total_files_tick = total_files;
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            loop {
                interval.tick().await;
                let d = GLOBAL_DOWNLOADED_BYTES.load(Ordering::SeqCst);
                let elapsed = global_start.elapsed();
                let speed = if elapsed.as_secs() > 0 { d / elapsed.as_secs() } else { 0 };
                let current_file_name = {
                    let current_file = CURRENT_FILE_NAME.read().unwrap();
                    current_file.clone()
                };
                
                let payload = ProgressPayload {
                    file_name: current_file_name,
                    progress: if total_bytes_tick > 0 { (d as f64 / total_bytes_tick as f64) * 100.0 } else { 0.0 },
                    speed: speed as f64,
                    downloaded_bytes: d,
                    total_bytes: total_bytes_tick,
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
        let permit = semaphore.clone().acquire_owned().await.map_err(|e| e.to_string())?;
        let app_handle_cl = app_handle.clone();
        let window_cl = window.clone();
        let file_info_cl = file_info.clone();
        join_set.spawn(async move {
            let _permit = permit;
            let res = update_file(
                app_handle_cl,
                window_cl,
                file_info_cl,
                total_files,
                index + 1,
                total_size,
            ).await;
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
                    return Err(e);
                }
            }
            Err(e) => return Err(format!("Join error: {}", e)),
        }
    }

    let downloaded_sizes: Vec<u64> = results.into_iter().filter_map(|x| x).collect();

    if !CANCEL_DOWNLOAD.load(Ordering::SeqCst) {
        // Post-download verification of files that completed
        for (idx, maybe_size) in downloaded_sizes.iter().enumerate() {
            let _ = maybe_size; // size not needed for verification
        }
        for (idx, maybe_file) in files_by_index.into_iter().enumerate() {
            // Only verify files that finished successfully
            // (we used results before moving it; so check via downloaded_sizes length wouldn't align by idx)
            // Instead, recompute: if handler returned Some for this idx we already moved results, so we can't check here.
            // To avoid confusion, re-run a cheap metadata check: if the file exists, verify hash.
            if CANCEL_DOWNLOAD.load(Ordering::SeqCst) { break; }
            if maybe_file.is_none() { continue; }
            let file_info = maybe_file.unwrap();
            let game_path = get_game_path()?;
            let file_path = game_path.join(&file_info.path);
            if !file_path.exists() { continue; }
            let expected_hash = file_info.hash.clone();
            let calc = tokio::task::spawn_blocking(move || {
                calculate_file_hash(&file_path)
            }).await.map_err(|e| e.to_string())??;
            if calc != expected_hash {
                return Err(format!("Hash mismatch after download for file: {}", file_info.path));
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
    let files = server_hash_file["files"].as_array().ok_or("Invalid server hash file format")?;
    info!("Server hash file parsed, {} files found", files.len());

    info!("Starting file comparison");
    let _cache = load_cache_from_disk().unwrap_or_else(|_| HashMap::new());
    let cache = Arc::new(RwLock::new(_cache));

    let progress_bar = ProgressBar::new(files.len() as u64);
    progress_bar.set_style(ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
        .unwrap()
        .progress_chars("##-"));

    let processed_count = Arc::new(AtomicUsize::new(0));
    let files_to_update_count = Arc::new(AtomicUsize::new(0));
    let total_size = Arc::new(AtomicU64::new(0));

    let files_to_update: Vec<FileInfo> = files.par_iter().enumerate()
        .filter_map(|(_index, file_info)| {
            let path = file_info["path"].as_str().unwrap_or("");
            let server_hash = file_info["hash"].as_str().unwrap_or("");
            let size = file_info["size"].as_u64().unwrap_or(0);
            let url = file_info["url"].as_str().unwrap_or("").to_string();

            let local_file_path = local_game_path.join(path);

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

                let _ = window.emit("file_check_progress", progress_payload)
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
                    });
                }
            };

            let last_modified = metadata.modified().ok();

            let cache_read = cache.read().unwrap();
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
                    });
                }
            };

            let mut cache_write = cache.write().unwrap();
            cache_write.insert(path.to_string(), CachedFileInfo {
                hash: local_hash.clone(),
                last_modified: last_modified.unwrap_or_else(SystemTime::now),
            });
            drop(cache_write);

            if local_hash != server_hash {
                files_to_update_count.fetch_add(1, Ordering::SeqCst);
                total_size.fetch_add(size, Ordering::SeqCst);
                Some(FileInfo {
                    path: path.to_string(),
                    hash: server_hash.to_string(),
                    size,
                    url,
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

    let _ = window.emit("file_check_progress", final_progress).map_err(|e| {
        error!("Error emitting final file_check_progress event: {}", e);
        e.to_string()
    });

    // Save the updated cache to disk
    let final_cache = cache.read().unwrap();
    if let Err(e) = save_cache_to_disk(&*final_cache) {
        error!("Failed to save cache to disk: {}", e);
    }

    let total_time = start_time.elapsed();
    info!("File comparison completed. Files to update: {}", files_to_update.len());

    // Emit a final event with complete statistics
    let _ = window.emit("file_check_completed", json!({
        "total_files": files.len(),
        "files_to_update": files_to_update.len(),
        "total_size": total_size.load(Ordering::SeqCst),
        "total_time_seconds": total_time.as_secs(),
        "average_time_per_file_ms": (total_time.as_millis() as f64) / (files.len() as f64)
    }));

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
    state: tauri::State<'_, GameState>
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

    let auth_info = GLOBAL_AUTH_INFO.read().unwrap();
    let account_name = auth_info.user_no.to_string();
    let characters_count = auth_info.character_count.clone();
    let ticket = auth_info.auth_key.clone();
    let (game_path, game_lang) = load_config()?;

    let full_game_path = game_path.join("Binaries").join("TERA.exe");

    if !full_game_path.exists() {
        *is_launching = false;
        return Err(format!("Game executable not found at: {:?}", full_game_path));
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

        match
            run_game(
                &account_name,
                &characters_count,
                &ticket,
                &game_lang,
                &full_game_path_str
            ).await
        {
            Ok(exit_status) => {
                let result = format!("Game exited with status: {:?}", exit_status);
                app_handle_clone.emit_all("game_status", &result).unwrap();
                info!("{}", result);
            }
            Err(e) => {
                let error = format!("Error launching game: {:?}", e);
                app_handle_clone.emit_all("game_status", &error).unwrap();
                error!("{}", error);
            }
        }

        info!("Emitting game_ended event");
        if let Err(e) = app_handle_clone.emit_all("game_ended", ()) {
            error!("Failed to emit game_ended event: {:?}", e);
        }

        // Stop mirror client when the game ends to clean up background tasks
        if let Err(e) = stop_mirror_client().await {
            error!("Failed to stop mirror client after game end: {}", e);
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
    let mut conf = Ini::load_from_file(&config_path).map_err(|e|
        format!("Failed to load config: {}", e)
    )?;

    conf.with_section(Some("game")).set("lang", &language);

    conf.write_to_file(&config_path).map_err(|e| format!("Failed to write config: {}", e))?;

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
    let mut auth_info = GLOBAL_AUTH_INFO.write().unwrap();
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

    let character_count_data: Value = character_count_res.json().await.map_err(|e| e.to_string())?;
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
async fn register_new_account(login: String, email: String, password: String) -> Result<String, String> {
    if login.is_empty() || email.is_empty() || password.is_empty() {
        return Err("All fields must be provided".to_string());
    }

    let client = reqwest::Client::builder()
        .cookie_store(true)
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
    let mut auth_info = GLOBAL_AUTH_INFO.write().unwrap();
    auth_info.auth_key = String::new();
    auth_info.user_name = String::new();
    auth_info.user_no = 0;
    auth_info.character_count = String::new();

    Ok(())
}

#[tauri::command]
async fn check_server_connection() -> Result<bool, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    match client.get(get_files_server_url()).send().await {
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
        use winapi::um::shellapi::ShellExecuteA;
        use winapi::um::winuser::SW_SHOWNORMAL;
        use std::ptr;

        // If the special flag is not present, relaunch self elevated and append it.
        let is_guard_present = std::env::args().any(|a| a == "--elevated");
        if !is_guard_present {
            if let Ok(current_exe) = std::env::current_exe() {
                // Preserve original args and append our guard flag
                let mut args: Vec<String> = std::env::args().skip(1).collect();
                args.push("--elevated".to_string());
                let args_str = args.join(" ");

                // Convert to CString for Windows API
                let exe_path = CString::new(current_exe.to_string_lossy().as_ref()).unwrap();
                let parameters = CString::new(args_str).unwrap();
                let verb = CString::new("runas").unwrap();

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

    let (tera_logger, mut tera_log_receiver) = teralib::setup_logging();

    // Configure only the teralib logger
    log::set_boxed_logger(Box::new(tera_logger)).expect("Failed to set logger");
    log::set_max_level(LevelFilter::Info);

    // Create an asynchronous channel for logs
    let (log_sender, mut log_receiver) = mpsc::channel::<String>(100);

    // Create a Tokio runtime
    let rt = Runtime::new().expect("Failed to create Tokio runtime");

    // Spawn a task to receive logs and send them through the channel
    rt.spawn(async move {
        while let Some(log_message) = tera_log_receiver.recv().await {
            info!("Teralib: {}", log_message);
            if let Err(e) = log_sender.send(log_message).await {
                error!("Failed to send log message: {}", e);
            }
        }
    });


    let game_status_receiver = get_game_status_receiver();
    let game_state = GameState {
        status_receiver: Arc::new(Mutex::new(game_status_receiver)),
        is_launching: Arc::new(Mutex::new(false)),
    };

    tauri::Builder
        ::default()
        .manage(game_state)
        .setup(|app| {
            let window = app.get_window("main").unwrap();
            let app_handle = app.handle();
            info!("Tauri setup started");

            // Spawn an asynchronous task to receive logs from the channel and send them to the frontend
            tauri::async_runtime::spawn(async move {
                while let Some(log_message) = log_receiver.recv().await {
                    let _ = app_handle.emit_all("log_message", log_message);
                }
            });

            // Start localhost broadcast server (127.0.0.1:7802) and keep it running
            start_broadcast_server(&app.handle());

            // Health check to ensure broadcast server stays running
            let app_handle_broadcaster = app.handle();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    start_broadcast_server(&app_handle_broadcaster); // Restart if needed
                }
            });

            // Subscribe to game events
            let app_handle_events = app.handle();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create Tokio runtime");
                
                rt.block_on(async move {
                    let mut rx = subscribe_game_events();
                    
                    loop {
                        match rx.recv().await {
                            Ok((code, _payload)) => {
                                let event = S1Event::from(code);

                                if event.should_stop_mirror_client() {
                                    if let Err(e) = stop_mirror_client().await {
                                        error!("Failed to stop mirror client: {}", e);
                                    }
                                } else if event.should_start_mirror_client() {
                                    let app_handle_clone = app_handle_events.clone();
                                    
                                    if let Some(pid) = get_last_spawned_pid() {
                                        if let Some((host, port)) = detect_remote_by_pid(pid).await {
                                            if let Some(win) = app_handle_clone.get_window("main") {
                                                if let Err(e) = start_mirror_client(win, host, port).await {
                                                    error!("Failed to start mirror client: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }

                                let _ = app_handle_events.emit_all("s1_event", code);
                            }
                            Err(e) => {
                                match e {
                                    tokio::sync::broadcast::error::RecvError::Closed => {
                                        break;
                                    }
                                    tokio::sync::broadcast::error::RecvError::Lagged(skipped) => {
                                        error!("Event receiver lagged, skipped {} events", skipped);
                                    }
                                }
                            }
                        }
                    }
                });
            });

            // Ensure mirror client is stopped when the main window is closing
            let window_for_close = window.clone();
            window_for_close.on_window_event(|event| {
                if let WindowEvent::CloseRequested { .. } = event {
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = stop_mirror_client().await {
                            error!("Failed to stop mirror client on window close: {}", e);
                        }
                    });
                }
            });

            info!("Tauri setup completed");
            Ok(())
        })
        .invoke_handler(
            tauri::generate_handler![
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
                set_logging,
                update_launcher,
                start_mirror_client,
                stop_mirror_client,
                set_mirror_target,
                is_debug,
            ]
        )
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ... (rest of the code remains the same)
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_login_with_empty_username() {
        let result = login("".to_string(), "pass".to_string()).await;
        assert_eq!(result.unwrap_err(), "Username and password cannot be empty");
    }
}
