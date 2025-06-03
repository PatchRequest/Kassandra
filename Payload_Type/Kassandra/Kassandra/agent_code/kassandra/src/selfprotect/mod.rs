/*
    BlockHandle PoC
    Author: @5mukx
*/

use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr::null_mut};

use winapi::{
    shared::sddl::ConvertStringSecurityDescriptorToSecurityDescriptorW, 
    um::{processthreadsapi::{GetCurrentProcess, GetCurrentProcessId}, 
    securitybaseapi::SetKernelObjectSecurity, winbase::LocalFree, 
    winnt::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR}
}};

pub fn set_process_security_descriptor(){
    let sddl: Vec<u16> = OsStr::new(
        "D:P\
         (D;OICI;GA;;;WD)\
         (A;OICI;GA;;;SY)\
         (A;OICI;GA;;;OW)"
    ).encode_wide().chain(Some(0)).collect();

    let mut security_descriptor: PSECURITY_DESCRIPTOR = null_mut();

    let result = unsafe{
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut security_descriptor,
            null_mut(),
        )
    };

    if result == 0 {
        eprintln!("Failed to convert SDDL to security descriptor. Error: {}", 
            std::io::Error::last_os_error());
        return;
    }

    let process_handle = unsafe {
        GetCurrentProcess()
    };

    let set_result = unsafe {
        SetKernelObjectSecurity(
            process_handle,
            DACL_SECURITY_INFORMATION,
            security_descriptor,      
        )
    };

    if set_result == 0 {
        eprintln!("Failed to set security descriptor. Error: {}", 
            std::io::Error::last_os_error());
    }

    unsafe {
        LocalFree(security_descriptor as *mut _);
    }
}