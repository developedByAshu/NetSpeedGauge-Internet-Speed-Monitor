//! Reads the Mac's byte counters per adapter. Only local tables are read, nothing is sent.

pub use netspeedgauge_shared::net_counters::{Counter, DeltaTracker, InterfaceMode};
use std::cell::RefCell;

const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;
// Not in the libc crate. Marks a route tied to one adapter, which is never the Mac's main route.
const RTF_IFSCOPE: i32 = 0x0100_0000;
// Wi-Fi and wired adapters, and AirDrop's direct link. Tunnels and VPNs resend bytes these already carry.
const PHYSICAL_PREFIXES: [&str; 2] = ["en", "awdl"];

/// One adapter as the Mac lists it, for the adapter picker and diagnostics.
#[derive(Clone, Debug)]
pub struct AdapterRow {
    pub luid: u64,
    pub index: u16,
    pub name: String,
    pub up: u64,
    pub down: u64,
    pub is_physical: bool,
}

thread_local! {
    // Reused for every reading, so the meter does not allocate on each tick.
    static TABLE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/**
 * Returns the number saved for an adapter name. It must never change between runs because history
 * is stored under it, so it is a fixed hash rather than Rust's default one.
 */
pub fn adapter_id(name: &str) -> u64 {
    name.bytes().fold(FNV_OFFSET, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME))
}

/// Returns true for an adapter that carries real traffic exactly once.
pub fn is_physical(name: &str) -> bool {
    PHYSICAL_PREFIXES.iter().any(|prefix| {
        name.strip_prefix(prefix).is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()))
    })
}

/// Reads a whole system table into `buffer`.
fn read_table(request: &mut [libc::c_int; 6], buffer: &mut Vec<u8>) -> bool {
    let mut size = 0usize;
    // SAFETY: a null output pointer asks only for the size, and `request` and `size` are valid.
    let sized = unsafe { libc::sysctl(request.as_mut_ptr(), 6, std::ptr::null_mut(), &mut size, std::ptr::null_mut(), 0) };
    if sized != 0 {
        return false;
    }
    buffer.resize(size, 0);
    // SAFETY: `buffer` holds `size` bytes, and sysctl writes back how many it used.
    let status = unsafe { libc::sysctl(request.as_mut_ptr(), 6, buffer.as_mut_ptr().cast(), &mut size, std::ptr::null_mut(), 0) };
    buffer.truncate(if status == 0 { size } else { 0 });
    status == 0
}

/**
 * Returns an adapter's exact upload and download bytes. The adapter table and getifaddrs both round
 * these and cut them to 32 bits for ordinary apps, which would lose usage at every 4 GB, so the
 * table is only used for names. `netstat -ib` reports the same numbers as this call.
 */
pub fn adapter_bytes(index: u16) -> Option<(u64, u64)> {
    let mut request = [libc::CTL_NET, libc::PF_LINK, libc::NETLINK_GENERIC, libc::IFMIB_IFDATA, libc::c_int::from(index), libc::IFDATA_GENERAL];
    let mut size = std::mem::size_of::<libc::ifmibdata>();
    // SAFETY: an all-zero ifmibdata is valid, and `size` matches the buffer passed.
    let (status, data) = unsafe {
        let mut data: libc::ifmibdata = std::mem::zeroed();
        let status = libc::sysctl(request.as_mut_ptr(), 6, std::ptr::from_mut(&mut data).cast(), &mut size, std::ptr::null_mut(), 0);
        (status, data)
    };
    (status == 0).then_some((data.ifmd_data.ifi_obytes, data.ifmd_data.ifi_ibytes))
}

/// Calls `visit` with each adapter's index and name found in a table.
pub fn parse_adapter_table(table: &[u8], mut visit: impl FnMut(u16, &str)) {
    let header_size = std::mem::size_of::<libc::if_msghdr2>();
    let mut offset = 0;
    while offset + std::mem::size_of::<libc::if_msghdr>() <= table.len() {
        // SAFETY: the bytes are in bounds, and read_unaligned accepts any address.
        let basic = unsafe { table.as_ptr().add(offset).cast::<libc::if_msghdr>().read_unaligned() };
        let length = usize::from(basic.ifm_msglen);
        if length == 0 || offset + length > table.len() {
            return;
        }
        if i32::from(basic.ifm_type) == libc::RTM_IFINFO2 && length >= header_size + 8 {
            // SAFETY: `length` covers the whole header, checked above.
            let row = unsafe { table.as_ptr().add(offset).cast::<libc::if_msghdr2>().read_unaligned() };
            // The adapter's link address follows the header: its name length is at byte 5, and the name starts at byte 8.
            let link = &table[offset + header_size..offset + length];
            let end = (8 + usize::from(link[5])).min(link.len());
            if let Ok(name) = std::str::from_utf8(&link[8..end]) {
                visit(row.ifm_index, name);
            }
        }
        offset += length;
    }
}

/// Runs `action` on the fresh adapter table.
fn with_adapter_table(action: impl FnOnce(&[u8])) -> bool {
    let mut request = [libc::CTL_NET, libc::PF_ROUTE, 0, 0, libc::NET_RT_IFLIST2, 0];
    TABLE.with(|cell| {
        let mut buffer = cell.borrow_mut();
        let is_read = read_table(&mut request, &mut buffer);
        if is_read {
            action(&buffer);
        }
        is_read
    })
}

/// Returns the adapter index that the Mac's main IPv4 route uses.
fn main_route_index() -> Option<u16> {
    let mut request = [libc::CTL_NET, libc::PF_ROUTE, 0, libc::AF_INET, libc::NET_RT_DUMP, 0];
    let mut found = None;
    TABLE.with(|cell| {
        let mut buffer = cell.borrow_mut();
        if read_table(&mut request, &mut buffer) {
            found = parse_main_route(&buffer);
        }
    });
    found
}

/// Finds the route for every address in a route table: destination and mask both empty.
pub fn parse_main_route(table: &[u8]) -> Option<u16> {
    let header_size = std::mem::size_of::<libc::rt_msghdr>();
    let mut offset = 0;
    while offset + header_size <= table.len() {
        // SAFETY: the bytes are in bounds, and read_unaligned accepts any address.
        let row = unsafe { table.as_ptr().add(offset).cast::<libc::rt_msghdr>().read_unaligned() };
        let length = usize::from(row.rtm_msglen);
        if length == 0 || offset + length > table.len() {
            return None;
        }
        let is_usable = row.rtm_flags & libc::RTF_UP != 0 && row.rtm_flags & (RTF_IFSCOPE | libc::RTF_HOST) == 0;
        if is_usable && is_everything_route(&table[offset + header_size..offset + length], row.rtm_addrs) {
            return Some(row.rtm_index);
        }
        offset += length;
    }
    None
}

/// Returns true when a route's addresses mean 0.0.0.0/0. Each address starts with its length and is padded to 4 bytes.
fn is_everything_route(addresses: &[u8], present: i32) -> bool {
    let mut offset = 0;
    let mut destination_empty = false;
    let mut mask_empty = false;
    for bit in 0..8 {
        if present & (1 << bit) == 0 {
            continue;
        }
        let Some(&length) = addresses.get(offset) else {
            return false;
        };
        let size = usize::from(length);
        let value = addresses.get(offset..offset + size).unwrap_or(&[]);
        // Bytes 0 and 1 are the length and family, 2 and 3 the port, then the address itself.
        let is_empty = value.iter().skip(4).all(|&byte| byte == 0);
        if bit == libc::RTAX_DST {
            destination_empty = is_empty;
        }
        if bit == libc::RTAX_NETMASK {
            mask_empty = is_empty;
        }
        offset += if size == 0 { 4 } else { size.next_multiple_of(4) };
    }
    destination_empty && mask_empty
}

/**
 * Fills `out` with counters for the adapters the mode includes, reusing its memory.
 * "Main only" counts the adapter behind the main route. When a VPN owns that route it counts the
 * real adapters instead, because the tunnel's bytes also pass through them.
 */
pub fn read_counters(mode: &InterfaceMode, out: &mut Vec<Counter>) -> bool {
    out.clear();
    let main_index = matches!(mode, InterfaceMode::Auto).then(main_route_index).flatten();
    let mut main_counter = None;
    let is_read = with_adapter_table(|table| {
        parse_adapter_table(table, |index, name| {
            let luid = adapter_id(name);
            let physical = is_physical(name);
            let is_counted = match mode {
                InterfaceMode::Auto | InterfaceMode::AllPhysical => physical,
                InterfaceMode::Selected(luids) => luids.contains(&luid),
            };
            if !is_counted {
                return;
            }
            if let Some((up, down)) = adapter_bytes(index) {
                let counter = Counter { luid, up, down };
                if physical && Some(index) == main_index {
                    main_counter = Some(counter);
                }
                out.push(counter);
            }
        });
    });
    if let Some(counter) = main_counter {
        out.clear();
        out.push(counter);
    }
    is_read
}

/// Lists every adapter the Mac reports.
pub fn list_adapters() -> Vec<AdapterRow> {
    let mut rows = Vec::new();
    with_adapter_table(|table| {
        parse_adapter_table(table, |index, name| {
            let (up, down) = adapter_bytes(index).unwrap_or((0, 0));
            rows.push(AdapterRow {
                luid: adapter_id(name),
                index,
                name: friendly_name(name),
                up,
                down,
                is_physical: is_physical(name),
            });
        });
    });
    rows
}

#[link(name = "SystemConfiguration", kind = "framework")]
extern "C" {
    fn SCNetworkInterfaceCopyAll() -> *const std::ffi::c_void;
    fn SCNetworkInterfaceGetBSDName(interface: *const std::ffi::c_void) -> *const std::ffi::c_void;
    fn SCNetworkInterfaceGetLocalizedDisplayName(interface: *const std::ffi::c_void) -> *const std::ffi::c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFArrayGetCount(array: *const std::ffi::c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const std::ffi::c_void, index: isize) -> *const std::ffi::c_void;
    fn CFStringGetCString(text: *const std::ffi::c_void, buffer: *mut libc::c_char, size: isize, encoding: u32) -> u8;
    fn CFRelease(object: *const std::ffi::c_void);
}

const UTF8_ENCODING: u32 = 0x0800_0100;
const NAME_BUFFER_SIZE: usize = 256;

/// Copies a system text value to a String.
fn system_text(text: *const std::ffi::c_void) -> Option<String> {
    if text.is_null() {
        return None;
    }
    let mut buffer = [0 as libc::c_char; NAME_BUFFER_SIZE];
    // SAFETY: `text` is a live string, and the buffer's real size is passed.
    let is_copied = unsafe { CFStringGetCString(text, buffer.as_mut_ptr(), NAME_BUFFER_SIZE as isize, UTF8_ENCODING) } != 0;
    // SAFETY: on success the buffer holds a null-terminated string.
    is_copied.then(|| unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy().into_owned())
}

/// Returns the name System Settings shows for an adapter, such as "Wi-Fi", or its system name if it has none.
pub fn friendly_name(system_name: &str) -> String {
    // SAFETY: the array is owned here and released once. Values read from it are not kept past the release.
    unsafe {
        let all = SCNetworkInterfaceCopyAll();
        if all.is_null() {
            return system_name.to_string();
        }
        let found = (0..CFArrayGetCount(all)).find_map(|position| {
            let interface = CFArrayGetValueAtIndex(all, position);
            (system_text(SCNetworkInterfaceGetBSDName(interface)).as_deref() == Some(system_name))
                .then(|| system_text(SCNetworkInterfaceGetLocalizedDisplayName(interface)))
                .flatten()
        });
        CFRelease(all);
        found.unwrap_or_else(|| system_name.to_string())
    }
}
