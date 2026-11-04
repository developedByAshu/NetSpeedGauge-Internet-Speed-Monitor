//! The theme choice, title bar look and the footer line shared by every NetSpeedGauge window.

use crate::canvas::{self, Align, Font};
use crate::surface::main_thread;
use netspeedgauge_shared::palette::{dark_palette, figure_color, light_palette};
pub use netspeedgauge_shared::palette::Theme;
use objc2::rc::Retained;
use objc2_app_kit::{NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication, NSColor, NSColorSpace, NSWindow};
use std::sync::atomic::{AtomicI64, Ordering};

const FALLBACK_ACCENT: u32 = 0x4C9AFF;
const MADE_WITH_LOVE: &str = "Made in India with ";
const ICON_HEART: &str = "heart.fill";
const HEART_COLOR: u32 = 0xE5484D;
const HEART_POINTS: f64 = 8.0;
const HEART_BOX: i32 = 14;

/// The height of the strip along the bottom of the usage window.
pub const FOOTER_HEIGHT: i32 = 24;

static THEME_CHOICE: AtomicI64 = AtomicI64::new(0);
static UP_ICON_COLOR: AtomicI64 = AtomicI64::new(-1);
static DOWN_ICON_COLOR: AtomicI64 = AtomicI64::new(-1);

/// Returns the Mac's accent colour as 0xRRGGBB, or a calm blue if it would be hard to see.
fn accent_color() -> u32 {
    let Some(accent) = NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace()) else {
        return FALLBACK_ACCENT;
    };
    let channel = |value: f64| (value * 255.0).round() as u32;
    let (red, green, blue) = (channel(accent.redComponent()), channel(accent.greenComponent()), channel(accent.blueComponent()));
    let brightest = red.max(green).max(blue);
    let dullest = red.min(green).min(blue);
    if brightest < 90 || brightest - dullest < 40 {
        return FALLBACK_ACCENT;
    }
    (red << 16) | (green << 8) | blue
}

pub fn dark_theme() -> Theme {
    dark_palette(accent_color())
}

pub fn light_theme() -> Theme {
    light_palette(accent_color())
}

/// Sets the user's theme choice: 0 follows the Mac, 1 is light, 2 is dark.
pub fn set_theme_choice(choice: i64) {
    THEME_CHOICE.store(choice, Ordering::Relaxed);
}

/// Remembers the user's upload and download icon colours. A negative value means automatic.
pub fn set_icon_colors(up: i64, down: i64) {
    UP_ICON_COLOR.store(up, Ordering::Relaxed);
    DOWN_ICON_COLOR.store(down, Ordering::Relaxed);
}

/// Returns the (upload, download) colours for figures drawn on a surface.
pub fn figure_colors(theme: &Theme, surface: u32) -> (u32, u32) {
    (
        figure_color(UP_ICON_COLOR.load(Ordering::Relaxed), theme.upload, surface),
        figure_color(DOWN_ICON_COLOR.load(Ordering::Relaxed), theme.download, surface),
    )
}

/// Returns true when an appearance is one of the Mac's dark ones.
pub fn is_dark_appearance(appearance: &NSAppearance) -> bool {
    appearance.name().to_string().contains("Dark")
}

/// Returns the colours for the user's theme choice.
pub fn current_theme() -> Theme {
    match THEME_CHOICE.load(Ordering::Relaxed) {
        1 => light_theme(),
        2 => dark_theme(),
        _ => {
            if is_dark_appearance(&NSApplication::sharedApplication(main_thread()).effectiveAppearance()) {
                dark_theme()
            } else {
                light_theme()
            }
        }
    }
}

/// Draws the "Made in India with love" line, centred.
pub fn draw_made_with_love(center_x: i32, y: i32, font: &Font, color: u32) {
    let text_width = canvas::text_width(MADE_WITH_LOVE, font);
    let left = center_x - (text_width + HEART_BOX) / 2;
    canvas::text(left, y, MADE_WITH_LOVE, font, color, Align::Left);
    canvas::symbol(left + text_width + HEART_BOX / 2, y + font.box_height / 2, ICON_HEART, HEART_POINTS, HEART_COLOR);
}

/// Draws a full-width strip along the bottom of a window that has no sidebar to hold the line.
pub fn draw_footer_strip(theme: &Theme, top: i32, width: i32, height: i32, font: &Font) {
    canvas::round_rect(0, top, width, height, 0.0, theme.footer);
    canvas::round_rect(0, top, width, 1, 0.0, theme.border);
    draw_made_with_love(width / 2, top + (height - font.box_height) / 2, font, theme.muted);
}

/// Returns the Mac's own light or dark look for a theme.
pub fn appearance_for(theme: &Theme) -> Option<Retained<NSAppearance>> {
    // SAFETY: these are AppKit's own constants.
    let name = unsafe { if theme.is_dark { NSAppearanceNameDarkAqua } else { NSAppearanceNameAqua } };
    NSAppearance::appearanceNamed(name)
}

/// Makes a title bar match its window, so the two read as one surface.
pub fn apply_title_bar(window: &NSWindow, theme: &Theme) {
    window.setAppearance(appearance_for(theme).as_deref());
    window.setBackgroundColor(Some(&canvas::color(theme.background)));
    window.setTitlebarAppearsTransparent(true);
}
