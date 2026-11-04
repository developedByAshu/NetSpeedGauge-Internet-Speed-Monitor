//! Lists every adapter with its flags, then optionally totals counted traffic for N seconds.
//! Usage: cargo run --example list_adapters [seconds]

use netspeedgauge_windows::net_counters::{list_adapters, read_counters, Counter, DeltaTracker, InterfaceMode};
use std::time::{Duration, Instant};

fn main() {
    for row in list_adapters() {
        println!(
            "{} type={:<3} flags={:08b} up={:<14} down={:<14} {} | {}",
            if row.is_physical { "COUNT" } else { "  -  " },
            row.if_type,
            row.flags,
            row.up,
            row.down,
            row.alias,
            row.description,
        );
    }

    let Some(seconds) = std::env::args().nth(1).and_then(|text| text.parse::<u64>().ok()) else {
        return;
    };
    let mode = InterfaceMode::AllPhysical;
    let mut counters: Vec<Counter> = Vec::new();
    let mut tracker = DeltaTracker::default();
    read_counters(&mode, &mut counters);
    tracker.update(&counters, 0.0);
    let mut last = Instant::now();
    let (mut up, mut down) = (0u64, 0u64);
    for _ in 0..seconds {
        std::thread::sleep(Duration::from_secs(1));
        read_counters(&mode, &mut counters);
        let (moved_up, moved_down) = tracker.update(&counters, last.elapsed().as_secs_f64());
        last = Instant::now();
        up += moved_up;
        down += moved_down;
    }
    println!("TOTAL up={up} down={down}");
}
