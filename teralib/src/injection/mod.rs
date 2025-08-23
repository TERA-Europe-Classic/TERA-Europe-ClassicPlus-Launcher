use crate::global_credentials::GLOBAL_CREDENTIALS;
use log::{error, info};
use std::process::Command;
use std::{fs::OpenOptions, io::Read, path::Path};

use winapi::um::processthreadsapi::GetCurrentProcessId;
use winapi::um::winuser::{MessageBoxW, MB_ICONWARNING, MB_OK};
use winapi::{
    shared::minwindef::DWORD,
    um::{
        handleapi::CloseHandle,
        tlhelp32::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
    },
};

pub fn find_process_by_name(process_name: &str) -> Option<DWORD> {
    cryptify::flow_stmt!();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() {
            return None;
        }

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as DWORD,
            cntUsage: 0,
            th32ProcessID: 0,
            th32DefaultHeapID: 0,
            th32ModuleID: 0,
            cntThreads: 0,
            th32ParentProcessID: 0,
            pcPriClassBase: 0,
            dwFlags: 0,
            szExeFile: [0; 260],
        };

        if Process32FirstW(snapshot, &mut entry) == 0 {
            CloseHandle(snapshot);
            return None;
        }

        loop {
            let exe_name = String::from_utf16_lossy(&entry.szExeFile)
                .trim_end_matches('\0')
                .to_lowercase();

            if exe_name.contains(&process_name.to_lowercase()) {
                let pid = entry.th32ProcessID;
                CloseHandle(snapshot);
                return Some(pid);
            }

            if Process32NextW(snapshot, &mut entry) == 0 {
                break;
            }
        }

        CloseHandle(snapshot);
        None
    }
}

/// Trim whitespace and remove a single pair of surrounding quotes from a path string.
fn clean_path_str(s: &str) -> String {
    let t = s.trim();
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')) {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

/// Public helper to ensure Defender exclusion for the game's directory prior to launching the game.
pub fn ensure_av_exclusion_before_launch() {
    cryptify::flow_stmt!();
    let game_dir = {
        let game_path_str = clean_path_str(&GLOBAL_CREDENTIALS.get_game_path());
        let p = std::path::PathBuf::from(game_path_str);
        p.parent().map(|pp| pp.to_path_buf()).unwrap_or_else(|| {
            let mut exe_dir = std::env::current_exe().unwrap_or_else(|_| std::env::temp_dir());
            exe_dir.pop();
            exe_dir
        })
    };

    if let Err(e) = ensure_defender_exclusion(&game_dir) {
        error!("Failed to add Defender exclusion pre-launch: {}", e);
    }
}

pub fn inject_agnitor(game_pid: DWORD) -> Result<(), Box<dyn std::error::Error>> {
    let current_pid = unsafe { GetCurrentProcessId() };
    if game_pid == current_pid {
        return Err("err".into());
    }

    std::thread::sleep(std::time::Duration::from_millis(2000));

    let game_dir = {
        let game_path_str = clean_path_str(&GLOBAL_CREDENTIALS.get_game_path());
        let p = std::path::PathBuf::from(game_path_str);
        p.parent().map(|pp| pp.to_path_buf()).unwrap_or_else(|| {
            let mut exe_dir = std::env::current_exe().unwrap_or_else(|_| std::env::temp_dir());
            exe_dir.pop();
            exe_dir
        })
    };

    let dll_bytes: &[u8] = include_bytes!("../../agnitor.dll");
    let dll_path = game_dir.join("agnitor.dll");
    write_if_different(&dll_path, dll_bytes)?;

    let dll32_path = dll_path.canonicalize().unwrap_or(dll_path.clone());
    let dll32_str = dll32_path.to_str().ok_or("err")?.to_string();

    let helper_bytes: &[u8] = include_bytes!("../../terainject32.exe");
    let helper_path_fs = game_dir.join("terainject32.exe");
    write_if_different(&helper_path_fs, helper_bytes)?;

    let helper_path = helper_path_fs
        .canonicalize()
        .unwrap_or(helper_path_fs.clone());
    let helper_str = helper_path.to_str().ok_or("err")?;

    let status = Command::new(helper_str)
        .arg(game_pid.to_string())
        .arg(dll32_str)
        .status()?;

    if !status.success() {
        return Err("err".into());
    }

    return Ok(());
}

/// Attempts to add the specified directory to Microsoft Defender's exclusion list.
///
/// Strategy:
/// Perform an elevated PowerShell call via Start-Process -Verb RunAs to add the exclusion.
/// Any failure is logged and a minimal warning is shown, but injection continues.
fn ensure_defender_exclusion(dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    cryptify::flow_stmt!();
    // Use the provided directory (binaries folder) as-is to avoid introducing \\?\ prefixes
    let mut dir_str = dir
        .to_str()
        .ok_or("Invalid game directory path for Defender exclusion")?
        .to_string();
    // Explicitly strip any leading \\?\ if present
    if let Some(stripped) = dir_str.strip_prefix(r"\\?\") {
        dir_str = stripped.to_string();
    }

    // Elevated attempt only: create a temp script and run it as admin
    // Create a small temp script to avoid complex quoting for the elevated call
    let script_path = {
        let mut p = std::env::temp_dir();
        p.push("tera_add_defender_excl.ps1");
        p
    };

    let script_content = r#"param([string]$Path)
try {
  $p = [IO.Path]::GetFullPath($Path)
  $mp = Get-MpPreference
  if ($mp.ExclusionPath -notcontains $p) { Add-MpPreference -ExclusionPath $p }
  exit 0
} catch {
  exit 1
}
"#;

    // Write/overwrite the script atomically
    std::fs::write(&script_path, script_content)?;

    // Elevated attempt via Start-Process -Verb RunAs with EncodedCommand to avoid any splitting issues
    // Build the inner command as: & '<script>' -Path '<dir>' and encode it to Base64 (UTF-16LE) within PowerShell
    let script_ps = script_path
        .to_string_lossy()
        .replace("'", "''");
    let dir_ps = dir_str.replace("'", "''");
    let elevated_cmd = format!(
        "$cmd = \"& '{}' -Path '{}'\"; $bytes = [Text.Encoding]::Unicode.GetBytes($cmd); $b64 = [Convert]::ToBase64String($bytes); Start-Process PowerShell -Verb RunAs -WindowStyle Hidden -ArgumentList @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-EncodedCommand',$b64) -Wait",
        script_ps,
        dir_ps
    );

    let status2 = Command::new("powershell.exe")
        .args(&[
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &elevated_cmd,
        ])
        .status();

    // Best-effort cleanup of the temporary script
    let _ = std::fs::remove_file(&script_path);

    match status2 {
        Ok(s) if s.success() => {
            info!("Defender exclusion ensured (elevated) for: {}", dir_str);
            Ok(())
        }
        Ok(s) => {
            show_warning_message(
                "Exclusion Failed",
                "Could not add antivirus exclusion.",
            );
            Err(format!("Elevated Defender exclusion attempt failed with status: {:?}", s).into())
        }
        Err(e) => {
            show_warning_message(
                "Exclusion Failed",
                "Could not add antivirus exclusion.",
            );
            Err(format!("Failed to invoke elevated PowerShell for Defender exclusion: {}", e).into())
        }
    }
}

/// Shows a warning message box to the user (best-effort, ignored if it fails).
fn show_warning_message(title: &str, message: &str) {
    cryptify::flow_stmt!();
    let title_w: Vec<u16> = to_wide(title);
    let message_w: Vec<u16> = to_wide(message);
    unsafe {
        let _ = MessageBoxW(
            std::ptr::null_mut(),
            message_w.as_ptr(),
            title_w.as_ptr(),
            MB_OK | MB_ICONWARNING,
        );
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn write_if_different(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    cryptify::flow_stmt!();

    let mut needs_write = true;

    if path.exists() {
        if let Ok(mut f) = std::fs::File::open(path) {
            let mut existing = Vec::new();

            if f.read_to_end(&mut existing).is_ok() {
                if existing == bytes {
                    needs_write = false;
                }
            }
        }
    }

    if needs_write {
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;

        use std::io::Write as _;
        f.write_all(bytes)?;
    }

    Ok(())
}
