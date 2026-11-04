//! The colours every NetSpeedGauge window is drawn with, in one place so the apps cannot drift apart.

// Below this total difference across red, green and blue, a colour is hard to see on its surface.
const MIN_COLOR_DISTANCE: u32 = 90;
const VISIBILITY_SHIFT: f32 = 0.35;
const BRIGHT_SURFACE: u32 = 128;

/// The colours for one theme.
#[derive(Clone, Copy)]
pub struct Theme {
    pub is_dark: bool,
    pub background: u32,
    pub card: u32,
    pub control: u32,
    pub selected: u32,
    // The control that picks one of several options, on a card row and directly on the page.
    pub segment_track_on_card: u32,
    pub segment_track_on_page: u32,
    pub segment_selected: u32,
    pub surface: u32,
    pub border: u32,
    pub footer: u32,
    pub button: u32,
    pub button_hover: u32,
    pub text: u32,
    pub muted: u32,
    pub download: u32,
    pub upload: u32,
}

/// Blends two 0xRRGGBB colours. An `amount` of 0 gives `from`, and 1 gives `to`.
pub fn mix(from: u32, to: u32, amount: f32) -> u32 {
    let channel = |shift: u32| {
        let start = ((from >> shift) & 0xFF) as f32;
        let end = ((to >> shift) & 0xFF) as f32;
        ((start + (end - start) * amount).round() as u32).min(255) << shift
    };
    channel(16) | channel(8) | channel(0)
}

/// Returns the dark colours built around an accent colour.
pub fn dark_palette(accent: u32) -> Theme {
    let download = mix(accent, 0xFFFFFF, 0.15);
    Theme {
        is_dark: true,
        background: 0x202020,
        card: 0x2B2B2B,
        control: 0x2B2B2B,
        selected: 0x454545,
        segment_track_on_card: 0x232323,
        segment_track_on_page: 0x2B2B2B,
        segment_selected: 0x454545,
        surface: 0x323232,
        border: 0x3A3A3A,
        footer: 0x171717,
        button: 0x3A3A3A,
        button_hover: 0x454545,
        text: 0xF5F5F5,
        muted: 0xA6A6A6,
        download,
        upload: mix(download, 0xFFFFFF, 0.55),
    }
}

/// Returns the light colours built around an accent colour.
pub fn light_palette(accent: u32) -> Theme {
    Theme {
        is_dark: false,
        // A small step between greys is far harder to see near white than near black, so these steps are bigger than dark's.
        background: 0xE8E8E8,
        card: 0xFFFFFF,
        control: 0xE2E2E2,
        selected: 0xFFFFFF,
        // The same grey as the plus and minus buttons, with no edge on the chosen option (his call 04 Nov 15:34).
        segment_track_on_card: 0xE2E2E2,
        segment_track_on_page: 0xE2E2E2,
        segment_selected: 0xFFFFFF,
        surface: 0xFFFFFF,
        border: 0xD4D4D4,
        footer: 0xDFDFDF,
        button: 0xE2E2E2,
        button_hover: 0xD6D6D6,
        text: 0x1B1B1B,
        muted: 0x5F5F5F,
        download: accent,
        upload: mix(accent, 0xFFFFFF, 0.42),
    }
}

/**
 * Returns the colour to draw an upload or download figure with: the user's choice, or the theme's
 * colour on automatic. A choice that would vanish on its surface, like white on white, is shifted
 * just enough to stay visible.
 */
pub fn figure_color(chosen: i64, automatic: u32, surface: u32) -> u32 {
    let Ok(color) = u32::try_from(chosen) else {
        return automatic;
    };
    let channel = |value: u32, shift: u32| (value >> shift) & 0xFF;
    let distance: u32 = [0, 8, 16].iter().map(|&shift| channel(color, shift).abs_diff(channel(surface, shift))).sum();
    if distance >= MIN_COLOR_DISTANCE {
        return color;
    }
    let is_bright = channel(surface, 8) >= BRIGHT_SURFACE;
    mix(color, if is_bright { 0x000000 } else { 0xFFFFFF }, VISIBILITY_SHIFT)
}
