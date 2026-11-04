//! Tests for the Windows parts of NetSpeedGauge that do not need a screen.

use netspeedgauge_windows::canvas::round_rect_coverage;
use netspeedgauge_windows::net_counters::is_physical;
use netspeedgauge_windows::popup_ui::{slide_in_offset, slide_out_offset};
use netspeedgauge_windows::readout::{arrow_column_left, selected_colors, tint_pixel, ReadoutColors};
use netspeedgauge_windows::startup::startup_command;
use netspeedgauge_windows::taskbar_dock::{covers_monitor, is_taskbar_collapsed, place_readout, to_taskbar_coords};
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

fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT { left, top, right, bottom }
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

#[test]
fn rounded_rectangle_covers_its_middle_and_trims_its_corner() {
    let cover = |px, py| round_rect_coverage(px, py, 0.0, 0.0, 20.0, 20.0, 6.0);
    assert_eq!(cover(10.0, 10.0), 1.0);
    assert_eq!(cover(0.5, 0.5), 0.0);
    assert_eq!(cover(25.0, 10.0), 0.0);
    let edge_of_corner = cover(1.76, 1.76);
    assert!(edge_of_corner > 0.0 && edge_of_corner < 1.0);
}

fn plain_colors(background: u32, background_alpha: u32) -> ReadoutColors {
    ReadoutColors { up_arrow: 1, down_arrow: 2, up_value: 3, down_value: 4, background, background_alpha }
}

#[test]
fn selected_numbers_with_no_background_get_a_soft_one_in_the_taskbar_text_colour() {
    let selected = selected_colors(&plain_colors(0, 0), 0x00FF_FFFF);
    assert_eq!(selected.background, 0x00FF_FFFF);
    assert!(selected.background_alpha > 0 && selected.background_alpha < 64);
}

#[test]
fn selected_numbers_shift_a_chosen_background_and_keep_its_strength() {
    let selected = selected_colors(&plain_colors(0x0020_2020, 200), 0x00FF_FFFF);
    assert_ne!(selected.background, 0x0020_2020);
    assert_eq!(selected.background_alpha, 200);
}

#[test]
fn selected_look_never_changes_the_number_colours() {
    let selected = selected_colors(&plain_colors(0, 0), 0x00FF_FFFF);
    assert_eq!((selected.up_arrow, selected.down_arrow, selected.up_value, selected.down_value), (1, 2, 3, 4));
}

#[test]
fn slide_in_starts_fully_down_and_lands_exactly_in_place() {
    assert_eq!(slide_in_offset(0.0), 1.0);
    assert_eq!(slide_in_offset(180.0), 0.0);
    assert_eq!(slide_in_offset(100_000.0), 0.0);
}

#[test]
fn slide_in_slows_as_it_arrives() {
    let first_half = slide_in_offset(0.0) - slide_in_offset(90.0);
    let second_half = slide_in_offset(90.0) - slide_in_offset(180.0);
    assert!(first_half > second_half);
}

#[test]
fn slide_out_ends_fully_down_and_speeds_up_as_it_leaves() {
    assert_eq!(slide_out_offset(0.0), 0.0);
    assert_eq!(slide_out_offset(160.0), 1.0);
    assert!(slide_out_offset(80.0) < 0.5);
}
