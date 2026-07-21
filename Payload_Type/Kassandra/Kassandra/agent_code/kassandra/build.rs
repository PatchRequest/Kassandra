use std::env;
use std::path::Path;

fn main() {
    let target = env::var("TARGET").expect("Missing TARGET environment variable");

    if !target.contains("x86_64") {
        panic!("This build script only supports x86_64 targets.");
    }

    // Link the tailscale FFI Go static library only when the feature is enabled
    if cfg!(feature = "tailscale") {
        let ts_lib_dir = "/opt/tailscale_ffi";
        if Path::new(ts_lib_dir).join("libtailscale_ffi.a").exists() {
            println!("cargo:rustc-link-search=native={}", ts_lib_dir);
            println!("cargo:rustc-link-lib=static=tailscale_ffi");
            // Go runtime dependencies on Windows
            println!("cargo:rustc-link-lib=dylib=ws2_32");
            println!("cargo:rustc-link-lib=dylib=winmm");
            println!("cargo:rustc-link-lib=dylib=iphlpapi");
            println!("cargo:rustc-link-lib=dylib=ntdll");
            println!("cargo:rustc-link-lib=dylib=bcrypt");
            println!("cargo:rustc-link-lib=dylib=userenv");
            println!("cargo:rustc-link-lib=dylib=crypt32");
            println!("cargo:rustc-link-lib=dylib=ncrypt");
            println!("cargo:rustc-link-lib=dylib=ole32");
        } else {
            panic!(
                "tailscale feature enabled but libtailscale_ffi.a not found at {}",
                ts_lib_dir
            );
        }
    }
}
