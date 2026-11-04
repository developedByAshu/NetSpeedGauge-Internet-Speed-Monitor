//! The Settings window. It exists only while open, then frees its memory. Changes apply at once.

use crate::canvas::{create_font, mix, Align, Canvas};
use crate::format::format_speed;
use crate::net_counters::list_adapters;
use crate::messages::WM_APP_SETTINGS_CHANGED;
use crate::settings::{Field, Settings, COLOR_AUTO, FIELDS, PALETTE};
pub use netspeedgauge_shared::settings::speed_color;
use crate::startup;
use crate::taskbar_dock::to_wide;
use crate::readout::arrow_column_left;
use crate::theme::{
    app_icon, apply_title_bar, current_theme, dark_theme, draw_made_with_love, light_theme, set_theme_choice, Theme,
};
use std::cell::RefCell;
use std::rc::Rc;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, DeleteObject, EndPaint, GetMonitorInfoW, InvalidateRect, MonitorFromWindow, HFONT,
    MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_ESCAPE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, LoadCursorW, PostMessageW, RegisterClassW, SetCursor,
    SetForegroundWindow, SetWindowPos, ShowWindow, HTCLIENT, IDC_ARROW, IDC_HAND, SWP_NOACTIVATE,
    SWP_NOZORDER, SW_SHOW, WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETCURSOR, WM_SETTINGCHANGE, WNDCLASSW, WS_CAPTION,
    WS_EX_TOPMOST, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
};

// The windows-sys crate does not export this message, so it is declared here.
const WM_MOUSELEAVE: u32 = 0x02A3;

const CLASS_NAME: &str = "NetSpeedGaugeSettings";
const WINDOW_TITLE: &str = "NetSpeedGauge Settings";
const BASE_DPI: f32 = 96.0;
const WIDTH: i32 = 680;
const HEIGHT: i32 = 640;
const SIDEBAR_WIDTH: i32 = 176;
const MARGIN: i32 = 20;
const HEADER_HEIGHT: i32 = 62;
const ROW_HEIGHT: i32 = 50;
const ROW_GAP: i32 = 4;
const SECTION_GAP: i32 = 34;
// The height a heading toggle is centred within, which lines it up with the heading text.
const HEADING_TOGGLE_ROW: i32 = 46;
const PREVIEW_HEIGHT: i32 = 92;
const ABOUT_CARD_HEIGHT: i32 = 96;
const PRIVACY_LINE: i32 = 26;
const ICON_CHECK: &str = "\u{E73E}";
const ICON_LOCK: &str = "\u{E72E}";
const PRIVACY_HEADER_HEIGHT: i32 = 68;
const LOCK_TILE: i32 = 40;
const DISABLED_FILL: f32 = 0.35;
const DISABLED_TEXT: f32 = 0.5;
const IRRELEVANT_ROW_FADE: f32 = 0.65;
const CARD_RADIUS: i32 = 6;
const NAV_TOP: i32 = 62;
const NAV_ROW: i32 = 40;
const SCROLL_STEP: i32 = 72;
const SCREEN_MARGIN: i32 = 12;
const ICON_MINUS: &str = "\u{E738}";
const ICON_PLUS: &str = "\u{E710}";
const WHEEL_NOTCH: i32 = 120;

/// The pages in the sidebar, with their icon and title.
const PAGES: [(&str, &str); 6] = [
    ("\u{E713}", "General"),
    ("\u{E790}", "Appearance"),
    ("\u{E9D2}", "Units"),
    ("\u{E839}", "Network"),
    ("\u{E9E9}", "Advanced"),
    ("\u{E946}", "Details"),
];

/// What NetSpeedGauge does and does not do with data, shown on the Details page.
const PRIVACY_LINES: [&str; 4] = [
    "Nothing leaves this PC. NetSpeedGauge never connects to the internet.",
    "It only reads your network adapters' byte counts from Windows.",
    "Your usage and settings are stored only on this computer.",
    "No account, no tracking, no ads and no servers.",
];

const APP_NAME: &str = "NetSpeedGauge";
const APP_TAGLINE: &str = "A tiny network speed monitor for the Windows taskbar.";
const DEVELOPER_NAME: &str = "Ashutosh Anand";
const DEVELOPER_ORIGIN: &str = "From India";
// One short, factual line. Left empty until the developer supplies it, so nothing here is made up.
const DEVELOPER_NOTE: &str = "";

/// How one row is controlled.
#[derive(Clone, Copy)]
enum Kind {
    Toggle,
    Segments(&'static [&'static str]),
    Stepper { step: i64, suffix: &'static str },
    Swatches { auto_label: &'static str },
}

/// One row on a page.
#[derive(Clone, Copy)]
enum Row {
    Section(&'static str),
    // A heading with a toggle: on shows the rows under it, off collapses them.
    ToggleSection { title: &'static str, field: Field },
    Preview,
    Setting { field: Field, label: &'static str, hint: &'static str, kind: Kind },
    StartWithWindows,
    AdapterList,
    Note(&'static str),
    ResetPage,
    ResetAll,
    AboutCard,
    Privacy,
    Developer,
}

const fn setting(field: Field, label: &'static str, hint: &'static str, kind: Kind) -> Row {
    Row::Setting { field, label, hint, kind }
}

const APPEARANCE_ROWS: &[Row] = &[
    Row::Section("Theme"),
    setting(Field::WindowTheme, "NetSpeedGauge windows", "", Kind::Segments(&["Same as system", "Light", "Dark"])),
    Row::Preview,
    Row::Section("Text"),
    setting(Field::FontFamily, "Font", "", Kind::Segments(&["Segoe", "Semibold", "Bahnschrift", "Consolas"])),
    setting(Field::FontPoints, "Size", "", Kind::Stepper { step: 1, suffix: " pt" }),
    setting(Field::TextColor, "Colour", "", Kind::Swatches { auto_label: "A" }),
    setting(Field::SwapRows, "Download on top", "Swaps the two rows", Kind::Toggle),
    Row::ToggleSection { title: "Icons", field: Field::ShowIcons },
    setting(Field::ArrowStyle, "Icon style", "", Kind::Segments(&["\u{2191} \u{2193}", "\u{25B2} \u{25BC}", "U D"])),
    setting(Field::IconThickness, "Icon thickness", "", Kind::Stepper { step: 1, suffix: "" }),
    setting(Field::UpColor, "Upload colour", "", Kind::Swatches { auto_label: "A" }),
    setting(Field::DownColor, "Download colour", "", Kind::Swatches { auto_label: "A" }),
    Row::ToggleSection { title: "Background", field: Field::ShowBackground },
    setting(Field::BackgroundColor, "Colour", "", Kind::Swatches { auto_label: "" }),
    setting(Field::BackgroundOpacity, "Opacity", "", Kind::Stepper { step: 10, suffix: "%" }),
    Row::ToggleSection { title: "Colour by speed", field: Field::ColorCoding },
    setting(Field::LowLimit, "Slow below", "", Kind::Stepper { step: 1, suffix: " MBps" }),
    setting(Field::LowColor, "Slow colour", "", Kind::Swatches { auto_label: "" }),
    setting(Field::HighLimit, "Fast above", "", Kind::Stepper { step: 5, suffix: " MBps" }),
    setting(Field::HighColor, "Fast colour", "", Kind::Swatches { auto_label: "" }),
];

const UNITS_ROWS: &[Row] = &[
    Row::Preview,
    Row::Section("Speed"),
    setting(Field::Unit, "Unit", "Megabytes or megabits", Kind::Segments(&["MBps", "Mbps", "MiBps", "Mibps"])),
    setting(Field::Scale, "Scale", "", Kind::Segments(&["Auto", "Always Mega"])),
    setting(Field::Decimals, "Decimal places", "", Kind::Stepper { step: 1, suffix: "" }),
    Row::Section("Display"),
    setting(Field::HideSuffix, "Hide the unit", "Show only the number", Kind::Toggle),
    setting(Field::ShortLabels, "Short unit", "M instead of MBps", Kind::Toggle),
];

const GENERAL_ROWS: &[Row] = &[
    Row::Section("Startup"),
    Row::StartWithWindows,
    Row::Section("Updates"),
    setting(Field::UpdateRate, "Update every", "", Kind::Segments(&["0.5 s", "1 s", "2 s", "5 s"])),
    setting(Field::SmartUpdates, "Slow down when idle", "Uses less power when nothing is moving", Kind::Toggle),
    Row::Section("Position"),
    setting(Field::OffsetX, "Move left or right", "", Kind::Stepper { step: 4, suffix: " px" }),
    setting(Field::OffsetY, "Move up or down", "", Kind::Stepper { step: 1, suffix: " px" }),
];

const NETWORK_ROWS: &[Row] = &[
    Row::Section("What to count"),
    setting(Field::InterfaceMode, "Adapters", "", Kind::Segments(&["All physical", "Main only", "Chosen"])),
    Row::Note("All physical counts Wi-Fi, LAN, Wi-Fi Direct and Bluetooth.\nVPN traffic counts once, on the real adapter."),
    Row::AdapterList,
];

const ADVANCED_ROWS: &[Row] = &[
    Row::Section("Data"),
    setting(Field::KeepHistory, "Keep usage history for", "", Kind::Segments(&["3 mo", "6 mo", "1 yr", "2 yr"])),
    Row::Section("Behaviour"),
    setting(Field::HoverAction, "On hover", "When the mouse rests on it", Kind::Segments(&["Nothing", "Usage card", "Usage window"])),
    setting(Field::ClickAction, "On click", "Right-click opens the menu", Kind::Segments(&["Nothing", "Usage card", "Usage window"])),
    setting(Field::KeepOnTop, "Keep windows on top", "Usage and Settings stay above other apps", Kind::Toggle),
    Row::Section("Reset"),
    Row::ResetPage,
    Row::ResetAll,
];

const ABOUT_ROWS: &[Row] = &[
    Row::AboutCard,
    Row::Section("Privacy"),
    Row::Privacy,
    Row::Section("Know your developer"),
    Row::Developer,
];

fn page_rows(page: usize) -> &'static [Row] {
    match page {
        0 => GENERAL_ROWS,
        1 => APPEARANCE_ROWS,
        2 => UNITS_ROWS,
        3 => NETWORK_ROWS,
        4 => ADVANCED_ROWS,
        _ => ABOUT_ROWS,
    }
}

// The pages the "Appearance and units" reset covers.
const APPEARANCE_PAGE: usize = 1;
const UNITS_PAGE: usize = 2;

/// Returns every saved field on a page, for its reset button.
pub fn page_fields(page: usize) -> Vec<Field> {
    page_rows(page)
        .iter()
        .filter_map(|row| match row {
            Row::Setting { field, .. } | Row::ToggleSection { field, .. } => Some(*field),
            _ => None,
        })
        .collect()
}

/// What a click on an area does.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Page(usize),
    Set(Field, i64),
    ToggleStartWithWindows,
    ToggleAdapter(u64),
    ResetPage,
    ResetAll,
}

#[derive(Clone, Copy)]
struct HitArea {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    action: Action,
}

struct Fonts {
    body: HFONT,
    strong: HFONT,
    small: HFONT,
    title: HFONT,
    icon: HFONT,
    small_icon: HFONT,
    preview: HFONT,
    preview_arrow: HFONT,
    heart: HFONT,
    lock: HFONT,
}

impl Fonts {
    fn new(dpi: u32, settings: &Settings) -> Self {
        let points = settings.get(Field::FontPoints) as f32;
        Self {
            body: create_font("Segoe UI", 10.0, 400, dpi),
            strong: create_font("Segoe UI Semibold", 10.0, 400, dpi),
            small: create_font("Segoe UI", 9.0, 400, dpi),
            title: create_font("Segoe UI Semibold", 16.0, 400, dpi),
            icon: create_font("Segoe MDL2 Assets", 11.0, 400, dpi),
            small_icon: create_font("Segoe MDL2 Assets", 8.0, 400, dpi),
            preview: create_font(settings.font_family(), points, 400, dpi),
            preview_arrow: create_font(settings.font_family(), points, settings.icon_weight(), dpi),
            heart: create_font("Segoe MDL2 Assets", 9.0, 400, dpi),
            lock: create_font("Segoe MDL2 Assets", 15.0, 400, dpi),
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        // SAFETY: each font was created in new() and is released once here.
        unsafe {
            for font in [self.body, self.strong, self.small, self.title, self.icon, self.small_icon, self.preview, self.preview_arrow, self.heart, self.lock] {
                DeleteObject(font);
            }
        }
    }
}

struct SettingsWindow {
    window: HWND,
    anchor: HWND,
    settings: Rc<RefCell<Settings>>,
    canvas: Canvas,
    fonts: Fonts,
    theme: Theme,
    dpi: u32,
    page: usize,
    scroll: i32,
    content_height: i32,
    hits: Vec<HitArea>,
    hover: Option<Action>,
    adapters: Vec<(u64, String)>,
    is_confirming_reset: bool,
    is_tracking_mouse: bool,
}

thread_local! {
    static OPEN_WINDOW: RefCell<Option<SettingsWindow>> = const { RefCell::new(None) };
}

fn with_window(action: impl FnOnce(&mut SettingsWindow)) {
    OPEN_WINDOW.with(|cell| {
        if let Ok(mut state) = cell.try_borrow_mut() {
            if let Some(window) = state.as_mut() {
                action(window);
            }
        }
    });
}

impl SettingsWindow {
    fn px(&self, value: i32) -> i32 {
        (value as f32 * self.dpi as f32 / BASE_DPI).round() as i32
    }

    fn add_hit(&mut self, x: i32, y: i32, width: i32, height: i32, action: Action) {
        self.hits.push(HitArea { x, y, width, height, action });
    }

    fn is_hover(&self, action: Action) -> bool {
        self.hover == Some(action)
    }

    /// Paints the window: the page first, then the fixed header and sidebar over anything scrolled under them.
    fn draw(&mut self) {
        let theme = self.theme;
        self.hits.clear();
        self.canvas.clear(theme.background);
        self.draw_page();
        let sidebar = self.px(SIDEBAR_WIDTH);
        let header = self.px(HEADER_HEIGHT);
        self.hits.retain(|hit| hit.y + hit.height > header);
        self.canvas.round_rect(sidebar, 0, self.px(WIDTH) - sidebar, header, 0.0, theme.background);
        let title = PAGES[self.page].1;
        self.canvas.text(sidebar + self.px(MARGIN - 4), self.px(20), title, self.fonts.title, theme.text, Align::Left);
        self.draw_sidebar();
        self.draw_scrollbar();
    }

    fn draw_sidebar(&mut self) {
        let theme = self.theme;
        let sidebar = self.px(SIDEBAR_WIDTH);
        self.canvas.round_rect(0, 0, sidebar, self.px(HEIGHT), 0.0, mix(theme.background, theme.card, 0.45));
        self.canvas.text(self.px(18), self.px(22), "NetSpeedGauge", self.fonts.strong, theme.muted, Align::Left);
        for (position, (icon, label)) in PAGES.iter().enumerate() {
            let y = self.px(NAV_TOP + position as i32 * NAV_ROW);
            let (x, width, height) = (self.px(8), sidebar - self.px(16), self.px(NAV_ROW - 4));
            let is_selected = position == self.page;
            let action = Action::Page(position);
            if is_selected || self.is_hover(action) {
                let fill = if is_selected { theme.selected } else { mix(theme.background, theme.selected, 0.5) };
                self.canvas.round_rect(x, y, width, height, self.px(CARD_RADIUS) as f32, fill);
            }
            if is_selected {
                self.canvas.round_rect(x, y + self.px(10), self.px(3), height - self.px(20), self.px(2) as f32, theme.download);
            }
            let color = if is_selected { theme.text } else { theme.muted };
            self.canvas.text(x + self.px(12), y + self.px(10), icon, self.fonts.icon, color, Align::Left);
            let font = if is_selected { self.fonts.strong } else { self.fonts.body };
            self.canvas.text(x + self.px(38), y + self.px(9), label, font, theme.text, Align::Left);
            self.add_hit(x, y, width, height, action);
        }
        let footer_y = self.px(HEIGHT - 34);
        draw_made_with_love(&mut self.canvas, sidebar / 2, footer_y, self.fonts.small, self.fonts.heart, theme.muted);
    }

    fn draw_scrollbar(&mut self) {
        let view = self.px(HEIGHT) - self.px(HEADER_HEIGHT);
        if self.content_height <= view {
            return;
        }
        let track_x = self.px(WIDTH) - self.px(7);
        let thumb_height = (view * view / self.content_height).max(self.px(30));
        let thumb_y = self.px(HEADER_HEIGHT) + (view - thumb_height) * self.scroll / (self.content_height - view).max(1);
        let color = mix(self.theme.background, self.theme.muted, 0.5);
        self.canvas.round_rect(track_x, thumb_y, self.px(3), thumb_height, self.px(2) as f32, color);
    }

    /// Draws every row of the current page, top to bottom, and works out how tall the page is.
    fn draw_page(&mut self) {
        let rows = page_rows(self.page);
        let left = self.px(SIDEBAR_WIDTH + MARGIN - 4);
        let width = self.px(WIDTH - SIDEBAR_WIDTH - MARGIN * 2 + 4);
        let top = self.px(HEADER_HEIGHT);
        let mut y = top - self.scroll;
        for row in rows {
            y = self.draw_row(*row, left, y, width);
        }
        self.content_height = y + self.scroll - top + self.px(MARGIN);
        // Collapsing a section can leave the page scrolled past its new end.
        let limit = (self.content_height - (self.px(HEIGHT) - top)).max(0);
        if self.scroll > limit {
            self.scroll = limit;
            self.invalidate();
        }
    }

    fn row_card(&mut self, left: i32, y: i32, width: i32, height: i32) {
        self.canvas.round_rect(left, y, width, height, self.px(CARD_RADIUS) as f32, self.theme.card);
    }

    /// Draws a row's label, with its hint under it when there is one.
    fn row_label(&mut self, left: i32, y: i32, label: &str, hint: &str) {
        let theme = self.theme;
        let x = left + self.px(14);
        if hint.is_empty() {
            self.canvas.text(x, y + self.px(15), label, self.fonts.body, theme.text, Align::Left);
        } else {
            self.canvas.text(x, y + self.px(7), label, self.fonts.body, theme.text, Align::Left);
            self.canvas.text(x, y + self.px(27), hint, self.fonts.small, theme.muted, Align::Left);
        }
    }

    /// Draws one row and returns the y position of the next.
    fn draw_row(&mut self, row: Row, left: i32, y: i32, width: i32) -> i32 {
        let row_height = self.px(ROW_HEIGHT);
        let next = y + row_height + self.px(ROW_GAP);
        match row {
            Row::Section(title) => {
                let text_y = y + self.px(14);
                self.canvas.text(left + self.px(2), text_y, title, self.fonts.strong, self.theme.text, Align::Left);
                y + self.px(SECTION_GAP) + self.px(8)
            }
            Row::ToggleSection { title, field } => {
                let text_y = y + self.px(14);
                self.canvas.text(left + self.px(2), text_y, title, self.fonts.strong, self.theme.text, Align::Left);
                let is_on = self.settings.borrow().is_on(field);
                // Centred on the heading text, in the same column as the toggles in the rows below.
                self.draw_toggle(left + width - self.px(14), y, self.px(HEADING_TOGGLE_ROW), is_on);
                let height = self.px(SECTION_GAP) + self.px(8);
                self.add_hit(left, y, width, height, Action::Set(field, i64::from(!is_on)));
                y + height
            }
            Row::Preview => {
                self.draw_preview(left, y, width);
                y + self.px(PREVIEW_HEIGHT) + self.px(ROW_GAP)
            }
            Row::Setting { field, label, hint, kind } => {
                let is_collapsed = Settings::section_switch(field).is_some_and(|switch| !self.settings.borrow().is_on(switch));
                if is_collapsed {
                    return y;
                }
                let hits_before = self.hits.len();
                self.row_card(left, y, width, row_height);
                self.row_label(left, y, label, hint);
                let right = left + width - self.px(14);
                self.draw_control(field, kind, right, y, left, width, row_height);
                if !self.settings.borrow().is_relevant(field) {
                    // Another choice leaves this row with no effect, so it greys out and ignores clicks.
                    self.hits.truncate(hits_before);
                    let radius = self.px(CARD_RADIUS) as f32;
                    self.canvas.fade_round_rect(left, y, width, row_height, radius, self.theme.card, IRRELEVANT_ROW_FADE);
                }
                next
            }
            Row::StartWithWindows => {
                let is_on = startup::is_enabled();
                self.row_card(left, y, width, row_height);
                self.row_label(left, y, "Start with Windows", "Opens NetSpeedGauge when you sign in");
                self.draw_toggle(left + width - self.px(14), y, row_height, is_on);
                self.add_hit(left, y, width, row_height, Action::ToggleStartWithWindows);
                next
            }
            Row::AdapterList => self.draw_adapter_list(left, y, width),
            Row::Note(text) => {
                let text_y = y + self.px(2);
                let (first, second) = split_note(text);
                self.canvas.text(left + self.px(2), text_y, first, self.fonts.small, self.theme.muted, Align::Left);
                self.canvas.text(left + self.px(2), text_y + self.px(17), second, self.fonts.small, self.theme.muted, Align::Left);
                y + self.px(46)
            }
            Row::ResetPage => {
                self.draw_button_row(left, y, width, "Appearance and units", "Back to how NetSpeedGauge started", "Reset", Action::ResetPage);
                next
            }
            Row::ResetAll => {
                let button = if self.is_confirming_reset { "Click to confirm" } else { "Reset all" };
                self.draw_button_row(left, y, width, "Every setting", "Usage history is kept", button, Action::ResetAll);
                next
            }
            Row::AboutCard => {
                let height = self.px(ABOUT_CARD_HEIGHT);
                self.row_card(left, y, width, height);
                let x = left + self.px(16);
                let version = format!("Version {}", env!("CARGO_PKG_VERSION"));
                self.canvas.text(x, y + self.px(14), APP_NAME, self.fonts.title, self.theme.text, Align::Left);
                self.canvas.text(x, y + self.px(46), APP_TAGLINE, self.fonts.body, self.theme.muted, Align::Left);
                self.canvas.text(x, y + self.px(68), &version, self.fonts.small, self.theme.muted, Align::Left);
                y + height + self.px(ROW_GAP)
            }
            Row::Privacy => {
                let line_height = self.px(PRIVACY_LINE);
                let header = self.px(PRIVACY_HEADER_HEIGHT);
                let height = header + line_height * PRIVACY_LINES.len() as i32 + self.px(20);
                self.row_card(left, y, width, height);
                self.draw_privacy_header(left, y, width);
                for (position, line) in PRIVACY_LINES.iter().enumerate() {
                    let line_y = y + header + self.px(10) + position as i32 * line_height;
                    self.canvas.text(left + self.px(14), line_y + self.px(5), ICON_CHECK, self.fonts.small_icon, self.theme.download, Align::Left);
                    self.canvas.text(left + self.px(36), line_y, line, self.fonts.body, self.theme.text, Align::Left);
                }
                y + height + self.px(ROW_GAP)
            }
            Row::Developer => {
                let hint = if DEVELOPER_NOTE.is_empty() {
                    DEVELOPER_ORIGIN.to_string()
                } else {
                    format!("{DEVELOPER_ORIGIN}. {DEVELOPER_NOTE}")
                };
                self.row_card(left, y, width, row_height);
                self.row_label(left, y, DEVELOPER_NAME, &hint);
                next
            }
        }
    }

    /// Draws the top of the privacy card: a lock on a tinted tile, its promise, and a dividing line.
    fn draw_privacy_header(&mut self, left: i32, y: i32, width: i32) {
        let theme = self.theme;
        let tile = self.px(LOCK_TILE);
        let (tile_x, tile_y) = (left + self.px(14), y + self.px(14));
        let tile_color = mix(theme.card, theme.download, 0.18);
        self.canvas.round_rect(tile_x, tile_y, tile, tile, self.px(CARD_RADIUS + 2) as f32, tile_color);
        let lock_y = tile_y + (tile - self.canvas.text_height(self.fonts.lock)) / 2;
        self.canvas.text(tile_x + tile / 2, lock_y, ICON_LOCK, self.fonts.lock, theme.download, Align::Center);
        let text_x = tile_x + tile + self.px(12);
        self.canvas.text(text_x, tile_y + self.px(1), "Your data stays with you", self.fonts.strong, theme.text, Align::Left);
        self.canvas.text(text_x, tile_y + self.px(21), "Private and stored only on this PC", self.fonts.small, theme.muted, Align::Left);
        let divider_y = y + self.px(PRIVACY_HEADER_HEIGHT);
        self.canvas.round_rect(left + self.px(14), divider_y, width - self.px(28), 1, 0.0, theme.border);
    }

    fn draw_button_row(&mut self, left: i32, y: i32, width: i32, label: &str, hint: &str, button: &str, action: Action) {
        let theme = self.theme;
        let row_height = self.px(ROW_HEIGHT);
        self.row_card(left, y, width, row_height);
        self.row_label(left, y, label, hint);
        let text_width = self.canvas.text_width(button, self.fonts.body);
        let button_width = text_width + self.px(28);
        let (x, button_y, height) = (left + width - self.px(14) - button_width, y + self.px(10), self.px(30));
        let fill = if self.is_hover(action) { theme.button_hover } else { theme.button };
        self.canvas.round_rect(x, button_y, button_width, height, self.px(CARD_RADIUS) as f32, fill);
        self.canvas.text(x + button_width / 2, button_y + self.px(5), button, self.fonts.body, theme.text, Align::Center);
        self.add_hit(x, button_y, button_width, height, action);
    }

    /// Draws a small taskbar with the live readout on it, using the current choices.
    fn draw_preview(&mut self, left: i32, y: i32, width: i32) {
        let theme = self.theme;
        let height = self.px(PREVIEW_HEIGHT);
        self.canvas.round_rect(left, y, width, height, self.px(8) as f32, theme.card);
        let bar_height = self.px(48);
        let bar_y = y + (height - bar_height) / 2;
        let bar_x = left + self.px(14);
        let bar_width = width - self.px(28);
        let bar_color = if theme.is_dark { 0x1C1C1C } else { 0xE4E4E4 };
        self.canvas.round_rect(bar_x, bar_y, bar_width, bar_height, self.px(6) as f32, bar_color);
        self.canvas.text(bar_x + self.px(12), bar_y + self.px(15), "Preview", self.fonts.small, theme.muted, Align::Left);

        let settings = self.settings.borrow().clone();
        let format = settings.speed_format();
        let automatic = if theme.is_dark { 0xFFFFFF } else { 0x000000 };
        let base = settings.color_or(Field::TextColor, automatic);
        let up_speed = 1_800_000.0;
        let down_speed = 124_000_000.0;
        let value_color = |speed: f64| speed_color(&settings, speed, base);
        let (up_value, down_value) = (value_color(up_speed), value_color(down_speed));
        let up_row = (settings.arrow_symbols().0, format_speed(up_speed, &format), settings.color_or(Field::UpColor, up_value), up_value);
        let down_row = (settings.arrow_symbols().1, format_speed(down_speed, &format), settings.color_or(Field::DownColor, down_value), down_value);
        let rows = if settings.is_on(Field::SwapRows) { [down_row, up_row] } else { [up_row, down_row] };

        let right = bar_x + bar_width - self.px(16);
        let row_step = self.px(settings.get(Field::FontPoints) as i32 * 5 / 3 + 3);
        let start_y = bar_y + (bar_height - row_step * 2) / 2 + self.px(1);
        let gap = if settings.arrow_symbols().0.is_empty() { 0 } else { self.canvas.text_width("    ", self.fonts.preview) };
        let stroke = settings.icon_stroke(self.dpi);
        let widest = rows.iter().map(|row| self.canvas.text_width(&row.1, self.fonts.preview)).max().unwrap_or(0);
        let arrow_width =
            rows.iter().map(|row| self.canvas.text_width(row.0, self.fonts.preview_arrow)).max().unwrap_or(0);
        let arrow_left = arrow_column_left(right, widest, gap, stroke, arrow_width);
        if let Some(background) = settings.background() {
            let amount = settings.get(Field::BackgroundOpacity) as f32 / 100.0;
            let (pad, box_x) = (self.px(7), arrow_left - self.px(7));
            self.canvas.round_rect(box_x, bar_y + self.px(4), right + pad - box_x, bar_height - self.px(8), self.px(6) as f32, mix(bar_color, background, amount));
        }
        for (position, (arrow, value, arrow_color, value_color)) in rows.iter().enumerate() {
            let row_y = start_y + position as i32 * row_step;
            self.canvas.text(right, row_y, value, self.fonts.preview, *value_color, Align::Right);
            for offset in 0..=stroke {
                self.canvas.text(arrow_left + offset, row_y, arrow, self.fonts.preview_arrow, *arrow_color, Align::Left);
            }
        }
    }

    fn draw_control(&mut self, field: Field, kind: Kind, right: i32, y: i32, left: i32, width: i32, row_height: i32) {
        let value = self.settings.borrow().get(field);
        match kind {
            Kind::Toggle => {
                self.draw_toggle(right, y, row_height, value != 0);
                self.add_hit(left, y, width, row_height, Action::Set(field, i64::from(value == 0)));
            }
            Kind::Segments(options) => self.draw_segments(field, options, right, y, value),
            Kind::Stepper { step, suffix } => self.draw_stepper(field, right, y, value, step, suffix),
            Kind::Swatches { auto_label } => self.draw_swatches(field, right, y, value, auto_label),
        }
    }

    fn draw_toggle(&mut self, right: i32, y: i32, row_height: i32, is_on: bool) {
        let theme = self.theme;
        let (width, height) = (self.px(40), self.px(20));
        let (x, top) = (right - width, y + (row_height - height) / 2);
        if is_on {
            self.canvas.round_rect(x, top, width, height, height as f32 / 2.0, theme.download);
        } else {
            self.canvas.round_rect(x, top, width, height, height as f32 / 2.0, theme.muted);
            self.canvas.round_rect(x + 1, top + 1, width - 2, height - 2, (height - 2) as f32 / 2.0, theme.card);
        }
        let knob = self.px(12);
        let knob_x = if is_on { x + width - knob - self.px(4) } else { x + self.px(4) };
        let knob_color = if is_on { if theme.is_dark { 0x1B1B1B } else { 0xFFFFFF } } else { theme.muted };
        self.canvas.round_rect(knob_x, top + (height - knob) / 2, knob, knob, knob as f32 / 2.0, knob_color);
    }

    fn draw_segments(&mut self, field: Field, options: &'static [&'static str], right: i32, y: i32, value: i64) {
        let theme = self.theme;
        let pad = self.px(13);
        let widths: Vec<i32> = options.iter().map(|option| self.canvas.text_width(option, self.fonts.body) + pad * 2).collect();
        let total: i32 = widths.iter().sum();
        let (height, top) = (self.px(32), y + self.px(9));
        let mut x = right - total;
        let radius = self.px(CARD_RADIUS) as f32;
        self.canvas.round_rect(x, top, total, height, radius, theme.segment_track_on_card);
        let inset = self.px(3);
        for (position, option) in options.iter().enumerate() {
            let segment = widths[position];
            let action = Action::Set(field, position as i64);
            let is_selected = value == position as i64;
            if is_selected {
                self.canvas.round_rect(x + inset, top + inset, segment - inset * 2, height - inset * 2, radius - inset as f32 / 2.0, theme.segment_selected);
            }
            let color = if is_selected || self.is_hover(action) { theme.text } else { theme.muted };
            let font = if is_selected { self.fonts.strong } else { self.fonts.body };
            self.canvas.text(x + segment / 2, top + self.px(6), option, font, color, Align::Center);
            self.add_hit(x, top, segment, height, action);
            x += segment;
        }
    }

    fn draw_stepper(&mut self, field: Field, right: i32, y: i32, value: i64, step: i64, suffix: &str) {
        let theme = self.theme;
        let (size, top) = (self.px(30), y + self.px(10));
        let value_width = self.px(76);
        let plus_x = right - size;
        let minus_x = plus_x - value_width - size;
        let limits = &FIELDS[field as usize];
        let buttons = [
            (minus_x, ICON_MINUS, value - step, value > limits.min),
            (plus_x, ICON_PLUS, value + step, value < limits.max),
        ];
        for (x, icon, new_value, is_enabled) in buttons {
            let action = Action::Set(field, new_value);
            let radius = self.px(CARD_RADIUS) as f32;
            if is_enabled {
                let fill = if self.is_hover(action) { theme.button_hover } else { theme.button };
                self.canvas.round_rect(x, top, size, size, radius, fill);
                self.canvas.text(x + size / 2, top + self.px(9), icon, self.fonts.small_icon, theme.text, Align::Center);
                self.add_hit(x, top, size, size, action);
            } else {
                // At the end of its range the button looks switched off and cannot be clicked.
                self.canvas.round_rect(x, top, size, size, radius, mix(theme.card, theme.button, DISABLED_FILL));
                self.canvas.text(x + size / 2, top + self.px(9), icon, self.fonts.small_icon, mix(theme.card, theme.muted, DISABLED_TEXT), Align::Center);
            }
        }
        let label = format!("{value}{suffix}");
        self.canvas.text(minus_x + size + value_width / 2, top + self.px(5), &label, self.fonts.body, theme.text, Align::Center);
    }

    fn draw_swatches(&mut self, field: Field, right: i32, y: i32, value: i64, auto_label: &str) {
        let theme = self.theme;
        let size = self.px(20);
        let step = self.px(25);
        let top = y + (self.px(ROW_HEIGHT) - size) / 2;
        let count = PALETTE.len() as i32;
        let palette_left = right - step * count + (step - size);
        for (position, color) in PALETTE.iter().enumerate() {
            let x = palette_left + position as i32 * step;
            let is_selected = value == i64::from(*color);
            let action = Action::Set(field, i64::from(*color));
            if is_selected || self.is_hover(action) {
                let ring = self.px(3);
                let ring_color = if is_selected { theme.text } else { theme.muted };
                self.canvas.round_rect(x - ring, top - ring, size + ring * 2, size + ring * 2, (size + ring * 2) as f32 / 2.0, ring_color);
                self.canvas.round_rect(x - ring + 2, top - ring + 2, size + ring * 2 - 4, size + ring * 2 - 4, (size + ring * 2 - 4) as f32 / 2.0, theme.card);
            }
            // A thin outline keeps a colour visible when it matches the card, like white on the light theme.
            self.canvas.round_rect(x - 1, top - 1, size + 2, size + 2, (size + 2) as f32 / 2.0, mix(theme.card, theme.muted, 0.6));
            self.canvas.round_rect(x, top, size, size, size as f32 / 2.0, *color);
            self.add_hit(x, top, size, size, action);
        }
        if auto_label.is_empty() {
            return;
        }
        let x = palette_left - step;
        let action = Action::Set(field, COLOR_AUTO);
        let is_selected = value == COLOR_AUTO;
        let ring_color = if is_selected { theme.text } else { theme.muted };
        self.canvas.round_rect(x, top, size, size, size as f32 / 2.0, ring_color);
        self.canvas.round_rect(x + 1, top + 1, size - 2, size - 2, (size - 2) as f32 / 2.0, theme.card);
        let font = if is_selected { self.fonts.strong } else { self.fonts.small };
        self.canvas.text(x + size / 2, top + self.px(1), auto_label, font, ring_color, Align::Center);
        self.add_hit(x, top, size, size, action);
    }

    fn draw_adapter_list(&mut self, left: i32, y: i32, width: i32) -> i32 {
        let settings = self.settings.borrow().clone();
        let is_active = settings.get(Field::InterfaceMode) == 2;
        let row_height = self.px(ROW_HEIGHT);
        let mut y = y;
        if !is_active {
            return y;
        }
        let adapters = self.adapters.clone();
        for (luid, name) in adapters {
            self.row_card(left, y, width, row_height);
            self.row_label(left, y, &name, "");
            let is_on = settings.chosen_adapters.contains(&luid);
            self.draw_toggle(left + width - self.px(14), y, row_height, is_on);
            self.add_hit(left, y, width, row_height, Action::ToggleAdapter(luid));
            y += row_height + self.px(ROW_GAP);
        }
        y
    }

    fn invalidate(&self) {
        // SAFETY: the window is valid while this state exists.
        unsafe { InvalidateRect(self.window, std::ptr::null(), 0) };
    }

    /// Saves the choices and tells the app to apply them at once.
    fn commit(&mut self) {
        self.settings.borrow().save();
        self.fonts = Fonts::new(self.dpi, &self.settings.borrow());
        set_theme_choice(self.settings.borrow().get(Field::WindowTheme));
        self.theme = current_theme();
        apply_title_bar(self.window, &self.theme);
        if self.settings.borrow().is_saved_to_disk {
            // SAFETY: the readout window lives as long as the app. Previews have no readout to tell.
            unsafe { PostMessageW(self.anchor, WM_APP_SETTINGS_CHANGED, 0, 0) };
        }
        self.invalidate();
    }

    fn on_click(&mut self, x: i32, y: i32) {
        let Some(action) = self.action_at(x, y) else {
            return;
        };
        let was_confirming = self.is_confirming_reset;
        self.is_confirming_reset = false;
        match action {
            Action::Page(page) => {
                self.page = page;
                self.scroll = 0;
                self.invalidate();
            }
            Action::Set(field, value) => {
                self.settings.borrow_mut().set(field, value);
                self.commit();
            }
            Action::ToggleStartWithWindows => {
                startup::set_enabled(!startup::is_enabled());
                self.invalidate();
            }
            Action::ToggleAdapter(luid) => {
                {
                    let mut settings = self.settings.borrow_mut();
                    match settings.chosen_adapters.iter().position(|known| *known == luid) {
                        Some(position) => {
                            settings.chosen_adapters.remove(position);
                        }
                        None => settings.chosen_adapters.push(luid),
                    }
                }
                self.commit();
            }
            Action::ResetPage => {
                {
                    let mut settings = self.settings.borrow_mut();
                    for field in page_fields(APPEARANCE_PAGE).into_iter().chain(page_fields(UNITS_PAGE)) {
                        settings.reset(field);
                    }
                }
                self.commit();
            }
            Action::ResetAll => {
                if was_confirming {
                    {
                        let mut settings = self.settings.borrow_mut();
                        FIELDS.iter().for_each(|spec| settings.reset(spec.field));
                        settings.chosen_adapters.clear();
                    }
                    self.commit();
                } else {
                    self.is_confirming_reset = true;
                    self.invalidate();
                }
            }
        }
    }

    /// Returns what is under the mouse. The newest areas sit on top, so they are checked first.
    fn action_at(&self, x: i32, y: i32) -> Option<Action> {
        self.hits
            .iter()
            .rev()
            .find(|hit| x >= hit.x && x < hit.x + hit.width && y >= hit.y && y < hit.y + hit.height)
            .map(|hit| hit.action)
    }

    fn on_mouse_move(&mut self, x: i32, y: i32) {
        if !self.is_tracking_mouse {
            let mut request = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: self.window,
                dwHoverTime: 0,
            };
            // SAFETY: `request` is valid for the duration of the call.
            unsafe { TrackMouseEvent(&mut request) };
            self.is_tracking_mouse = true;
        }
        let hover = self.action_at(x, y);
        if hover != self.hover {
            self.hover = hover;
            self.invalidate();
        }
    }

    fn on_wheel(&mut self, notches: i32) {
        let view = self.px(HEIGHT) - self.px(HEADER_HEIGHT);
        let limit = (self.content_height - view).max(0);
        self.scroll = (self.scroll - notches * self.px(SCROLL_STEP)).clamp(0, limit);
        self.invalidate();
    }
}

/// Splits a note into its two lines.
fn split_note(text: &'static str) -> (&'static str, &'static str) {
    text.split_once('\n').unwrap_or((text, ""))
}

/// Handles the window's messages.
unsafe extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_PAINT => {
            let mut paint: PAINTSTRUCT = std::mem::zeroed();
            let target = BeginPaint(window, &mut paint);
            with_window(|state| {
                state.draw();
                state.canvas.present(target);
            });
            EndPaint(window, &paint);
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEMOVE => {
            let (x, y) = (i32::from(lparam as u16 as i16), i32::from((lparam >> 16) as u16 as i16));
            with_window(|state| state.on_mouse_move(x, y));
            0
        }
        WM_MOUSELEAVE => {
            with_window(|state| {
                state.is_tracking_mouse = false;
                state.hover = None;
                state.invalidate();
            });
            0
        }
        WM_LBUTTONUP => {
            let (x, y) = (i32::from(lparam as u16 as i16), i32::from((lparam >> 16) as u16 as i16));
            with_window(|state| state.on_click(x, y));
            0
        }
        WM_MOUSEWHEEL => {
            let notches = i32::from((wparam >> 16) as u16 as i16) / WHEEL_NOTCH;
            with_window(|state| state.on_wheel(notches));
            0
        }
        WM_SETCURSOR if (lparam & 0xFFFF) as u32 == HTCLIENT => {
            let mut is_clickable = false;
            with_window(|state| is_clickable = state.hover.is_some());
            SetCursor(LoadCursorW(std::ptr::null_mut(), if is_clickable { IDC_HAND } else { IDC_ARROW }));
            1
        }
        WM_KEYDOWN if wparam as u16 == VK_ESCAPE => {
            PostMessageW(window, WM_CLOSE, 0, 0);
            0
        }
        WM_SETTINGCHANGE => {
            with_window(|state| {
                state.theme = current_theme();
                apply_title_bar(state.window, &state.theme);
                state.invalidate();
            });
            0
        }
        WM_DPICHANGED => {
            let dpi = (wparam & 0xFFFF) as u32;
            with_window(|state| {
                state.dpi = dpi;
                state.fonts = Fonts::new(dpi, &state.settings.borrow());
                state.canvas = Canvas::new(state.px(WIDTH), state.px(HEIGHT));
            });
            let suggested = &*(lparam as *const RECT);
            SetWindowPos(
                window,
                std::ptr::null_mut(),
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            0
        }
        WM_DESTROY => {
            OPEN_WINDOW.with(|cell| {
                if let Ok(mut state) = cell.try_borrow_mut() {
                    state.take();
                }
            });
            SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
            0
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

/// Shows the open window in a chosen theme, for previews and screenshots.
pub fn preview_theme(is_dark: bool) {
    with_window(|state| {
        state.theme = if is_dark { dark_theme() } else { light_theme() };
        apply_title_bar(state.window, &state.theme);
        state.invalidate();
    });
}

/// Closes the window, if it is open.
pub fn close() {
    let mut open_window = None;
    with_window(|state| open_window = Some(state.window));
    if let Some(window) = open_window {
        // SAFETY: the handle came from the live window state.
        unsafe { PostMessageW(window, WM_CLOSE, 0, 0) };
    }
}

/// Opens Settings just above the taskbar, or brings it forward if it is already open.
pub fn open(settings: Rc<RefCell<Settings>>, anchor: HWND, keep_on_top: bool) {
    let mut existing = None;
    with_window(|state| existing = Some(state.window));
    if let Some(window) = existing {
        // SAFETY: the handle came from the live window state.
        unsafe { SetForegroundWindow(window) };
        return;
    }
    let class_name = to_wide(CLASS_NAME);
    let title = to_wide(WINDOW_TITLE);
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
    let mut adapters: Vec<(u64, String)> =
        list_adapters().into_iter().filter(|row| row.is_physical).map(|row| (row.luid, row.alias)).collect();
    adapters.sort_by(|first, second| first.1.cmp(&second.1));
    // SAFETY: Win32 window setup on the main thread. All strings and structs outlive their calls.
    unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: module,
            hIcon: app_icon(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&class);
        let dpi = GetDpiForWindow(anchor).max(BASE_DPI as u32);
        let scale = |value: i32| (value as f32 * dpi as f32 / BASE_DPI).round() as i32;
        let mut frame = RECT { left: 0, top: 0, right: scale(WIDTH), bottom: scale(HEIGHT) };
        AdjustWindowRectExForDpi(&mut frame, style, 0, 0, dpi);
        let (outer_width, outer_height) = (frame.right - frame.left, frame.bottom - frame.top);
        let mut monitor: MONITORINFO = std::mem::zeroed();
        monitor.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(MonitorFromWindow(anchor, MONITOR_DEFAULTTONEAREST), &mut monitor);
        let x = monitor.rcWork.right - outer_width - scale(SCREEN_MARGIN);
        let y = monitor.rcWork.bottom - outer_height - scale(SCREEN_MARGIN);
        let window = CreateWindowExW(
            if keep_on_top { WS_EX_TOPMOST } else { 0 },
            class_name.as_ptr(),
            title.as_ptr(),
            style,
            x,
            y,
            outer_width,
            outer_height,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            module,
            std::ptr::null(),
        );
        let fonts = Fonts::new(dpi, &settings.borrow());
        let state = SettingsWindow {
            window,
            anchor,
            settings,
            canvas: Canvas::new(scale(WIDTH), scale(HEIGHT)),
            fonts,
            theme: current_theme(),
            dpi,
            page: 0,
            scroll: 0,
            content_height: 0,
            hits: Vec::new(),
            hover: None,
            adapters,
            is_confirming_reset: false,
            is_tracking_mouse: false,
        };
        apply_title_bar(window, &state.theme);
        OPEN_WINDOW.with(|cell| *cell.borrow_mut() = Some(state));
        ShowWindow(window, SW_SHOW);
        SetForegroundWindow(window);
        SetFocus(window);
    }
}
