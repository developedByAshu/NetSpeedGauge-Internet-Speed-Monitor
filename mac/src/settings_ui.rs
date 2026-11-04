//! The Settings window. It exists only while open, then frees its memory. Changes apply at once.

use crate::app::{self, Command};
use crate::canvas::{self, mix, Align, Font, WEIGHT_REGULAR, WEIGHT_SEMIBOLD};
use crate::net_counters::list_adapters;
use crate::readout::{pair_texts, readout_colors, Readout};
use crate::startup;
use crate::surface::{self, GaugeView, Surface, KEY_ESCAPE, WINDOW_LEVEL_FLOATING, WINDOW_LEVEL_NORMAL};
use crate::theme::{apply_title_bar, current_theme, dark_theme, draw_made_with_love, light_theme, set_theme_choice, Theme};
use netspeedgauge_shared::settings::{Field, Settings, COLOR_AUTO, FIELDS, PALETTE};
use objc2::rc::Retained;
use objc2_app_kit::NSWindow;
use std::cell::RefCell;
use std::rc::Rc;

const WINDOW_TITLE: &str = "NetSpeedGauge Settings";
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
const ICON_CHECK: &str = "checkmark";
const ICON_LOCK: &str = "lock.fill";
const ICON_MINUS: &str = "minus";
const ICON_PLUS: &str = "plus";
const PRIVACY_HEADER_HEIGHT: i32 = 68;
const LOCK_TILE: i32 = 40;
const DISABLED_FILL: f32 = 0.35;
const DISABLED_TEXT: f32 = 0.5;
const IRRELEVANT_ROW_FADE: f32 = 0.65;
const CARD_RADIUS: i32 = 6;
const NAV_TOP: i32 = 62;
const NAV_ROW: i32 = 40;
// Space each side of a segment's text. The system font runs wider than Windows', so this is tighter than there.
const SEGMENT_PADDING: i32 = 8;
const CONTROL_SPACE: i32 = 8;
const STEPPER_WIDTH: i32 = 136;
const SWATCH_ROW_WIDTH: i32 = 270;

/// The pages in the sidebar, with their icon and title.
const PAGES: [(&str, &str); 6] = [
    ("gearshape", "General"),
    ("paintbrush", "Appearance"),
    ("speedometer", "Units"),
    ("network", "Network"),
    ("slider.horizontal.3", "Advanced"),
    ("info.circle", "Details"),
];

/// What NetSpeedGauge does and does not do with data, shown on the Details page.
const PRIVACY_LINES: [&str; 4] = [
    "Nothing leaves this Mac. It never connects to the internet.",
    "It only reads your network adapters' byte counts from macOS.",
    "Your usage and settings are stored only on this computer.",
    "No account, no tracking, no ads and no servers.",
];

const APP_NAME: &str = "NetSpeedGauge";
const APP_TAGLINE: &str = "A tiny network speed monitor for the Mac menu bar.";
const DEVELOPER_NAME: &str = "Ashutosh Anand";
const DEVELOPER_ORIGIN: &str = "From India";

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
    StartAtLogin,
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

// The menu bar sets its own look, so the Mac has no Background section.
const APPEARANCE_ROWS: &[Row] = &[
    Row::Section("Theme"),
    setting(Field::WindowTheme, "NetSpeedGauge windows", "", Kind::Segments(&["Same as system", "Light", "Dark"])),
    Row::Preview,
    Row::Section("Text"),
    setting(Field::FontFamily, "Font", "", Kind::Segments(&["System", "Menlo", "SF Mono", "Helvetica"])),
    setting(Field::FontPoints, "Size", "", Kind::Stepper { step: 1, suffix: " pt" }),
    setting(Field::TextColor, "Colour", "", Kind::Swatches { auto_label: "A" }),
    setting(Field::SwapRows, "Download first", "Swaps the two speeds", Kind::Toggle),
    Row::ToggleSection { title: "Icons", field: Field::ShowIcons },
    setting(Field::ArrowStyle, "Icon style", "", Kind::Segments(&["\u{2191} \u{2193}", "\u{25B2} \u{25BC}", "U D"])),
    setting(Field::IconThickness, "Icon thickness", "", Kind::Stepper { step: 1, suffix: "" }),
    setting(Field::UpColor, "Upload colour", "", Kind::Swatches { auto_label: "A" }),
    setting(Field::DownColor, "Download colour", "", Kind::Swatches { auto_label: "A" }),
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

// Command and drag moves a menu bar item on the Mac, so there are no position settings.
const GENERAL_ROWS: &[Row] = &[
    Row::Section("Startup"),
    Row::StartAtLogin,
    Row::Section("Updates"),
    setting(Field::UpdateRate, "Update every", "", Kind::Segments(&["0.5 s", "1 s", "2 s", "5 s"])),
    setting(Field::SmartUpdates, "Slow down when idle", "Uses less power when nothing is moving", Kind::Toggle),
];

const NETWORK_ROWS: &[Row] = &[
    Row::Section("What to count"),
    setting(Field::InterfaceMode, "Adapters", "", Kind::Segments(&["All physical", "Main only", "Chosen"])),
    Row::Note("All physical counts Wi-Fi, Ethernet and AirDrop.\nVPN traffic counts once, on the real adapter."),
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

const ABOUT_ROWS: &[Row] = &[Row::AboutCard, Row::Section("Privacy"), Row::Privacy, Row::Section("Know your developer"), Row::Developer];

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
    ToggleStartAtLogin,
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
    body: Font,
    strong: Font,
    small: Font,
    title: Font,
}

impl Fonts {
    fn new() -> Self {
        Self {
            body: Font::system(10.0, WEIGHT_REGULAR),
            strong: Font::system(10.0, WEIGHT_SEMIBOLD),
            small: Font::system(9.0, WEIGHT_REGULAR),
            title: Font::system(16.0, WEIGHT_SEMIBOLD),
        }
    }
}

struct SettingsWindow {
    window: Retained<NSWindow>,
    view: Retained<GaugeView>,
    settings: Rc<RefCell<Settings>>,
    fonts: Fonts,
    theme: Theme,
    page: usize,
    scroll: i32,
    content_height: i32,
    hits: Vec<HitArea>,
    hover: Option<Action>,
    adapters: Vec<(u64, String)>,
    is_confirming_reset: bool,
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

/// Splits a note into its two lines.
fn split_note(text: &'static str) -> (&'static str, &'static str) {
    text.split_once('\n').unwrap_or((text, ""))
}

impl SettingsWindow {
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
        canvas::round_rect(0, 0, WIDTH, HEIGHT, 0.0, theme.background);
        self.draw_page();
        self.hits.retain(|hit| hit.y + hit.height > HEADER_HEIGHT);
        canvas::round_rect(SIDEBAR_WIDTH, 0, WIDTH - SIDEBAR_WIDTH, HEADER_HEIGHT, 0.0, theme.background);
        canvas::text(SIDEBAR_WIDTH + MARGIN - 4, 20, PAGES[self.page].1, &self.fonts.title, theme.text, Align::Left);
        self.draw_sidebar();
        self.draw_scrollbar();
    }

    fn draw_sidebar(&mut self) {
        let theme = self.theme;
        canvas::round_rect(0, 0, SIDEBAR_WIDTH, HEIGHT, 0.0, mix(theme.background, theme.card, 0.45));
        canvas::text(18, 22, APP_NAME, &self.fonts.strong, theme.muted, Align::Left);
        for (position, (icon, label)) in PAGES.iter().enumerate() {
            let y = NAV_TOP + position as i32 * NAV_ROW;
            let (x, width, height) = (8, SIDEBAR_WIDTH - 16, NAV_ROW - 4);
            let is_selected = position == self.page;
            let action = Action::Page(position);
            if is_selected || self.is_hover(action) {
                let fill = if is_selected { theme.selected } else { mix(theme.background, theme.selected, 0.5) };
                canvas::round_rect(x, y, width, height, CARD_RADIUS as f32, fill);
            }
            if is_selected {
                canvas::round_rect(x, y + 10, 3, height - 20, 2.0, theme.download);
            }
            canvas::symbol(x + 20, y + height / 2, icon, 10.0, if is_selected { theme.text } else { theme.muted });
            let font = if is_selected { &self.fonts.strong } else { &self.fonts.body };
            canvas::text(x + 38, y + 9, label, font, theme.text, Align::Left);
            self.add_hit(x, y, width, height, action);
        }
        draw_made_with_love(SIDEBAR_WIDTH / 2, HEIGHT - 34, &self.fonts.small, theme.muted);
    }

    fn draw_scrollbar(&self) {
        let view = HEIGHT - HEADER_HEIGHT;
        if self.content_height <= view {
            return;
        }
        let thumb_height = (view * view / self.content_height).max(30);
        let thumb_y = HEADER_HEIGHT + (view - thumb_height) * self.scroll / (self.content_height - view).max(1);
        canvas::round_rect(WIDTH - 7, thumb_y, 3, thumb_height, 2.0, mix(self.theme.background, self.theme.muted, 0.5));
    }

    /// Draws every row of the current page, top to bottom, and works out how tall the page is.
    fn draw_page(&mut self) {
        let left = SIDEBAR_WIDTH + MARGIN - 4;
        let width = WIDTH - SIDEBAR_WIDTH - MARGIN * 2 + 4;
        let mut y = HEADER_HEIGHT - self.scroll;
        for row in page_rows(self.page) {
            y = self.draw_row(*row, left, y, width);
        }
        self.content_height = y + self.scroll - HEADER_HEIGHT + MARGIN;
        // Collapsing a section can leave the page scrolled past its new end.
        let limit = (self.content_height - (HEIGHT - HEADER_HEIGHT)).max(0);
        if self.scroll > limit {
            self.scroll = limit;
            self.invalidate();
        }
    }

    fn row_card(&self, left: i32, y: i32, width: i32, height: i32) {
        canvas::round_rect(left, y, width, height, CARD_RADIUS as f32, self.theme.card);
    }

    /// Draws a row's label, with its hint under it when there is one.
    fn row_label(&self, left: i32, y: i32, label: &str, hint: &str) {
        let theme = self.theme;
        let x = left + 14;
        if hint.is_empty() {
            canvas::text(x, y + 15, label, &self.fonts.body, theme.text, Align::Left);
        } else {
            canvas::text(x, y + 7, label, &self.fonts.body, theme.text, Align::Left);
            canvas::text(x, y + 27, hint, &self.fonts.small, theme.muted, Align::Left);
        }
    }

    /// Draws one row and returns the y position of the next.
    fn draw_row(&mut self, row: Row, left: i32, y: i32, width: i32) -> i32 {
        let next = y + ROW_HEIGHT + ROW_GAP;
        match row {
            Row::Section(title) => {
                canvas::text(left + 2, y + 14, title, &self.fonts.strong, self.theme.text, Align::Left);
                y + SECTION_GAP + 8
            }
            Row::ToggleSection { title, field } => {
                canvas::text(left + 2, y + 14, title, &self.fonts.strong, self.theme.text, Align::Left);
                let is_on = self.settings.borrow().is_on(field);
                // Centred on the heading text, in the same column as the toggles in the rows below.
                self.draw_toggle(left + width - 14, y, HEADING_TOGGLE_ROW, is_on);
                let height = SECTION_GAP + 8;
                self.add_hit(left, y, width, height, Action::Set(field, i64::from(!is_on)));
                y + height
            }
            Row::Preview => {
                self.draw_preview(left, y, width);
                y + PREVIEW_HEIGHT + ROW_GAP
            }
            Row::Setting { field, label, hint, kind } => {
                let is_collapsed = Settings::section_switch(field).is_some_and(|switch| !self.settings.borrow().is_on(switch));
                if is_collapsed {
                    return y;
                }
                let hits_before = self.hits.len();
                self.row_card(left, y, width, ROW_HEIGHT);
                self.row_label(left, y, label, hint);
                self.draw_control(field, kind, left + width - 14, y, left, width);
                if !self.settings.borrow().is_relevant(field) {
                    // Another choice leaves this row with no effect, so it greys out and ignores clicks.
                    self.hits.truncate(hits_before);
                    canvas::fade_round_rect(left, y, width, ROW_HEIGHT, CARD_RADIUS as f32, self.theme.card, IRRELEVANT_ROW_FADE);
                }
                next
            }
            Row::StartAtLogin => {
                let is_on = startup::is_enabled();
                self.row_card(left, y, width, ROW_HEIGHT);
                self.row_label(left, y, "Start at login", "Opens NetSpeedGauge when you log in");
                self.draw_toggle(left + width - 14, y, ROW_HEIGHT, is_on);
                self.add_hit(left, y, width, ROW_HEIGHT, Action::ToggleStartAtLogin);
                next
            }
            Row::AdapterList => self.draw_adapter_list(left, y, width),
            Row::Note(text) => {
                let (first, second) = split_note(text);
                canvas::text(left + 2, y + 2, first, &self.fonts.small, self.theme.muted, Align::Left);
                canvas::text(left + 2, y + 19, second, &self.fonts.small, self.theme.muted, Align::Left);
                y + 46
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
                self.row_card(left, y, width, ABOUT_CARD_HEIGHT);
                let x = left + 16;
                let version = format!("Version {}", env!("CARGO_PKG_VERSION"));
                canvas::text(x, y + 14, APP_NAME, &self.fonts.title, self.theme.text, Align::Left);
                canvas::text(x, y + 46, APP_TAGLINE, &self.fonts.body, self.theme.muted, Align::Left);
                canvas::text(x, y + 68, &version, &self.fonts.small, self.theme.muted, Align::Left);
                y + ABOUT_CARD_HEIGHT + ROW_GAP
            }
            Row::Privacy => {
                let height = PRIVACY_HEADER_HEIGHT + PRIVACY_LINE * PRIVACY_LINES.len() as i32 + 20;
                self.row_card(left, y, width, height);
                self.draw_privacy_header(left, y, width);
                for (position, line) in PRIVACY_LINES.iter().enumerate() {
                    let line_y = y + PRIVACY_HEADER_HEIGHT + 10 + position as i32 * PRIVACY_LINE;
                    canvas::symbol(left + 20, line_y + 10, ICON_CHECK, 8.0, self.theme.download);
                    canvas::text(left + 36, line_y, line, &self.fonts.body, self.theme.text, Align::Left);
                }
                y + height + ROW_GAP
            }
            Row::Developer => {
                self.row_card(left, y, width, ROW_HEIGHT);
                self.row_label(left, y, DEVELOPER_NAME, DEVELOPER_ORIGIN);
                next
            }
        }
    }

    /// Draws the top of the privacy card: a lock on a tinted tile, its promise, and a dividing line.
    fn draw_privacy_header(&self, left: i32, y: i32, width: i32) {
        let theme = self.theme;
        let (tile_x, tile_y) = (left + 14, y + 14);
        canvas::round_rect(tile_x, tile_y, LOCK_TILE, LOCK_TILE, (CARD_RADIUS + 2) as f32, mix(theme.card, theme.download, 0.18));
        canvas::symbol(tile_x + LOCK_TILE / 2, tile_y + LOCK_TILE / 2, ICON_LOCK, 13.0, theme.download);
        let text_x = tile_x + LOCK_TILE + 12;
        canvas::text(text_x, tile_y + 1, "Your data stays with you", &self.fonts.strong, theme.text, Align::Left);
        canvas::text(text_x, tile_y + 21, "Private and stored only on this Mac", &self.fonts.small, theme.muted, Align::Left);
        canvas::round_rect(left + 14, y + PRIVACY_HEADER_HEIGHT, width - 28, 1, 0.0, theme.border);
    }

    fn draw_button_row(&mut self, left: i32, y: i32, width: i32, label: &str, hint: &str, button: &str, action: Action) {
        let theme = self.theme;
        self.row_card(left, y, width, ROW_HEIGHT);
        self.row_label(left, y, label, hint);
        let button_width = canvas::text_width(button, &self.fonts.body) + 28;
        let (x, button_y, height) = (left + width - 14 - button_width, y + 10, 30);
        let fill = if self.is_hover(action) { theme.button_hover } else { theme.button };
        canvas::round_rect(x, button_y, button_width, height, CARD_RADIUS as f32, fill);
        canvas::text(x + button_width / 2, button_y + 5, button, &self.fonts.body, theme.text, Align::Center);
        self.add_hit(x, button_y, button_width, height, action);
    }

    /// Draws a small menu bar with the live readout on it, using the current choices.
    fn draw_preview(&self, left: i32, y: i32, width: i32) {
        let theme = self.theme;
        canvas::round_rect(left, y, width, PREVIEW_HEIGHT, 8.0, theme.card);
        let bar_height = 32;
        let bar_y = y + (PREVIEW_HEIGHT - bar_height) / 2;
        let bar_x = left + 14;
        let bar_width = width - 28;
        let bar_color = if theme.is_dark { 0x1C1C1C } else { 0xE4E4E4 };
        canvas::round_rect(bar_x, bar_y, bar_width, bar_height, 6.0, bar_color);
        canvas::text(bar_x + 12, bar_y + 7, "Preview", &self.fonts.small, theme.muted, Align::Left);

        let settings = self.settings.borrow().clone();
        let format = settings.speed_format();
        let automatic = if theme.is_dark { 0xFFFFFF } else { 0x000000 };
        let (up_speed, down_speed) = (1_800_000.0, 124_000_000.0);
        let colors = readout_colors(&settings, automatic, up_speed, down_speed);
        let readout = Readout::new(&settings);
        let readout_left = bar_x + bar_width - 8 - readout.width;
        let (up_text, down_text) = pair_texts(up_speed, down_speed, &format, settings.is_on(Field::SwapRows));
        readout.draw(readout_left, bar_y, bar_height, &up_text, &down_text, &colors);
    }

    fn draw_control(&mut self, field: Field, kind: Kind, right: i32, y: i32, left: i32, width: i32) {
        let value = self.settings.borrow().get(field);
        match kind {
            Kind::Toggle => {
                self.draw_toggle(right, y, ROW_HEIGHT, value != 0);
                self.add_hit(left, y, width, ROW_HEIGHT, Action::Set(field, i64::from(value == 0)));
            }
            Kind::Segments(options) => self.draw_segments(field, options, right, y, value),
            Kind::Stepper { step, suffix } => self.draw_stepper(field, right, y, value, step, suffix),
            Kind::Swatches { auto_label } => self.draw_swatches(field, right, y, value, auto_label),
        }
    }

    fn draw_toggle(&self, right: i32, y: i32, row_height: i32, is_on: bool) {
        let theme = self.theme;
        let (width, height) = (40, 20);
        let (x, top) = (right - width, y + (row_height - height) / 2);
        if is_on {
            canvas::round_rect(x, top, width, height, height as f32 / 2.0, theme.download);
        } else {
            canvas::round_rect(x, top, width, height, height as f32 / 2.0, theme.muted);
            canvas::round_rect(x + 1, top + 1, width - 2, height - 2, (height - 2) as f32 / 2.0, theme.card);
        }
        let knob = 12;
        let knob_x = if is_on { x + width - knob - 4 } else { x + 4 };
        let knob_color = if is_on { if theme.is_dark { 0x1B1B1B } else { 0xFFFFFF } } else { theme.muted };
        canvas::round_rect(knob_x, top + (height - knob) / 2, knob, knob, knob as f32 / 2.0, knob_color);
    }

    fn draw_segments(&mut self, field: Field, options: &'static [&'static str], right: i32, y: i32, value: i64) {
        let theme = self.theme;
        let widths: Vec<i32> = options.iter().map(|option| canvas::text_width(option, &self.fonts.strong) + SEGMENT_PADDING * 2).collect();
        let total: i32 = widths.iter().sum();
        let (height, top) = (32, y + 9);
        let mut x = right - total;
        let radius = CARD_RADIUS as f32;
        canvas::round_rect(x, top, total, height, radius, theme.segment_track_on_card);
        let inset = 3;
        for (position, option) in options.iter().enumerate() {
            let segment = widths[position];
            let action = Action::Set(field, position as i64);
            let is_selected = value == position as i64;
            if is_selected {
                canvas::round_rect(x + inset, top + inset, segment - inset * 2, height - inset * 2, radius - inset as f32 / 2.0, theme.segment_selected);
            }
            let color = if is_selected || self.is_hover(action) { theme.text } else { theme.muted };
            let font = if is_selected { &self.fonts.strong } else { &self.fonts.body };
            canvas::text(x + segment / 2, top + 6, option, font, color, Align::Center);
            self.add_hit(x, top, segment, height, action);
            x += segment;
        }
    }

    fn draw_stepper(&mut self, field: Field, right: i32, y: i32, value: i64, step: i64, suffix: &str) {
        let theme = self.theme;
        let (size, top) = (30, y + 10);
        let value_width = 76;
        let plus_x = right - size;
        let minus_x = plus_x - value_width - size;
        let limits = &FIELDS[field as usize];
        let buttons = [(minus_x, ICON_MINUS, value - step, value > limits.min), (plus_x, ICON_PLUS, value + step, value < limits.max)];
        for (x, icon, new_value, is_enabled) in buttons {
            let action = Action::Set(field, new_value);
            let radius = CARD_RADIUS as f32;
            if is_enabled {
                let fill = if self.is_hover(action) { theme.button_hover } else { theme.button };
                canvas::round_rect(x, top, size, size, radius, fill);
                canvas::symbol(x + size / 2, top + size / 2, icon, 8.0, theme.text);
                self.add_hit(x, top, size, size, action);
            } else {
                // At the end of its range the button looks switched off and cannot be clicked.
                canvas::round_rect(x, top, size, size, radius, mix(theme.card, theme.button, DISABLED_FILL));
                canvas::symbol(x + size / 2, top + size / 2, icon, 8.0, mix(theme.card, theme.muted, DISABLED_TEXT));
            }
        }
        canvas::text(minus_x + size + value_width / 2, top + 5, &format!("{value}{suffix}"), &self.fonts.body, theme.text, Align::Center);
    }

    fn draw_swatches(&mut self, field: Field, right: i32, y: i32, value: i64, auto_label: &str) {
        let theme = self.theme;
        let size = 20;
        let step = 25;
        let top = y + (ROW_HEIGHT - size) / 2;
        let count = PALETTE.len() as i32;
        let palette_left = right - step * count + (step - size);
        for (position, color) in PALETTE.iter().enumerate() {
            let x = palette_left + position as i32 * step;
            let is_selected = value == i64::from(*color);
            let action = Action::Set(field, i64::from(*color));
            if is_selected || self.is_hover(action) {
                let ring = 3;
                let ring_color = if is_selected { theme.text } else { theme.muted };
                canvas::round_rect(x - ring, top - ring, size + ring * 2, size + ring * 2, (size + ring * 2) as f32 / 2.0, ring_color);
                canvas::round_rect(x - ring + 2, top - ring + 2, size + ring * 2 - 4, size + ring * 2 - 4, (size + ring * 2 - 4) as f32 / 2.0, theme.card);
            }
            // A thin outline keeps a colour visible when it matches the card, like white on the light theme.
            canvas::round_rect(x - 1, top - 1, size + 2, size + 2, (size + 2) as f32 / 2.0, mix(theme.card, theme.muted, 0.6));
            canvas::round_rect(x, top, size, size, size as f32 / 2.0, *color);
            self.add_hit(x, top, size, size, action);
        }
        if auto_label.is_empty() {
            return;
        }
        let x = palette_left - step;
        let action = Action::Set(field, COLOR_AUTO);
        let is_selected = value == COLOR_AUTO;
        let ring_color = if is_selected { theme.text } else { theme.muted };
        canvas::round_rect(x, top, size, size, size as f32 / 2.0, ring_color);
        canvas::round_rect(x + 1, top + 1, size - 2, size - 2, (size - 2) as f32 / 2.0, theme.card);
        let font = if is_selected { &self.fonts.strong } else { &self.fonts.small };
        canvas::text(x + size / 2, top + 1, auto_label, font, ring_color, Align::Center);
        self.add_hit(x, top, size, size, action);
    }

    fn draw_adapter_list(&mut self, left: i32, y: i32, width: i32) -> i32 {
        let settings = self.settings.borrow().clone();
        let mut y = y;
        if settings.get(Field::InterfaceMode) != 2 {
            return y;
        }
        for (luid, name) in self.adapters.clone() {
            self.row_card(left, y, width, ROW_HEIGHT);
            self.row_label(left, y, &name, "");
            self.draw_toggle(left + width - 14, y, ROW_HEIGHT, settings.chosen_adapters.contains(&luid));
            self.add_hit(left, y, width, ROW_HEIGHT, Action::ToggleAdapter(luid));
            y += ROW_HEIGHT + ROW_GAP;
        }
        y
    }

    fn invalidate(&self) {
        self.view.setNeedsDisplay(true);
    }

    /// Saves the choices and tells the app to apply them at once.
    fn commit(&mut self) {
        self.settings.borrow().save();
        set_theme_choice(self.settings.borrow().get(Field::WindowTheme));
        self.theme = current_theme();
        apply_title_bar(&self.window, &self.theme);
        if self.settings.borrow().is_saved_to_disk {
            app::post(Command::SettingsChanged);
        }
        self.invalidate();
    }

    fn handle_click(&mut self, x: i32, y: i32) {
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
            Action::ToggleStartAtLogin => {
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
}

pub fn draw() {
    with_window(SettingsWindow::draw);
}

/// Reacts to the mouse moving and returns true when it is over something clickable.
pub fn on_mouse_move(x: i32, y: i32) -> bool {
    let mut is_clickable = false;
    with_window(|state| {
        let hover = state.action_at(x, y);
        if hover != state.hover {
            state.hover = hover;
            state.invalidate();
        }
        is_clickable = hover.is_some();
    });
    is_clickable
}

pub fn on_mouse_exit() {
    with_window(|state| {
        state.hover = None;
        state.invalidate();
    });
}

pub fn on_click(x: i32, y: i32) {
    with_window(|state| state.handle_click(x, y));
}

/// Scrolls the page by the distance the trackpad or wheel reports.
pub fn on_scroll(delta: i32) {
    with_window(|state| {
        let limit = (state.content_height - (HEIGHT - HEADER_HEIGHT)).max(0);
        state.scroll = (state.scroll - delta).clamp(0, limit);
        state.invalidate();
    });
}

pub fn on_key(code: u16) {
    if code == KEY_ESCAPE {
        app::post(Command::CloseSettings);
    }
}

/// Frees the window's state once it has closed.
pub fn on_window_closed(window: &NSWindow) {
    let mut closed = None;
    OPEN_WINDOW.with(|cell| {
        if let Ok(mut state) = cell.try_borrow_mut() {
            if state.as_ref().is_some_and(|open| std::ptr::eq(&*open.window, window)) {
                closed = state.take();
            }
        }
    });
    if let Some(state) = closed {
        surface::hold_closed_window(state.window.clone());
        app::sync_dock_entry();
    }
}

/// Returns true while the window is open.
pub fn is_open() -> bool {
    let mut is_open = false;
    with_window(|_| is_open = true);
    is_open
}

/// Shows the open window in a chosen theme, for checks.
pub fn preview_theme(is_dark: bool) {
    with_window(|state| {
        state.theme = if is_dark { dark_theme() } else { light_theme() };
        apply_title_bar(&state.window, &state.theme);
        state.invalidate();
    });
}

/// Applies the user's current theme choice to the open window, if any.
pub fn apply_theme() {
    with_window(|state| {
        state.theme = current_theme();
        apply_title_bar(&state.window, &state.theme);
        state.invalidate();
    });
}

/// Closes the window, if it is open.
pub fn close() {
    let mut open_window = None;
    with_window(|state| open_window = Some(state.window.clone()));
    if let Some(window) = open_window {
        window.close();
    }
}

/// Opens Settings under the menu bar, or brings it forward if it is already open.
pub fn open(settings: Rc<RefCell<Settings>>, keep_on_top: bool) {
    let mut existing = None;
    with_window(|state| existing = Some((state.window.clone(), state.view.clone())));
    if let Some((window, view)) = existing {
        surface::show_with_focus(&window, &view);
        return;
    }
    let mut adapters: Vec<(u64, String)> =
        list_adapters().into_iter().filter(|row| row.is_physical).map(|row| (row.luid, row.name)).collect();
    adapters.sort_by(|first, second| first.1.cmp(&second.1));
    let level = if keep_on_top { WINDOW_LEVEL_FLOATING } else { WINDOW_LEVEL_NORMAL };
    let (window, view) = surface::create_window(Surface::Settings, WIDTH, HEIGHT, WINDOW_TITLE, level);
    app::place_below_item(&window);
    let theme = current_theme();
    apply_title_bar(&window, &theme);
    let state = SettingsWindow {
        window: window.clone(),
        view: view.clone(),
        settings,
        fonts: Fonts::new(),
        theme,
        page: 0,
        scroll: 0,
        content_height: 0,
        hits: Vec::new(),
        hover: None,
        adapters,
        is_confirming_reset: false,
    };
    OPEN_WINDOW.with(|cell| *cell.borrow_mut() = Some(state));
    app::sync_dock_entry();
    surface::show_with_focus(&window, &view);
}

/// Paints the open window with its title bar to a PNG file, for checks.
pub fn capture_with_title_bar(path: &str) -> bool {
    let mut window = None;
    with_window(|state| window = Some(state.window.clone()));
    // SAFETY: the window is alive, so its content view's parent is too.
    let frame = window.and_then(|window| window.contentView()).and_then(|content| unsafe { content.superview() });
    frame.is_some_and(|frame| surface::capture_view(&frame, path))
}

/// Paints the open window to a PNG file, for checks.
pub fn capture(path: &str) -> bool {
    // The state must be free while painting, since painting reads it.
    let mut view = None;
    with_window(|state| view = Some(state.view.clone()));
    view.is_some_and(|view| surface::capture_view(&view, path))
}

/// Switches page, for checks.
pub fn show_page_for_checks(page: usize) {
    with_window(|state| {
        state.page = page;
        state.scroll = 0;
    });
}

/// Scrolls the page, for checks.
pub fn scroll_for_checks(distance: i32) {
    with_window(|state| state.scroll = distance);
}

/// Clicks the area that sets a field to a value, as a user would. Returns false when no such area is showing.
pub fn click_setting_for_checks(field: Field, value: i64) -> bool {
    let mut position = None;
    with_window(|state| {
        position = state
            .hits
            .iter()
            .find(|hit| hit.action == Action::Set(field, value))
            .map(|hit| (hit.x + hit.width / 2, hit.y + hit.height / 2));
    });
    if let Some((x, y)) = position {
        on_click(x, y);
    }
    position.is_some()
}

/// Returns every label, hint or line that would run into its control or off its card, for checks.
pub fn rows_that_do_not_fit() -> Vec<String> {
    let fonts = Fonts::new();
    let inner = WIDTH - SIDEBAR_WIDTH - MARGIN * 2 + 4 - 28;
    let mut problems = Vec::new();
    for page in 0..PAGES.len() {
        for row in page_rows(page) {
            let Row::Setting { label, hint, kind, .. } = row else {
                continue;
            };
            let control = match kind {
                Kind::Toggle => 40,
                Kind::Stepper { .. } => STEPPER_WIDTH,
                Kind::Swatches { .. } => SWATCH_ROW_WIDTH,
                Kind::Segments(options) => {
                    options.iter().map(|option| canvas::text_width(option, &fonts.strong) + SEGMENT_PADDING * 2).sum()
                }
            };
            let text = canvas::text_width(label, &fonts.body).max(canvas::text_width(hint, &fonts.small));
            if text + CONTROL_SPACE + control > inner {
                problems.push((*label).to_string());
            }
        }
    }
    for line in PRIVACY_LINES {
        if canvas::text_width(line, &fonts.body) > inner - 22 {
            problems.push(line.to_string());
        }
    }
    if canvas::text_width(APP_TAGLINE, &fonts.body) > inner {
        problems.push(APP_TAGLINE.to_string());
    }
    problems
}
