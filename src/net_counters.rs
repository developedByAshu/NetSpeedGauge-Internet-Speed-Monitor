//! Reads Windows' byte counters per adapter and works out how many bytes moved.

use windows_sys::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetBestInterface, GetIfTable2, MIB_IF_ROW2, MIB_IF_TABLE2,
};

// Fastest speed treated as real: 100 Gbps, in bytes per second. Anything above it is a counter glitch.
const MAX_BYTES_PER_SEC: f64 = 12_500_000_000.0;

// 8.8.8.8 as Windows expects it. Only used for a local route lookup, nothing is sent.
const ROUTE_PROBE_ADDRESS: u32 = u32::from_ne_bytes([8, 8, 8, 8]);

const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;
const IF_TYPE_PPP: u32 = 23;
const IF_TYPE_TUNNEL: u32 = 131;

const FLAG_HARDWARE_INTERFACE: u8 = 1 << 0;
const FLAG_FILTER_INTERFACE: u8 = 1 << 1;
const FLAG_CONNECTOR_PRESENT: u8 = 1 << 2;

// Direct device links that Windows reports as non-hardware but that carry real traffic.
const PEER_ADAPTER_NAMES: [&str; 2] = ["Wi-Fi Direct", "Bluetooth"];

/// Which adapters count towards the speed and usage totals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InterfaceMode {
    Auto,
    AllPhysical,
    Selected(Vec<u64>),
}

/// One adapter's lifetime byte counters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counter {
    pub luid: u64,
    pub up: u64,
    pub down: u64,
}

/// One adapter as Windows lists it, for the adapter picker and diagnostics.
#[derive(Clone, Debug)]
pub struct AdapterRow {
    pub luid: u64,
    pub index: u32,
    pub if_type: u32,
    pub flags: u8,
    pub alias: String,
    pub description: String,
    pub up: u64,
    pub down: u64,
    pub is_physical: bool,
}

/// Returns true when a Windows text buffer contains the given ASCII text.
fn wide_contains(buffer: &[u16], text: &str) -> bool {
    buffer
        .windows(text.len())
        .any(|window| window.iter().zip(text.bytes()).all(|(&unit, byte)| unit == u16::from(byte)))
}

/**
 * Returns true for an adapter that carries real traffic exactly once.
 * Filter layers repeat the real adapter's counters and VPN tunnels resend bytes the real adapter
 * also carries, so both are left out. Windows does not mark Wi-Fi Direct or Bluetooth as hardware,
 * so those are recognised by name.
 */
pub fn is_physical(if_type: u32, flags: u8, description: &[u16]) -> bool {
    let is_filter = flags & FLAG_FILTER_INTERFACE != 0;
    let is_tunnel_type = matches!(if_type, IF_TYPE_SOFTWARE_LOOPBACK | IF_TYPE_PPP | IF_TYPE_TUNNEL);
    if is_filter || is_tunnel_type {
        return false;
    }
    let is_hardware = flags & FLAG_HARDWARE_INTERFACE != 0;
    let has_connector = flags & FLAG_CONNECTOR_PRESENT != 0;
    (is_hardware && has_connector)
        || PEER_ADAPTER_NAMES.iter().any(|name| wide_contains(description, name))
}

/// Calls `visit` for each adapter row, then frees the table Windows allocated.
fn for_each_row(mut visit: impl FnMut(&MIB_IF_ROW2)) -> bool {
    let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
    // SAFETY: GetIfTable2 fills `table` on success, and FreeMibTable releases it exactly once.
    unsafe {
        if GetIfTable2(&mut table) != 0 {
            return false;
        }
        let count = (*table).NumEntries as usize;
        let first = std::ptr::addr_of!((*table).Table).cast::<MIB_IF_ROW2>();
        for position in 0..count {
            visit(&*first.add(position));
        }
        FreeMibTable(table.cast());
    }
    true
}

/// Returns the index of the adapter Windows would route internet traffic through.
fn best_interface_index() -> Option<u32> {
    let mut index = 0u32;
    // SAFETY: `index` is a valid out pointer for the duration of the call.
    let status = unsafe { GetBestInterface(ROUTE_PROBE_ADDRESS, &mut index) };
    (status == 0).then_some(index)
}

/// Fills `out` with counters for the adapters the mode includes, reusing its memory.
pub fn read_counters(mode: &InterfaceMode, out: &mut Vec<Counter>) -> bool {
    out.clear();
    let best_index = matches!(mode, InterfaceMode::Auto).then(best_interface_index).flatten();
    for_each_row(|row| {
        // SAFETY: every bit pattern of the LUID union is a valid u64.
        let luid = unsafe { row.InterfaceLuid.Value };
        let is_counted = match mode {
            InterfaceMode::Auto => Some(row.InterfaceIndex) == best_index,
            InterfaceMode::AllPhysical => {
                is_physical(row.Type, row.InterfaceAndOperStatusFlags._bitfield, &row.Description)
            }
            InterfaceMode::Selected(luids) => luids.contains(&luid),
        };
        if is_counted {
            out.push(Counter { luid, up: row.OutOctets, down: row.InOctets });
        }
    })
}

/// Converts a fixed Windows text buffer to a String.
fn wide_to_string(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&unit| unit == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// Lists every adapter Windows reports.
pub fn list_adapters() -> Vec<AdapterRow> {
    let mut rows = Vec::new();
    for_each_row(|row| {
        let flags = row.InterfaceAndOperStatusFlags._bitfield;
        rows.push(AdapterRow {
            // SAFETY: every bit pattern of the LUID union is a valid u64.
            luid: unsafe { row.InterfaceLuid.Value },
            index: row.InterfaceIndex,
            if_type: row.Type,
            flags,
            alias: wide_to_string(&row.Alias),
            description: wide_to_string(&row.Description),
            up: row.OutOctets,
            down: row.InOctets,
            is_physical: is_physical(row.Type, flags, &row.Description),
        });
    });
    rows
}

/// Remembers the previous counters and reports the bytes moved since the last reading.
#[derive(Default)]
pub struct DeltaTracker {
    previous: Vec<Counter>,
}

impl DeltaTracker {
    /**
     * Returns the upload and download bytes moved since the last call.
     * Counters keep running through sleep, so a long gap still counts the real bytes.
     * A counter that went backwards was reset and counts 0. A newly seen adapter only sets its baseline.
     */
    pub fn update(&mut self, current: &[Counter], elapsed_secs: f64) -> (u64, u64) {
        self.update_each(current, elapsed_secs, |_, _, _| {})
    }

    /// Does the same as `update`, also reporting each adapter's bytes to `on_moved(luid, up, down)`.
    pub fn update_each(
        &mut self,
        current: &[Counter],
        elapsed_secs: f64,
        mut on_moved: impl FnMut(u64, u64, u64),
    ) -> (u64, u64) {
        let cap = (MAX_BYTES_PER_SEC * elapsed_secs.max(0.0)) as u64;
        let mut total_up = 0u64;
        let mut total_down = 0u64;
        for counter in current {
            match self.previous.iter_mut().find(|known| known.luid == counter.luid) {
                Some(known) => {
                    let up = counter.up.saturating_sub(known.up).min(cap);
                    let down = counter.down.saturating_sub(known.down).min(cap);
                    if up > 0 || down > 0 {
                        on_moved(counter.luid, up, down);
                    }
                    total_up += up;
                    total_down += down;
                    *known = *counter;
                }
                None => self.previous.push(*counter),
            }
        }
        (total_up, total_down)
    }

    /// Starts from counters saved earlier, so traffic since then is still counted.
    pub fn with_baseline(baseline: Vec<Counter>) -> Self {
        Self { previous: baseline }
    }
}
