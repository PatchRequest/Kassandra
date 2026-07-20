#![allow(internal_features)]
#![feature(c_variadic)]
#![feature(core_intrinsics)]
pub mod loader;
pub mod nt_mem;

use loader::ObjectLoader;

#[no_mangle]
pub unsafe extern "C" fn execute_bof(
    bof: *const u8,
    bof_len: usize,
    args: *const u8,
    args_len: usize,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    if bof.is_null() || bof_len == 0 {
        return -1;
    }

    let bof_bytes = std::slice::from_raw_parts(bof, bof_len);

    let loader = match ObjectLoader::new(bof_bytes) {
        Ok(l) => l,
        Err(_) => return -1,
    };

    let (arguments, argument_size) = if !args.is_null() && args_len > 0 {
        (Some(args), Some(args_len))
    } else {
        (None, None)
    };

    let result = match loader.execute(arguments, argument_size, &Some("go".to_string())) {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let bytes = result.into_bytes();
    let len = bytes.len();
    let ptr = bytes.as_ptr();
    std::mem::forget(bytes);

    *out = ptr as *mut u8;
    *out_len = len;

    0
}
