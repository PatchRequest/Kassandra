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
    let uuid = config::UUID.read().unwrap();
    BusyWork::new(Intensity::Ultra)
        .feed(uuid.as_str())
        .feed(config::callback_host)
        .feed(config::user_agent)
        .run();
}

pub fn idle() {
    let uuid = config::UUID.read().unwrap();
    BusyWork::new(intensity())
        .feed(uuid.as_str())
        .feed(config::callback_host)
        .run();
    std::thread::sleep(std::time::Duration::from_millis(500));
    BusyWork::new(intensity())
        .feed(config::user_agent)
        .feed(uuid.as_str())
        .run();
    std::thread::sleep(std::time::Duration::from_millis(500));
    BusyWork::new(Intensity::High)
        .feed(config::callback_host)
        .feed(config::user_agent)
        .feed(uuid.as_str())
        .run();
}

pub fn churn(data: &(impl FeedWork + ?Sized)) {
    BusyWork::new(Intensity::Low)
        .feed(data)
        .run();
}
