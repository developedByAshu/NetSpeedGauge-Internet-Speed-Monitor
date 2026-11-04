//! Turns a speed in bytes per second into the text shown on the taskbar.

/// Bits or bytes, each counted in thousands (decimal) or 1024s (binary).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    BitsDecimal,
    BitsBinary,
    BytesDecimal,
    BytesBinary,
}

/// Whether the value always shows in mega units or moves between kilo, mega and giga.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Auto,
    AlwaysMega,
}

/// The display options that affect the speed text.
#[derive(Clone, Copy, Debug)]
pub struct SpeedFormat {
    pub unit: Unit,
    pub scale: Scale,
    pub decimals: usize,
    pub hide_suffix: bool,
    pub short_labels: bool,
}

impl Default for SpeedFormat {
    fn default() -> Self {
        Self {
            unit: Unit::BytesDecimal,
            scale: Scale::AlwaysMega,
            decimals: 1,
            hide_suffix: false,
            short_labels: false,
        }
    }
}

/** Returns the bits-per-byte multiplier, the size of one step, and the kilo, mega and giga labels. */
fn unit_table(unit: Unit) -> (f64, f64, [&'static str; 3]) {
    match unit {
        Unit::BitsDecimal => (8.0, 1000.0, ["Kbps", "Mbps", "Gbps"]),
        Unit::BitsBinary => (8.0, 1024.0, ["Kibps", "Mibps", "Gibps"]),
        Unit::BytesDecimal => (1.0, 1000.0, ["KBps", "MBps", "GBps"]),
        Unit::BytesBinary => (1.0, 1024.0, ["KiBps", "MiBps", "GiBps"]),
    }
}

/// Picks 0 for kilo, 1 for mega or 2 for giga. Mega still moves up to giga once it would need four digits.
fn scale_index(value: f64, step: f64, scale: Scale) -> usize {
    match scale {
        Scale::AlwaysMega if value < step * step * step => 1,
        Scale::Auto if value < step * step => 0,
        Scale::Auto if value < step * step * step => 1,
        _ => 2,
    }
}

/// Returns the widest text the window is sized for, so its width never changes with speed.
pub fn widest_speed_text(format: &SpeedFormat) -> String {
    let (multiplier, step, _) = unit_table(format.unit);
    let widest = SpeedFormat { scale: Scale::AlwaysMega, ..*format };
    format_speed(999.0 * step * step / multiplier, &widest)
}

/// Formats an amount of data for the usage window, such as "512 KB" or "4.82 GB".
pub fn format_bytes(bytes: u64) -> String {
    const STEPS: [(f64, &str, usize); 4] =
        [(1e12, "TB", 2), (1e9, "GB", 2), (1e6, "MB", 1), (1e3, "KB", 0)];
    let value = bytes as f64;
    match STEPS.iter().find(|(size, _, _)| value >= *size) {
        Some((size, label, decimals)) => format!("{:.*} {label}", *decimals, value / size),
        None => format!("{bytes} B"),
    }
}

/// Formats a speed in bytes per second as display text.
pub fn format_speed(bytes_per_sec: f64, format: &SpeedFormat) -> String {
    let (multiplier, step, labels) = unit_table(format.unit);
    let value = bytes_per_sec.max(0.0) * multiplier;
    let index = scale_index(value, step, format.scale);
    let scaled = value / step.powi(index as i32 + 1);
    let number = format!("{:.*}", format.decimals, scaled);
    if format.hide_suffix {
        return number;
    }
    if format.short_labels {
        return format!("{number} {}", &labels[index][..1]);
    }
    format!("{number} {}", labels[index])
}
