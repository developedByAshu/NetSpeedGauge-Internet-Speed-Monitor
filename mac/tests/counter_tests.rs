//! Tests for reading adapters, on made-up tables and on this Mac's real ones.
#![cfg(target_os = "macos")]

use netspeedgauge_mac::clock::{boot_time, today, unix_now};
use netspeedgauge_mac::net_counters::{adapter_id, is_physical, list_adapters, read_counters, Counter, InterfaceMode};
use netspeedgauge_mac::startup::agent_text;

#[test]
fn adapter_id_never_changes_between_runs() {
    assert_ne!(adapter_id("en0"), adapter_id("en1"));
    // Known FNV-1a value: history on disk depends on this exact number.
    assert_eq!(adapter_id("a"), 0xAF63_DC4C_8601_EC8C);
}

#[test]
fn physical_means_real_adapters_only() {
    for name in ["en0", "en12", "awdl0"] {
        assert!(is_physical(name), "{name}");
    }
    for name in ["lo0", "utun3", "ipsec0", "bridge0", "gif0", "stf0", "llw0", "anpi0", "ap1", "en", "enx", "awdl"] {
        assert!(!is_physical(name), "{name}");
    }
}

#[test]
fn real_table_lists_adapters_and_counts_nothing_twice() {
    let rows = list_adapters();
    assert!(rows.iter().any(|row| row.name.contains("Wi-Fi") || row.is_physical), "{rows:?}");
    let mut counters = Vec::new();
    assert!(read_counters(&InterfaceMode::AllPhysical, &mut counters));
    let mut ids: Vec<u64> = counters.iter().map(|counter| counter.luid).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), counters.len());
    assert!(!counters.iter().any(|counter| *counter == Counter { luid: adapter_id("lo0"), up: counter.up, down: counter.down }));
}

#[test]
fn main_only_counts_a_single_real_adapter_or_falls_back() {
    let mut main_only = Vec::new();
    let mut physical = Vec::new();
    assert!(read_counters(&InterfaceMode::Auto, &mut main_only));
    assert!(read_counters(&InterfaceMode::AllPhysical, &mut physical));
    assert!(main_only.len() == 1 || main_only.len() == physical.len());
    assert!(main_only.iter().all(|counter| physical.iter().any(|known| known.luid == counter.luid)));
}

#[test]
fn chosen_mode_counts_only_chosen_adapters() {
    let mut counters = Vec::new();
    assert!(read_counters(&InterfaceMode::Selected(vec![adapter_id("en0")]), &mut counters));
    assert!(counters.iter().all(|counter| counter.luid == adapter_id("en0")));
}

/// Returns what the system's own tool reports for an adapter, as (upload, download).
fn netstat_bytes(name: &str) -> Option<(u64, u64)> {
    let output = std::process::Command::new("netstat").arg("-ib").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let row = text.lines().find(|line| line.starts_with(name) && line.contains("<Link"))?;
    let parts: Vec<&str> = row.split_whitespace().collect();
    // Without an address column the bytes sit two places earlier, so count back from the end.
    let down = parts[parts.len() - 5].parse().ok()?;
    let up = parts[parts.len() - 2].parse().ok()?;
    Some((up, down))
}

/**
 * The table the Mac gives ordinary apps rounds these numbers and cuts them to 32 bits, which loses
 * usage at every 4 GB. Reading them from there again would fail this on any Mac past 4 GB of traffic.
 */
#[test]
fn counts_match_the_system_tool_exactly_enough() {
    const ALLOWED_DRIFT: u64 = 200_000_000;
    let mut checked = 0;
    for row in list_adapters().iter().filter(|row| row.is_physical && row.down > 0) {
        let name = ["en0", "en1", "en2", "en3", "en4", "en5", "awdl0"].into_iter().find(|name| adapter_id(name) == row.luid);
        let Some((up, down)) = name.and_then(netstat_bytes) else {
            continue;
        };
        assert!(row.up.abs_diff(up) < ALLOWED_DRIFT, "{row:?} against netstat upload {up}");
        assert!(row.down.abs_diff(down) < ALLOWED_DRIFT, "{row:?} against netstat download {down}");
        checked += 1;
    }
    assert!(checked > 0, "no active adapter to compare");
}

#[test]
fn clock_values_are_sane() {
    assert!(today() > 20_000);
    assert!(boot_time() > 1_500_000_000 && boot_time() <= unix_now());
}

#[test]
fn start_at_login_file_is_escaped() {
    let text = agent_text("/Apps/A&B <x>/NetSpeedGauge");
    assert!(text.contains("/Apps/A&amp;B &lt;x&gt;/NetSpeedGauge"));
    assert!(text.contains("<key>RunAtLoad</key><true/>"));
}
