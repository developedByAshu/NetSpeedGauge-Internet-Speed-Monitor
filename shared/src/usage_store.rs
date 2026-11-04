//! Keeps daily upload and download totals per adapter in one small file, with no database.

use crate::calendar::{month_start, week_start, year_start};
use crate::net_counters::Counter;
use std::path::{Path, PathBuf};

const FILE_MAGIC: &[u8; 4] = b"NSU1";
const FILE_NAME: &str = "usage.bin";
const TEMP_FILE_NAME: &str = "usage.bin.tmp";
const DAMAGED_FILE_NAME: &str = "usage.bin.damaged";
const APP_FOLDER: &str = "NetSpeedGauge";
// Two start times this close together mean the computer has not restarted in between.
const SAME_BOOT_TOLERANCE_SECS: i64 = 120;

// Usage history is kept for two years.
pub const KEEP_DAYS: i32 = 731;

/// The ways usage can be grouped in the usage window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Day,
    Week,
    Month,
    Year,
}

impl Period {
    /// Returns how many groups the usage chart shows for this period.
    pub fn bucket_count(self) -> usize {
        match self {
            Period::Day => 30,
            Period::Week | Period::Month => 12,
            Period::Year => 2,
        }
    }

    /// Returns the first day of the group holding this day.
    pub fn start_of(self, day: i32) -> i32 {
        match self {
            Period::Day => day,
            Period::Week => week_start(day),
            Period::Month => month_start(day),
            Period::Year => year_start(day),
        }
    }

    /// Returns the first day of the group just before the one starting on this day.
    fn previous_start(self, start: i32) -> i32 {
        match self {
            Period::Day => start - 1,
            Period::Week => start - 7,
            Period::Month | Period::Year => self.start_of(start - 1),
        }
    }
}

/// One adapter's totals for one day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DayRecord {
    pub day: i32,
    pub adapter: u32,
    pub up: u64,
    pub down: u64,
}

/// An adapter that has carried traffic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterName {
    pub luid: u64,
    pub name: String,
}

/// One bar in the usage chart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bucket {
    pub start: i32,
    pub up: u64,
    pub down: u64,
}

/// All saved usage, plus what is needed to count traffic that moved while the app was closed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsageStore {
    pub adapters: Vec<AdapterName>,
    pub records: Vec<DayRecord>,
    pub baseline: Vec<Counter>,
    pub saved_unix: i64,
    pub boot_unix: i64,
    pub is_dirty: bool,
    // How long history is kept. Comes from the user's settings, so it is not saved here. 0 means the maximum.
    pub keep_days: i32,
}

impl UsageStore {
    /// Returns true when this adapter already has a name on file.
    pub fn knows_adapter(&self, luid: u64) -> bool {
        self.adapters.iter().any(|adapter| adapter.luid == luid)
    }

    /// Records a new adapter's name, if it is new.
    pub fn add_adapter(&mut self, luid: u64, name: String) {
        if !self.knows_adapter(luid) {
            self.adapters.push(AdapterName { luid, name });
            self.is_dirty = true;
        }
    }

    /// Adds bytes to an adapter's total for the given day.
    pub fn record(&mut self, luid: u64, day: i32, up: u64, down: u64) {
        let Some(adapter) = self.adapters.iter().position(|known| known.luid == luid) else {
            return;
        };
        let adapter = adapter as u32;
        let today_rows = self.records.iter_mut().rev().take_while(|row| row.day >= day);
        match today_rows.into_iter().find(|row| row.day == day && row.adapter == adapter) {
            Some(row) => {
                row.up += up;
                row.down += down;
            }
            None => self.records.push(DayRecord { day, adapter, up, down }),
        }
        self.is_dirty = true;
    }

    /// Removes days older than the chosen history limit.
    pub fn prune(&mut self, today: i32) {
        let keep_days = if self.keep_days > 0 { self.keep_days } else { KEEP_DAYS };
        let before = self.records.len();
        self.records.retain(|row| row.day > today - keep_days);
        self.is_dirty |= self.records.len() != before;
    }

    /// Returns the chart's bars, oldest first, for one adapter or for all of them when None.
    pub fn buckets(&self, period: Period, adapter: Option<u32>, today: i32) -> Vec<Bucket> {
        let mut start = period.start_of(today);
        let mut buckets: Vec<Bucket> = (0..period.bucket_count())
            .map(|_| {
                let bucket = Bucket { start, up: 0, down: 0 };
                start = period.previous_start(start);
                bucket
            })
            .collect();
        buckets.reverse();
        for row in &self.records {
            if adapter.is_some_and(|wanted| wanted != row.adapter) {
                continue;
            }
            let row_start = period.start_of(row.day);
            if let Ok(position) = buckets.binary_search_by_key(&row_start, |bucket| bucket.start) {
                buckets[position].up += row.up;
                buckets[position].down += row.down;
            }
        }
        buckets
    }

    /// Returns the first day that has any usage.
    pub fn first_day(&self) -> Option<i32> {
        self.records.iter().map(|row| row.day).min()
    }

    /// Converts the store to the bytes saved on disk.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(32 + self.records.len() * 24);
        bytes.extend_from_slice(FILE_MAGIC);
        bytes.extend_from_slice(&(self.adapters.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(self.records.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(self.baseline.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.saved_unix.to_le_bytes());
        bytes.extend_from_slice(&self.boot_unix.to_le_bytes());
        for adapter in &self.adapters {
            let name: Vec<u16> = adapter.name.encode_utf16().collect();
            bytes.extend_from_slice(&adapter.luid.to_le_bytes());
            bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
            name.iter().for_each(|unit| bytes.extend_from_slice(&unit.to_le_bytes()));
        }
        for row in &self.records {
            bytes.extend_from_slice(&row.day.to_le_bytes());
            bytes.extend_from_slice(&row.adapter.to_le_bytes());
            bytes.extend_from_slice(&row.up.to_le_bytes());
            bytes.extend_from_slice(&row.down.to_le_bytes());
        }
        for counter in &self.baseline {
            bytes.extend_from_slice(&counter.luid.to_le_bytes());
            bytes.extend_from_slice(&counter.up.to_le_bytes());
            bytes.extend_from_slice(&counter.down.to_le_bytes());
        }
        bytes
    }

    /// Rebuilds a store from saved bytes. Returns None if they are damaged.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut reader = Reader { bytes, position: 0 };
        if reader.take(4)? != FILE_MAGIC {
            return None;
        }
        let adapter_count = reader.read_u32()?;
        let record_count = reader.read_u32()?;
        let baseline_count = reader.read_u32()?;
        let saved_unix = reader.read_i64()?;
        let boot_unix = reader.read_i64()?;
        let mut store = Self { saved_unix, boot_unix, ..Self::default() };
        for _ in 0..adapter_count {
            let luid = reader.read_u64()?;
            let length = u16::from_le_bytes(reader.take(2)?.try_into().ok()?) as usize;
            let raw = reader.take(length * 2)?;
            let units: Vec<u16> =
                raw.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
            store.adapters.push(AdapterName { luid, name: String::from_utf16_lossy(&units) });
        }
        for _ in 0..record_count {
            let day = reader.read_u32()? as i32;
            let adapter = reader.read_u32()?;
            let up = reader.read_u64()?;
            let down = reader.read_u64()?;
            if adapter as usize >= store.adapters.len() {
                return None;
            }
            store.records.push(DayRecord { day, adapter, up, down });
        }
        for _ in 0..baseline_count {
            let luid = reader.read_u64()?;
            let up = reader.read_u64()?;
            let down = reader.read_u64()?;
            store.baseline.push(Counter { luid, up, down });
        }
        (reader.position == bytes.len()).then_some(store)
    }

    /**
     * Writes the store safely: a temporary file first, then a swap, so a crash or power cut during
     * the save can never lose the existing history.
     */
    pub fn save_to(&self, folder: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(folder)?;
        let temp_path = folder.join(TEMP_FILE_NAME);
        std::fs::write(&temp_path, self.encode())?;
        std::fs::rename(&temp_path, folder.join(FILE_NAME))
    }

    /// Loads the store. A damaged file is set aside rather than overwritten, and counting starts fresh.
    pub fn load_from(folder: &Path) -> Self {
        let path = folder.join(FILE_NAME);
        let Ok(bytes) = std::fs::read(&path) else {
            return Self::default();
        };
        Self::decode(&bytes).unwrap_or_else(|| {
            let _ = std::fs::rename(&path, folder.join(DAMAGED_FILE_NAME));
            Self::default()
        })
    }
}

/// Returns the folder that holds NetSpeedGauge's data.
pub fn data_folder() -> PathBuf {
    data_base().join(APP_FOLDER)
}

#[cfg(not(target_os = "macos"))]
fn data_base() -> PathBuf {
    std::env::var_os("APPDATA").map_or_else(std::env::temp_dir, PathBuf::from)
}

#[cfg(target_os = "macos")]
fn data_base() -> PathBuf {
    std::env::var_os("HOME").map_or_else(std::env::temp_dir, |home| PathBuf::from(home).join("Library/Application Support"))
}

/// Returns how many seconds to catch up since the last save, or None when the computer has restarted since.
pub fn catch_up_gap_secs(saved_boot: i64, current_boot: i64, saved_unix: i64, now_unix: i64) -> Option<f64> {
    let is_same_boot = (saved_boot - current_boot).abs() <= SAME_BOOT_TOLERANCE_SECS;
    is_same_boot.then(|| (now_unix - saved_unix).max(0) as f64)
}

/// Reads values from saved bytes, stopping safely at the end of the data.
struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.position.checked_add(count)?;
        let slice = self.bytes.get(self.position..end)?;
        self.position = end;
        Some(slice)
    }

    fn read_u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn read_u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn read_i64(&mut self) -> Option<i64> {
        Some(i64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
}
