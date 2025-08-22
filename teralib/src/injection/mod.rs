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

    // Resolve paths
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dll32_path = manifest_dir.join("agnitor.dll");
    let dll32_str = dll32_path.to_str().ok_or("err")?.to_string();
    let helper_path = manifest_dir.join("terainject32.exe");

    let helper_str = helper_path.to_str().ok_or("err")?;

    let status = Command::new(helper_str)
        .arg(game_pid.to_string())
        .arg(dll32_str)
        .status()?;

    if !status.success() {
        return Err("err".into());
    }

    // No cleanup needed when loading from manifest dir
    return Ok(());
}

