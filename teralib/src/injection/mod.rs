use log::{error, info};
use std::io::Write;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
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
use cryptify;

pub fn find_process_by_name(ogpuex: &str) -> Option<DWORD> {
    cryptify::flow_stmt!();
    unsafe {
        let l_tb = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if l_tb.is_null() {
            return None;
        }
        let mut ipoxcje = PROCESSENTRY32W {
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
        if Process32FirstW(l_tb, &mut ipoxcje) == 0 {
            CloseHandle(l_tb);
            return None;
        }
        loop {
            let haff = String::from_utf16_lossy(&ipoxcje.szExeFile)
                .trim_end_matches('\0')
                .to_lowercase();
            if haff.contains(&ogpuex.to_lowercase()) {
                let amammu_mzg = ipoxcje.th32ProcessID;
                CloseHandle(l_tb);
                return Some(amammu_mzg);
            }
            if Process32NextW(l_tb, &mut ipoxcje) == 0 {
                break;
            }
        }
        CloseHandle(l_tb);
        None
    }
}
pub fn inject_agnitor(gajx_wmwm: DWORD) -> Result<(), Box<dyn std::error::Error>> {
    let brrrt_j = unsafe { GetCurrentProcessId() };
    if gajx_wmwm == brrrt_j {
        return Err("err".into());
    }
    std::thread::sleep(std::time::Duration::from_millis(2000));
    let dll_bytes: &[u8] = include_bytes!("../../agnitor.dll");
    let mut o_zy = std::env::temp_dir();
    let qlvqb = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    o_zy.push(format!("{} {} {}", lc!("agnitor_{}_{}.dll"), std::process::id(), qlvqb));
    {
        let mut mnzvqc_nl = std::fs::File::create(&o_zy)?;
        use std::io::Write as _;
        mnzvqc_nl.write_all(dll_bytes)?;
    }
    let x_pryeza_d = o_zy.canonicalize().unwrap_or(o_zy.clone());
    let zxesldwrdg = x_pryeza_d.to_str().ok_or("err")?.to_string();
    let helper_bytes: &[u8] = include_bytes!("../../terainject32.exe");
    let mut fylvovcs = std::env::temp_dir();
    fylvovcs.push(format!("{} {} {}", lc!("terainject32_{}_{}.exe"), std::process::id(), qlvqb));
    {
        let mut y_bydzbrf = std::fs::File::create(&fylvovcs)?;
        use std::io::Write as _;
        y_bydzbrf.write_all(helper_bytes)?;
    }
    let ynhp_tp = fylvovcs.canonicalize().unwrap_or(fylvovcs.clone());
    let xntjxyla = ynhp_tp.to_str().ok_or("err")?;
    let cfmy = Command::new(xntjxyla)
        .arg(gajx_wmwm.to_string())
        .arg(zxesldwrdg)
        .status()?;
    if !cfmy.success() {
        let _ = std::fs::remove_file(&ynhp_tp);
        let _ = std::fs::remove_file(&x_pryeza_d);
        return Err("err".into());
    }
    let _ = std::fs::remove_file(&ynhp_tp);
    let _ = std::fs::remove_file(&x_pryeza_d);
    return Ok(());
}
