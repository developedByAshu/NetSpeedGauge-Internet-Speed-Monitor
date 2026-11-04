//! The right-click menu and the hover card, drawn in the same style as the usage window.

use crate::app::{self, Command};
use crate::canvas::{self, mix, Align, Font, WEIGHT_BOLD, WEIGHT_REGULAR, WEIGHT_SEMIBOLD};
use crate::clock::today;
use crate::surface::{self, main_thread, GaugeView, KEY_DOWN, KEY_ESCAPE, KEY_RETURN, KEY_UP};
use crate::theme::{appearance_for, current_theme, figure_colors, Theme};
use crate::usage_ui::{self, OpenedBy};
use crate::settings_ui;
use netspeedgauge_shared::format::format_bytes;
use netspeedgauge_shared::usage_store::{Period, UsageStore};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSAppearanceCustomization, NSApplication, NSMenu, NSMenuItem, NSWindow};
use objc2_foundation::NSPoint;
use std::cell::RefCell;

const MENU_WIDTH: i32 = 164;
const CARD_WIDTH: i32 = 216;
const SUMMARY_HEIGHT: i32 = 34;
const CARD_HEIGHT: i32 = 124;
const ROW_HEIGHT: i32 = 32;
const DIVIDER_HEIGHT: i32 = 9;
const PADDING: i32 = 5;
const CORNER_RADIUS: f32 = 9.0;
const ICON_POINTS: f64 = 10.0;
// The Mac's own panels, measured on Weather, start one point above the bottom of the menu bar.
const MENU_OVERLAP_WITH_BAR: f64 = 1.0;
// The selected background ends 1 point past the item, and 2 more makes the rounded corner look lined up with it.
const MENU_PAST_ITEM: f64 = 3.0;
const ARROW_UP: &str = "\u{2191}";
const ARROW_DOWN: &str = "\u{2193}";

/// One thing the menu can do, with its icon, label and the work it asks for.
const MENU_ITEMS: [(&str, &str, Command); 3] = [
    ("chart.bar", "Usage", Command::OpenUsage(OpenedBy::Menu)),
    ("gearshape", "Settings", Command::OpenSettings),
    ("power", "Quit", Command::Quit),
];
const QUIT_ROW: usize = 2;

struct PopupFonts {
    body: Font,
    strong: Font,
    small: Font,
    value: Font,
    arrow: Font,
}

impl PopupFonts {
    fn new() -> Self {
        Self {
            body: Font::system(10.0, WEIGHT_REGULAR),
            strong: Font::system(10.0, WEIGHT_SEMIBOLD),
            small: Font::system(9.0, WEIGHT_REGULAR),
            value: Font::system(14.0, WEIGHT_SEMIBOLD),
            arrow: Font::system(10.0, WEIGHT_BOLD),
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
    Totals { today_up: today_bucket.up, today_down: today_bucket.down, month_up: month_bucket.up, month_down: month_bucket.down }
}

/// What holds a popup on screen. A click uses the Mac's own menu, which gives the same selected
/// background, opening, closing and press-and-hold behaviour as every other menu bar item.
enum PopupHost {
    Window(Retained<NSWindow>),
    SystemMenu(Retained<NSMenu>),
}

/// The open popup. `is_menu` tells the menu from the hover card.
struct Popup {
    host: PopupHost,
    view: Retained<GaugeView>,
    fonts: PopupFonts,
    theme: Theme,
    width: i32,
    height: i32,
    totals: Totals,
    is_menu: bool,
    // A card opened by a click stays until the next click, here or anywhere else.
    is_pinned: bool,
    hover_row: Option<usize>,
}

thread_local! {
    static OPEN_POPUP: RefCell<Option<Popup>> = const { RefCell::new(None) };
    static SYSTEM_MENU: std::cell::OnceCell<Retained<NSMenu>> = const { std::cell::OnceCell::new() };
    // What a menu row asked for, waiting until the menu has fully closed so its window gets the focus.
    static PENDING_COMMAND: RefCell<Option<Command>> = const { RefCell::new(None) };
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

/// Returns the y position of a menu row. Quit sits under a divider.
fn row_top(row: usize) -> i32 {
    let divider = if row >= QUIT_ROW { DIVIDER_HEIGHT } else { 0 };
    SUMMARY_HEIGHT + PADDING + row as i32 * ROW_HEIGHT + divider
}

impl Popup {
    fn is_held_by(&self, window: &NSWindow) -> bool {
        matches!(&self.host, PopupHost::Window(held) if std::ptr::eq(&**held, window))
    }

    fn row_at(&self, y: i32) -> Option<usize> {
        if !self.is_menu {
            return None;
        }
        (0..MENU_ITEMS.len()).find(|&row| y >= row_top(row) && y < row_top(row) + ROW_HEIGHT)
    }

    /// Draws the labelled download and upload totals used by both popups.
    fn draw_totals(&self, y: i32, label: &str, up: u64, down: u64, arrow_right: i32) {
        let theme = self.theme;
        let left = 16;
        let right = self.width - 16;
        canvas::text(left, y, label, &self.fonts.small, theme.muted, Align::Left);
        canvas::text(left, y + 16, &format_bytes(up + down), &self.fonts.value, theme.text, Align::Left);
        let (upload, download) = figure_colors(&theme, theme.surface);
        for (arrow, color, bytes, row_y) in [(ARROW_DOWN, download, down, y + 4), (ARROW_UP, upload, up, y + 24)] {
            canvas::text(right, row_y, &format_bytes(bytes), &self.fonts.small, theme.text, Align::Right);
            canvas::text(arrow_right, row_y, arrow, &self.fonts.arrow, color, Align::Right);
        }
    }

    fn draw(&self) {
        let theme = self.theme;
        let totals = self.totals;
        // The Mac's own menu paints its background, so only a window paints one here.
        if matches!(self.host, PopupHost::Window(_)) {
            canvas::round_rect(0, 0, self.width, self.height, CORNER_RADIUS, theme.border);
            canvas::round_rect(1, 1, self.width - 2, self.height - 2, CORNER_RADIUS - 1.0, theme.surface);
        }
        if !self.is_menu {
            // Every arrow shares one column, set by the widest number, so they line up at any speed.
            let widest = [totals.today_up, totals.today_down, totals.month_up, totals.month_down]
                .iter()
                .map(|bytes| canvas::text_width(&format_bytes(*bytes), &self.fonts.small))
                .max()
                .unwrap_or(0);
            let arrow_right = self.width - 16 - widest - 10;
            self.draw_totals(12, "Today", totals.today_up, totals.today_down, arrow_right);
            canvas::round_rect(16, 62, self.width - 32, 1, 0.0, theme.border);
            self.draw_totals(72, "This month", totals.month_up, totals.month_down, arrow_right);
            return;
        }
        let today_total = format_bytes(totals.today_up + totals.today_down);
        canvas::text(14, 9, "Today", &self.fonts.small, theme.muted, Align::Left);
        canvas::text(self.width - 14, 8, &today_total, &self.fonts.strong, theme.text, Align::Right);
        let divider_width = self.width - PADDING * 2;
        canvas::round_rect(PADDING, SUMMARY_HEIGHT, divider_width, 1, 0.0, theme.border);
        for (row, (icon, label, _)) in MENU_ITEMS.iter().enumerate() {
            let top = row_top(row);
            let is_hover = self.hover_row == Some(row);
            if is_hover {
                canvas::round_rect(PADDING, top, divider_width, ROW_HEIGHT, 5.0, mix(theme.surface, theme.text, 0.09));
            }
            canvas::symbol(22, top + ROW_HEIGHT / 2, icon, ICON_POINTS, if is_hover { theme.text } else { theme.muted });
            let font = if is_hover { &self.fonts.strong } else { &self.fonts.body };
            canvas::text(40, top + 7, label, font, theme.text, Align::Left);
        }
        canvas::round_rect(PADDING, row_top(QUIT_ROW) - (DIVIDER_HEIGHT / 2 + 1), divider_width, 1, 0.0, theme.border);
    }

    fn invalidate(&self) {
        self.view.setNeedsDisplay(true);
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
}

/// Asks the app to do what a menu row says, then closes the menu.
fn choose(row: usize) {
    let mut is_system_menu = false;
    with_popup(|popup| is_system_menu = matches!(popup.host, PopupHost::SystemMenu(_)));
    if is_system_menu {
        PENDING_COMMAND.with(|cell| *cell.borrow_mut() = Some(MENU_ITEMS[row].2));
    } else {
        app::post(MENU_ITEMS[row].2);
    }
    app::post(Command::ClosePopup);
}

/// Paints the open popup.
pub fn draw() {
    with_popup(|popup| popup.draw());
}

/// Reacts to the mouse moving and returns true when it is over something clickable.
pub fn on_mouse_move(_x: i32, y: i32) -> bool {
    let mut is_clickable = false;
    with_popup(|popup| {
        let row = popup.row_at(y);
        popup.set_hover(row);
        is_clickable = row.is_some();
    });
    is_clickable
}

pub fn on_mouse_exit() {
    with_popup(|popup| popup.set_hover(None));
}

pub fn on_click(_x: i32, y: i32) {
    let mut chosen = None;
    with_popup(|popup| chosen = popup.row_at(y));
    if let Some(row) = chosen {
        choose(row);
    }
}

pub fn on_key(code: u16) {
    let mut chosen = None;
    match code {
        KEY_ESCAPE => app::post(Command::ClosePopup),
        KEY_DOWN => with_popup(|popup| popup.move_hover(1)),
        KEY_UP => with_popup(|popup| popup.move_hover(MENU_ITEMS.len() - 1)),
        KEY_RETURN => with_popup(|popup| chosen = popup.hover_row),
        _ => {}
    }
    if let Some(row) = chosen {
        choose(row);
    }
}

/// A menu or a pinned card closes as soon as the user clicks anywhere else.
pub fn on_window_resigned(window: &NSWindow) {
    with_popup(|popup| {
        if popup.is_held_by(window) {
            if popup.is_pinned {
                app::note_auto_close();
            }
            app::post(Command::ClosePopup);
        }
    });
}

/// Frees the popup once its window has closed, and hands the keyboard back if nothing else is open.
pub fn on_window_closed(window: &NSWindow) {
    let mut closed = None;
    OPEN_POPUP.with(|cell| {
        if let Ok(mut state) = cell.try_borrow_mut() {
            if state.as_ref().is_some_and(|popup| popup.is_held_by(window)) {
                closed = state.take();
            }
        }
    });
    let Some(popup) = closed else {
        return;
    };
    let took_focus = popup.is_menu || popup.is_pinned;
    if let PopupHost::Window(window) = &popup.host {
        surface::hold_closed_window(window.clone());
    }
    if took_focus && usage_ui::opened_by().is_none() && !settings_ui::is_open() && !surface::is_headless() {
        NSApplication::sharedApplication(main_thread()).deactivate();
    }
}

fn popup_size(is_menu: bool) -> (i32, i32) {
    if is_menu {
        (MENU_WIDTH, SUMMARY_HEIGHT + PADDING * 2 + ROW_HEIGHT * MENU_ITEMS.len() as i32 + DIVIDER_HEIGHT)
    } else {
        (CARD_WIDTH, CARD_HEIGHT)
    }
}

/// Opens a popup just under the menu bar item.
fn open(store: &UsageStore, is_menu: bool, is_pinned: bool) {
    close();
    let totals = read_totals(store);
    let (width, height) = popup_size(is_menu);
    let takes_focus = is_menu || is_pinned;
    let (window, view) = surface::create_popup(width, height);
    app::place_below_item(&window);
    let popup = new_popup(PopupHost::Window(window.clone()), view.clone(), current_theme(), (width, height), totals, is_menu, is_pinned);
    OPEN_POPUP.with(|cell| *cell.borrow_mut() = Some(popup));
    if takes_focus {
        surface::show_with_focus(&window, &view);
    } else {
        surface::show_without_focus(&window);
    }
}

fn new_popup(host: PopupHost, view: Retained<GaugeView>, theme: Theme, size: (i32, i32), totals: Totals, is_menu: bool, is_pinned: bool) -> Popup {
    let (width, height) = size;
    Popup { host, view, fonts: PopupFonts::new(), theme, width, height, totals, is_menu, is_pinned, hover_row: None }
}

/// Places the Mac's own menu just under the menu bar item, with its right edge at the end of the selected background.
pub fn on_will_draw_in(window: &NSWindow) {
    let mut is_system_menu = false;
    with_popup(|popup| is_system_menu = matches!(popup.host, PopupHost::SystemMenu(_)));
    if is_system_menu {
        let size = window.frame().size;
        let area = app::screen_area();
        let anchor = app::anchor_frame();
        let top = area.origin.y + area.size.height + MENU_OVERLAP_WITH_BAR;
        let right = anchor.origin.x + anchor.size.width + MENU_PAST_ITEM;
        let origin = NSPoint::new((right - size.width).max(area.origin.x), top - size.height);
        if window.frame().origin != origin {
            window.setFrameOrigin(origin);
        }
    }
}

/// Returns the Mac's own menu that stays on the menu bar item. Its content is built each time it opens.
pub fn system_menu() -> Retained<NSMenu> {
    SYSTEM_MENU.with(|cell| {
        cell.get_or_init(|| {
            let menu = NSMenu::new(main_thread());
            menu.addItem(&NSMenuItem::new(main_thread()));
            menu.setDelegate(Some(ProtocolObject::from_ref(&*surface::delegate())));
            menu
        })
        .clone()
    })
}

/// Gives the Mac's own menu the chosen light or dark look. Setting it as the menu opens is too late for that opening.
pub fn sync_system_menu_look() {
    system_menu().setAppearance(appearance_for(&current_theme()).as_deref());
}

/// Builds the card or the right-click menu inside the Mac's own menu, just before it opens.
pub fn fill_system_menu(store: &UsageStore, is_menu: bool) {
    close();
    let menu = system_menu();
    let (width, height) = popup_size(is_menu);
    let view = GaugeView::create(surface::Surface::Popup, width, height);
    let item = NSMenuItem::new(main_thread());
    item.setView(Some(&view));
    menu.removeAllItems();
    menu.addItem(&item);
    let popup = new_popup(PopupHost::SystemMenu(menu), view, current_theme(), (width, height), read_totals(store), is_menu, !is_menu);
    OPEN_POPUP.with(|cell| *cell.borrow_mut() = Some(popup));
}

/// Forgets the popup once the Mac's own menu has closed, and runs what a row asked for just after.
pub fn on_system_menu_closed() {
    OPEN_POPUP.with(|cell| cell.borrow_mut().take());
    if PENDING_COMMAND.with(|cell| cell.borrow().is_some()) {
        surface::run_pending_command_soon();
    }
}

/// Runs what a menu row asked for, now that the menu has fully closed.
pub fn run_pending_command() {
    if let Some(command) = PENDING_COMMAND.with(|cell| cell.borrow_mut().take()) {
        app::post(command);
    }
}

/// Opens the right-click menu.
pub fn show_menu(store: &UsageStore) {
    open(store, true, false);
}

/// Opens the card for a click. Unlike the hover card, it stays when the mouse moves away.
pub fn show_pinned_card(store: &UsageStore) {
    open(store, false, true);
}

/// Opens the hover card with today's and this month's totals, unless a popup is showing.
pub fn show_hover_card(store: &UsageStore) {
    if !is_open() {
        open(store, false, false);
    }
}

/// Returns true while a card opened by a click is showing, so the next click can close it.
pub fn is_pinned_card_open() -> bool {
    let mut is_open = false;
    with_popup(|popup| is_open = popup.is_pinned);
    is_open
}

/// Returns true while any popup is showing.
pub fn is_open() -> bool {
    let mut is_open = false;
    with_popup(|_| is_open = true);
    is_open
}

/// Returns true while the menu, rather than a card, is showing.
pub fn is_menu_open() -> bool {
    let mut is_open = false;
    with_popup(|popup| is_open = popup.is_menu);
    is_open
}

/// Closes the hover card when the mouse leaves, leaving a menu or a pinned card open.
pub fn hide_hover_card() {
    with_popup(|popup| {
        if !popup.is_menu && !popup.is_pinned {
            app::post(Command::ClosePopup);
        }
    });
}

/// Closes whichever popup is open.
pub fn close() {
    let mut window = None;
    let mut menu = None;
    with_popup(|popup| match &popup.host {
        PopupHost::Window(held) => window = Some(held.clone()),
        PopupHost::SystemMenu(held) => menu = Some(held.clone()),
    });
    if let Some(window) = window {
        window.close();
    }
    if let Some(menu) = menu {
        menu.cancelTracking();
    }
}

/// Paints the open popup to a PNG file, for checks.
pub fn capture(path: &str) -> bool {
    // The state must be free while painting, since painting reads it.
    let mut view = None;
    with_popup(|popup| view = Some(popup.view.clone()));
    view.is_some_and(|view| surface::capture_view(&view, path))
}

/// Shows the open popup in a chosen theme, for checks.
pub fn preview_theme(is_dark: bool) {
    with_popup(|popup| popup.theme = if is_dark { crate::theme::dark_theme() } else { crate::theme::light_theme() });
}
