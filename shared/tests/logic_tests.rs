//! Tests for speed text and for counting the bytes moved.

use netspeedgauge_shared::format::{format_speed, widest_speed_text, Scale, SpeedFormat, Unit};
use netspeedgauge_shared::net_counters::{Counter, DeltaTracker};

fn format_with(unit: Unit, scale: Scale) -> SpeedFormat {
    SpeedFormat { unit, scale, ..SpeedFormat::default() }
}

fn counter(luid: u64, up: u64, down: u64) -> Counter {
    Counter { luid, up, down }
}

#[test]
fn formats_all_four_units_in_mega() {
    let speed = 1_000_000.0;
    assert_eq!(format_speed(speed, &format_with(Unit::BytesDecimal, Scale::AlwaysMega)), "1.0 MBps");
    assert_eq!(format_speed(speed, &format_with(Unit::BitsDecimal, Scale::AlwaysMega)), "8.0 Mbps");
    assert_eq!(format_speed(speed, &format_with(Unit::BytesBinary, Scale::AlwaysMega)), "1.0 MiBps");
    assert_eq!(format_speed(speed, &format_with(Unit::BitsBinary, Scale::AlwaysMega)), "7.6 Mibps");
}

#[test]
fn formats_zero() {
    assert_eq!(format_speed(0.0, &SpeedFormat::default()), "0.0 MBps");
}

#[test]
fn formats_very_large_speed_in_giga_on_auto() {
    let format = format_with(Unit::BytesDecimal, Scale::Auto);
    assert_eq!(format_speed(2_500_000_000.0, &format), "2.5 GBps");
}

#[test]
fn auto_scale_uses_kilo_for_small_speeds() {
    let format = format_with(Unit::BytesDecimal, Scale::Auto);
    assert_eq!(format_speed(5_000.0, &format), "5.0 KBps");
}

#[test]
fn display_options_change_the_text() {
    let hidden = SpeedFormat { hide_suffix: true, ..SpeedFormat::default() };
    let short = SpeedFormat { short_labels: true, ..SpeedFormat::default() };
    let two_places = SpeedFormat { decimals: 2, ..SpeedFormat::default() };
    assert_eq!(format_speed(1_500_000.0, &hidden), "1.5");
    assert_eq!(format_speed(1_500_000.0, &short), "1.5 M");
    assert_eq!(format_speed(1_500_000.0, &two_places), "1.50 MBps");
}

#[test]
fn three_digit_speeds_keep_their_decimal() {
    assert_eq!(format_speed(125_000_000.0, &SpeedFormat::default()), "125.0 MBps");
}

#[test]
fn mega_moves_to_giga_instead_of_growing_to_four_digits() {
    assert_eq!(format_speed(1_300_000_000.0, &SpeedFormat::default()), "1.3 GBps");
}

#[test]
fn speed_column_is_sized_from_the_chosen_unit() {
    assert_eq!(widest_speed_text(&SpeedFormat::default()), "999.0 MBps");
    assert_eq!(widest_speed_text(&format_with(Unit::BitsBinary, Scale::Auto)), "999.0 Mibps");
}

#[test]
fn first_reading_only_sets_the_baseline() {
    let mut tracker = DeltaTracker::default();
    assert_eq!(tracker.update(&[counter(1, 5_000, 9_000)], 1.0), (0, 0));
}

#[test]
fn normal_reading_counts_the_difference() {
    let mut tracker = DeltaTracker::default();
    tracker.update(&[counter(1, 100, 200)], 1.0);
    assert_eq!(tracker.update(&[counter(1, 150, 500)], 1.0), (50, 300));
}

#[test]
fn counter_reset_counts_zero_then_resumes() {
    let mut tracker = DeltaTracker::default();
    tracker.update(&[counter(1, 1_000, 1_000)], 1.0);
    assert_eq!(tracker.update(&[counter(1, 10, 20)], 1.0), (0, 0));
    assert_eq!(tracker.update(&[counter(1, 30, 70)], 1.0), (20, 50));
}

#[test]
fn sleep_gap_keeps_the_real_bytes() {
    let mut tracker = DeltaTracker::default();
    tracker.update(&[counter(1, 0, 0)], 1.0);
    assert_eq!(tracker.update(&[counter(1, 40_000_000, 900_000_000)], 3_600.0), (40_000_000, 900_000_000));
}

#[test]
fn impossible_jump_is_capped() {
    let mut tracker = DeltaTracker::default();
    tracker.update(&[counter(1, 0, 0)], 1.0);
    let (_, down) = tracker.update(&[counter(1, 0, 10_000_000_000_000)], 1.0);
    assert_eq!(down, 12_500_000_000);
}

#[test]
fn adapter_that_disappears_resumes_without_a_false_jump() {
    let mut tracker = DeltaTracker::default();
    tracker.update(&[counter(1, 100, 100), counter(2, 500, 500)], 1.0);
    assert_eq!(tracker.update(&[counter(1, 110, 120)], 1.0), (10, 20));
    assert_eq!(tracker.update(&[counter(1, 110, 120), counter(2, 600, 800)], 1.0), (100, 300));
}

#[test]
fn several_adapters_add_up() {
    let mut tracker = DeltaTracker::default();
    tracker.update(&[counter(1, 0, 0), counter(2, 0, 0)], 1.0);
    assert_eq!(tracker.update(&[counter(1, 10, 20), counter(2, 30, 40)], 1.0), (40, 60));
}
