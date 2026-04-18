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
use std::process::Command;

use sha2::{Digest, Sha256};
use sysinfo::System;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Downloads the zip at `url`, verifies the SHA-256 matches `expected_sha256`
/// (hex, lowercase), and extracts it into `dest_dir`. Any existing contents
/// of `dest_dir` are wiped first — this is an install, not a merge.
///
/// Returns the absolute path to the extracted root directory.
pub async fn download_and_extract(
    url: &str,
    expected_sha256: &str,
    dest_dir: &Path,
) -> Result<PathBuf, String> {
    let bytes = fetch_bytes(url).await?;

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

async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
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

    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("Failed to read download body: {}", e))
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

    let mut cmd = Command::new(exe_path);
    cmd.args(args);
    if let Some(parent) = exe_path.parent() {
        cmd.current_dir(parent);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", exe_path.display(), e))?;
    Ok(child.id())
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

    #[test]
    fn extract_zip_rejects_zip_slip() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("out");
        fs::create_dir_all(&dest).unwrap();

        let mut buf = Vec::new();
        {
            let cursor = Cursor::new(&mut buf);
            let mut w = zip::ZipWriter::new(cursor);
            let opts: zip::write::SimpleFileOptions = Default::default();
            w.start_file("../evil.txt", opts).unwrap();
            use std::io::Write;
            w.write_all(b"pwn").unwrap();
            w.finish().unwrap();
        }

        let err = extract_zip(&buf, &dest).unwrap_err();
        assert!(err.contains("zip-slip"));
    }

    // Note: download_and_extract is network-bound and not unit-tested here.
    // Integration coverage via a mock HTTP server is tracked separately.
}
