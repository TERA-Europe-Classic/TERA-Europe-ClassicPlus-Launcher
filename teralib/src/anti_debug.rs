use std::mem::size_of;
use std::ffi::CString;
use widestring::U16CString;
use winapi::ctypes::c_void;
use winapi::shared::minwindef::{BOOL, FARPROC, HMODULE};
use winapi::shared::ntdef::{NTSTATUS, PVOID, ULONG};
use winapi::um::libloaderapi::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use winapi::um::processthreadsapi::GetCurrentProcess;

// Types for dynamic resolution
type IsDebuggerPresentFn = unsafe extern "system" fn() -> BOOL;
type CheckRemoteDebuggerPresentFn = unsafe extern "system" fn(*mut c_void, *mut BOOL) -> BOOL;
type NtQueryInformationProcessFn = unsafe extern "system" fn(
    ProcessHandle: *mut c_void,
    ProcessInformationClass: ULONG,
    ProcessInformation: PVOID,
    ProcessInformationLength: ULONG,
    ReturnLength: *mut ULONG,
) -> NTSTATUS;

// PROCESSINFOCLASS values
const PROCESS_DEBUG_PORT: ULONG = 7;
const PROCESS_DEBUG_FLAGS: ULONG = 31;

fn get_module_w(name: &str) -> Option<HMODULE> {
    let wide = U16CString::from_str(name).ok()?;
    unsafe {
        let h = GetModuleHandleW(wide.as_ptr());
        if h.is_null() {
            let h2 = LoadLibraryW(wide.as_ptr());
            if h2.is_null() { None } else { Some(h2) }
        } else {
            Some(h)
        }
    }
}

fn get_proc(module: HMODULE, name: &str) -> Option<FARPROC> {
    unsafe {
        let cstr = CString::new(name).ok()?;
        let proc: FARPROC = GetProcAddress(module, cstr.as_ptr());
        if proc.is_null() { None } else { Some(proc) }
    }
}

fn check_is_debugger_present() -> bool {
    if let Some(hmod) = get_module_w(&lc!("kernel32.dll")) {
        if let Some(p) = get_proc(hmod, &lc!("IsDebuggerPresent")) {
            let func: IsDebuggerPresentFn = unsafe { std::mem::transmute(p) };
            unsafe { return func() != 0; }
        }
    }
    false
}

fn check_remote_debugger_present() -> bool {
    if let Some(hmod) = get_module_w(&lc!("kernel32.dll")) {
        if let Some(p) = get_proc(hmod, &lc!("CheckRemoteDebuggerPresent")) {
            let func: CheckRemoteDebuggerPresentFn = unsafe { std::mem::transmute(p) };
            unsafe {
                let hproc = GetCurrentProcess();
                let mut present: BOOL = 0;
                if func(hproc, &mut present) != 0 { return present != 0; }
            }
        }
    }
    false
}

fn ntquery_u32(info_class: ULONG) -> Option<u32> {
    let hmod = get_module_w(&lc!("ntdll.dll"))?;
    let p = get_proc(hmod, &lc!("NtQueryInformationProcess"))?;
    let func: NtQueryInformationProcessFn = unsafe { std::mem::transmute(p) };
    unsafe {
        let hproc = GetCurrentProcess();
        let mut out: u32 = 0;
        let mut ret_len: ULONG = 0;
        let status = func(
            hproc as *mut c_void,
            info_class,
            &mut out as *mut u32 as PVOID,
            size_of::<u32>() as ULONG,
            &mut ret_len as *mut ULONG,
        );
        if status == 0 { Some(out) } else { None }
    }
}

fn check_nt_debug_port() -> bool {
    // If ProcessDebugPort returns non-zero or -1, a debugger is attached
    if let Some(port) = ntquery_u32(PROCESS_DEBUG_PORT) {
        return port != 0 && port != u32::MAX;
    }
    false
}

fn check_nt_debug_flags() -> bool {
    // ProcessDebugFlags returns 1 when no debugger is present; 0 if debugged
    if let Some(flags) = ntquery_u32(PROCESS_DEBUG_FLAGS) {
        return flags == 0;
    }
    false
}

pub fn anti_debug_detected() -> bool {
    // Combine multiple light checks
    check_is_debugger_present()
        || check_remote_debugger_present()
        || check_nt_debug_port()
        || check_nt_debug_flags()
}
