//! Draws upload and download side by side, for the menu bar and the Settings preview.

use crate::canvas::{self, Align, Font, WEIGHT_BOLD, WEIGHT_HEAVY, WEIGHT_REGULAR, WEIGHT_SEMIBOLD};
use netspeedgauge_shared::format::{format_speed, widest_speed_text, SpeedFormat, Unit};
use netspeedgauge_shared::settings::{speed_color, Field, Settings};

const SIDE_PADDING: i32 = 4;
const ARROW_GAP: i32 = 3;
const PAIR_GAP: i32 = 8;
// Icon thickness 1 to 4. The system font's arrows do get heavier, so no multiple drawing is needed here.
const ARROW_WEIGHTS: [f64; 4] = [WEIGHT_REGULAR, WEIGHT_SEMIBOLD, WEIGHT_BOLD, WEIGHT_HEAVY];

/// The colours for one drawing of the readout, as 0xRRGGBB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadoutColors {
    pub up_arrow: u32,
    pub down_arrow: u32,
    pub up_value: u32,
    pub down_value: u32,
}

/// Works out every colour for the readout from the settings and the current speeds.
pub fn readout_colors(settings: &Settings, automatic: u32, up_speed: f64, down_speed: f64) -> ReadoutColors {
    let text = settings.color_or(Field::TextColor, automatic);
    let up_value = speed_color(settings, up_speed, text);
    let down_value = speed_color(settings, down_speed, text);
    ReadoutColors {
        up_arrow: settings.color_or(Field::UpColor, up_value),
        down_arrow: settings.color_or(Field::DownColor, down_value),
        up_value,
        down_value,
    }
}

/// Returns the fixed width: its size never changes with speed, so the icons beside it never jump.
pub fn total_width(arrow_width: i32, first_value_width: i32, last_value_width: i32) -> i32 {
    let gap = if arrow_width > 0 { ARROW_GAP } else { 0 };
    SIDE_PADDING * 2 + (arrow_width + gap) * 2 + first_value_width + last_value_width + PAIR_GAP
}

/// Returns how many bytes per second make one of a unit whose label starts with the given letter.
fn bytes_per_label(unit: Unit, label: &str) -> f64 {
    let (bits_per_byte, step) = match unit {
        Unit::BitsDecimal => (8.0, 1000.0_f64),
        Unit::BitsBinary => (8.0, 1024.0),
        Unit::BytesDecimal => (1.0, 1000.0),
        Unit::BytesBinary => (1.0, 1024.0),
    };
    let power = match label.chars().next() {
        Some('K') => 1,
        Some('G') => 3,
        _ => 2,
    };
    step.powi(power) / bits_per_byte
}

/**
 * Returns the (upload, download) text with the unit shown once, on whichever is drawn last, to save
 * space beside the notch. Both use the faster speed's unit, so one label is true for both.
 */
pub fn pair_texts(up_speed: f64, down_speed: f64, format: &SpeedFormat, is_swapped: bool) -> (String, String) {
    let labelled = format_speed(up_speed.max(down_speed), &SpeedFormat { hide_suffix: false, ..*format });
    let label = labelled.rsplit_once(' ').map_or("", |(_, label)| label);
    let divisor = bytes_per_label(format.unit, label);
    let number = |speed: f64| format!("{:.*}", format.decimals, speed.max(0.0) / divisor);
    let with_label = |speed: f64| if format.hide_suffix { number(speed) } else { format!("{} {label}", number(speed)) };
    if is_swapped { (with_label(up_speed), number(down_speed)) } else { (number(up_speed), with_label(down_speed)) }
}

/**
 * Returns where each pair's arrow and number start, as (arrow x, number x). Each number sits right
 * next to its arrow and the pairs stay close, ending at `right` so the right edge never moves.
 */
pub fn pair_positions(right: i32, arrow_width: i32, value_widths: [i32; 2]) -> [(i32, i32); 2] {
    let gap = if arrow_width > 0 { ARROW_GAP } else { 0 };
    let second_arrow = right - SIDE_PADDING - value_widths[1] - gap - arrow_width;
    let first_arrow = second_arrow - PAIR_GAP - value_widths[0] - gap - arrow_width;
    [(first_arrow, first_arrow + arrow_width + gap), (second_arrow, second_arrow + arrow_width + gap)]
}

/// The fonts and column sizes for the current settings.
pub struct Readout {
    value_font: Font,
    arrow_font: Font,
    arrows: (&'static str, &'static str),
    arrow_width: i32,
    is_swapped: bool,
    pub width: i32,
}

impl Readout {
    pub fn new(settings: &Settings) -> Self {
        let points = settings.get(Field::FontPoints) as f64;
        let value_font = Font::family(settings.font_family(), points, WEIGHT_REGULAR);
        let thickness = (settings.get(Field::IconThickness) as usize).clamp(1, ARROW_WEIGHTS.len());
        let arrow_font = Font::system(points, ARROW_WEIGHTS[thickness - 1]);
        let arrows = settings.arrow_symbols();
        let arrow_width = canvas::text_width(arrows.0, &arrow_font).max(canvas::text_width(arrows.1, &arrow_font));
        let widest = widest_speed_text(&settings.speed_format());
        let widest_number = widest.split(' ').next().unwrap_or(&widest);
        let first_value_width = canvas::text_width(widest_number, &value_font);
        let last_value_width = canvas::text_width(&widest, &value_font);
        Self {
            width: total_width(arrow_width, first_value_width, last_value_width),
            value_font,
            arrow_font,
            arrows,
            arrow_width,
            is_swapped: settings.is_on(Field::SwapRows),
        }
    }

    /// Draws both speeds in the box starting at `left`, centred within `height`.
    pub fn draw(&self, left: i32, top: i32, height: i32, up_text: &str, down_text: &str, colors: &ReadoutColors) {
        let up = (self.arrows.0, up_text, colors.up_arrow, colors.up_value);
        let down = (self.arrows.1, down_text, colors.down_arrow, colors.down_value);
        let pairs = if self.is_swapped { [down, up] } else { [up, down] };
        let y = top + (height - self.value_font.box_height) / 2;
        let value_widths = pairs.map(|pair| canvas::text_width(pair.1, &self.value_font));
        let positions = pair_positions(left + self.width, self.arrow_width, value_widths);
        for ((arrow, value, arrow_color, value_color), (arrow_x, value_x)) in pairs.into_iter().zip(positions) {
            canvas::text(arrow_x, y, arrow, &self.arrow_font, arrow_color, Align::Left);
            canvas::text(value_x, y, value, &self.value_font, value_color, Align::Left);
        }
    }
}
