//! Starts NetSpeedGauge.

#[cfg(target_os = "macos")]
fn main() {
    netspeedgauge_mac::app::run();
}

#[cfg(not(target_os = "macos"))]
fn main() {}
