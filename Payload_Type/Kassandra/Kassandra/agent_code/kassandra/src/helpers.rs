use crate::config;
use busywork::{BusyWork, Intensity, FeedWork};

fn intensity() -> Intensity {
    match config::busywork_intensity {
        "low" => Intensity::Low,
        "high" => Intensity::High,
        "ultra" => Intensity::Ultra,
        _ => Intensity::Medium,
    }
}

pub fn startup_delay() {
    println!("[BW] startup_delay: ULTRA");
    let uuid = config::UUID.read().unwrap();
    BusyWork::new(Intensity::Ultra)
        .feed(uuid.as_str())
        .feed(config::callback_host)
        .feed(config::user_agent)
        .run();
    println!("[BW] startup_delay: done");
}

pub fn idle() {
    println!("[BW] idle: phase1 medium");
    let uuid = config::UUID.read().unwrap();
    BusyWork::new(intensity())
        .feed(uuid.as_str())
        .feed(config::callback_host)
        .run();
    println!("[BW] idle: sleep 500ms");
    std::thread::sleep(std::time::Duration::from_millis(500));
    println!("[BW] idle: phase2 medium");
    BusyWork::new(intensity())
        .feed(config::user_agent)
        .feed(uuid.as_str())
        .run();
    println!("[BW] idle: sleep 500ms");
    std::thread::sleep(std::time::Duration::from_millis(500));
    println!("[BW] idle: phase3 heavy");
    BusyWork::new(Intensity::High)
        .feed(config::callback_host)
        .feed(config::user_agent)
        .feed(uuid.as_str())
        .run();
    println!("[BW] idle: done");
}

pub fn churn(data: &(impl FeedWork + ?Sized)) {
    BusyWork::new(Intensity::Low)
        .feed(data)
        .run();
}
