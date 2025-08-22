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
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

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

    // Embed and extract required binaries to temporary files at runtime
    // Extract 32-bit agnitor.dll
    let dll_bytes: &[u8] = include_bytes!("../../agnitor.dll");
    let mut dll_tmp = std::env::temp_dir();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    dll_tmp.push(format!("agnitor_{}_{}.dll", std::process::id(), now_ms));
    {
        let mut f = std::fs::File::create(&dll_tmp)?;
        use std::io::Write as _;
        f.write_all(dll_bytes)?;
    }
    let dll32_path = dll_tmp.canonicalize().unwrap_or(dll_tmp.clone());
    let dll32_str = dll32_path.to_str().ok_or("err")?.to_string();

    // Extract 32-bit helper terainject32.exe
    let helper_bytes: &[u8] = include_bytes!("../../terainject32.exe");
    let mut helper_tmp = std::env::temp_dir();
    helper_tmp.push(format!(
        "terainject32_{}_{}.exe",
        std::process::id(),
        now_ms
    ));
    {
        let mut f = std::fs::File::create(&helper_tmp)?;
        use std::io::Write as _;
        f.write_all(helper_bytes)?;
    }
    let helper_path = helper_tmp.canonicalize().unwrap_or(helper_tmp.clone());
    let helper_str = helper_path.to_str().ok_or("err")?;

    let status = Command::new(helper_str)
        .arg(game_pid.to_string())
        .arg(dll32_str)
        .status()?;

    if !status.success() {
        // Cleanup temp files best-effort before returning error
        let _ = std::fs::remove_file(&helper_path);
        let _ = std::fs::remove_file(&dll32_path);
        return Err("err".into());
    }

    // Best-effort cleanup of temp files after successful injection
    let _ = std::fs::remove_file(&helper_path);
    let _ = std::fs::remove_file(&dll32_path);
    return Ok(());
}

