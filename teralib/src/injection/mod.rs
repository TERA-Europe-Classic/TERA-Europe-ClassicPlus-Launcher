use log::{error, info};
use std::process::Command;
use winapi::um::processthreadsapi::GetCurrentProcessId;
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
 
use crate::global_credentials::GLOBAL_CREDENTIALS;
use std::{fs::OpenOptions, io::Read, path::Path};

/// Find process ID by name
pub fn find_process_by_name(process_name: &str) -> Option<DWORD> {
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

/// Inject DLL into the game process (minimal logs/strings)
pub fn inject_agnitor(game_pid: DWORD) -> Result<(), Box<dyn std::error::Error>> {
    // Skip if we're trying to inject into ourselves
    let current_pid = unsafe { GetCurrentProcessId() };
    if game_pid == current_pid {
        return Err("err".into());
    }

    // Wait a moment for the game process to fully initialize
    std::thread::sleep(std::time::Duration::from_millis(2000));

    // Determine the game directory from GLOBAL_CREDENTIALS.get_game_path()
    let game_dir = {
        let game_path_str = GLOBAL_CREDENTIALS.get_game_path();
        let p = std::path::PathBuf::from(game_path_str);
        p.parent()
            .map(|pp| pp.to_path_buf())
            .unwrap_or_else(|| {
                let mut exe_dir = std::env::current_exe().unwrap_or_else(|_| std::env::temp_dir());
                exe_dir.pop();
                exe_dir
            })
    };

    // Embed and extract required binaries to the game folder (only overwrite if changed).
    // Extract 32-bit agnitor.dll
    let dll_bytes: &[u8] = include_bytes!("../../agnitor.dll");
    let dll_path = game_dir.join("agnitor.dll");
    write_if_different(&dll_path, dll_bytes)?;
    let dll32_path = dll_path.canonicalize().unwrap_or(dll_path.clone());
    let dll32_str = dll32_path.to_str().ok_or("err")?.to_string();

    // Extract 32-bit helper terainject32.exe
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

/// Write bytes to path only if the file is missing or contents differ.
fn write_if_different(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
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

