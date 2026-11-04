//! Prints what the app reads for each adapter, to compare against the system's own numbers.

#[cfg(target_os = "macos")]
fn main() {
    use netspeedgauge_mac::net_counters::{list_adapters, read_counters, InterfaceMode};
    for row in list_adapters() {
        println!("{:>3} {:<28} physical={:<5} up={} down={}", row.index, row.name, row.is_physical, row.up, row.down);
    }
    let mut counters = Vec::new();
    read_counters(&InterfaceMode::Auto, &mut counters);
    println!("main only: {counters:?}");
}

#[cfg(not(target_os = "macos"))]
fn main() {}
