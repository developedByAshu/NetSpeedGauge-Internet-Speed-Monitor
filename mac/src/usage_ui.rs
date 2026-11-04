//! The usage window. It exists only while open, then frees its memory.

use crate::app::{self, Command};
use crate::canvas::{self, mix, Align, Font, WEIGHT_LIGHT, WEIGHT_REGULAR, WEIGHT_SEMIBOLD};
use crate::clock::today;
use crate::surface::{self, GaugeView, Surface, KEY_ESCAPE, KEY_LEFT, KEY_RIGHT, WINDOW_LEVEL_FLOATING, WINDOW_LEVEL_NORMAL};
use crate::theme::{apply_title_bar, current_theme, dark_theme, draw_footer_strip, figure_colors, light_theme, Theme, FOOTER_HEIGHT};
use netspeedgauge_shared::calendar::{date_from_day, short_date, MONTH_NAMES};
use netspeedgauge_shared::format::format_bytes;
use netspeedgauge_shared::usage_store::{Bucket, Period, UsageStore};
use objc2::rc::Retained;
use objc2::sel;
use objc2_app_kit::{NSEvent, NSWindow};
use objc2_foundation::{NSPoint, NSRect, NSTimer};
use std::cell::RefCell;
use std::rc::Rc;

const WINDOW_TITLE: &str = "NetSpeedGauge Usage";
const WIDTH: i32 = 440;
const HEIGHT: i32 = 558;
const MARGIN: i32 = 20;
const CARD_RADIUS: f32 = 8.0;
const CONTROL_RADIUS: f32 = 6.0;
const MENU_WIDTH: i32 = 190;
const MENU_ROW: i32 = 32;
const MAX_NAME_CHARS: usize = 22;
const ICON_SETTINGS: &str = "gearshape";
const ICON_CHEVRON_DOWN: &str = "chevron.down";
const ICON_CHECK: &str = "checkmark";
const PERIODS: [(Period, &str); 4] =
    [(Period::Day, "Day"), (Period::Week, "Week"), (Period::Month, "Month"), (Period::Year, "Year")];
const ALL_ADAPTERS: &str = "All adapters";
// A guess, not measured: frequent enough to feel immediate, with a little time to cross from the numbers to the window.
const MOUSE_CHECK_SECONDS: f64 = 0.15;
const AWAY_CHECKS_TO_CLOSE: u32 = 2;

/// What opened the window, which decides what closes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenedBy {
    /// Stays until the user closes it.
    Menu,
    /// Stays until the next click, on the numbers or anywhere else.
    Click,
    /// Stays only while the mouse is on the numbers or on the window.
    Hover,
}

/// The fonts the window uses.
struct Fonts {
    body: Font,
    strong: Font,
    small: Font,
    hero: Font,
    value: Font,
}

impl Fonts {
    fn new() -> Self {
        Self {
            body: Font::system(10.0, WEIGHT_REGULAR),
            strong: Font::system(10.0, WEIGHT_SEMIBOLD),
            small: Font::system(9.0, WEIGHT_REGULAR),
            hero: Font::system(32.0, WEIGHT_LIGHT),
            value: Font::system(13.0, WEIGHT_SEMIBOLD),
        }
    }
}

/// A rectangle in window coordinates.
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
    window: Retained<NSWindow>,
    view: Retained<GaugeView>,
    store: Rc<RefCell<UsageStore>>,
    fonts: Fonts,
    theme: Theme,
    period: Period,
    adapter: Option<u32>,
    // How many bars back from the newest the selected bar is, so it stays right across midnight.
    selected_back: usize,
    hover_bar: Option<usize>,
    hover_control: Option<Control>,
    is_menu_open: bool,
    opened_by: OpenedBy,
    away_checks: u32,
    mouse_timer: Option<Retained<NSTimer>>,
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

fn contains_point(frame: NSRect, point: NSPoint) -> bool {
    point.x >= frame.origin.x
        && point.x < frame.origin.x + frame.size.width
        && point.y >= frame.origin.y
        && point.y < frame.origin.y + frame.size.height
}

fn period_area(position: usize) -> Area {
    let width = 58;
    Area { x: MARGIN + position as i32 * width, y: 16, width, height: 32 }
}

fn settings_area() -> Area {
    let size = 32;
    Area { x: WIDTH - MARGIN - size, y: 16, width: size, height: size }
}

fn adapter_area() -> Area {
    let width = 120;
    Area { x: settings_area().x - 8 - width, y: 16, width, height: 32 }
}

fn hero_area() -> Area {
    Area { x: MARGIN, y: 60, width: WIDTH - MARGIN * 2, height: 150 }
}

fn chart_area() -> Area {
    Area { x: MARGIN, y: 222, width: WIDTH - MARGIN * 2, height: 272 }
}

fn bars_area() -> Area {
    let chart = chart_area();
    Area { x: chart.x + 14, y: chart.y + 46, width: chart.width - 28, height: chart.height - (46 + 34) }
}

fn menu_row_area(position: usize) -> Area {
    let adapter = adapter_area();
    Area {
        x: adapter.x + adapter.width - MENU_WIDTH,
        y: adapter.y + adapter.height + 6 + position as i32 * MENU_ROW,
        width: MENU_WIDTH,
        height: MENU_ROW,
    }
}

impl UsageWindow {
    /// Counts how long the mouse has been off both the numbers and the window, and says when that is long enough.
    fn has_mouse_left(&mut self) -> bool {
        let cursor = NSEvent::mouseLocation();
        let is_over = contains_point(self.window.frame(), cursor) || contains_point(app::anchor_frame(), cursor);
        self.away_checks = if is_over { 0 } else { self.away_checks + 1 };
        self.away_checks >= AWAY_CHECKS_TO_CLOSE
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
            let hit = (0..self.menu_len()).find(|&row| menu_row_area(row).contains(x, y));
            if let Some(row) = hit {
                return Some(Control::MenuRow(row));
            }
        }
        if adapter_area().contains(x, y) {
            return Some(Control::Adapter);
        }
        if settings_area().contains(x, y) {
            return Some(Control::Settings);
        }
        (0..PERIODS.len()).find(|&position| period_area(position).contains(x, y)).map(Control::Period)
    }

    /// Returns which chart bar is under the mouse.
    fn bar_at(&self, x: i32, y: i32) -> Option<usize> {
        let bars = bars_area();
        if !bars.contains(x, y) || self.is_menu_open {
            return None;
        }
        let count = self.period.bucket_count() as i32;
        Some((((x - bars.x) * count) / bars.width) as usize)
    }

    /// Paints the whole window.
    fn draw(&self) {
        let theme = self.theme;
        let today_day = today();
        let buckets = self.store.borrow().buckets(self.period, self.adapter, today_day);
        let last = buckets.len() - 1;
        let selected = last.saturating_sub(self.selected_back);
        canvas::round_rect(0, 0, WIDTH, HEIGHT, 0.0, theme.background);
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

    fn draw_period_control(&self) {
        let theme = self.theme;
        let first = period_area(0);
        let track_width = first.width * PERIODS.len() as i32;
        canvas::round_rect(first.x, first.y, track_width, first.height, CONTROL_RADIUS, theme.segment_track_on_page);
        let inset = 3;
        for (position, (period, label)) in PERIODS.iter().enumerate() {
            let area = period_area(position);
            let is_selected = *period == self.period;
            let is_hover = self.hover_control == Some(Control::Period(position));
            if is_selected {
                canvas::round_rect(
                    area.x + inset,
                    area.y + inset,
                    area.width - inset * 2,
                    area.height - inset * 2,
                    CONTROL_RADIUS - inset as f32 / 2.0,
                    theme.segment_selected,
                );
            }
            let color = if is_selected || is_hover { theme.text } else { theme.muted };
            let font = if is_selected { &self.fonts.strong } else { &self.fonts.body };
            canvas::text(area.x + area.width / 2, area.y + 7, label, font, color, Align::Center);
        }
    }

    fn draw_adapter_control(&self) {
        let theme = self.theme;
        let area = adapter_area();
        let is_active = self.hover_control == Some(Control::Adapter) || self.is_menu_open;
        let fill = if is_active { mix(theme.control, theme.selected, 0.6) } else { theme.control };
        canvas::round_rect(area.x, area.y, area.width, area.height, CONTROL_RADIUS, fill);
        let label = self.menu_label(self.adapter.map_or(0, |adapter| adapter as usize + 1));
        let label: String = label.chars().take(16).collect();
        canvas::text(area.x + 12, area.y + 7, &label, &self.fonts.body, theme.text, Align::Left);
        canvas::symbol(area.x + area.width - 16, area.y + area.height / 2, ICON_CHEVRON_DOWN, 7.0, theme.muted);
    }

    fn draw_settings_button(&self) {
        let theme = self.theme;
        let area = settings_area();
        let is_hover = self.hover_control == Some(Control::Settings);
        let fill = if is_hover { mix(theme.control, theme.selected, 0.6) } else { theme.control };
        canvas::round_rect(area.x, area.y, area.width, area.height, CONTROL_RADIUS, fill);
        let color = if is_hover { theme.text } else { theme.muted };
        canvas::symbol(area.x + area.width / 2, area.y + area.height / 2, ICON_SETTINGS, 11.0, color);
    }

    fn draw_hero(&self, bucket: &Bucket, today_day: i32) {
        let theme = self.theme;
        let area = hero_area();
        let (pad, column) = (18, area.width / 2);
        canvas::round_rect(area.x, area.y, area.width, area.height, CARD_RADIUS, theme.card);
        let label = period_label(self.period, bucket.start, today_day);
        canvas::text(area.x + pad, area.y + 14, &label, &self.fonts.body, theme.muted, Align::Left);
        let total = format_bytes(bucket.up + bucket.down);
        canvas::text(area.x + pad - 2, area.y + 32, &total, &self.fonts.hero, theme.text, Align::Left);
        let stats_y = area.y + 94;
        let (upload, download) = figure_colors(&theme, theme.card);
        let stats = [(area.x + pad, download, "Download", bucket.down), (area.x + column, upload, "Upload", bucket.up)];
        for (x, color, name, bytes) in stats {
            canvas::round_rect(x, stats_y + 5, 8, 8, 4.0, color);
            canvas::text(x + 14, stats_y, name, &self.fonts.small, theme.muted, Align::Left);
            canvas::text(x + 14, stats_y + 17, &format_bytes(bytes), &self.fonts.value, theme.text, Align::Left);
        }
    }

    fn draw_chart(&self, buckets: &[Bucket], selected: usize) {
        let theme = self.theme;
        let card = chart_area();
        let pad = 14;
        canvas::round_rect(card.x, card.y, card.width, card.height, CARD_RADIUS, theme.card);
        canvas::text(card.x + pad, card.y + 13, chart_title(self.period), &self.fonts.strong, theme.text, Align::Left);
        let peak = buckets.iter().map(|bucket| bucket.up + bucket.down).max().unwrap_or(0);
        if peak > 0 {
            let peak_label = format!("Peak {}", format_bytes(peak));
            canvas::text(card.x + card.width - pad, card.y + 14, &peak_label, &self.fonts.small, theme.muted, Align::Right);
        }

        let (upload, download) = figure_colors(&theme, theme.card);
        let bars = bars_area();
        let baseline = bars.y + bars.height;
        canvas::round_rect(bars.x, baseline, bars.width, 1, 0.0, theme.border);
        let count = buckets.len() as i32;
        let slot = bars.width as f32 / count as f32;
        let bar_width = ((slot * 0.62).round() as i32).clamp(4, 44);
        let usable_height = bars.height - 4;
        for (position, bucket) in buckets.iter().enumerate() {
            let center = bars.x as f32 + slot * (position as f32 + 0.5);
            let x = (center - bar_width as f32 / 2.0).round() as i32;
            let is_selected = position == selected;
            let is_hover = self.hover_bar == Some(position);
            let strength = if is_selected { 1.0 } else if is_hover { 0.85 } else { 0.62 };
            let total = bucket.up + bucket.down;
            if is_selected {
                let tint = mix(theme.card, theme.text, 0.07);
                let tint_x = (bars.x as f32 + slot * position as f32).round() as i32;
                canvas::round_rect(tint_x, bars.y, slot.round() as i32, bars.height, 4.0, tint);
            }
            if total == 0 {
                let color = mix(theme.card, theme.muted, strength * 0.5);
                canvas::round_rect(x, baseline - 3, bar_width, 3, 1.0, color);
            } else {
                let height = ((total as f32 / peak as f32) * usable_height as f32).round().max(3.0) as i32;
                let upload_height = ((bucket.up as f32 / total as f32) * height as f32).round() as i32;
                let top = baseline - height;
                let radius = (bar_width as f32 / 2.0).min(3.0);
                let download_color = mix(theme.card, download, strength);
                canvas::round_rect(x, top, bar_width, height, radius, download_color);
                if upload_height >= 2 {
                    canvas::round_rect(x, top, bar_width, upload_height, radius, mix(theme.card, upload, strength));
                    canvas::round_rect(x, top + upload_height, bar_width, 1, 0.0, download_color);
                }
            }
            if shows_axis_label(self.period, position, buckets.len()) {
                let label = axis_label(self.period, bucket.start);
                let color = if is_selected { theme.text } else { theme.muted };
                canvas::text(center.round() as i32, baseline + 9, &label, &self.fonts.small, color, Align::Center);
            }
        }
    }

    fn draw_footer(&self) {
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
                format!("Counting since {} {year}. Kept for {kept}, only on this Mac.", short_date(day))
            }
            None => format!("Counting has just started. Kept for {kept}, only on this Mac."),
        };
        canvas::text(WIDTH / 2, HEIGHT - FOOTER_HEIGHT - 28, &note, &self.fonts.small, theme.muted, Align::Center);
        draw_footer_strip(&theme, HEIGHT - FOOTER_HEIGHT, WIDTH, FOOTER_HEIGHT, &self.fonts.small);
    }

    fn draw_menu(&self) {
        let theme = self.theme;
        let rows = self.menu_len();
        let first = menu_row_area(0);
        let (height, pad) = (first.height * rows as i32, 4);
        let (outer_x, outer_y) = (first.x - pad, first.y - pad);
        let (outer_width, outer_height) = (first.width + pad * 2, height + pad * 2);
        canvas::round_rect(outer_x - 1, outer_y - 1, outer_width + 2, outer_height + 2, CARD_RADIUS + 1.0, theme.border);
        canvas::round_rect(outer_x, outer_y, outer_width, outer_height, CARD_RADIUS, theme.surface);
        let current = self.adapter.map_or(0, |adapter| adapter as usize + 1);
        for row in 0..rows {
            let area = menu_row_area(row);
            if self.hover_control == Some(Control::MenuRow(row)) {
                canvas::round_rect(area.x, area.y, area.width, area.height, CONTROL_RADIUS - 2.0, mix(theme.surface, theme.text, 0.09));
            }
            let label = self.menu_label(row);
            canvas::text(area.x + 30, area.y + 7, &label, &self.fonts.body, theme.text, Align::Left);
            if row == current {
                canvas::symbol(area.x + 16, area.y + area.height / 2, ICON_CHECK, 8.0, theme.download);
            }
        }
    }

    fn invalidate(&self) {
        self.view.setNeedsDisplay(true);
    }

    /// Reacts to a left click.
    fn handle_click(&mut self, x: i32, y: i32) {
        let control = self.control_at(x, y);
        let was_menu_open = self.is_menu_open;
        self.is_menu_open = false;
        match control {
            Some(Control::Period(position)) => {
                self.period = PERIODS[position].0;
                self.selected_back = 0;
            }
            Some(Control::Adapter) => self.is_menu_open = !was_menu_open,
            Some(Control::Settings) => app::post(Command::OpenSettings),
            Some(Control::MenuRow(row)) => self.adapter = row.checked_sub(1).map(|adapter| adapter as u32),
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
        self.selected_back = if towards_newer { self.selected_back.saturating_sub(1) } else { (self.selected_back + 1).min(oldest) };
        self.invalidate();
    }

    fn stop_mouse_check(&mut self) {
        if let Some(timer) = self.mouse_timer.take() {
            timer.invalidate();
        }
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

pub fn draw() {
    with_window(|state| state.draw());
}

/// Reacts to the mouse moving, redrawing only when what is highlighted changed. Returns true over something clickable.
pub fn on_mouse_move(x: i32, y: i32) -> bool {
    let mut is_clickable = false;
    with_window(|state| {
        let control = state.control_at(x, y);
        let bar = state.bar_at(x, y);
        if control != state.hover_control || bar != state.hover_bar {
            state.hover_control = control;
            state.hover_bar = bar;
            state.invalidate();
        }
        is_clickable = control.is_some() || bar.is_some();
    });
    is_clickable
}

pub fn on_mouse_exit() {
    with_window(|state| {
        state.hover_bar = None;
        state.hover_control = None;
        state.invalidate();
    });
}

pub fn on_click(x: i32, y: i32) {
    with_window(|state| state.handle_click(x, y));
}

pub fn on_key(code: u16) {
    match code {
        KEY_ESCAPE => app::post(Command::CloseUsage),
        KEY_LEFT => with_window(|state| state.move_selection(false)),
        KEY_RIGHT => with_window(|state| state.move_selection(true)),
        _ => {}
    }
}

/// Closes a hovered window once the mouse has moved away from it and from the numbers.
pub fn check_mouse() {
    let mut has_left = false;
    with_window(|state| has_left = state.has_mouse_left() && !state.window.isMiniaturized());
    if has_left {
        app::post(Command::CloseUsage);
    }
}

/// A window opened by a click closes when the user clicks anywhere else. Minimizing does not count.
pub fn on_window_resigned(window: &NSWindow) {
    with_window(|state| {
        if std::ptr::eq(&*state.window, window) && state.opened_by == OpenedBy::Click && !window.isMiniaturized() {
            app::note_auto_close();
            app::post(Command::CloseUsage);
        }
    });
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
    if let Some(mut state) = closed {
        state.stop_mouse_check();
        surface::hold_closed_window(state.window.clone());
        app::sync_dock_entry();
    }
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

/// Asks the open window, if any, to redraw with the latest totals.
pub fn refresh() {
    with_window(|state| state.invalidate());
}

/// Closes the window, if it is open.
pub fn close() {
    let mut open_window = None;
    with_window(|state| open_window = Some(state.window.clone()));
    if let Some(window) = open_window {
        window.close();
    }
}

/// Returns what opened the usage window, or None while it is closed.
pub fn opened_by() -> Option<OpenedBy> {
    let mut opened_by = None;
    with_window(|state| opened_by = Some(state.opened_by));
    opened_by
}

/// Opens the usage window under the menu bar. If it is already open, brings it forward and it now closes the new way.
pub fn open(store: Rc<RefCell<UsageStore>>, keep_on_top: bool, opened_by: OpenedBy) {
    let is_hover = opened_by == OpenedBy::Hover;
    let level = if keep_on_top || is_hover { WINDOW_LEVEL_FLOATING } else { WINDOW_LEVEL_NORMAL };
    let mut existing = None;
    with_window(|state| {
        state.opened_by = opened_by;
        state.stop_mouse_check();
        state.window.setLevel(level);
        existing = Some((state.window.clone(), state.view.clone()));
    });
    if let Some((window, view)) = existing {
        app::sync_dock_entry();
        surface::show_with_focus(&window, &view);
        return;
    }
    let (window, view) = surface::create_window(Surface::Usage, WIDTH, HEIGHT, WINDOW_TITLE, level);
    app::place_below_item(&window);
    let theme = current_theme();
    apply_title_bar(&window, &theme);
    let mouse_timer = (is_hover && !surface::is_headless()).then(|| surface::start_timer(MOUSE_CHECK_SECONDS, sel!(mouseCheck:), true));
    let state = UsageWindow {
        window: window.clone(),
        view: view.clone(),
        store,
        fonts: Fonts::new(),
        theme,
        period: Period::Day,
        adapter: None,
        selected_back: 0,
        hover_bar: None,
        hover_control: None,
        is_menu_open: false,
        opened_by,
        away_checks: 0,
        mouse_timer,
    };
    OPEN_WINDOW.with(|cell| *cell.borrow_mut() = Some(state));
    app::sync_dock_entry();
    if is_hover {
        surface::show_without_focus(&window);
    } else {
        surface::show_with_focus(&window, &view);
    }
}

/// Paints the open window to a PNG file, for checks.
pub fn capture(path: &str) -> bool {
    // The state must be free while painting, since painting reads it.
    let mut view = None;
    with_window(|state| view = Some(state.view.clone()));
    view.is_some_and(|view| surface::capture_view(&view, path))
}

/// Clicks the middle of a period button or the adapter control, for checks.
pub fn click_for_checks(control: &str, position: usize) {
    let area = match control {
        "period" => period_area(position),
        "adapter" => adapter_area(),
        "settings" => settings_area(),
        _ => menu_row_area(position),
    };
    on_click(area.x + area.width / 2, area.y + area.height / 2);
}

/// Returns the chosen period's position, for checks.
pub fn period_for_checks() -> Option<usize> {
    let mut found = None;
    with_window(|state| found = PERIODS.iter().position(|(period, _)| *period == state.period));
    found
}
