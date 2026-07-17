#[no_mangle]
pub unsafe extern "C" fn execute_bof(
    _bof: *const u8,
    _bof_len: usize,
    _args: *const u8,
    _args_len: usize,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    let msg = b"reflective loader works!";
    let buf = msg.to_vec();
    let len = buf.len();
    let ptr = buf.as_ptr();
    std::mem::forget(buf);
    *out = ptr as *mut u8;
    *out_len = len;
    0
}
