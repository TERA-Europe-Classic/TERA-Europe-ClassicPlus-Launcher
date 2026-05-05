// Shared between the main launcher bin and several experimental tooling
// bins via `#[path = ...]` includes; each compilation context exercises
// a different subset, so any single bin sees the rest as "dead".
#![allow(dead_code)]

//! External-app mod lifecycle: download, extract, spawn, monitor.
//!
//! External apps are separate executables (Shinra Meter, TCC). Per the design
//! doc they:
//!   1. Are downloaded from GitHub Releases (zip) via [`download_and_extract`].
//!   2. Live under `<app_data>/mods/external/<mod-id>/`.
//!   3. Are spawned via [`spawn_app`] when the user enables + auto-launch is on,
//!      or when the game launches.
//!   4. Are detected as already-running via [`is_process_running`] so we
//!      don't double-spawn.

use std::fs;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::process::Command;

use reqwest::header::LOCATION;
use sha2::{Digest, Sha256};
use sysinfo::System;

const MAX_ALLOWED_REDIRECTS: usize = 3;
const ALLOWED_REDIRECT_HOSTS: &[&str] = &[
    "github.com",
    "release-assets.githubusercontent.com",
    "objects.githubusercontent.com",
];

/// Downloads the zip at `url`, verifies the SHA-256 matches `expected_sha256`
/// (hex, lowercase), and extracts it into `dest_dir`. Any existing contents
/// of `dest_dir` are wiped first — this is an install, not a merge.
///
/// `on_progress` is called with (bytes_read, total_or_zero) as the HTTP body
/// streams in so the UI download tray can render a live progress bar.
/// Returns the absolute path to the extracted root directory.
pub async fn download_and_extract(
    url: &str,
    expected_sha256: &str,
    dest_dir: &Path,
    on_progress: impl FnMut(u64, u64) + Send,
) -> Result<PathBuf, String> {
    let bytes = fetch_bytes_streaming(url, on_progress).await?;

    let actual = hex_lower(&Sha256::digest(&bytes));
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(format!(
            "Download hash mismatch: expected {}, got {}",
            expected_sha256, actual
        ));
    }

    let preserved_settings = collect_preserved_user_settings(dest_dir)?;

    if let Err(e) = clear_existing_install_dir(dest_dir) {
        return restore_preserved_then_return_error(dest_dir, &preserved_settings, e);
    }
    if let Err(e) = fs::create_dir_all(dest_dir) {
        return restore_preserved_then_return_error(
            dest_dir,
            &preserved_settings,
            format!("Failed to create {}: {}", dest_dir.display(), e),
        );
    }

    // PRD 3.2.8.disk-full-revert: if zip extraction fails partway — classic
    // trigger is ENOSPC on Windows, where half the files are on disk and
    // the rest error out — remove the entire dest dir so the user's
    // next retry starts from a clean slate. Without this, Play would try
    // to spawn an executable that's missing its dependent DLLs.
    if let Err(e) = extract_zip(&bytes, dest_dir) {
        revert_partial_install_dir(dest_dir);
        return restore_preserved_then_return_error(dest_dir, &preserved_settings, e);
    }
    restore_preserved_user_settings(dest_dir, &preserved_settings)?;

    Ok(dest_dir.to_path_buf())
}

fn restore_preserved_then_return_error<T>(
    dest_dir: &Path,
    preserved: &[(PathBuf, Vec<u8>)],
    original_error: String,
) -> Result<T, String> {
    if let Err(restore_error) = restore_preserved_user_settings(dest_dir, preserved) {
        return Err(format!(
            "{}; additionally failed to restore preserved settings: {}",
            original_error, restore_error
        ));
    }
    Err(original_error)
}

fn clear_existing_install_dir(dest_dir: &Path) -> Result<(), String> {
    if !dest_dir.exists() {
        return Ok(());
    }

    let mut last_error = None;
    for attempt in 1..=3 {
        make_tree_writable(dest_dir);
        match fs::remove_dir_all(dest_dir) {
            Ok(_) => return Ok(()),
            Err(err) => {
                last_error = Some(err);
                if attempt < 3 {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
            }
        }
    }

    let err = last_error
        .map(|e| e.to_string())
        .unwrap_or_else(|| "unknown error".to_string());
    Err(format!("Failed to clear {}: {}", dest_dir.display(), err))
}

fn make_tree_writable(root: &Path) {
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if let Ok(metadata) = fs::metadata(path) {
            let mut permissions = metadata.permissions();
            if permissions.readonly() {
                // `set_readonly(false)` is correct on Windows (the only target
                // for the launcher binary). The clippy `permissions_set_readonly_false`
                // lint warns because on POSIX this clears all write bits including
                // group/other, which is undesirable. The launcher only ships for
                // Windows, where this maps to clearing FILE_ATTRIBUTE_READONLY.
                #[allow(clippy::permissions_set_readonly_false)]
                permissions.set_readonly(false);
                let _ = fs::set_permissions(path, permissions);
            }
        }
    }
}

fn collect_preserved_user_settings(dest_dir: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    if !dest_dir.exists() {
        return Ok(Vec::new());
    }

    let mut preserved = Vec::new();
    for entry in walkdir::WalkDir::new(dest_dir)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(dest_dir)
            .map_err(|e| format!("Failed to inspect {}: {}", path.display(), e))?;
        if is_preserved_user_state_path(relative) {
            let bytes = fs::read(path)
                .map_err(|e| format!("Failed to preserve {}: {}", path.display(), e))?;
            preserved.push((relative.to_path_buf(), bytes));
        }
    }
    Ok(preserved)
}

fn is_preserved_user_state_path(relative: &Path) -> bool {
    let components: Vec<String> = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str().map(str::to_ascii_lowercase))
        .collect();

    matches_user_state_dir(&components, &["config"])
        || matches_user_state_dir(&components, &["sound"])
        || matches_user_state_dir(&components, &["resources", "config"])
        || matches_user_state_dir(&components, &["resources", "sound"])
}

fn matches_user_state_dir(components: &[String], prefix: &[&str]) -> bool {
    components.len() > prefix.len()
        && components
            .iter()
            .zip(prefix.iter())
            .all(|(component, expected)| component == expected)
}

fn restore_preserved_user_settings(
    dest_dir: &Path,
    preserved: &[(PathBuf, Vec<u8>)],
) -> Result<(), String> {
    for (relative, bytes) in preserved {
        let path = dest_dir.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!("Failed to restore settings dir {}: {}", parent.display(), e)
            })?;
        }
        fs::write(&path, bytes).map_err(|e| {
            format!(
                "Failed to restore preserved setting {}: {}",
                path.display(),
                e
            )
        })?;
    }
    Ok(())
}

/// Best-effort cleanup of a partially-populated install dir after a
/// download/extract failure. Logs but never propagates — the primary
/// error the caller is returning is what matters; cleanup failure just
/// means the user retry will take slightly longer.
pub(crate) fn revert_partial_install_dir(dest_dir: &Path) {
    match fs::remove_dir_all(dest_dir) {
        Ok(_) => log::info!(
            "Reverted partial install at {} after extract failure",
            dest_dir.display()
        ),
        Err(e) => log::warn!(
            "Could not fully revert partial install at {}: {}",
            dest_dir.display(),
            e
        ),
    }
}

/// Best-effort cleanup of a partially-written file (e.g. a GPK the OS
/// truncated after ENOSPC mid-write). Symmetric to
/// `revert_partial_install_dir` for the single-file path.
pub(crate) fn revert_partial_install_file(dest_file: &Path) {
    if !dest_file.exists() {
        return;
    }
    match fs::remove_file(dest_file) {
        Ok(_) => log::info!(
            "Reverted partial file at {} after write failure",
            dest_file.display()
        ),
        Err(e) => log::warn!(
            "Could not remove partial file at {}: {}",
            dest_file.display(),
            e
        ),
    }
}

/// Downloads any URL, verifies its SHA-256, and writes it to `dest_file`.
/// Used by GPK install where we only need the file on disk, no zip
/// extraction. Same streaming progress contract as `download_and_extract`.
pub async fn download_file(
    url: &str,
    expected_sha256: &str,
    dest_file: &Path,
    on_progress: impl FnMut(u64, u64) + Send,
) -> Result<PathBuf, String> {
    let bytes = fetch_bytes_streaming(url, on_progress).await?;

    let actual = hex_lower(&Sha256::digest(&bytes));
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(format!(
            "Download hash mismatch: expected {}, got {}",
            expected_sha256, actual
        ));
    }

    if let Some(parent) = dest_file.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
    }
    // PRD 3.2.8.disk-full-revert: if fs::write fails mid-stream (ENOSPC is
    // the common cause on the Windows install path), the OS may have
    // truncated the dest file — remove the partial so next retry doesn't
    // feed a zero-byte GPK to the mapper patcher.
    if let Err(e) = fs::write(dest_file, &bytes) {
        revert_partial_install_file(dest_file);
        return Err(format!("Failed to write {}: {}", dest_file.display(), e));
    }
    Ok(dest_file.to_path_buf())
}

/// Streams the HTTP body into memory, invoking `on_progress(bytes_read, total)`
/// every time a chunk arrives. `total = 0` means Content-Length was unknown,
/// in which case the UI should render an indeterminate bar. The final call
/// fires after the last chunk, so callers can treat that as 100%.
async fn fetch_bytes_streaming(
    url: &str,
    mut on_progress: impl FnMut(u64, u64) + Send,
) -> Result<Vec<u8>, String> {
    use futures_util::StreamExt;

    // adv.http-redirect-offlist: the launcher's HTTP scope is pinned to a
    // handful of known hosts (capabilities/migrated.json + the
    // http_allowlist integration test). reqwest's default redirect policy
    // follows up to 10 redirects, which would let a compromised allowlist
    // host bounce downloads to an off-list server via 3xx. Policy::none()
    // surfaces 302s as status codes the `!is_success()` branch rejects.
    let client = reqwest::Client::builder()
        .user_agent("TERA-Europe-ClassicPlus-Launcher")
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let mut current_url = reqwest::Url::parse(url)
        .map_err(|e| format!("Failed to parse download URL {}: {}", url, e))?;
    let mut redirects_followed = 0usize;
    let response = loop {
        let response = client
            .get(current_url.clone())
            .send()
            .await
            .map_err(|e| format!("Failed to download from {}: {}", current_url, e))?;

        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(LOCATION)
                .ok_or_else(|| {
                    format!(
                        "Download returned HTTP {} from {} without a Location header",
                        response.status(),
                        current_url
                    )
                })?
                .to_str()
                .map_err(|e| format!("Redirect Location header was not valid UTF-8: {}", e))?;

            let next_url = current_url
                .join(location)
                .or_else(|_| reqwest::Url::parse(location))
                .map_err(|e| format!("Failed to resolve redirect target {}: {}", location, e))?;

            let host = next_url.host_str().ok_or_else(|| {
                format!("Redirect target {} has no valid host component", next_url)
            })?;

            if !redirect_host_is_allowed(host) {
                return Err(format!(
                    "Download redirect to {} is not allowed (from {})",
                    host, current_url
                ));
            }

            redirects_followed += 1;
            if redirects_followed > MAX_ALLOWED_REDIRECTS {
                return Err(format!(
                    "Download exceeded the redirect limit ({}) starting from {}",
                    MAX_ALLOWED_REDIRECTS, url
                ));
            }

            current_url = next_url;
            continue;
        }

        break response;
    };

    if !response.status().is_success() {
        return Err(format!(
            "Download returned HTTP {} from {}",
            response.status(),
            current_url
        ));
    }

    let total = response.content_length().unwrap_or(0);
    let mut buf: Vec<u8> = Vec::with_capacity(total as usize);
    let mut stream = response.bytes_stream();
    let mut received: u64 = 0;
    on_progress(0, total);

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download stream failed: {}", e))?;
        received += chunk.len() as u64;
        buf.extend_from_slice(&chunk);
        on_progress(received, total);
    }

    Ok(buf)
}

fn redirect_host_is_allowed(host: &str) -> bool {
    ALLOWED_REDIRECT_HOSTS
        .iter()
        .any(|allowed| host.eq_ignore_ascii_case(allowed))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

/// Extracts a zip archive's contents into `dest_dir`. Rejects entries whose
/// path escapes the destination (zip-slip).
fn extract_zip(data: &[u8], dest_dir: &Path) -> Result<(), String> {
    let cursor = Cursor::new(data);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("Invalid zip archive: {}", e))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read zip entry {}: {}", i, e))?;

        let rel = match file.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => {
                return Err(format!(
                    "Zip entry '{}' escapes the archive root (zip-slip rejected)",
                    file.name()
                ));
            }
        };
        let out_path = dest_dir.join(&rel);

        if file.is_dir() {
            fs::create_dir_all(&out_path).map_err(to_extract_err)?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(to_extract_err)?;
        }
        let mut buf = Vec::with_capacity(file.size() as usize);
        io::copy(&mut file, &mut buf).map_err(to_extract_err)?;
        fs::write(&out_path, &buf).map_err(to_extract_err)?;
    }
    Ok(())
}

fn to_extract_err(e: io::Error) -> String {
    format!("Zip extract failed: {}", e)
}

/// Spawns the external app. Uses `CREATE_NO_WINDOW` on Windows to avoid a
/// console flash; returns the child PID for tracking.
///
/// Note: we deliberately do NOT wait on the child. The launcher monitors
/// liveness via [`is_process_running`] in a polling loop; the child is its
/// own top-level process so it survives the launcher restarting.
pub fn spawn_app(exe_path: &Path, args: &[String]) -> Result<u32, String> {
    if !exe_path.exists() {
        return Err(format!("Executable not found: {}", exe_path.display()));
    }

    // On Windows, prefer ShellExecuteW over CreateProcess because some
    // distributable exes (old ShinraMeter releases, any app with a
    // requireAdministrator manifest) refuse to start via CreateProcess
    // with "The requested operation requires elevation. (os error 740)".
    // ShellExecute with the default verb triggers the UAC prompt if
    // elevation is needed; otherwise it launches silently like CreateProcess.
    #[cfg(windows)]
    {
        spawn_app_shellexec(exe_path, args)
    }

    #[cfg(not(windows))]
    {
        let mut cmd = Command::new(exe_path);
        cmd.args(args);
        if let Some(parent) = exe_path.parent() {
            cmd.current_dir(parent);
        }
        let child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn {}: {}", exe_path.display(), e))?;
        Ok(child.id())
    }
}

#[cfg(windows)]
fn spawn_app_shellexec(exe_path: &Path, args: &[String]) -> Result<u32, String> {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use winapi::um::shellapi::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use winapi::um::winuser::SW_SHOWNORMAL;

    let to_wide = |s: &OsStr| -> Vec<u16> { s.encode_wide().chain(once(0)).collect() };
    let file = to_wide(exe_path.as_os_str());
    let params_string: String = args.join(" ");
    let params = to_wide(OsStr::new(&params_string));
    let dir_owned: Option<Vec<u16>> = exe_path.parent().map(|p| to_wide(p.as_os_str()));

    let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    sei.fMask = SEE_MASK_NOCLOSEPROCESS;
    sei.hwnd = null_mut();
    sei.lpVerb = null_mut(); // default verb — handles UAC when needed
    sei.lpFile = file.as_ptr();
    sei.lpParameters = if params.len() > 1 {
        params.as_ptr()
    } else {
        null_mut()
    };
    sei.lpDirectory = dir_owned.as_ref().map(|v| v.as_ptr()).unwrap_or(null_mut());
    sei.nShow = SW_SHOWNORMAL;

    let ok = unsafe { ShellExecuteExW(&mut sei) };
    if ok == 0 {
        let err = unsafe { winapi::um::errhandlingapi::GetLastError() };
        return Err(format!(
            "ShellExecuteEx failed for {}: Win32 error {}",
            exe_path.display(),
            err
        ));
    }

    // Best-effort PID — derive it from the returned process handle.
    let pid = if sei.hProcess.is_null() {
        0
    } else {
        unsafe { winapi::um::processthreadsapi::GetProcessId(sei.hProcess) }
    };
    if !sei.hProcess.is_null() {
        unsafe { winapi::um::handleapi::CloseHandle(sei.hProcess) };
    }
    Ok(pid)
}

/// Whether the launcher should start a new external-app process, or attach
/// to (i.e. leave alone) an existing one. PRD 3.2.11: when a 2nd TERA.exe
/// fires `spawn_auto_launch_external_apps`, Shinra/TCC must NOT double-spawn.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SpawnDecision {
    /// A process with this name is already running; attach (do nothing).
    Attach,
    /// No process found; the caller should start one.
    Spawn,
}

/// Pure decision function — keep so both `launch_external_app_impl` and
/// `spawn_auto_launch_external_apps` route through the same predicate.
/// Called by `check_spawn_decision` for the I/O-bound variant.
pub fn decide_spawn(already_running: bool) -> SpawnDecision {
    if already_running {
        SpawnDecision::Attach
    } else {
        SpawnDecision::Spawn
    }
}

/// Convenience: queries the process table via `is_process_running` and
/// returns the decision. Not pure (touches the OS); callers that want
/// deterministic testing pass the bool to `decide_spawn` directly.
pub fn check_spawn_decision(exe_name: &str) -> SpawnDecision {
    decide_spawn(is_process_running(exe_name))
}

/// Whether the overlay (Shinra / TCC) should stay alive or be torn down
/// when a `TERA.exe` client exits. PRD 3.2.12 / 3.2.13.
///
/// The call-site wiring (listening to the teralib game-count watch channel
/// and emitting stop events to the frontend) lands with the broader
/// multi-client lifecycle work. For now the predicate is the tested
/// contract and is exported pub so the future caller has a stable import.
#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OverlayLifecycleAction {
    /// At least one other TERA.exe client is still running — keep overlays.
    KeepRunning,
    /// Last client closed — tear overlays down with the game.
    Terminate,
}

/// Pure decision function on the remaining-client count **after** a close
/// event. `>= 1` → KeepRunning (partial close), `0` → Terminate (last close).
///
/// `remaining_clients` is the count of live `TERA.exe` processes measured
/// AFTER the close event fires; `teralib::get_running_game_count()` is the
/// production source. Passing it in explicitly keeps the function pure.
#[allow(dead_code)]
pub fn decide_overlay_action(remaining_clients: usize) -> OverlayLifecycleAction {
    if remaining_clients == 0 {
        OverlayLifecycleAction::Terminate
    } else {
        OverlayLifecycleAction::KeepRunning
    }
}

/// Returns true if any running process matches the given executable name
/// (case-insensitive, matches the leaf filename, not the full path).
///
/// Used to avoid double-spawning: if Shinra.exe is already running when the
/// user clicks Play, we skip the spawn and attach to the existing process.
pub fn is_process_running(exe_name: &str) -> bool {
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    system
        .processes()
        .values()
        .any(|p| process_name_matches(exe_name, &p.name().to_string_lossy()))
}

/// Stops processes whose executable name matches. Windows posts WM_CLOSE
/// first so WPF apps like ShinraMeter run their Closing handler and persist
/// window position; falls back to TerminateProcess after the timeout.
pub fn stop_process_by_name(exe_name: &str) -> Result<u32, String> {
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let target = exe_name.to_ascii_lowercase();
    let mut stopped = 0u32;
    for process in system.processes().values() {
        let proc_name = process.name().to_string_lossy().to_ascii_lowercase();
        if !process_name_matches(&target, &proc_name) {
            continue;
        }
        #[cfg(windows)]
        let killed = graceful_stop_process(process.pid().as_u32(), GRACEFUL_CLOSE_TIMEOUT_MS);
        #[cfg(not(windows))]
        let killed = process.kill();
        if killed {
            stopped += 1;
        }
    }
    Ok(stopped)
}

fn process_name_matches(exe_name: &str, process_name: &str) -> bool {
    let target = exe_name.trim().to_ascii_lowercase();
    let process = process_name.trim().to_ascii_lowercase();
    if process == target {
        return true;
    }

    let target_stem = target.strip_suffix(".exe").unwrap_or(&target);
    let process_stem = process.strip_suffix(".exe").unwrap_or(&process);
    process_stem == target_stem
}

/// Window-message timeout for graceful close before falling back to
/// `TerminateProcess`. WPF apps like ShinraMeter persist window position
/// and user settings in their `Closing`/`Application.Exit` handlers; a
/// hard kill bypasses those, dropping unsaved state.
///
/// 3s was the original budget but turned out too tight: Shinra's Closing
/// handler does several things in sequence (save window.xml, flush the
/// EntityFramework DPS log DB, tear down the network sniffer, stop the
/// DiscordRPC client) and on cold-cache disks would routinely overrun
/// the timeout — TerminateProcess fired before window.xml was rewritten,
/// so the user kept finding their meter back at (0, 0) every relaunch.
/// 10s leaves plenty of headroom and only delays the closing flow when
/// the game is already exiting (i.e. when the user is leaving anyway).
const GRACEFUL_CLOSE_TIMEOUT_MS: u32 = 10000;

/// Pure decision helper: should we fall back to `TerminateProcess` after
/// posting WM_CLOSE? Yes if no windows took the message (background
/// services, console-only processes), or if the process didn't exit
/// within the timeout (overlay ignored WM_CLOSE).
fn should_force_kill_fallback(windows_messaged: u32, graceful_exit: bool) -> bool {
    windows_messaged == 0 || !graceful_exit
}

/// Graceful Windows process stop: post `WM_CLOSE` to every top-level
/// window the PID owns, wait `timeout_ms` for natural exit, fall back
/// to `TerminateProcess` if needed. Returns true once the PID is gone.
#[cfg(windows)]
fn graceful_stop_process(pid: u32, timeout_ms: u32) -> bool {
    use winapi::shared::minwindef::{BOOL, FALSE, LPARAM, TRUE};
    use winapi::shared::windef::HWND;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::{OpenProcess, TerminateProcess};
    use winapi::um::synchapi::WaitForSingleObject;
    use winapi::um::winbase::WAIT_OBJECT_0;
    use winapi::um::winnt::{PROCESS_QUERY_INFORMATION, PROCESS_TERMINATE, SYNCHRONIZE};
    use winapi::um::winuser::{
        EnumWindows, GetWindow, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible,
        PostMessageW, GW_OWNER, WM_CLOSE,
    };

    // SYNCHRONIZE for WaitForSingleObject; PROCESS_TERMINATE for the
    // fallback; PROCESS_QUERY_INFORMATION reserved for future exit-code
    // checks. FALSE = don't inherit the handle.
    let access = SYNCHRONIZE | PROCESS_TERMINATE | PROCESS_QUERY_INFORMATION;
    let handle = unsafe { OpenProcess(access, FALSE, pid) };
    if handle.is_null() {
        // Either the process exited between enumeration and OpenProcess,
        // or the launcher lacks rights. Caller treats as "already gone".
        return false;
    }

    // Post WM_CLOSE only to the MAIN window, not every top-level window
    // owned by the PID. Sending WM_CLOSE to all of Shinra's auxiliary
    // windows (boss gauge, debuff uptime, upload history, …) at once
    // races with its packet-processor thread: the auxiliary closes tear
    // down their dispatchers while PacketAnalysisLoop is mid-flight,
    // throwing TaskCanceledException out of EntityStatsMain.Update and
    // tripping Shinra's BasicTeraData crash dialog before window.xml
    // gets persisted. Closing only the main window lets WPF's normal
    // shutdown chain run (Application.Exit handler stops the sniffer
    // and packet thread, then closes child windows, then saves
    // settings) — exactly what happens when the user clicks the X
    // button manually.
    struct EnumCtx {
        pid: u32,
        main_hwnd: HWND,
    }
    let mut ctx = EnumCtx {
        pid,
        main_hwnd: std::ptr::null_mut(),
    };

    extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = unsafe { &mut *(lparam as *mut EnumCtx) };
        if !ctx.main_hwnd.is_null() {
            return TRUE; // already found a main window
        }
        let mut win_pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut win_pid) };
        if win_pid != ctx.pid {
            return TRUE;
        }
        // Heuristic for "main" window: visible, has a title bar, and
        // has no owner. WPF's primary application window matches all
        // three; tooltip/popup/auxiliary windows fail at least one.
        let visible = unsafe { IsWindowVisible(hwnd) } == TRUE;
        let titled = unsafe { GetWindowTextLengthW(hwnd) } > 0;
        let unowned = unsafe { GetWindow(hwnd, GW_OWNER) }.is_null();
        if visible && titled && unowned {
            ctx.main_hwnd = hwnd;
        }
        TRUE
    }

    unsafe {
        EnumWindows(Some(cb), &mut ctx as *mut _ as LPARAM);
    }

    let posted: u32 = if !ctx.main_hwnd.is_null() {
        unsafe { PostMessageW(ctx.main_hwnd, WM_CLOSE, 0, 0) };
        1
    } else {
        0
    };
    // The previous `ctx: EnumCtx` is shadowed below by the new `ctx:
    // PostResult`. No explicit drop needed — `EnumCtx` doesn't implement
    // `Drop` and the only consumer of its raw pointer (the EnumWindows
    // callback) has already returned by the time we reach this line.
    struct PostResult {
        posted: u32,
    }
    let ctx = PostResult { posted };

    let graceful_exit = if ctx.posted > 0 {
        unsafe { WaitForSingleObject(handle, timeout_ms) == WAIT_OBJECT_0 }
    } else {
        false
    };

    if should_force_kill_fallback(ctx.posted, graceful_exit) {
        unsafe {
            TerminateProcess(handle, 1);
            // Brief wait so the kernel finalises teardown before we
            // tell the caller the PID is gone.
            WaitForSingleObject(handle, 1000);
        }
    }

    unsafe {
        CloseHandle(handle);
    }
    true
}

/// Joins the extracted root + a relative executable path from the catalog
/// entry. Rejects paths that escape `install_dir` (defence in depth — catalog
/// is trusted, but cheap to validate).
pub fn executable_path(install_dir: &Path, executable_relpath: &str) -> Result<PathBuf, String> {
    let rel = Path::new(executable_relpath);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!(
            "Catalog executable_relpath '{}' escapes install dir",
            executable_relpath
        ));
    }
    Ok(install_dir.join(rel))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn hex_lower_produces_lowercase() {
        assert_eq!(hex_lower(&[0x0a, 0xff, 0x12]), "0aff12");
    }

    #[test]
    fn executable_path_joins_simple_relpath() {
        let tmp = TempDir::new().unwrap();
        let result = executable_path(tmp.path(), "ShinraMeter.exe").unwrap();
        assert_eq!(result, tmp.path().join("ShinraMeter.exe"));
    }

    #[test]
    fn executable_path_joins_subdir_relpath() {
        let tmp = TempDir::new().unwrap();
        let result = executable_path(tmp.path(), "bin/app.exe").unwrap();
        assert_eq!(result, tmp.path().join("bin").join("app.exe"));
    }

    #[test]
    fn executable_path_rejects_absolute() {
        let tmp = TempDir::new().unwrap();
        #[cfg(windows)]
        let abs = r"C:\Windows\System32\cmd.exe";
        #[cfg(not(windows))]
        let abs = "/bin/sh";
        let err = executable_path(tmp.path(), abs).unwrap_err();
        assert!(err.contains("escapes"));
    }

    #[test]
    fn executable_path_rejects_parent_traversal() {
        let tmp = TempDir::new().unwrap();
        let err = executable_path(tmp.path(), "../evil.exe").unwrap_err();
        assert!(err.contains("escapes"));
    }

    #[test]
    fn spawn_app_errors_when_executable_missing() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("nope.exe");
        let err = spawn_app(&missing, &[]).unwrap_err();
        assert!(err.contains("not found"));
    }

    #[test]
    fn is_process_running_returns_false_for_garbage_name() {
        // A process name we're sure doesn't exist on any sane system.
        assert!(!is_process_running("zzzz_nonexistent_binary_name_qqqq.exe"));
    }

    // --- Graceful close fallback (WM_CLOSE → TerminateProcess) -------------
    //
    // Pins the policy contract: WPF apps like ShinraMeter persist window
    // position only when their `OnClosing`/`Application.Exit` handlers
    // run. Force-killing via TerminateProcess bypasses those handlers,
    // so we post WM_CLOSE first and only fall back to TerminateProcess
    // when the app has no top-level windows or ignores the message.

    #[test]
    fn force_kill_when_no_windows_received_message() {
        // Background services / console apps own no top-level windows;
        // WM_CLOSE goes nowhere, so the only way to stop them is
        // TerminateProcess.
        assert!(should_force_kill_fallback(0, false));
        assert!(should_force_kill_fallback(0, true));
    }

    #[test]
    fn no_force_kill_when_app_exited_gracefully() {
        // App acknowledged WM_CLOSE within the timeout — settings
        // were persisted via the .NET Closing handler. Don't terminate.
        assert!(!should_force_kill_fallback(1, true));
        assert!(!should_force_kill_fallback(5, true));
    }

    #[test]
    fn force_kill_when_app_ignores_wm_close() {
        // Posted WM_CLOSE to ≥1 window but the app didn't exit within
        // the timeout (overlay hung, modal dialog blocking shutdown).
        // Fall back to TerminateProcess so a stuck overlay can't keep
        // the launcher waiting forever.
        assert!(should_force_kill_fallback(1, false));
        assert!(should_force_kill_fallback(10, false));
    }

    #[test]
    fn process_name_matches_windows_processes_with_or_without_exe_suffix() {
        assert!(process_name_matches("TCC.exe", "TCC"));
        assert!(process_name_matches("TCC.exe", "TCC.exe"));
        assert!(process_name_matches("ShinraMeter.exe", "shinrameter"));
        assert!(!process_name_matches("TCC.exe", "ShinraMeter"));
    }

    #[test]
    fn redirect_host_allowlist_is_narrow_and_matches_github_release_assets() {
        assert!(redirect_host_is_allowed("github.com"));
        assert!(redirect_host_is_allowed(
            "release-assets.githubusercontent.com"
        ));
        assert!(redirect_host_is_allowed("objects.githubusercontent.com"));

        assert!(!redirect_host_is_allowed("raw.githubusercontent.com"));
        assert!(!redirect_host_is_allowed("example.com"));
        assert!(!redirect_host_is_allowed("githubusercontent.com"));
    }

    // --- PRD 3.2.11.multi-client-attach-once --------------------------------

    #[test]
    fn decide_spawn_attaches_when_already_running() {
        assert_eq!(decide_spawn(true), SpawnDecision::Attach);
    }

    #[test]
    fn decide_spawn_spawns_when_not_running() {
        assert_eq!(decide_spawn(false), SpawnDecision::Spawn);
    }

    #[test]
    fn second_client_no_duplicate_spawn() {
        // Scenario: first TERA.exe client triggers auto-launch, sees Shinra
        // not running, decides Spawn. Before the second client starts,
        // Shinra is up. Second client's auto-launch queries the predicate
        // again and must see Attach so it doesn't double-spawn.
        //
        // We model the OS state as a boolean instead of actually spawning
        // a real process — `decide_spawn` is the single authority and both
        // call sites route through it (see `check_spawn_decision` +
        // launch_external_app_impl + spawn_auto_launch_external_apps).
        let first_client_decision = decide_spawn(/* already_running = */ false);
        assert_eq!(first_client_decision, SpawnDecision::Spawn);

        // After the first spawn Shinra is running.
        let already_running_after_first = true;

        let second_client_decision = decide_spawn(already_running_after_first);
        assert_eq!(
            second_client_decision,
            SpawnDecision::Attach,
            "2nd TERA.exe must attach to the existing Shinra/TCC, not spawn a duplicate"
        );
    }

    #[test]
    fn check_spawn_decision_returns_spawn_when_nothing_running() {
        // Integration-ish: query the real OS for a name guaranteed not to
        // exist. Must return Spawn (no running process to attach to).
        let d = check_spawn_decision("zzzz_nonexistent_binary_name_qqqq.exe");
        assert_eq!(d, SpawnDecision::Spawn);
    }

    // --- PRD 3.2.12.multi-client-partial-close / 3.2.13.multi-client-last-close

    #[test]
    fn partial_close_keeps_overlays() {
        // Two clients launched, user closes one → remaining_clients == 1 →
        // overlays must stay alive. PRD 3.2.12.
        let action = decide_overlay_action(1);
        assert_eq!(action, OverlayLifecycleAction::KeepRunning);
    }

    #[test]
    fn three_clients_one_closes_keeps_overlays() {
        // Boundary sanity: arbitrary multi-client counts keep overlays.
        for n in 1..=10 {
            assert_eq!(
                decide_overlay_action(n),
                OverlayLifecycleAction::KeepRunning,
                "remaining={n} must KeepRunning"
            );
        }
    }

    #[test]
    fn last_close_terminates_overlays() {
        // Only client closes → remaining_clients == 0 → overlays torn down.
        // PRD 3.2.13.
        let action = decide_overlay_action(0);
        assert_eq!(action, OverlayLifecycleAction::Terminate);
    }

    #[test]
    fn extract_zip_writes_files() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("out");
        fs::create_dir_all(&dest).unwrap();

        // Build a minimal zip with one file.
        let mut buf = Vec::new();
        {
            let cursor = Cursor::new(&mut buf);
            let mut w = zip::ZipWriter::new(cursor);
            let opts: zip::write::SimpleFileOptions = Default::default();
            w.start_file("hello.txt", opts).unwrap();
            use std::io::Write;
            w.write_all(b"world").unwrap();
            w.finish().unwrap();
        }

        extract_zip(&buf, &dest).unwrap();
        let body = fs::read_to_string(dest.join("hello.txt")).unwrap();
        assert_eq!(body, "world");
    }

    /// Builds a minimal zip whose single entry has the requested path,
    /// bypassing normalisation the writer might do on well-formed strings.
    fn build_malicious_zip(entry_name: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let cursor = Cursor::new(&mut buf);
            let mut w = zip::ZipWriter::new(cursor);
            let opts: zip::write::SimpleFileOptions = Default::default();
            w.start_file(entry_name, opts).unwrap();
            use std::io::Write;
            w.write_all(b"pwn").unwrap();
            w.finish().unwrap();
        }
        buf
    }

    #[test]
    fn extract_zip_rejects_zip_slip() {
        // PRD 3.1.3: at least three attack vectors must be rejected.
        // Parent-traversal, POSIX-absolute, and Windows drive-letter each
        // trip zip::read::ZipFile::enclosed_name(), so each returns Err
        // before any byte is written.
        let vectors = [
            "../evil.txt",           // parent traversal
            "/etc/passwd",           // POSIX absolute
            "C:/Windows/evil.txt",   // Windows drive-letter absolute (forward slash)
            "C:\\Windows\\evil.txt", // Windows drive-letter absolute (backslash)
        ];

        for name in vectors {
            let tmp = TempDir::new().unwrap();
            let dest = tmp.path().join("out");
            fs::create_dir_all(&dest).unwrap();

            let buf = build_malicious_zip(name);
            extract_zip(&buf, &dest)
                .expect_err(&format!("vector '{name}' should have been rejected"));

            // Defence in depth: dest root should be untouched (only the empty
            // "out" dir we created pre-call).
            let entries: Vec<_> = fs::read_dir(&dest).unwrap().collect();
            assert!(
                entries.is_empty(),
                "vector '{name}' left side effects in dest: {entries:?}"
            );

            // Also assert nothing escaped into the parent.
            let escape_siblings: Vec<_> = fs::read_dir(tmp.path())
                .unwrap()
                .flatten()
                .filter(|e| e.file_name() != "out")
                .collect();
            assert!(
                escape_siblings.is_empty(),
                "vector '{name}' escaped into parent: {escape_siblings:?}"
            );
        }
    }

    // Note: download_and_extract is network-bound and not unit-tested here.
    // Integration coverage via a mock HTTP server is tracked separately.

    // --- Fail-closed SHA verification (PRD 3.1.1) ---------------------------
    //
    // Spin a one-shot HTTP/1.1 server on a loopback port, have `download_file`
    // fetch from it with a deliberately-wrong `expected_sha256`, and assert:
    //   (a) the function returns Err with "hash mismatch" wording,
    //   (b) the destination file is NOT created (0 bytes touch disk).

    async fn serve_once(body: &'static [u8]) -> u16 {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                // Drain the request headers; we don't care what they are.
                let _ = sock.read(&mut buf).await;

                let response = format!(
                    "HTTP/1.1 200 OK\r\n\
                     Content-Length: {}\r\n\
                     Content-Type: application/octet-stream\r\n\
                     Connection: close\r\n\r\n",
                    body.len()
                );
                let _ = sock.write_all(response.as_bytes()).await;
                let _ = sock.write_all(body).await;
                let _ = sock.shutdown().await;
            }
        });

        port
    }

    async fn serve_once_owned(body: Vec<u8>) -> u16 {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;

                let response = format!(
                    "HTTP/1.1 200 OK\r\n\
                     Content-Length: {}\r\n\
                     Content-Type: application/zip\r\n\
                     Connection: close\r\n\r\n",
                    body.len()
                );
                let _ = sock.write_all(response.as_bytes()).await;
                let _ = sock.write_all(&body).await;
                let _ = sock.shutdown().await;
            }
        });

        port
    }

    #[tokio::test]
    async fn sha_mismatch_aborts_before_write() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("payload.bin");

        let port = serve_once(b"real-server-bytes").await;
        let url = format!("http://127.0.0.1:{port}/payload.bin");

        // SHA-256 of "never-matches" — guaranteed not the hash of the body above.
        let wrong_sha = hex_lower(&Sha256::digest(b"never-matches"));

        let result = download_file(&url, &wrong_sha, &dest, |_, _| {}).await;

        let err = result.expect_err("SHA mismatch must return Err");
        assert!(
            err.contains("hash mismatch") || err.contains("Hash mismatch"),
            "unexpected error message: {err}"
        );
        assert!(
            !dest.exists(),
            "dest must not exist on SHA mismatch (fail-closed); found {}",
            dest.display()
        );
    }

    #[tokio::test]
    async fn sha_match_writes_file() {
        // Sanity control: same path on a correct hash must succeed so the
        // negative test above isn't passing for the wrong reason.
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("payload.bin");

        let body: &'static [u8] = b"exact-bytes";
        let port = serve_once(body).await;
        let url = format!("http://127.0.0.1:{port}/payload.bin");
        let correct_sha = hex_lower(&Sha256::digest(body));

        download_file(&url, &correct_sha, &dest, |_, _| {})
            .await
            .expect("matching SHA must succeed");

        assert_eq!(fs::read(&dest).unwrap(), body);
    }

    // PRD 3.1.2 — GPK install pathway fail-closed.
    //
    // `commands::mods::install_gpk_mod` writes to `<app_data>/mods/gpk/<id>.gpk`
    // via `external_app::download_file`. If the SHA doesn't match the catalog's
    // `sha256`, nothing must touch disk. This test pins the contract on the
    // GPK-shaped dest path — same download_file call, but named and framed
    // around the GPK install site so a future refactor to install_gpk_mod
    // that sidesteps download_file will trip here.
    #[tokio::test]
    async fn sha_mismatch_aborts_before_write_gpk() {
        let tmp = TempDir::new().unwrap();
        let gpk_dir = tmp.path().join("mods").join("gpk");
        fs::create_dir_all(&gpk_dir).unwrap();
        // Match the id-derived filename install_gpk_mod produces.
        let dest = gpk_dir.join("classicplus_example_mod.gpk");

        let port = serve_once(b"pretend-this-is-a-real-gpk").await;
        let url = format!("http://127.0.0.1:{port}/example.gpk");

        let wrong_sha = hex_lower(&Sha256::digest(b"never-matches-gpk"));

        let result = download_file(&url, &wrong_sha, &dest, |_, _| {}).await;

        let err = result.expect_err("GPK SHA mismatch must return Err");
        assert!(
            err.contains("hash mismatch") || err.contains("Hash mismatch"),
            "unexpected error message: {err}"
        );
        assert!(
            !dest.exists(),
            "GPK dest must not exist on SHA mismatch (0 bytes touch disk); found {}",
            dest.display()
        );
        // gpk_dir itself was pre-created, but no other entries should have
        // appeared in it.
        let leaked: Vec<_> = fs::read_dir(&gpk_dir).unwrap().flatten().collect();
        assert!(
            leaked.is_empty(),
            "GPK dir got polluted on SHA mismatch: {leaked:?}"
        );
    }

    /// PRD 3.2.8.disk-full-revert: if zip extraction fails partway through
    /// (classic trigger is ENOSPC — disk fills up, half the DLLs land on
    /// disk, the rest error out), the dest dir must be removed so the
    /// user's next retry starts clean. Without this, Play would try to
    /// spawn an executable that's missing its dependent DLLs.
    ///
    /// We simulate the "partial install state" directly because we can't
    /// portably trigger real ENOSPC in a test. The helper is pure over
    /// `&Path` — given a populated dir, revert removes it; given a missing
    /// dir, revert is a best-effort no-op.
    #[test]
    fn revert_on_enospc() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("mod_root");

        // Simulate: download OK, SHA OK, dest dir created, extraction
        // wrote some files before disk filled. Seed a half-written state.
        fs::create_dir_all(&dest).unwrap();
        fs::create_dir_all(dest.join("bin")).unwrap();
        fs::write(dest.join("app.exe"), b"partial executable").unwrap();
        fs::write(dest.join("bin").join("plugin.dll"), b"partial dll").unwrap();

        // Extract failed with ENOSPC — call the production cleanup helper.
        revert_partial_install_dir(&dest);

        assert!(
            !dest.exists(),
            "dest dir must be removed after failed extract; got {}",
            dest.display()
        );
    }

    /// Revert on a directory that never existed is a no-op that doesn't
    /// panic — covers the "download failed before dest was created" branch.
    #[test]
    fn revert_on_missing_dest_is_noop() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("never_created");
        assert!(!dest.exists());
        revert_partial_install_dir(&dest);
        assert!(!dest.exists(), "still missing after revert");
    }

    /// Revert on a partial GPK file removes it so next retry doesn't see
    /// a zero-byte (or truncated) GPK and feed garbage to the mapper.
    #[test]
    fn revert_partial_gpk_file_removes_it() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("classicplus.minimap.gpk");

        // Simulate: fs::write opened + truncated + wrote some bytes, then
        // ENOSPC cut it off. The file is on disk but incomplete.
        fs::write(&dest, b"partial GPK bytes, truncated at ENOSPC").unwrap();
        assert!(dest.exists());

        revert_partial_install_file(&dest);

        assert!(
            !dest.exists(),
            "partial GPK must be removed after write failure; got {}",
            dest.display()
        );
    }

    /// Revert on a missing file is a no-op — covers the case where the OS
    /// never created the file before erroring out.
    #[test]
    fn revert_missing_file_is_noop() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("missing.gpk");
        revert_partial_install_file(&dest);
        assert!(!dest.exists());
    }

    // --- Progress-event rate (PRD 3.6.3) ------------------------------------
    //
    // Server emits N chunks with inter-chunk delays so the client-side
    // stream actually surfaces each chunk separately — on loopback, a
    // single `write_all(body)` gets coalesced into one hyper poll, which
    // isn't what a real 10 Mbit/s link does. The chunked helper reproduces
    // the "bytes trickle in over the wire" shape we actually want to pin.

    async fn serve_chunked(chunks: Vec<Vec<u8>>, delay: std::time::Duration) -> (u16, usize) {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let total: usize = chunks.iter().map(|c| c.len()).sum();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                use tokio::io::AsyncReadExt;
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;

                let response = format!(
                    "HTTP/1.1 200 OK\r\n\
                     Content-Length: {total}\r\n\
                     Content-Type: application/octet-stream\r\n\
                     Connection: close\r\n\r\n"
                );
                let _ = sock.write_all(response.as_bytes()).await;
                let _ = sock.flush().await;

                for chunk in chunks {
                    if sock.write_all(&chunk).await.is_err() {
                        return;
                    }
                    if sock.flush().await.is_err() {
                        return;
                    }
                    tokio::time::sleep(delay).await;
                }
                let _ = sock.shutdown().await;
            }
        });

        (port, total)
    }

    /// PRD 3.6.3 acceptance: progress events emit ≥ 10/s on a 10 Mbit/s
    /// simulated link. 20 chunks × 64 KB with 20 ms pacing ≈ 400 ms on
    /// the wire → ~50 callbacks/s, well above the bar. Assert both the
    /// count (≥ 10) and the rate (≥ 10 Hz).
    #[tokio::test]
    async fn at_least_10hz() {
        let chunk = vec![0xABu8; 64 * 1024];
        let chunks: Vec<Vec<u8>> = std::iter::repeat_n(chunk, 20).collect();
        let (port, total_bytes) = serve_chunked(chunks, std::time::Duration::from_millis(20)).await;
        let url = format!("http://127.0.0.1:{port}/stream.bin");

        let body = vec![0xABu8; total_bytes];
        let correct_sha = hex_lower(&Sha256::digest(&body));
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("stream.bin");

        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count_cb = count.clone();
        let start = std::time::Instant::now();

        download_file(&url, &correct_sha, &dest, move |_, _| {
            count_cb.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        })
        .await
        .expect("download should succeed");

        let elapsed = start.elapsed();
        let total = count.load(std::sync::atomic::Ordering::Relaxed);
        let rate = total as f64 / elapsed.as_secs_f64();

        assert!(
            total >= 10,
            "progress must fire at least 10 times on a 20-chunk stream; got {total} in {elapsed:?}"
        );
        assert!(
            rate >= 10.0,
            "progress rate must be ≥10 Hz; got {rate:.2}/s ({total} events in {elapsed:?})"
        );
    }

    /// Sanity control: prove the callback actually fires per chunk, not
    /// once per request. Without this, a broken implementation that
    /// coalesced everything into one final callback would still pass
    /// `at_least_10hz` if elapsed was short enough to push the rate
    /// above 10 Hz for 1 event.
    #[tokio::test]
    async fn callback_count_scales_with_chunks() {
        let small = vec![0x5Au8; 16 * 1024];
        let chunks_5: Vec<Vec<u8>> = std::iter::repeat_n(small.clone(), 5).collect();
        let chunks_15: Vec<Vec<u8>> = std::iter::repeat_n(small, 15).collect();

        async fn count_for(chunks: Vec<Vec<u8>>) -> usize {
            let total_bytes: usize = chunks.iter().map(|c| c.len()).sum();
            let (port, _) = serve_chunked(chunks, std::time::Duration::from_millis(10)).await;
            let url = format!("http://127.0.0.1:{port}/s.bin");
            let body = vec![0x5Au8; total_bytes];
            let sha = hex_lower(&Sha256::digest(&body));
            let tmp = TempDir::new().unwrap();
            let dest = tmp.path().join("s.bin");
            let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let count_cb = count.clone();
            download_file(&url, &sha, &dest, move |_, _| {
                count_cb.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            })
            .await
            .unwrap();
            count.load(std::sync::atomic::Ordering::Relaxed)
        }

        let c5 = count_for(chunks_5).await;
        let c15 = count_for(chunks_15).await;

        assert!(
            c15 > c5,
            "expected c15 > c5 to prove per-chunk emission; got c5={c5}, c15={c15}"
        );
    }

    // --- pin.external.download-extract (iter 94) ----------------------------
    //
    // Golden-file pin for the download+extract flow. The existing
    // `extract_zip_writes_files` test (single file) and
    // `extract_zip_rejects_zip_slip` (4 adversarial fixtures) cover the
    // happy path and the security gate. This section pins the
    // multi-entry output-tree shape byte-for-byte: a zip with 3 files
    // across 2 directories must produce exactly that file tree with
    // exactly the expected contents — no renames, no re-ordering, no
    // silently-collapsed directories, no loss of binary fidelity.
    //
    // Matches the pin.* pattern established by iters 89 (parser), 92
    // (cipher), 93 (merger).

    /// Build a deterministic fixture zip with 3 entries:
    ///   plugins/hello.txt  ASCII "world"
    ///   plugins/data.bin   256 bytes 0x00..0xFF (tests binary fidelity)
    ///   README.md          ASCII "root-level file"
    ///
    /// The fixture is stable across runs — `SimpleFileOptions::default()`
    /// picks a compression method + mtime that depend only on the
    /// `zip` crate version (pinned to 2.4.2 for now; see
    /// dep-dedup-investigation.md). A future zip-crate major bump that
    /// changed the default would surface as a different byte layout but
    /// still round-trip through `extract_zip` to the same output tree —
    /// which is what this test pins (the OUTPUT, not the zip bytes).
    #[cfg(test)]
    fn build_golden_fixture_zip() -> Vec<u8> {
        let mut buf = Vec::new();
        let cursor = Cursor::new(&mut buf);
        let mut w = zip::ZipWriter::new(cursor);
        let opts: zip::write::SimpleFileOptions = Default::default();

        use std::io::Write;
        w.start_file("plugins/hello.txt", opts).unwrap();
        w.write_all(b"world").unwrap();

        w.start_file("plugins/data.bin", opts).unwrap();
        let bin: Vec<u8> = (0u8..=255u8).collect();
        w.write_all(&bin).unwrap();

        w.start_file("README.md", opts).unwrap();
        w.write_all(b"root-level file").unwrap();

        w.finish().unwrap();
        buf
    }

    fn build_update_fixture_zip() -> Vec<u8> {
        let mut buf = Vec::new();
        let cursor = Cursor::new(&mut buf);
        let mut w = zip::ZipWriter::new(cursor);
        let opts: zip::write::SimpleFileOptions = Default::default();

        use std::io::Write;
        w.start_file("ShinraMeter.exe", opts).unwrap();
        w.write_all(b"new executable").unwrap();

        w.start_file("resources/config/window.xml", opts).unwrap();
        w.write_all(b"<window x=\"default\" />").unwrap();

        w.start_file("resources/config/server-overrides.txt", opts)
            .unwrap();
        w.write_all(b"default overrides").unwrap();

        w.start_file("resources/sound/custom-alert.wav", opts)
            .unwrap();
        w.write_all(b"default sound").unwrap();

        w.start_file("assets/config/app.xml", opts).unwrap();
        w.write_all(b"<app version=\"new\" />").unwrap();

        w.finish().unwrap();
        buf
    }

    #[tokio::test]
    async fn download_and_extract_preserves_user_config_xml_on_update() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("shinra");
        let user_window = dest.join("resources").join("config").join("window.xml");
        let user_overrides = dest
            .join("resources")
            .join("config")
            .join("server-overrides.txt");
        let user_sound = dest
            .join("resources")
            .join("sound")
            .join("custom-alert.wav");
        let app_xml = dest.join("assets").join("config").join("app.xml");
        fs::create_dir_all(user_window.parent().unwrap()).unwrap();
        fs::write(&user_window, b"<window x=\"user\" />").unwrap();
        fs::write(&user_overrides, b"user overrides").unwrap();
        fs::create_dir_all(user_sound.parent().unwrap()).unwrap();
        fs::write(&user_sound, b"user sound").unwrap();
        fs::create_dir_all(app_xml.parent().unwrap()).unwrap();
        fs::write(&app_xml, b"<app version=\"old\" />").unwrap();
        fs::write(dest.join("stale.dll"), b"old payload").unwrap();

        let zip_bytes = build_update_fixture_zip();
        let correct_sha = hex_lower(&Sha256::digest(&zip_bytes));
        let port = serve_once_owned(zip_bytes).await;
        let url = format!("http://127.0.0.1:{port}/shinra.zip");

        download_and_extract(&url, &correct_sha, &dest, |_, _| {})
            .await
            .expect("update extract should succeed");

        assert_eq!(
            fs::read_to_string(&user_window).unwrap(),
            "<window x=\"user\" />",
            "Shinra window.xml is user settings and must survive updates"
        );
        assert_eq!(
            fs::read_to_string(&user_overrides).unwrap(),
            "user overrides",
            "Shinra config contains non-XML user state and must survive updates"
        );
        assert_eq!(
            fs::read_to_string(&user_sound).unwrap(),
            "user sound",
            "custom Shinra alert sounds must survive updates"
        );
        assert_eq!(
            fs::read_to_string(&app_xml).unwrap(),
            "<app version=\"new\" />",
            "only known user-state directories should be preserved; unrelated config XML must update normally"
        );
        assert!(dest.join("ShinraMeter.exe").is_file());
        assert!(
            !dest.join("stale.dll").exists(),
            "update must still clear stale binaries while preserving user config"
        );
    }

    #[tokio::test]
    async fn download_and_extract_clears_readonly_stale_payload_on_update() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("shinra");
        fs::create_dir_all(&dest).unwrap();
        let stale = dest.join("stale.dll");
        fs::write(&stale, b"old payload").unwrap();
        let mut permissions = fs::metadata(&stale).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&stale, permissions).unwrap();

        let zip_bytes = build_update_fixture_zip();
        let correct_sha = hex_lower(&Sha256::digest(&zip_bytes));
        let port = serve_once_owned(zip_bytes).await;
        let url = format!("http://127.0.0.1:{port}/shinra.zip");

        download_and_extract(&url, &correct_sha, &dest, |_, _| {})
            .await
            .expect("update extract should clear readonly stale payloads");

        assert!(dest.join("ShinraMeter.exe").is_file());
        assert!(
            !stale.exists(),
            "readonly stale files must not block update cleanup or remain after extract"
        );
    }

    #[test]
    fn download_and_extract_uses_resilient_install_dir_clear() {
        let source = include_str!("external_app.rs");
        let fn_start = source
            .find("pub async fn download_and_extract")
            .expect("download_and_extract present");
        let fn_tail = &source[fn_start..];
        let fn_end = fn_tail[1..]
            .find("\npub async fn ")
            .map(|i| i + 1)
            .unwrap_or(fn_tail.len());
        let body = &fn_tail[..fn_end];

        assert!(
            body.contains("clear_existing_install_dir"),
            "external app updates must use resilient cleanup so one Windows remove_dir_all failure does not abort Shinra/TCC updates"
        );
    }

    /// Pin the output tree shape: exactly the 3 expected files, exactly
    /// at the expected relative paths, with the expected contents.
    #[test]
    fn golden_extract_multi_entry_tree() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("out");
        fs::create_dir_all(&dest).unwrap();

        let zip_bytes = build_golden_fixture_zip();
        extract_zip(&zip_bytes, &dest).expect("fixture must extract cleanly");

        // File 1: ASCII inside a subdirectory.
        let hello = dest.join("plugins").join("hello.txt");
        assert!(hello.is_file(), "plugins/hello.txt must be a file");
        assert_eq!(
            fs::read_to_string(&hello).unwrap(),
            "world",
            "plugins/hello.txt content round-trip"
        );

        // File 2: binary 0..=255 round-trip. This catches any UTF-8
        // coercion, newline conversion, or line-ending munging in the
        // extract path — "binary fidelity" is not optional for mod
        // bundles that ship DLLs / configs.
        let data_bin = dest.join("plugins").join("data.bin");
        assert!(data_bin.is_file(), "plugins/data.bin must be a file");
        let data_bytes = fs::read(&data_bin).unwrap();
        let expected_bin: Vec<u8> = (0u8..=255u8).collect();
        assert_eq!(
            data_bytes, expected_bin,
            "binary payload must round-trip byte-for-byte (256 bytes 0x00..0xFF)"
        );

        // File 3: root-level sibling.
        let readme = dest.join("README.md");
        assert!(readme.is_file(), "README.md must be a root-level file");
        assert_eq!(
            fs::read_to_string(&readme).unwrap(),
            "root-level file",
            "README.md content round-trip"
        );
    }

    /// No silent extra files: the output tree must contain ONLY the
    /// three fixture entries. A refactor that silently injected a
    /// marker file ("extracted_at.stamp" etc.) would fail here.
    #[test]
    fn golden_extract_no_surprise_entries() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("out");
        fs::create_dir_all(&dest).unwrap();
        extract_zip(&build_golden_fixture_zip(), &dest).unwrap();

        // Walk the output tree, collect all file paths relative to dest,
        // sort, assert the set.
        fn walk(p: &Path, dest: &Path, out: &mut Vec<String>) {
            for entry in fs::read_dir(p).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, dest, out);
                } else {
                    out.push(
                        path.strip_prefix(dest)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        let mut found = Vec::new();
        walk(&dest, &dest, &mut found);
        found.sort();

        assert_eq!(
            found,
            vec![
                "README.md".to_string(),
                "plugins/data.bin".to_string(),
                "plugins/hello.txt".to_string(),
            ],
            "output tree must contain EXACTLY the 3 fixture entries"
        );
    }

    /// Re-extracting into the same dest directory overwrites cleanly —
    /// no leftover files, no error, no half-merged tree. Matches the
    /// re-install semantic mod-manager uses when a user reinstalls a
    /// mod whose previous files still sit in the target dir.
    #[test]
    fn golden_extract_is_idempotent_on_reinstall() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("out");
        fs::create_dir_all(&dest).unwrap();
        let zip_bytes = build_golden_fixture_zip();

        extract_zip(&zip_bytes, &dest).unwrap();
        extract_zip(&zip_bytes, &dest).unwrap();

        // Same contents both runs, no duplicate paths.
        let hello = fs::read_to_string(dest.join("plugins").join("hello.txt")).unwrap();
        assert_eq!(hello, "world", "second extract preserves file content");
    }
}
