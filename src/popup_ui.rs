//! The right-click menu and the hover card, drawn in the same style as the usage window.

use crate::calendar::today;
use crate::canvas::{create_font, mix, Align, Canvas};
use crate::format::format_bytes;
use crate::settings::{WM_APP_EXIT, WM_APP_OPEN_SETTINGS, WM_APP_OPEN_USAGE};
use crate::taskbar_dock::to_wide;
use crate::theme::{apply_popup_frame, current_theme, figure_colors, Theme};
use crate::usage_store::{Period, UsageStore};
use std::cell::RefCell;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, DeleteObject, EndPaint, InvalidateRect, HFONT, PAINTSTRUCT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_DOWN, VK_ESCAPE, VK_RETURN, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowRect, LoadCursorW, PostMessageW,
    RegisterClassW, SetCursor, SetForegroundWindow, ShowWindow, CS_DROPSHADOW, HTCLIENT, IDC_ARROW,
    IDC_HAND, SW_SHOW, SW_SHOWNOACTIVATE, WM_ACTIVATE, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SETCURSOR, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

// The windows-sys crate does not export this message, so it is declared here.
const WM_MOUSELEAVE: u32 = 0x02A3;

const CLASS_NAME: &str = "NetSpeedGaugePopup";
const BASE_DPI: f32 = 96.0;
const MENU_WIDTH: i32 = 164;
const CARD_WIDTH: i32 = 216;
const SUMMARY_HEIGHT: i32 = 34;
const CARD_HEIGHT: i32 = 124;
const ROW_HEIGHT: i32 = 32;
const DIVIDER_HEIGHT: i32 = 9;
const PADDING: i32 = 5;
const GAP_ABOVE_TASKBAR: i32 = 10;
const ACTIVE_NONE: u32 = 0;
const ARROW_UP: &str = "\u{2191}";
const ARROW_DOWN: &str = "\u{2193}";

/// One thing the menu can do, with its icon, label and the message it sends.
const MENU_ITEMS: [(&str, &str, u32); 3] = [
    ("\u{E9D2}", "Usage", WM_APP_OPEN_USAGE),
    ("\u{E713}", "Settings", WM_APP_OPEN_SETTINGS),
    ("\u{E7E8}", "Exit", WM_APP_EXIT),
];
const EXIT_ROW: usize = 2;

struct PopupFonts {
    body: HFONT,
    strong: HFONT,
    small: HFONT,
    value: HFONT,
    icon: HFONT,
    arrow: HFONT,
}

impl PopupFonts {
    fn new(dpi: u32) -> Self {
        Self {
            body: create_font("Segoe UI", 10.0, 400, dpi),
            strong: create_font("Segoe UI Semibold", 10.0, 400, dpi),
            small: create_font("Segoe UI", 9.0, 400, dpi),
            value: create_font("Segoe UI Semibold", 14.0, 400, dpi),
            icon: create_font("Segoe MDL2 Assets", 11.0, 400, dpi),
            arrow: create_font("Segoe UI", 10.0, 900, dpi),
        }
    }
}

impl Drop for PopupFonts {
    fn drop(&mut self) {
        // SAFETY: each font was created in new() and is released once here.
        unsafe {
            for font in [self.body, self.strong, self.small, self.value, self.icon, self.arrow] {
                DeleteObject(font);
            }
        }
    }
}

/// Today's and this month's totals, read once when the popup opens.
#[derive(Clone, Copy)]
struct Totals {
    today_up: u64,
    today_down: u64,
    month_up: u64,
    month_down: u64,
}

fn read_totals(store: &UsageStore) -> Totals {
    let day = today();
    let today_bucket = store.buckets(Period::Day, None, day)[Period::Day.bucket_count() - 1];
    let month_bucket = store.buckets(Period::Month, None, day)[Period::Month.bucket_count() - 1];
    Totals {
        today_up: today_bucket.up,
        today_down: today_bucket.down,
        month_up: month_bucket.up,
        month_down: month_bucket.down,
    }
}

/// The open popup. `is_menu` tells the menu from the hover card.
struct Popup {
    window: HWND,
    anchor: HWND,
    canvas: Canvas,
    fonts: PopupFonts,
    theme: Theme,
    dpi: u32,
    width: i32,
    totals: Totals,
    is_menu: bool,
    hover_row: Option<usize>,
    is_tracking_mouse: bool,
}

thread_local! {
    static OPEN_POPUP: RefCell<Option<Popup>> = const { RefCell::new(None) };
}

fn with_popup(action: impl FnOnce(&mut Popup)) {
    OPEN_POPUP.with(|cell| {
        if let Ok(mut state) = cell.try_borrow_mut() {
            if let Some(popup) = state.as_mut() {
                action(popup);
            }
        }
    });
}

impl Popup {
    fn px(&self, value: i32) -> i32 {
        (value as f32 * self.dpi as f32 / BASE_DPI).round() as i32
    }

    /// Returns the y position of a menu row. Exit sits under a divider.
    fn row_top(&self, row: usize) -> i32 {
        let divider = if row >= EXIT_ROW { DIVIDER_HEIGHT } else { 0 };
        self.px(SUMMARY_HEIGHT + PADDING + row as i32 * ROW_HEIGHT + divider)
    }

    fn row_at(&self, y: i32) -> Option<usize> {
        if !self.is_menu {
            return None;
        }
        (0..MENU_ITEMS.len()).find(|&row| y >= self.row_top(row) && y < self.row_top(row) + self.px(ROW_HEIGHT))
    }

    /// Draws the labelled download and upload totals used by both popups.
    fn draw_totals(&mut self, y: i32, label: &str, up: u64, down: u64, arrow_right: i32) {
        let theme = self.theme;
        let left = self.px(16);
        let right = self.width - self.px(16);
        self.canvas.text(left, y, label, self.fonts.small, theme.muted, Align::Left);
        let total = format_bytes(up + down);
        self.canvas.text(left, y + self.px(16), &total, self.fonts.value, theme.text, Align::Left);
        let (upload, download) = figure_colors(&theme, theme.surface);
        let rows = [
            (ARROW_DOWN, download, down, y + self.px(4)),
            (ARROW_UP, upload, up, y + self.px(24)),
        ];
        for (arrow, color, bytes, row_y) in rows {
            self.canvas.text(right, row_y, &format_bytes(bytes), self.fonts.small, theme.text, Align::Right);
            // Drawn twice, one pixel apart, because this font's arrows stay thin even at its heaviest weight.
            for offset in 0..=1 {
                self.canvas.text(arrow_right + offset, row_y, arrow, self.fonts.arrow, color, Align::Right);
            }
        }
    }

    fn draw(&mut self) {
        let theme = self.theme;
        let totals = self.totals;
        self.canvas.clear(theme.surface);
        if self.is_menu {
            let today_total = format_bytes(totals.today_up + totals.today_down);
            self.canvas.text(self.px(14), self.px(9), "Today", self.fonts.small, theme.muted, Align::Left);
            self.canvas.text(self.width - self.px(14), self.px(8), &today_total, self.fonts.strong, theme.text, Align::Right);
        } else {
            // Every arrow shares one column, set by the widest number, so they line up at any speed.
            let values = [totals.today_up, totals.today_down, totals.month_up, totals.month_down];
            let widest = values
                .iter()
                .map(|bytes| self.canvas.text_width(&format_bytes(*bytes), self.fonts.small))
                .max()
                .unwrap_or(0);
            let arrow_right = self.width - self.px(16) - widest - self.px(10);
            self.draw_totals(self.px(12), "Today", totals.today_up, totals.today_down, arrow_right);
            let (x, y) = (self.px(16), self.px(62));
            self.canvas.round_rect(x, y, self.width - x * 2, self.px(1).max(1), 0.0, theme.border);
            self.draw_totals(self.px(72), "This month", totals.month_up, totals.month_down, arrow_right);
            return;
        }
        let divider_x = self.px(PADDING);
        let divider_width = self.width - divider_x * 2;
        self.canvas.round_rect(divider_x, self.px(SUMMARY_HEIGHT), divider_width, self.px(1).max(1), 0.0, theme.border);
        for (row, (icon, label, _)) in MENU_ITEMS.iter().enumerate() {
            let top = self.row_top(row);
            if self.hover_row == Some(row) {
                let hover = mix(theme.surface, theme.text, 0.09);
                self.canvas.round_rect(divider_x, top, divider_width, self.px(ROW_HEIGHT), self.px(5) as f32, hover);
            }
            let is_hover = self.hover_row == Some(row);
            let icon_color = if is_hover { theme.text } else { theme.muted };
            self.canvas.text(self.px(14), top + self.px(8), icon, self.fonts.icon, icon_color, Align::Left);
            let font = if is_hover { self.fonts.strong } else { self.fonts.body };
            self.canvas.text(self.px(40), top + self.px(7), label, font, theme.text, Align::Left);
        }
        let divider_y = self.row_top(EXIT_ROW) - self.px(DIVIDER_HEIGHT / 2 + 1);
        self.canvas.round_rect(divider_x, divider_y, divider_width, self.px(1).max(1), 0.0, theme.border);
    }

    fn invalidate(&self) {
        // SAFETY: the window is valid while this state exists.
        unsafe { InvalidateRect(self.window, std::ptr::null(), 0) };
    }

    fn set_hover(&mut self, row: Option<usize>) {
        if row != self.hover_row {
            self.hover_row = row;
            self.invalidate();
        }
    }

    /// Moves the keyboard highlight, wrapping around the ends.
    fn move_hover(&mut self, step: usize) {
        let count = MENU_ITEMS.len();
        let next = self.hover_row.map_or(if step == 1 { 0 } else { count - 1 }, |row| (row + step) % count);
        self.set_hover(Some(next));
    }

    /// Closes the menu and asks the app to do what the chosen row says.
    fn choose(&self, row: usize) {
        let (_, _, message) = MENU_ITEMS[row];
        // SAFETY: both windows are valid. The close is posted, so it runs after this state is released.
        unsafe {
            PostMessageW(self.anchor, message, 0, 0);
            PostMessageW(self.window, WM_APP_CLOSE_POPUP, 0, 0);
        }
    }
}

const WM_APP_CLOSE_POPUP: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 20;

/// Handles the popup's messages.
unsafe extern "system" fn popup_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_PAINT => {
            let mut paint: PAINTSTRUCT = std::mem::zeroed();
            let target = BeginPaint(window, &mut paint);
            with_popup(|popup| {
                popup.draw();
                popup.canvas.present(target);
            });
            EndPaint(window, &paint);
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEMOVE => {
            let y = i32::from((lparam >> 16) as u16 as i16);
            with_popup(|popup| {
                if !popup.is_tracking_mouse {
                    let mut request = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: popup.window,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut request);
                    popup.is_tracking_mouse = true;
                }
                let row = popup.row_at(y);
                popup.set_hover(row);
            });
            0
        }
        WM_MOUSELEAVE => {
            with_popup(|popup| {
                popup.is_tracking_mouse = false;
                popup.set_hover(None);
            });
            0
        }
        WM_LBUTTONUP => {
            let y = i32::from((lparam >> 16) as u16 as i16);
            with_popup(|popup| {
                if let Some(row) = popup.row_at(y) {
                    popup.choose(row);
                }
            });
            0
        }
        WM_SETCURSOR if (lparam & 0xFFFF) as u32 == HTCLIENT => {
            let mut is_clickable = false;
            with_popup(|popup| is_clickable = popup.hover_row.is_some());
            SetCursor(LoadCursorW(std::ptr::null_mut(), if is_clickable { IDC_HAND } else { IDC_ARROW }));
            1
        }
        WM_KEYDOWN => {
            match wparam as u16 {
                VK_ESCAPE => {
                    PostMessageW(window, WM_APP_CLOSE_POPUP, 0, 0);
                }
                VK_DOWN => with_popup(|popup| popup.move_hover(1)),
                VK_UP => with_popup(|popup| popup.move_hover(MENU_ITEMS.len() - 1)),
                VK_RETURN => with_popup(|popup| {
                    if let Some(row) = popup.hover_row {
                        popup.choose(row);
                    }
                }),
                _ => {}
            }
            0
        }
        // A menu closes as soon as the user clicks anywhere else.
        WM_ACTIVATE if (wparam & 0xFFFF) as u32 == ACTIVE_NONE => {
            PostMessageW(window, WM_APP_CLOSE_POPUP, 0, 0);
            0
        }
        WM_APP_CLOSE_POPUP => {
            DestroyWindow(window);
            0
        }
        WM_DESTROY => {
            OPEN_POPUP.with(|cell| {
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

/// Opens a popup just above the readout.
fn open(store: &UsageStore, anchor: HWND, is_menu: bool) {
    close();
    let totals = read_totals(store);
    let class_name = to_wide(CLASS_NAME);
    let title = to_wide("NetSpeedGauge");
    let base_height = if is_menu {
        SUMMARY_HEIGHT + PADDING * 2 + ROW_HEIGHT * MENU_ITEMS.len() as i32 + DIVIDER_HEIGHT
    } else {
        CARD_HEIGHT
    };
    // SAFETY: Win32 window setup on the main thread. All strings and structs outlive their calls.
    unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: CS_DROPSHADOW,
            lpfnWndProc: Some(popup_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: module,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&class);
        let dpi = GetDpiForWindow(anchor).max(BASE_DPI as u32);
        let scale = |value: i32| (value as f32 * dpi as f32 / BASE_DPI).round() as i32;
        let (width, height) = (scale(if is_menu { MENU_WIDTH } else { CARD_WIDTH }), scale(base_height));
        let mut anchor_area = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        GetWindowRect(anchor, &mut anchor_area);
        let x = anchor_area.right - width;
        let y = anchor_area.top - height - scale(GAP_ABOVE_TASKBAR);
        let extra_style = if is_menu { 0 } else { WS_EX_NOACTIVATE };
        let window = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | extra_style,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            x,
            y,
            width,
            height,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            module,
            std::ptr::null(),
        );
        let theme = current_theme();
        apply_popup_frame(window, &theme);
        let popup = Popup {
            window,
            anchor,
            canvas: Canvas::new(width, height),
            fonts: PopupFonts::new(dpi),
            theme,
            dpi,
            width,
            totals,
            is_menu,
            hover_row: None,
            is_tracking_mouse: false,
        };
        OPEN_POPUP.with(|cell| *cell.borrow_mut() = Some(popup));
        if is_menu {
            ShowWindow(window, SW_SHOW);
            SetForegroundWindow(window);
            SetFocus(window);
        } else {
            ShowWindow(window, SW_SHOWNOACTIVATE);
        }
    }
}

/// Opens the right-click menu.
pub fn show_menu(store: &UsageStore, anchor: HWND) {
    open(store, anchor, true);
}

/// Opens the hover card with today's and this month's totals, unless a menu is showing.
pub fn show_hover_card(store: &UsageStore, anchor: HWND) {
    let mut is_open = false;
    with_popup(|_| is_open = true);
    if !is_open {
        open(store, anchor, false);
    }
}

/// Closes the hover card, leaving a menu open.
pub fn hide_hover_card() {
    with_popup(|popup| {
        if !popup.is_menu {
            // SAFETY: the window is valid while this state exists.
            unsafe { PostMessageW(popup.window, WM_APP_CLOSE_POPUP, 0, 0) };
        }
    });
}

/// Closes whichever popup is open. The window is destroyed after this state is released.
pub fn close() {
    let mut window = None;
    with_popup(|popup| window = Some(popup.window));
    if let Some(window) = window {
        // SAFETY: the handle came from the live popup state.
        unsafe { DestroyWindow(window) };
    }
}
