//! Adapter counters and how many bytes moved between two readings.

// Fastest speed treated as real: 100 Gbps, in bytes per second. Anything above it is a counter glitch.
const MAX_BYTES_PER_SEC: f64 = 12_500_000_000.0;

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
