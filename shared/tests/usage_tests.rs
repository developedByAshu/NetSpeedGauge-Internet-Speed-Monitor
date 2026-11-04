//! Tests for usage history: dates, grouping, saving and damage handling.

use netspeedgauge_shared::calendar::{date_from_day, day_from_date, month_start, short_date, week_start, year_start};
use netspeedgauge_shared::format::format_bytes;
use netspeedgauge_shared::net_counters::{Counter, DeltaTracker};
use netspeedgauge_shared::palette::mix;
use netspeedgauge_shared::usage_store::{catch_up_gap_secs, Bucket, Period, UsageStore, KEEP_DAYS};

const WIFI: u64 = 111;
const LAN: u64 = 222;

fn store_with_adapters() -> UsageStore {
    let mut store = UsageStore::default();
    store.add_adapter(WIFI, "Wi-Fi".to_string());
    store.add_adapter(LAN, "Ethernet".to_string());
    store
}

fn temp_folder(name: &str) -> std::path::PathBuf {
    let folder = std::env::temp_dir().join(format!("netspeedgauge_test_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    folder
}

#[test]
fn dates_round_trip_across_leap_years_and_year_ends() {
    for (year, month, day) in [(1970, 1, 1), (2024, 2, 29), (2026, 11, 3), (2026, 12, 31), (2027, 1, 1)] {
        assert_eq!(date_from_day(day_from_date(year, month, day)), (year, month, day));
    }
    assert_eq!(day_from_date(1970, 1, 1), 0);
    assert_eq!(day_from_date(2024, 3, 1) - day_from_date(2024, 2, 28), 2);
}

#[test]
fn week_starts_on_monday_including_on_sunday_and_monday() {
    let monday = day_from_date(2026, 11, 2);
    assert_eq!(week_start(monday), monday);
    assert_eq!(week_start(day_from_date(2026, 11, 3)), monday);
    assert_eq!(week_start(day_from_date(2026, 11, 8)), monday);
    assert_eq!(week_start(day_from_date(2026, 11, 9)), day_from_date(2026, 11, 9));
}

#[test]
fn month_and_year_start_on_their_first_day() {
    let day = day_from_date(2026, 11, 3);
    assert_eq!(month_start(day), day_from_date(2026, 11, 1));
    assert_eq!(year_start(day), day_from_date(2026, 1, 1));
    assert_eq!(short_date(day), "3 Nov");
}

#[test]
fn recording_twice_on_one_day_adds_up() {
    let mut store = store_with_adapters();
    let today = day_from_date(2026, 11, 3);
    store.record(WIFI, today, 10, 100);
    store.record(WIFI, today, 5, 50);
    assert_eq!(store.records.len(), 1);
    assert_eq!((store.records[0].up, store.records[0].down), (15, 150));
}

#[test]
fn adapters_stay_separate_and_all_adds_them_up() {
    let mut store = store_with_adapters();
    let today = day_from_date(2026, 11, 3);
    store.record(WIFI, today, 10, 100);
    store.record(LAN, today, 1, 2);
    let last = |adapter| *store.buckets(Period::Day, adapter, today).last().unwrap();
    assert_eq!(last(None), Bucket { start: today, up: 11, down: 102 });
    assert_eq!(last(Some(0)), Bucket { start: today, up: 10, down: 100 });
    assert_eq!(last(Some(1)), Bucket { start: today, up: 1, down: 2 });
}

#[test]
fn midnight_starts_a_new_day() {
    let mut store = store_with_adapters();
    let today = day_from_date(2026, 11, 3);
    store.record(WIFI, today - 1, 1, 1);
    store.record(WIFI, today, 2, 2);
    assert_eq!(store.records.len(), 2);
}

#[test]
fn each_period_groups_days_correctly_and_shows_its_bar_count() {
    let mut store = store_with_adapters();
    let today = day_from_date(2026, 11, 3);
    store.record(WIFI, day_from_date(2025, 12, 31), 0, 1);
    store.record(WIFI, day_from_date(2026, 10, 31), 0, 10);
    store.record(WIFI, day_from_date(2026, 11, 2), 0, 100);
    store.record(WIFI, today, 0, 1000);

    let day = store.buckets(Period::Day, None, today);
    assert_eq!(day.len(), 30);
    assert_eq!(day[29].down, 1000);
    assert_eq!(day[28].down, 100);

    let week = store.buckets(Period::Week, None, today);
    assert_eq!(week.len(), 12);
    assert_eq!(week[11].down, 1100);
    assert_eq!(week[10].down, 10);

    let month = store.buckets(Period::Month, None, today);
    assert_eq!(month.len(), 12);
    assert_eq!(month[11].down, 1100);
    assert_eq!(month[10].down, 10);
    assert_eq!(month[0].start, day_from_date(2025, 12, 1));
    assert_eq!(month[0].down, 1);

    let year = store.buckets(Period::Year, None, today);
    assert_eq!(year.len(), 2);
    assert_eq!(year[1].down, 1110);
    assert_eq!(year[0].down, 1);
}

#[test]
fn history_older_than_two_years_is_removed() {
    let mut store = store_with_adapters();
    let today = day_from_date(2026, 11, 3);
    store.record(WIFI, today - KEEP_DAYS, 1, 1);
    store.record(WIFI, today - KEEP_DAYS + 1, 2, 2);
    store.prune(today);
    assert_eq!(store.records.len(), 1);
    assert_eq!(store.records[0].day, today - KEEP_DAYS + 1);
}

#[test]
fn saving_and_loading_gives_back_the_same_data() {
    let folder = temp_folder("round_trip");
    let mut store = store_with_adapters();
    store.record(WIFI, day_from_date(2026, 11, 3), 123, 456);
    store.baseline = vec![Counter { luid: WIFI, up: 9, down: 8 }];
    store.saved_unix = 1_800_000_000;
    store.boot_unix = 1_799_000_000;
    store.save_to(&folder).unwrap();
    let mut loaded = UsageStore::load_from(&folder);
    loaded.is_dirty = store.is_dirty;
    assert_eq!(loaded, store);
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn damaged_file_is_set_aside_not_overwritten() {
    let folder = temp_folder("damaged");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("usage.bin"), b"this is not usage data").unwrap();
    assert_eq!(UsageStore::load_from(&folder), UsageStore::default());
    assert!(folder.join("usage.bin.damaged").exists());
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn cut_off_file_is_treated_as_damaged() {
    let mut store = store_with_adapters();
    store.record(WIFI, day_from_date(2026, 11, 3), 1, 2);
    let bytes = store.encode();
    assert!(UsageStore::decode(&bytes).is_some());
    assert!(UsageStore::decode(&bytes[..bytes.len() - 1]).is_none());
}

#[test]
fn record_pointing_at_a_missing_adapter_is_treated_as_damaged() {
    let mut store = store_with_adapters();
    store.record(WIFI, day_from_date(2026, 11, 3), 1, 2);
    store.records[0].adapter = 99;
    assert!(UsageStore::decode(&store.encode()).is_none());
}

#[test]
fn saved_baseline_counts_traffic_from_while_the_app_was_closed() {
    let mut tracker = DeltaTracker::with_baseline(vec![Counter { luid: WIFI, up: 100, down: 100 }]);
    let moved = tracker.update(&[Counter { luid: WIFI, up: 150, down: 700 }], 600.0);
    assert_eq!(moved, (50, 600));
}

#[test]
fn per_adapter_report_matches_the_totals() {
    let mut tracker = DeltaTracker::with_baseline(vec![
        Counter { luid: WIFI, up: 0, down: 0 },
        Counter { luid: LAN, up: 0, down: 0 },
    ]);
    let mut seen = Vec::new();
    let totals = tracker.update_each(
        &[Counter { luid: WIFI, up: 1, down: 2 }, Counter { luid: LAN, up: 0, down: 0 }],
        1.0,
        |luid, up, down| seen.push((luid, up, down)),
    );
    assert_eq!(totals, (1, 2));
    assert_eq!(seen, vec![(WIFI, 1, 2)]);
}

#[test]
fn data_amounts_read_naturally() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(999), "999 B");
    assert_eq!(format_bytes(512_000), "512 KB");
    assert_eq!(format_bytes(12_300_000), "12.3 MB");
    assert_eq!(format_bytes(4_820_000_000), "4.82 GB");
    assert_eq!(format_bytes(1_500_000_000_000), "1.50 TB");
}

#[test]
fn colours_blend_evenly() {
    assert_eq!(mix(0x000000, 0xFFFFFF, 0.0), 0x000000);
    assert_eq!(mix(0x000000, 0xFFFFFF, 1.0), 0xFFFFFF);
    assert_eq!(mix(0x000000, 0x0000FF, 0.5), 0x000080);
}

#[test]
fn catch_up_counts_the_gap_only_when_the_computer_has_not_restarted() {
    assert_eq!(catch_up_gap_secs(1_000, 1_100, 5_000, 5_600), Some(600.0));
    assert_eq!(catch_up_gap_secs(1_000, 1_120, 5_000, 5_600), Some(600.0));
    assert_eq!(catch_up_gap_secs(1_000, 1_121, 5_000, 5_600), None);
    assert_eq!(catch_up_gap_secs(1_000, 1_000, 5_600, 5_000), Some(0.0));
}
