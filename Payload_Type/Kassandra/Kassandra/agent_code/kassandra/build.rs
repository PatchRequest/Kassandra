use std::env;
use std::path::Path;

fn main() {
    let target = env::var("TARGET").expect("Missing TARGET environment variable");
    let out_dir = env::var("OUT_DIR").expect("Missing OUT_DIR environment variable");

    if !target.contains("x86_64") {
        panic!("This build script only supports x86_64 targets.");
    }

    if target.contains("msvc") {
        cc::Build::new()
            .file("src/asm/msvc/hellsasm.asm")
            .compile("hellsasm");
    } else if target.contains("gnu") {
        let sources = ["src/asm/hellsasm.asm"];
        if let Err(e) = nasm_rs::compile_library("hellsasm", &sources) {
            panic!("Failed to compile with NASM [hellsasm]: {}", e);
        }
        for source in &sources {
            println!("cargo:rerun-if-changed={}", source);
        }
        println!("cargo:rustc-link-search=native={}", out_dir);
        println!("cargo:rustc-link-lib=static=hellsasm");

    } else {
        panic!("Unsupported target: {}", target);
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
            panic!("tailscale feature enabled but libtailscale_ffi.a not found at {}", ts_lib_dir);
        }
    }
}
