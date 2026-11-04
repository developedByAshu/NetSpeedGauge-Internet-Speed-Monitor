//! Tests for the parts of NetSpeedGauge that do not need a screen.

use netspeedgauge::format::{format_speed, widest_speed_text, Scale, SpeedFormat, Unit};
use netspeedgauge::net_counters::{is_physical, Counter, DeltaTracker};
use netspeedgauge::readout::{arrow_column_left, tint_pixel};
use netspeedgauge::startup::startup_command;
use netspeedgauge::taskbar_dock::{covers_monitor, is_taskbar_collapsed, place_readout, to_taskbar_coords};
use windows_sys::Win32::Foundation::RECT;

const TYPE_ETHERNET: u32 = 6;
const TYPE_WIFI: u32 = 71;
const TYPE_PPP: u32 = 23;
const TYPE_TUNNEL: u32 = 131;
const HARDWARE_WITH_CONNECTOR: u8 = 0b0000_0101;
const FILTER_LAYER: u8 = 0b0000_0010;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn format_with(unit: Unit, scale: Scale) -> SpeedFormat {
    SpeedFormat { unit, scale, ..SpeedFormat::default() }
}

fn counter(luid: u64, up: u64, down: u64) -> Counter {
    Counter { luid, up, down }
}

fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT { left, top, right, bottom }
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
fn fullscreen_means_covering_the_whole_monitor() {
    let monitor = rect(0, 0, 1920, 1080);
    assert!(covers_monitor(&rect(0, 0, 1920, 1080), &monitor));
    assert!(covers_monitor(&rect(-8, -8, 1928, 1088), &monitor));
    assert!(!covers_monitor(&rect(0, 0, 1920, 1032), &monitor));
}

#[test]
fn child_position_is_measured_from_the_taskbar_corner() {
    assert_eq!(to_taskbar_coords((1543, 1039), &rect(0, 1032, 1920, 1080)), (1543, 7));
    assert_eq!(to_taskbar_coords((6, 900), &rect(0, 0, 48, 1080)), (6, 900));
    assert_eq!(to_taskbar_coords((2000, 1039), &rect(1920, 1032, 3840, 1080)), (80, 7));
}

#[test]
fn startup_command_is_quoted_so_spaces_in_the_path_are_safe() {
    assert_eq!(startup_command("C:\\My Apps\\NetSpeedGauge.exe"), "\"C:\\My Apps\\NetSpeedGauge.exe\"");
}

#[test]
fn real_adapters_count_once() {
    assert!(is_physical(TYPE_WIFI, HARDWARE_WITH_CONNECTOR, &wide("MediaTek Wi-Fi 6")));
    assert!(is_physical(TYPE_ETHERNET, HARDWARE_WITH_CONNECTOR, &wide("Realtek PCIe GbE")));
}

#[test]
fn filter_layers_never_count() {
    assert!(!is_physical(TYPE_WIFI, FILTER_LAYER, &wide("MediaTek Wi-Fi 6-QoS Packet Scheduler-0000")));
    assert!(!is_physical(TYPE_WIFI, FILTER_LAYER, &wide("Microsoft Wi-Fi Direct Virtual Adapter #3-QoS")));
}

#[test]
fn direct_links_count_without_a_hardware_flag() {
    assert!(is_physical(TYPE_WIFI, 0, &wide("Microsoft Wi-Fi Direct Virtual Adapter #3")));
    assert!(is_physical(TYPE_ETHERNET, 0, &wide("Bluetooth Device (Personal Area Network)")));
}

#[test]
fn vpn_tunnels_never_count() {
    assert!(!is_physical(TYPE_TUNNEL, 0, &wide("WAN Miniport (IKEv2)")));
    assert!(!is_physical(TYPE_PPP, 0, &wide("WAN Miniport (PPPOE)")));
    assert!(!is_physical(TYPE_ETHERNET, 0, &wide("TAP-Windows Adapter V9")));
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

#[test]
fn readout_sits_left_of_the_tray_on_a_horizontal_taskbar() {
    let taskbar = rect(0, 1032, 1920, 1080);
    let tray = rect(1600, 1032, 1920, 1080);
    assert_eq!(place_readout(&taskbar, &tray, 100, 40, 1.0), (1496, 1036));
}

#[test]
fn readout_sits_above_the_tray_on_a_vertical_taskbar() {
    let taskbar = rect(0, 0, 200, 1080);
    let tray = rect(0, 900, 200, 1080);
    assert_eq!(place_readout(&taskbar, &tray, 100, 40, 1.0), (50, 856));
}

#[test]
fn collapsed_auto_hide_taskbar_is_detected() {
    assert!(is_taskbar_collapsed(&rect(0, 1078, 1920, 1080), 100, 40));
    assert!(!is_taskbar_collapsed(&rect(0, 1032, 1920, 1080), 100, 40));
}

#[test]
fn undrawn_pixels_stay_clickable_and_almost_invisible() {
    assert_eq!(tint_pixel(0, 0x00FF_FFFF, 0, 0), 0x0100_0000);
}

#[test]
fn drawn_pixels_take_the_chosen_colour() {
    assert_eq!(tint_pixel(0x00FF_FFFF, 0x0012_3456, 0, 0), 0xFF12_3456);
}

#[test]
fn arrow_column_is_shared_by_both_rows_whatever_their_digits() {
    let value_right = 100;
    let (gap, stroke, arrow_width) = (8, 1, 6);
    let three_digits = 50;
    let one_digit = 30;
    let widest = three_digits.max(one_digit);
    let column = arrow_column_left(value_right, widest, gap, stroke, arrow_width);
    assert_eq!(column, 100 - 50 - 8 - 1 - 6);
    assert_eq!(column, arrow_column_left(value_right, one_digit.max(three_digits), gap, stroke, arrow_width));
}

#[test]
fn background_shows_only_where_there_is_no_text() {
    assert_eq!(tint_pixel(0, 0, 0x00FF_0000, 255), 0xFFFF_0000);
    assert_eq!(tint_pixel(0x00FF_FFFF, 0x0000_00FF, 0x00FF_0000, 255), 0xFF00_00FF);
}
