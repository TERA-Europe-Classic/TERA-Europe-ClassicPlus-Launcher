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

use sha2::{Digest, Sha256};
use sysinfo::System;

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

    if dest_dir.exists() {
        fs::remove_dir_all(dest_dir)
            .map_err(|e| format!("Failed to clear {}: {}", dest_dir.display(), e))?;
    }
    fs::create_dir_all(dest_dir)
        .map_err(|e| format!("Failed to create {}: {}", dest_dir.display(), e))?;

    extract_zip(&bytes, dest_dir)?;

    Ok(dest_dir.to_path_buf())
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
    fs::write(dest_file, &bytes)
        .map_err(|e| format!("Failed to write {}: {}", dest_file.display(), e))?;
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

    let client = reqwest::Client::builder()
        .user_agent("TERA-Europe-ClassicPlus-Launcher")
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to download from {}: {}", url, e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download returned HTTP {} from {}",
            response.status(),
            url
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
        return Err(format!(
            "Executable not found: {}",
            exe_path.display()
        ));
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
    sei.lpVerb = null_mut();          // default verb — handles UAC when needed
    sei.lpFile = file.as_ptr();
    sei.lpParameters = if params.len() > 1 { params.as_ptr() } else { null_mut() };
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

/// Returns true if any running process matches the given executable name
/// (case-insensitive, matches the leaf filename, not the full path).
///
/// Used to avoid double-spawning: if Shinra.exe is already running when the
/// user clicks Play, we skip the spawn and attach to the existing process.
pub fn is_process_running(exe_name: &str) -> bool {
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let target = exe_name.to_ascii_lowercase();
    system.processes().values().any(|p| {
        p.name()
            .to_string_lossy()
            .to_ascii_lowercase()
            .contains(&target)
    })
}

/// Sends a terminate signal to processes whose executable name matches.
/// Best-effort; Windows processes that deny termination silently remain.
pub fn stop_process_by_name(exe_name: &str) -> Result<u32, String> {
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let target = exe_name.to_ascii_lowercase();
    let mut killed = 0u32;
    for process in system.processes().values() {
        let name_lower = process.name().to_string_lossy().to_ascii_lowercase();
        if name_lower.contains(&target) && process.kill() {
            killed += 1;
        }
    }
    Ok(killed)
}

/// Joins the extracted root + a relative executable path from the catalog
/// entry. Rejects paths that escape `install_dir` (defence in depth — catalog
/// is trusted, but cheap to validate).
pub fn executable_path(install_dir: &Path, executable_relpath: &str) -> Result<PathBuf, String> {
    let rel = Path::new(executable_relpath);
    if rel.is_absolute() || rel.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
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
}
