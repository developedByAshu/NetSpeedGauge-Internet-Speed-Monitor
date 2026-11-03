//! The colours, title bar look and app icon shared by every NetSpeedGauge window.

use crate::canvas::{mix, Align, Canvas};
use crate::taskbar_dock::to_wide;
use std::sync::atomic::{AtomicI64, Ordering};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::HFONT;
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::UI::WindowsAndMessaging::{LoadIconW, HICON};

const FALLBACK_ACCENT: u32 = 0x4C9AFF;
const APP_ICON_RESOURCE: usize = 1;
const MADE_WITH_LOVE: &str = "Made in India with ";
const ICON_HEART: &str = "\u{EB52}";
const HEART_COLOR: u32 = 0xE5484D;
const HEART_DROP: i32 = 3;

/// The height of the strip along the bottom of the usage window, at 100% scaling.
pub const FOOTER_HEIGHT: i32 = 24;

static THEME_CHOICE: AtomicI64 = AtomicI64::new(0);
static UP_ICON_COLOR: AtomicI64 = AtomicI64::new(-1);
static DOWN_ICON_COLOR: AtomicI64 = AtomicI64::new(-1);

// Below this total difference across red, green and blue, a colour is hard to see on its surface.
const MIN_COLOR_DISTANCE: u32 = 90;
const VISIBILITY_SHIFT: f32 = 0.35;
const BRIGHT_SURFACE: u32 = 128;

/// The colours for one Windows theme.
#[derive(Clone, Copy)]
pub struct Theme {
    pub is_dark: bool,
    pub background: u32,
    pub card: u32,
    pub control: u32,
    pub selected: u32,
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

/// Reads a whole-number setting from the user's registry.
fn read_user_dword(path: &str, name: &str) -> Option<u32> {
    let path = to_wide(path);
    let name = to_wide(name);
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: the strings are null-terminated, and `value` and `size` are valid out pointers.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            std::ptr::from_mut(&mut value).cast(),
            &mut size,
        )
    };
    (status == 0).then_some(value)
}

/// Returns the user's Windows accent colour as 0xRRGGBB, or a calm blue if it would be hard to see.
fn accent_color() -> u32 {
    let Some(stored) = read_user_dword("Software\\Microsoft\\Windows\\DWM", "AccentColor") else {
        return FALLBACK_ACCENT;
    };
    let (red, green, blue) = (stored & 0xFF, (stored >> 8) & 0xFF, (stored >> 16) & 0xFF);
    let brightest = red.max(green).max(blue);
    let dullest = red.min(green).min(blue);
    if brightest < 90 || brightest - dullest < 40 {
        return FALLBACK_ACCENT;
    }
    (red << 16) | (green << 8) | blue
}

/// Returns the dark colours built around an accent colour.
pub fn dark_theme() -> Theme {
    let download = mix(accent_color(), 0xFFFFFF, 0.15);
    Theme {
        is_dark: true,
        background: 0x202020,
        card: 0x2B2B2B,
        control: 0x2B2B2B,
        selected: 0x454545,
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
pub fn light_theme() -> Theme {
    let accent = accent_color();
    Theme {
        is_dark: false,
        background: 0xF3F3F3,
        card: 0xFFFFFF,
        control: 0xE9E9E9,
        selected: 0xFFFFFF,
        surface: 0xFFFFFF,
        border: 0xE0E0E0,
        footer: 0xE6E6E6,
        button: 0xE9E9E9,
        button_hover: 0xDDDDDD,
        text: 0x1B1B1B,
        muted: 0x5F5F5F,
        download: accent,
        upload: mix(accent, 0xFFFFFF, 0.42),
    }
}

/// Sets the user's theme choice: 0 follows Windows, 1 is light, 2 is dark.
pub fn set_theme_choice(choice: i64) {
    THEME_CHOICE.store(choice, Ordering::Relaxed);
}

/// Remembers the user's upload and download icon colours. A negative value means automatic.
pub fn set_icon_colors(up: i64, down: i64) {
    UP_ICON_COLOR.store(up, Ordering::Relaxed);
    DOWN_ICON_COLOR.store(down, Ordering::Relaxed);
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

/// Returns the (upload, download) colours for figures drawn on a surface. The usage window and hover card both use it.
pub fn figure_colors(theme: &Theme, surface: u32) -> (u32, u32) {
    (
        figure_color(UP_ICON_COLOR.load(Ordering::Relaxed), theme.upload, surface),
        figure_color(DOWN_ICON_COLOR.load(Ordering::Relaxed), theme.download, surface),
    )
}

/// Returns the colours for the user's theme choice.
pub fn current_theme() -> Theme {
    match THEME_CHOICE.load(Ordering::Relaxed) {
        1 => light_theme(),
        2 => dark_theme(),
        _ => {
            let path = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";
            if read_user_dword(path, "AppsUseLightTheme") == Some(1) { light_theme() } else { dark_theme() }
        }
    }
}

/// Draws the "Made in India with love" line, centred, with a real heart icon.
pub fn draw_made_with_love(canvas: &mut Canvas, center_x: i32, y: i32, font: HFONT, icon_font: HFONT, color: u32) {
    let text_width = canvas.text_width(MADE_WITH_LOVE, font);
    let heart_width = canvas.text_width(ICON_HEART, icon_font);
    let left = center_x - (text_width + heart_width) / 2;
    canvas.text(left, y, MADE_WITH_LOVE, font, color, Align::Left);
    canvas.text(left + text_width, y + HEART_DROP, ICON_HEART, icon_font, HEART_COLOR, Align::Left);
}

/// Draws a full-width strip in its own colour along the bottom of a window that has no sidebar to hold the line.
pub fn draw_footer_strip(canvas: &mut Canvas, theme: &Theme, top: i32, width: i32, height: i32, font: HFONT, icon_font: HFONT) {
    canvas.round_rect(0, top, width, height, 0.0, theme.footer);
    canvas.round_rect(0, top, width, 1, 0.0, theme.border);
    let text_y = top + (height - canvas.text_height(font)) / 2;
    draw_made_with_love(canvas, width / 2, text_y, font, icon_font, theme.muted);
}

fn to_windows_color(color: u32) -> u32 {
    ((color & 0xFF) << 16) | (color & 0xFF00) | ((color >> 16) & 0xFF)
}

fn set_attribute(window: HWND, attribute: i32, value: u32) {
    // SAFETY: `value` outlives the call. Windows 10 ignores the attributes it does not know.
    unsafe {
        DwmSetWindowAttribute(
            window,
            attribute as u32,
            std::ptr::from_ref(&value).cast(),
            std::mem::size_of::<u32>() as u32,
        );
    }
}

/// Makes a title bar match its window, so the two read as one surface.
pub fn apply_title_bar(window: HWND, theme: &Theme) {
    set_attribute(window, DWMWA_USE_IMMERSIVE_DARK_MODE, u32::from(theme.is_dark));
    set_attribute(window, DWMWA_CAPTION_COLOR, to_windows_color(theme.background));
    set_attribute(window, DWMWA_TEXT_COLOR, to_windows_color(theme.text));
}

/// Gives a borderless popup rounded corners and a theme-coloured edge on Windows 11.
pub fn apply_popup_frame(window: HWND, theme: &Theme) {
    set_attribute(window, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND as u32);
    set_attribute(window, DWMWA_BORDER_COLOR, to_windows_color(theme.border));
}

/// Returns the icon built into the program.
pub fn app_icon() -> HICON {
    // SAFETY: a small number in place of a name is how Windows identifies a numbered resource.
    unsafe { LoadIconW(GetModuleHandleW(std::ptr::null()), APP_ICON_RESOURCE as *const u16) }
}
