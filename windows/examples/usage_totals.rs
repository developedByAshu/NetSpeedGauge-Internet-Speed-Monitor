//! Prints what is saved in the real usage file, for checking its accuracy.
//! Usage: cargo run --example usage_totals

use netspeedgauge_windows::calendar::today;
use netspeedgauge_windows::usage_store::{data_folder, UsageStore};

fn main() {
    let store = UsageStore::load_from(&data_folder());
    let day = today();
    let saved_down: u64 = store.records.iter().map(|row| row.down).sum();
    let saved_up: u64 = store.records.iter().map(|row| row.up).sum();
    let counter_down: u64 = store.baseline.iter().map(|counter| counter.down).sum();
    let counter_up: u64 = store.baseline.iter().map(|counter| counter.up).sum();
    let today_rows = store.records.iter().filter(|row| row.day == day).count();
    println!(
        "saved_down={saved_down} saved_up={saved_up} counter_down={counter_down} counter_up={counter_up} adapters={} today_rows={today_rows}",
        store.adapters.len()
    );
    for adapter in &store.adapters {
        println!("adapter: {}", adapter.name);
    }
}
