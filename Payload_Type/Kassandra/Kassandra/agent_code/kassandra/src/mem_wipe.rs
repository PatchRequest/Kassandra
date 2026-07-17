use std::ffi::c_void;

#[link(name = "kernel32")]
extern "system" {
    fn VirtualFree(addr: *mut c_void, size: usize, free_type: u32) -> i32;
}

pub unsafe fn wipe(ptr: *mut u8, len: usize) {
    for i in 0..len {
        std::ptr::write_volatile(ptr.add(i), 0u8);
    }
}

pub unsafe fn wipe_vec(vec: &mut Vec<u8>) {
    let ptr = vec.as_mut_ptr();
    let len = vec.len();
    wipe(ptr, len);
    vec.clear();
}

pub unsafe fn wipe_and_free(base: *mut u8, size: usize) {
    wipe(base, size);
    VirtualFree(base as *mut c_void, 0, 0x8000); // MEM_RELEASE
}
