//! The usage window. It exists only while open, then frees its memory.

use crate::calendar::{date_from_day, short_date, today, MONTH_NAMES};
use crate::canvas::{create_font, mix, Align, Canvas};
use crate::format::format_bytes;
use crate::settings::WM_APP_OPEN_SETTINGS;
use crate::taskbar_dock::to_wide;
use crate::theme::{app_icon, apply_title_bar, current_theme, dark_theme, draw_footer_strip, figure_colors, light_theme, Theme, FOOTER_HEIGHT};
use crate::usage_store::{Bucket, Period, UsageStore};
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
    SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_ESCAPE, VK_LEFT, VK_RIGHT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, LoadCursorW, PostMessageW, RegisterClassW, SetCursor,
    SetForegroundWindow, SetWindowPos, ShowWindow, HTCLIENT, IDC_ARROW, IDC_HAND, SWP_NOACTIVATE,
    SWP_NOZORDER, SW_SHOW, WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_PAINT, WM_SETCURSOR, WM_SETTINGCHANGE, WNDCLASSW, WS_CAPTION, WS_MINIMIZEBOX,
    WS_EX_TOPMOST, WS_OVERLAPPED, WS_SYSMENU,
};

// The windows-sys crate does not export this message, so it is declared here.
const WM_MOUSELEAVE: u32 = 0x02A3;

const CLASS_NAME: &str = "NetSpeedGaugeUsage";
const WINDOW_TITLE: &str = "NetSpeedGauge Usage";
const BASE_DPI: f32 = 96.0;
const WIDTH: i32 = 440;
const HEIGHT: i32 = 558;
const MARGIN: i32 = 20;
const SCREEN_MARGIN: i32 = 12;
const CARD_RADIUS: f32 = 8.0;
const CONTROL_RADIUS: f32 = 6.0;
const MENU_WIDTH: i32 = 190;
const MENU_ROW: i32 = 32;
const MAX_NAME_CHARS: usize = 22;
const ICON_SETTINGS: &str = "\u{E713}";
const PERIODS: [(Period, &str); 4] =
    [(Period::Day, "Day"), (Period::Week, "Week"), (Period::Month, "Month"), (Period::Year, "Year")];
const ICON_CHEVRON_DOWN: &str = "\u{E70D}";
const ICON_CHECK: &str = "\u{E73E}";
const ALL_ADAPTERS: &str = "All adapters";

/// The fonts the window uses, released when it closes.
struct Fonts {
    body: HFONT,
    strong: HFONT,
    small: HFONT,
    hero: HFONT,
    value: HFONT,
    icon: HFONT,
    large_icon: HFONT,
    heart: HFONT,
}

impl Fonts {
    fn new(dpi: u32) -> Self {
        Self {
            body: create_font("Segoe UI", 10.0, 400, dpi),
            strong: create_font("Segoe UI Semibold", 10.0, 400, dpi),
            small: create_font("Segoe UI", 9.0, 400, dpi),
            hero: create_font("Segoe UI Light", 32.0, 400, dpi),
            value: create_font("Segoe UI Semibold", 13.0, 400, dpi),
            icon: create_font("Segoe MDL2 Assets", 8.0, 400, dpi),
            large_icon: create_font("Segoe MDL2 Assets", 12.0, 400, dpi),
            heart: create_font("Segoe MDL2 Assets", 9.0, 400, dpi),
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        // SAFETY: each font was created in new() and is released once here.
        unsafe {
            for font in [self.body, self.strong, self.small, self.hero, self.value, self.icon, self.large_icon, self.heart] {
                DeleteObject(font);
            }
        }
    }
}

/// A rectangle in window pixels.
#[derive(Clone, Copy)]
struct Area {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl Area {
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// Returns the words for one chart bar, such as "Today, 3 Nov".
pub fn period_label(period: Period, start: i32, today_day: i32) -> String {
    let (year, month, _) = date_from_day(start);
    match period {
        Period::Day if start == today_day => format!("Today, {}", short_date(start)),
        Period::Day if start == today_day - 1 => format!("Yesterday, {}", short_date(start)),
        Period::Day => format!("{} {year}", short_date(start)),
        Period::Week => {
            let range = format!("{} to {}", short_date(start), short_date(start + 6));
            if period.start_of(today_day) == start { format!("This week, {range}") } else { range }
        }
        Period::Month => format!("{} {year}", MONTH_NAMES[month as usize - 1]),
        Period::Year => year.to_string(),
    }
}

/// Returns the chart's heading.
pub fn chart_title(period: Period) -> &'static str {
    match period {
        Period::Day => "Last 30 days",
        Period::Week => "Last 12 weeks",
        Period::Month => "Last 12 months",
        Period::Year => "Last 2 years",
    }
}

/// Returns the short label under a chart bar.
pub fn axis_label(period: Period, start: i32) -> String {
    let (year, month, _) = date_from_day(start);
    match period {
        Period::Day | Period::Week => short_date(start),
        Period::Month => MONTH_NAMES[month as usize - 1][..3].to_string(),
        Period::Year => year.to_string(),
    }
}

/// Returns true for the bars that get a label, evenly spaced so they never collide.
pub fn shows_axis_label(period: Period, position: usize, count: usize) -> bool {
    let last = count - 1;
    match period {
        Period::Day => (last - position) % 7 == 0,
        Period::Week | Period::Month => (last - position) % 3 == 0,
        Period::Year => true,
    }
}

/// Everything the open window keeps.
struct UsageWindow {
    window: HWND,
    anchor: HWND,
    store: Rc<RefCell<UsageStore>>,
    canvas: Canvas,
    fonts: Fonts,
    theme: Theme,
    dpi: u32,
    period: Period,
    adapter: Option<u32>,
    // How many bars back from the newest the selected bar is, so it stays right across midnight.
    selected_back: usize,
    hover_bar: Option<usize>,
    hover_control: Option<Control>,
    is_menu_open: bool,
    is_tracking_mouse: bool,
}

/// Something clickable other than a chart bar.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    Period(usize),
    Adapter,
    Settings,
    MenuRow(usize),
}

thread_local! {
    static OPEN_WINDOW: RefCell<Option<UsageWindow>> = const { RefCell::new(None) };
}

impl UsageWindow {
    /// Converts a size at 100% scaling to pixels at the current scaling.
    fn px(&self, value: i32) -> i32 {
        (value as f32 * self.dpi as f32 / BASE_DPI).round() as i32
    }

    fn period_area(&self, position: usize) -> Area {
        let width = self.px(58);
        Area { x: self.px(MARGIN) + position as i32 * width, y: self.px(16), width, height: self.px(32) }
    }

    fn settings_area(&self) -> Area {
        let size = self.px(32);
        Area { x: self.px(WIDTH - MARGIN) - size, y: self.px(16), width: size, height: size }
    }

    fn adapter_area(&self) -> Area {
        let width = self.px(120);
        Area { x: self.settings_area().x - self.px(8) - width, y: self.px(16), width, height: self.px(32) }
    }

    fn hero_area(&self) -> Area {
        Area { x: self.px(MARGIN), y: self.px(60), width: self.px(WIDTH - MARGIN * 2), height: self.px(150) }
    }

    fn chart_area(&self) -> Area {
        Area { x: self.px(MARGIN), y: self.px(222), width: self.px(WIDTH - MARGIN * 2), height: self.px(272) }
    }

    fn bars_area(&self) -> Area {
        let chart = self.chart_area();
        Area {
            x: chart.x + self.px(14),
            y: chart.y + self.px(46),
            width: chart.width - self.px(28),
            height: chart.height - self.px(46 + 34),
        }
    }

    fn menu_row_area(&self, position: usize) -> Area {
        let adapter = self.adapter_area();
        let width = self.px(MENU_WIDTH);
        Area {
            x: adapter.x + adapter.width - width,
            y: adapter.y + adapter.height + self.px(6) + position as i32 * self.px(MENU_ROW),
            width,
            height: self.px(MENU_ROW),
        }
    }

    /// Returns the name of the adapter choice at this position in the menu.
    fn menu_label(&self, position: usize) -> String {
        if position == 0 {
            return ALL_ADAPTERS.to_string();
        }
        let store = self.store.borrow();
        let name = store.adapters.get(position - 1).map_or("", |adapter| adapter.name.as_str());
        name.chars().take(MAX_NAME_CHARS).collect()
    }

    fn menu_len(&self) -> usize {
        self.store.borrow().adapters.len() + 1
    }

    /// Returns the clickable control under the mouse.
    fn control_at(&self, x: i32, y: i32) -> Option<Control> {
        if self.is_menu_open {
            let hit = (0..self.menu_len()).find(|&row| self.menu_row_area(row).contains(x, y));
            if let Some(row) = hit {
                return Some(Control::MenuRow(row));
            }
        }
        if self.adapter_area().contains(x, y) {
            return Some(Control::Adapter);
        }
        if self.settings_area().contains(x, y) {
            return Some(Control::Settings);
        }
        (0..PERIODS.len()).find(|&position| self.period_area(position).contains(x, y)).map(Control::Period)
    }

    /// Returns which chart bar is under the mouse.
    fn bar_at(&self, x: i32, y: i32) -> Option<usize> {
        let bars = self.bars_area();
        if !bars.contains(x, y) || self.is_menu_open {
            return None;
        }
        let count = self.period.bucket_count() as i32;
        Some((((x - bars.x) * count) / bars.width) as usize)
    }

    /// Paints the whole window onto the off-screen canvas.
    fn draw(&mut self) {
        let theme = self.theme;
        let today_day = today();
        let buckets = self.store.borrow().buckets(self.period, self.adapter, today_day);
        let last = buckets.len() - 1;
        let selected = last.saturating_sub(self.selected_back);
        self.canvas.clear(theme.background);
        self.draw_period_control();
        self.draw_adapter_control();
        self.draw_settings_button();
        self.draw_hero(&buckets[selected], today_day);
        self.draw_chart(&buckets, selected);
        self.draw_footer();
        if self.is_menu_open {
            self.draw_menu();
        }
    }

    fn draw_period_control(&mut self) {
        let theme = self.theme;
        let first = self.period_area(0);
        let radius = self.px(CONTROL_RADIUS as i32) as f32;
        let track_width = first.width * PERIODS.len() as i32;
        self.canvas.round_rect(first.x, first.y, track_width, first.height, radius, theme.control);
        let inset = self.px(3);
        for (position, (period, label)) in PERIODS.iter().enumerate() {
            let area = self.period_area(position);
            let is_selected = *period == self.period;
            let is_hover = self.hover_control == Some(Control::Period(position));
            if is_selected {
                self.canvas.round_rect(
                    area.x + inset,
                    area.y + inset,
                    area.width - inset * 2,
                    area.height - inset * 2,
                    radius - inset as f32 / 2.0,
                    theme.selected,
                );
            }
            let color = if is_selected || is_hover { theme.text } else { theme.muted };
            let font = if is_selected { self.fonts.strong } else { self.fonts.body };
            self.canvas.text(area.x + area.width / 2, area.y + self.px(7), label, font, color, Align::Center);
        }
    }

    fn draw_adapter_control(&mut self) {
        let theme = self.theme;
        let area = self.adapter_area();
        let radius = self.px(CONTROL_RADIUS as i32) as f32;
        let is_active = self.hover_control == Some(Control::Adapter) || self.is_menu_open;
        let fill = if is_active { mix(theme.control, theme.selected, 0.6) } else { theme.control };
        self.canvas.round_rect(area.x, area.y, area.width, area.height, radius, fill);
        let label = self.menu_label(self.adapter.map_or(0, |adapter| adapter as usize + 1));
        let label: String = label.chars().take(16).collect();
        self.canvas.text(area.x + self.px(12), area.y + self.px(7), &label, self.fonts.body, theme.text, Align::Left);
        let chevron_x = area.x + area.width - self.px(12);
        self.canvas.text(chevron_x, area.y + self.px(11), ICON_CHEVRON_DOWN, self.fonts.icon, theme.muted, Align::Right);
    }

    fn draw_settings_button(&mut self) {
        let theme = self.theme;
        let area = self.settings_area();
        let is_hover = self.hover_control == Some(Control::Settings);
        let fill = if is_hover { mix(theme.control, theme.selected, 0.6) } else { theme.control };
        self.canvas.round_rect(area.x, area.y, area.width, area.height, self.px(CONTROL_RADIUS as i32) as f32, fill);
        let color = if is_hover { theme.text } else { theme.muted };
        self.canvas.text(area.x + area.width / 2, area.y + self.px(8), ICON_SETTINGS, self.fonts.large_icon, color, Align::Center);
    }

    fn draw_hero(&mut self, bucket: &Bucket, today_day: i32) {
        let theme = self.theme;
        let area = self.hero_area();
        let (pad, column) = (self.px(18), area.width / 2);
        self.canvas.round_rect(area.x, area.y, area.width, area.height, self.px(CARD_RADIUS as i32) as f32, theme.card);
        let label = period_label(self.period, bucket.start, today_day);
        self.canvas.text(area.x + pad, area.y + self.px(14), &label, self.fonts.body, theme.muted, Align::Left);
        let total = format_bytes(bucket.up + bucket.down);
        self.canvas.text(area.x + pad - self.px(2), area.y + self.px(32), &total, self.fonts.hero, theme.text, Align::Left);
        let stats_y = area.y + self.px(94);
        let (upload, download) = figure_colors(&theme, theme.card);
        let stats = [
            (area.x + pad, download, "Download", bucket.down),
            (area.x + column, upload, "Upload", bucket.up),
        ];
        for (x, color, name, bytes) in stats {
            let dot = self.px(8);
            self.canvas.round_rect(x, stats_y + self.px(5), dot, dot, dot as f32 / 2.0, color);
            self.canvas.text(x + self.px(14), stats_y, name, self.fonts.small, theme.muted, Align::Left);
            self.canvas.text(x + self.px(14), stats_y + self.px(17), &format_bytes(bytes), self.fonts.value, theme.text, Align::Left);
        }
    }

    fn draw_chart(&mut self, buckets: &[Bucket], selected: usize) {
        let theme = self.theme;
        let card = self.chart_area();
        let pad = self.px(14);
        self.canvas.round_rect(card.x, card.y, card.width, card.height, self.px(CARD_RADIUS as i32) as f32, theme.card);
        self.canvas.text(card.x + pad, card.y + self.px(13), chart_title(self.period), self.fonts.strong, theme.text, Align::Left);
        let peak = buckets.iter().map(|bucket| bucket.up + bucket.down).max().unwrap_or(0);
        if peak > 0 {
            let peak_label = format!("Peak {}", format_bytes(peak));
            self.canvas.text(card.x + card.width - pad, card.y + self.px(14), &peak_label, self.fonts.small, theme.muted, Align::Right);
        }

        let (upload, download) = figure_colors(&theme, theme.card);
        let bars = self.bars_area();
        let baseline = bars.y + bars.height;
        self.canvas.round_rect(bars.x, baseline, bars.width, self.px(1).max(1), 0.0, theme.border);
        let count = buckets.len() as i32;
        let slot = bars.width as f32 / count as f32;
        let bar_width = ((slot * 0.62).round() as i32).clamp(self.px(4), self.px(44));
        let usable_height = bars.height - self.px(4);
        let gap = self.px(1);
        for (position, bucket) in buckets.iter().enumerate() {
            let center = bars.x as f32 + slot * (position as f32 + 0.5);
            let x = (center - bar_width as f32 / 2.0).round() as i32;
            let is_selected = position == selected;
            let is_hover = self.hover_bar == Some(position);
            let strength = if is_selected { 1.0 } else if is_hover { 0.85 } else { 0.62 };
            let total = bucket.up + bucket.down;
            if is_selected {
                let tint = mix(theme.card, theme.text, 0.07);
                self.canvas.round_rect(
                    (bars.x as f32 + slot * position as f32).round() as i32,
                    bars.y,
                    slot.round() as i32,
                    bars.height,
                    self.px(4) as f32,
                    tint,
                );
            }
            if total == 0 {
                let color = mix(theme.card, theme.muted, strength * 0.5);
                self.canvas.round_rect(x, baseline - self.px(3), bar_width, self.px(3), self.px(1) as f32, color);
            } else {
                let height = ((total as f32 / peak as f32) * usable_height as f32).round().max(self.px(3) as f32) as i32;
                let upload_height = ((bucket.up as f32 / total as f32) * height as f32).round() as i32;
                let top = baseline - height;
                let radius = (bar_width as f32 / 2.0).min(self.px(3) as f32);
                let download_color = mix(theme.card, download, strength);
                self.canvas.round_rect(x, top, bar_width, height, radius, download_color);
                if upload_height >= self.px(2) {
                    let upload_color = mix(theme.card, upload, strength);
                    self.canvas.round_rect(x, top, bar_width, upload_height, radius, upload_color);
                    self.canvas.round_rect(x, top + upload_height, bar_width, gap, 0.0, download_color);
                }
            }
            if shows_axis_label(self.period, position, buckets.len()) {
                let label = axis_label(self.period, bucket.start);
                let color = if is_selected { theme.text } else { theme.muted };
                self.canvas.text(center.round() as i32, baseline + self.px(9), &label, self.fonts.small, color, Align::Center);
            }
        }
    }

    fn draw_footer(&mut self) {
        let theme = self.theme;
        let store = self.store.borrow();
        let kept = match store.keep_days {
            days if days > 0 && days < 100 => "3 months",
            days if days > 0 && days < 200 => "6 months",
            days if days > 0 && days < 400 => "1 year",
            _ => "2 years",
        };
        let note = match store.first_day() {
            Some(day) => {
                let (year, _, _) = date_from_day(day);
                format!("Counting since {} {year}. Kept for {kept}, only on this PC.", short_date(day))
            }
            None => format!("Counting has just started. Kept for {kept}, only on this PC."),
        };
        drop(store);
        self.canvas.text(self.px(WIDTH / 2), self.px(HEIGHT - FOOTER_HEIGHT - 28), &note, self.fonts.small, theme.muted, Align::Center);
        let (top, width, height) = (self.px(HEIGHT - FOOTER_HEIGHT), self.px(WIDTH), self.px(FOOTER_HEIGHT));
        draw_footer_strip(&mut self.canvas, &theme, top, width, height, self.fonts.small, self.fonts.heart);
    }

    fn draw_menu(&mut self) {
        let theme = self.theme;
        let rows = self.menu_len();
        let first = self.menu_row_area(0);
        let (height, pad) = (first.height * rows as i32, self.px(4));
        let radius = self.px(CARD_RADIUS as i32) as f32;
        let (outer_x, outer_y) = (first.x - pad, first.y - pad);
        let (outer_width, outer_height) = (first.width + pad * 2, height + pad * 2);
        self.canvas.round_rect(outer_x - 1, outer_y - 1, outer_width + 2, outer_height + 2, radius + 1.0, theme.border);
        let surface = theme.surface;
        self.canvas.round_rect(outer_x, outer_y, outer_width, outer_height, radius, surface);
        let current = self.adapter.map_or(0, |adapter| adapter as usize + 1);
        for row in 0..rows {
            let area = self.menu_row_area(row);
            if self.hover_control == Some(Control::MenuRow(row)) {
                let hover = mix(surface, theme.text, 0.09);
                self.canvas.round_rect(area.x, area.y, area.width, area.height, self.px(CONTROL_RADIUS as i32 - 2) as f32, hover);
            }
            let label = self.menu_label(row);
            self.canvas.text(area.x + self.px(30), area.y + self.px(7), &label, self.fonts.body, theme.text, Align::Left);
            if row == current {
                self.canvas.text(area.x + self.px(10), area.y + self.px(11), ICON_CHECK, self.fonts.icon, theme.download, Align::Left);
            }
        }
    }

    fn invalidate(&self) {
        // SAFETY: self.window is valid while this state exists.
        unsafe { InvalidateRect(self.window, std::ptr::null(), 0) };
    }

    /// Reacts to a mouse move, redrawing only when what is highlighted changed.
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
        let control = self.control_at(x, y);
        let bar = self.bar_at(x, y);
        if control != self.hover_control || bar != self.hover_bar {
            self.hover_control = control;
            self.hover_bar = bar;
            self.invalidate();
        }
    }

    /// Reacts to a left click.
    fn on_click(&mut self, x: i32, y: i32) {
        let control = self.control_at(x, y);
        let was_menu_open = self.is_menu_open;
        self.is_menu_open = false;
        match control {
            Some(Control::Period(position)) => {
                self.period = PERIODS[position].0;
                self.selected_back = 0;
            }
            Some(Control::Adapter) => self.is_menu_open = !was_menu_open,
            Some(Control::Settings) => {
                // SAFETY: the readout window lives as long as the app.
                unsafe { PostMessageW(self.anchor, WM_APP_OPEN_SETTINGS, 0, 0) };
            }
            Some(Control::MenuRow(row)) => {
                self.adapter = row.checked_sub(1).map(|adapter| adapter as u32);
            }
            None => {
                if let Some(bar) = self.bar_at(x, y) {
                    self.selected_back = self.period.bucket_count() - 1 - bar;
                }
            }
        }
        self.invalidate();
    }

    /// Moves the selection with the arrow keys.
    fn move_selection(&mut self, towards_newer: bool) {
        let oldest = self.period.bucket_count() - 1;
        self.selected_back = if towards_newer {
            self.selected_back.saturating_sub(1)
        } else {
            (self.selected_back + 1).min(oldest)
        };
        self.invalidate();
    }
}

/// Runs a closure on the open window's state, unless it is closed or already in use.
fn with_window(action: impl FnOnce(&mut UsageWindow)) {
    OPEN_WINDOW.with(|cell| {
        if let Ok(mut state) = cell.try_borrow_mut() {
            if let Some(window) = state.as_mut() {
                action(window);
            }
        }
    });
}

fn mouse_position(lparam: LPARAM) -> (i32, i32) {
    (i32::from(lparam as u16 as i16), i32::from((lparam >> 16) as u16 as i16))
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
            let (x, y) = mouse_position(lparam);
            with_window(|state| state.on_mouse_move(x, y));
            0
        }
        WM_MOUSELEAVE => {
            with_window(|state| {
                state.is_tracking_mouse = false;
                state.hover_bar = None;
                state.hover_control = None;
                state.invalidate();
            });
            0
        }
        WM_LBUTTONUP => {
            let (x, y) = mouse_position(lparam);
            with_window(|state| state.on_click(x, y));
            0
        }
        WM_SETCURSOR if (lparam & 0xFFFF) as u32 == HTCLIENT => {
            let mut is_clickable = false;
            with_window(|state| is_clickable = state.hover_control.is_some() || state.hover_bar.is_some());
            SetCursor(LoadCursorW(std::ptr::null_mut(), if is_clickable { IDC_HAND } else { IDC_ARROW }));
            1
        }
        WM_KEYDOWN => {
            match wparam as u16 {
                VK_ESCAPE => {
                    PostMessageW(window, WM_CLOSE, 0, 0);
                }
                VK_LEFT => with_window(|state| state.move_selection(false)),
                VK_RIGHT => with_window(|state| state.move_selection(true)),
                _ => {}
            }
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
                state.fonts = Fonts::new(dpi);
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
            // Hand back the memory the window used. Anything still needed returns on its own.
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

/// Applies the user's current theme choice to the open window, if any.
pub fn apply_theme() {
    with_window(|state| {
        state.theme = current_theme();
        apply_title_bar(state.window, &state.theme);
        state.invalidate();
    });
}

/// Asks the open window, if any, to redraw with the latest totals.
pub fn refresh() {
    with_window(|state| state.invalidate());
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

/// Opens the usage window just above the taskbar, or brings it forward if it is already open.
pub fn open(store: Rc<RefCell<UsageStore>>, anchor: HWND, keep_on_top: bool) {
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
        let state = UsageWindow {
            window,
            anchor,
            store,
            canvas: Canvas::new(scale(WIDTH), scale(HEIGHT)),
            fonts: Fonts::new(dpi),
            theme: current_theme(),
            dpi,
            period: Period::Day,
            adapter: None,
            selected_back: 0,
            hover_bar: None,
            hover_control: None,
            is_menu_open: false,
            is_tracking_mouse: false,
        };
        apply_title_bar(state.window, &state.theme);
        OPEN_WINDOW.with(|cell| *cell.borrow_mut() = Some(state));
        ShowWindow(window, SW_SHOW);
        SetForegroundWindow(window);
        SetFocus(window);
    }
}
