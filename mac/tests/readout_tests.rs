//! Tests for how the two speeds sit side by side in the menu bar.
#![cfg(target_os = "macos")]

use netspeedgauge_mac::readout::{pair_positions, pair_texts, total_width};
use netspeedgauge_shared::format::{Scale, SpeedFormat, Unit};

const RIGHT: i32 = 200;
const ARROW: i32 = 10;

#[test]
fn number_stays_next_to_its_arrow_at_any_speed() {
    for widths in [[20, 20], [60, 20], [20, 60], [70, 70]] {
        let [first, second] = pair_positions(RIGHT, ARROW, widths);
        assert_eq!(first.1 - first.0, second.1 - second.0);
        assert!(first.1 - first.0 - ARROW <= 3, "{widths:?}");
    }
}

#[test]
fn pairs_stay_close_whatever_the_first_number_is() {
    for first_width in [20, 40, 70] {
        let [first, second] = pair_positions(RIGHT, ARROW, [first_width, 30]);
        assert_eq!(second.0 - (first.1 + first_width), 8);
    }
}

#[test]
fn right_edge_never_moves() {
    for second_width in [20, 40, 70] {
        let [_, second] = pair_positions(RIGHT, ARROW, [30, second_width]);
        assert_eq!(second.1 + second_width, RIGHT - 4);
    }
}

#[test]
fn widest_numbers_fit_the_fixed_width() {
    let width = total_width(ARROW, 40, 70);
    let [first, _] = pair_positions(width, ARROW, [40, 70]);
    assert_eq!(first.0, 4);
}

#[test]
fn no_arrows_means_no_gap() {
    let [first, second] = pair_positions(RIGHT, 0, [30, 30]);
    assert_eq!(first.0, first.1);
    assert_eq!(second.0, second.1);
}

fn texts(up: f64, down: f64, format: SpeedFormat, is_swapped: bool) -> (String, String) {
    pair_texts(up, down, &format, is_swapped)
}

#[test]
fn unit_shows_once_on_whichever_is_drawn_last() {
    let format = SpeedFormat::default();
    assert_eq!(texts(1_800_000.0, 124_000_000.0, format, false), ("1.8".to_string(), "124.0 MBps".to_string()));
    assert_eq!(texts(1_800_000.0, 124_000_000.0, format, true), ("1.8 MBps".to_string(), "124.0".to_string()));
}

#[test]
fn both_use_the_faster_speeds_unit() {
    let auto = SpeedFormat { scale: Scale::Auto, ..SpeedFormat::default() };
    assert_eq!(texts(512_000.0, 1_200_000.0, auto, false), ("0.5".to_string(), "1.2 MBps".to_string()));
    assert_eq!(texts(1_200_000.0, 512_000.0, auto, false), ("1.2".to_string(), "0.5 MBps".to_string()));
    assert_eq!(texts(512_000.0, 900_000.0, auto, false), ("512.0".to_string(), "900.0 KBps".to_string()));
    let mega = SpeedFormat::default();
    assert_eq!(texts(1_500_000_000.0, 3_000_000.0, mega, false), ("1.5".to_string(), "0.0 GBps".to_string()));
}

#[test]
fn every_unit_keeps_its_own_size() {
    let bits = SpeedFormat { unit: Unit::BitsDecimal, ..SpeedFormat::default() };
    assert_eq!(texts(1_000_000.0, 2_000_000.0, bits, false), ("8.0".to_string(), "16.0 Mbps".to_string()));
    let binary = SpeedFormat { unit: Unit::BytesBinary, ..SpeedFormat::default() };
    assert_eq!(texts(1_048_576.0, 2_097_152.0, binary, false), ("1.0".to_string(), "2.0 MiBps".to_string()));
}

#[test]
fn hidden_and_short_units_still_work() {
    let hidden = SpeedFormat { hide_suffix: true, ..SpeedFormat::default() };
    assert_eq!(texts(1_800_000.0, 124_000_000.0, hidden, false), ("1.8".to_string(), "124.0".to_string()));
    let short = SpeedFormat { short_labels: true, ..SpeedFormat::default() };
    assert_eq!(texts(1_800_000.0, 124_000_000.0, short, false), ("1.8".to_string(), "124.0 M".to_string()));
}
