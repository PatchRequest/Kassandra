use std::slice;

use rustclr::{RustClr, RuntimeVersion};

#[no_mangle]
pub unsafe extern "C" fn execute_dot(
    asm: *const u8,
    asm_len: usize,
    args: *const u8,
    args_len: usize,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    if asm_len == 0 {
        return -1;
    }

    let asm_bytes = slice::from_raw_parts(asm, asm_len);

    let args_vec: Vec<&str> = if args_len == 0 {
        Vec::new()
    } else {
        let args_bytes = slice::from_raw_parts(args, args_len);
        match std::str::from_utf8(args_bytes) {
            Ok(s) => s.split_whitespace().collect(),
            Err(_) => return -1,
        }
    };

    let mut builder = match RustClr::new(asm_bytes) {
        Ok(b) => b,
        Err(_) => return -2,
    };

    builder = builder
        .with_runtime_version(RuntimeVersion::V4)
        .with_output();

    if !args_vec.is_empty() {
        builder = builder.with_args(args_vec);
    }

    let output = match builder.run() {
        Ok(o) => o,
        Err(_) => return -3,
    };

    let output_bytes = output.into_bytes();
    let mut output_vec = output_bytes.into_boxed_slice();
    let ptr = output_vec.as_mut_ptr();
    let len = output_vec.len();
    std::mem::forget(output_vec);

    *out = ptr;
    *out_len = len;

    0
}
