use std::env;
use std::path::PathBuf;

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


}