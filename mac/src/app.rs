//! The app itself: the menu bar item, the timer, and what hovering and clicking do.

use crate::clock::{boot_time, today, unix_now};
use crate::net_counters::{list_adapters, read_counters};
use crate::readout::{pair_texts, readout_colors, Readout, ReadoutColors};
use crate::surface::{self, main_thread, GaugeView, Surface};
use crate::theme::{is_dark_appearance, set_icon_colors, set_theme_choice};
use crate::usage_ui::{self, OpenedBy};
use crate::{popup_ui, settings_ui, startup};
use netspeedgauge_shared::format::SpeedFormat;
use netspeedgauge_shared::net_counters::{Counter, DeltaTracker, InterfaceMode};
use netspeedgauge_shared::settings::{Field, MeterAction, Settings};
use netspeedgauge_shared::usage_store::{catch_up_gap_secs, data_folder, UsageStore};
use objc2::rc::Retained;
use objc2::sel;
use objc2_app_kit::{NSAppearance, NSAppearanceCustomization, NSApplication, NSApplicationActivationPolicy, NSAutoresizingMaskOptions, NSEventMask, NSStatusBar, NSStatusItem};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString, NSTimer};
use std::cell::RefCell;
use std::os::fd::AsRawFd;
use std::rc::Rc;
use std::time::Instant;

const SAVE_EVERY_SECONDS: f64 = 60.0;
const HOVER_DELAY_SECONDS: f64 = 0.45;
const IDLE_TICKS: u32 = 10;
const IDLE_SLOWDOWN: u32 = 2;
const TEXT_ON_DARK: u32 = 0xFFFFFF;
const TEXT_ON_LIGHT: u32 = 0x000000;
const AUTOSAVE_NAME: &str = "NetSpeedGauge";
const LOCK_FILE_NAME: &str = "instance.lock";
const GAP_BELOW_ITEM: f64 = 6.0;
const SCREEN_MARGIN: f64 = 8.0;
// A click on the numbers can also close a window for losing focus. Within this time it must not reopen it.
const REOPEN_GUARD_SECONDS: f64 = 0.3;
// Used only when there is no menu bar item, in tests and captures.
const FALLBACK_ANCHOR: (f64, f64, f64, f64) = (1000.0, 1000.0, 170.0, 24.0);

/// Work to do once the current event has released everything it borrowed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    OpenUsage(OpenedBy),
    OpenSettings,
    Quit,
    ClosePopup,
    CloseUsage,
    CloseSettings,
    SettingsChanged,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static COMMANDS: RefCell<Vec<Command>> = const { RefCell::new(Vec::new()) };
    static LAST_AUTO_CLOSE: RefCell<Option<Instant>> = const { RefCell::new(None) };
}

/// Everything the menu bar item keeps between ticks.
struct App {
    status_item: Option<Retained<NSStatusItem>>,
    view: Option<Retained<GaugeView>>,
    readout: Readout,
    tracker: DeltaTracker,
    counters: Vec<Counter>,
    mode: InterfaceMode,
    format: SpeedFormat,
    settings: Rc<RefCell<Settings>>,
    usage: Rc<RefCell<UsageStore>>,
    last_tick: Instant,
    last_save: Instant,
    up_speed: f64,
    down_speed: f64,
    up_text: String,
    down_text: String,
    colors: ReadoutColors,
    automatic_text: u32,
    idle_ticks: u32,
    timer_ms: u32,
    timer: Option<Retained<NSTimer>>,
    hover_timer: Option<Retained<NSTimer>>,
}

fn with_app(action: impl FnOnce(&mut App)) {
    APP.with(|cell| {
        if let Ok(mut state) = cell.try_borrow_mut() {
            if let Some(app) = state.as_mut() {
                action(app);
            }
        }
    });
}

/// Asks for work to run once the current event has finished.
pub fn post(command: Command) {
    COMMANDS.with(|cell| cell.borrow_mut().push(command));
}

/// Runs the work asked for during the event that just finished.
pub fn flush_commands() {
    loop {
        let commands = COMMANDS.with(|cell| cell.take());
        if commands.is_empty() {
            return;
        }
        commands.into_iter().for_each(run_command);
    }
}

fn run_command(command: Command) {
    match command {
        Command::OpenUsage(opened_by) => open_usage(opened_by),
        Command::OpenSettings => open_settings(),
        Command::Quit => {
            NSApplication::sharedApplication(main_thread()).terminate(None);
        }
        Command::ClosePopup => popup_ui::close(),
        Command::CloseUsage => usage_ui::close(),
        Command::CloseSettings => settings_ui::close(),
        Command::SettingsChanged => with_app(App::apply_settings),
    }
}

/// Adds moved bytes to today's usage, learning the adapter's name the first time it is seen.
fn record_usage(usage: &mut UsageStore, luid: u64, up: u64, down: u64) {
    if !usage.knows_adapter(luid) {
        let name = list_adapters().into_iter().find(|row| row.luid == luid).map(|row| row.name);
        usage.add_adapter(luid, name.unwrap_or_else(|| "Adapter".to_string()));
    }
    usage.record(luid, today(), up, down);
}

impl App {
    /// Keeps the Mac's own menu on the item while a left click shows the card. It then handles both clicks
    /// itself, because opening it from a click leaves the button waiting and the next click gets lost.
    fn sync_item_menu(&self) {
        if let Some(item) = &self.status_item {
            let shows_card = self.settings.borrow().meter_action(Field::ClickAction) == MeterAction::Card;
            let menu = shows_card.then(popup_ui::system_menu);
            item.setMenu(menu.as_deref());
        }
    }

    fn create(with_menu_bar_item: bool) -> Self {
        let settings = Settings::load();
        set_theme_choice(settings.get(Field::WindowTheme));
        set_icon_colors(settings.get(Field::UpColor), settings.get(Field::DownColor));
        let format = settings.speed_format();
        let mode = settings.interface_mode();
        let mut counters = Vec::new();
        read_counters(&mode, &mut counters);

        // If the Mac has not restarted, its counters kept running, so catch up on what was missed.
        let mut usage = UsageStore::load_from(&data_folder());
        usage.keep_days = settings.keep_days();
        let catch_up_gap = catch_up_gap_secs(usage.boot_unix, boot_time(), usage.saved_unix, unix_now());
        let mut tracker = if catch_up_gap.is_some() {
            DeltaTracker::with_baseline(std::mem::take(&mut usage.baseline))
        } else {
            DeltaTracker::default()
        };
        tracker.update_each(&counters, catch_up_gap.unwrap_or(0.0), |luid, up, down| {
            record_usage(&mut usage, luid, up, down);
        });

        let readout = Readout::new(&settings);
        let (status_item, view) = if with_menu_bar_item { create_status_item(readout.width) } else { (None, None) };
        let automatic_text = view
            .as_ref()
            .map_or(TEXT_ON_DARK, |view| if is_dark_appearance(&view.effectiveAppearance()) { TEXT_ON_DARK } else { TEXT_ON_LIGHT });
        let timer_ms = settings.update_milliseconds();
        Self {
            status_item,
            view,
            readout,
            tracker,
            counters,
            mode,
            up_text: pair_texts(0.0, 0.0, &format, settings.is_on(Field::SwapRows)).0,
            down_text: pair_texts(0.0, 0.0, &format, settings.is_on(Field::SwapRows)).1,
            format,
            colors: readout_colors(&settings, automatic_text, 0.0, 0.0),
            settings: Rc::new(RefCell::new(settings)),
            usage: Rc::new(RefCell::new(usage)),
            last_tick: Instant::now(),
            last_save: Instant::now(),
            up_speed: 0.0,
            down_speed: 0.0,
            automatic_text,
            idle_ticks: 0,
            timer_ms,
            timer: None,
            hover_timer: None,
        }
    }

    fn redraw(&self) {
        if let Some(view) = &self.view {
            view.setNeedsDisplay(true);
        }
    }

    /// Redraws the readout only when its text or colours changed.
    fn update_readout(&mut self) {
        let is_swapped = self.settings.borrow().is_on(Field::SwapRows);
        let (up_text, down_text) = pair_texts(self.up_speed, self.down_speed, &self.format, is_swapped);
        let colors = readout_colors(&self.settings.borrow(), self.automatic_text, self.up_speed, self.down_speed);
        if up_text != self.up_text || down_text != self.down_text || colors != self.colors {
            self.up_text = up_text;
            self.down_text = down_text;
            self.colors = colors;
            self.redraw();
        }
    }

    /// Runs the timer at the chosen rate, slower when nothing has moved for a while.
    fn update_timer(&mut self) {
        let (rate, is_smart) = {
            let settings = self.settings.borrow();
            (settings.update_milliseconds(), settings.is_on(Field::SmartUpdates))
        };
        let wanted = if is_smart && self.idle_ticks >= IDLE_TICKS { rate * IDLE_SLOWDOWN } else { rate };
        if wanted != self.timer_ms {
            self.timer_ms = wanted;
            self.start_timer();
        }
    }

    fn start_timer(&mut self) {
        if let Some(timer) = self.timer.take() {
            timer.invalidate();
        }
        self.timer = Some(surface::start_timer(f64::from(self.timer_ms) / 1000.0, sel!(tick:), true));
    }

    /// Reads the counters and updates the readout and usage.
    fn tick(&mut self) {
        let elapsed = self.last_tick.elapsed().as_secs_f64();
        self.last_tick = Instant::now();
        if !read_counters(&self.mode, &mut self.counters) {
            return;
        }
        let usage = Rc::clone(&self.usage);
        let (up, down) = self.tracker.update_each(&self.counters, elapsed, |luid, up, down| {
            record_usage(&mut usage.borrow_mut(), luid, up, down);
        });
        if up > 0 || down > 0 {
            usage_ui::refresh();
            self.idle_ticks = 0;
        } else {
            self.idle_ticks = self.idle_ticks.saturating_add(1);
        }
        if self.last_save.elapsed().as_secs_f64() >= SAVE_EVERY_SECONDS {
            self.save_usage();
        }
        if elapsed > 0.0 {
            self.up_speed = up as f64 / elapsed;
            self.down_speed = down as f64 / elapsed;
            self.update_readout();
        }
        self.update_timer();
    }

    /// Saves usage if anything changed, along with the counters that let the next start catch up.
    fn save_usage(&mut self) {
        self.last_save = Instant::now();
        let mut usage = self.usage.borrow_mut();
        if !usage.is_dirty {
            return;
        }
        usage.prune(today());
        usage.baseline.clone_from(&self.counters);
        usage.saved_unix = unix_now();
        usage.boot_unix = boot_time();
        if usage.save_to(&data_folder()).is_ok() {
            usage.is_dirty = false;
        }
    }

    /// Applies changed settings at once.
    fn apply_settings(&mut self) {
        let settings = self.settings.borrow().clone();
        self.format = settings.speed_format();
        self.mode = settings.interface_mode();
        self.usage.borrow_mut().keep_days = settings.keep_days();
        self.readout = Readout::new(&settings);
        if let Some(item) = &self.status_item {
            item.setLength(f64::from(self.readout.width));
        }
        self.colors = readout_colors(&settings, self.automatic_text, self.up_speed, self.down_speed);
        (self.up_text, self.down_text) = pair_texts(self.up_speed, self.down_speed, &self.format, settings.is_on(Field::SwapRows));
        self.redraw();
        self.update_timer();
        set_theme_choice(settings.get(Field::WindowTheme));
        set_icon_colors(settings.get(Field::UpColor), settings.get(Field::DownColor));
        usage_ui::apply_theme();
        popup_ui::sync_system_menu_look();
        self.sync_item_menu();
    }
}

/// Creates the menu bar item, at a fixed width so the icons beside it never jump.
fn create_status_item(width: i32) -> (Option<Retained<NSStatusItem>>, Option<Retained<GaugeView>>) {
    let mtm = main_thread();
    let item = NSStatusBar::systemStatusBar().statusItemWithLength(f64::from(width));
    item.setAutosaveName(Some(&NSString::from_str(AUTOSAVE_NAME)));
    let Some(button) = item.button(mtm) else {
        return (Some(item), None);
    };
    let bounds = button.bounds();
    let view = GaugeView::create(Surface::Readout, bounds.size.width as i32, bounds.size.height as i32);
    view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
    button.addSubview(&view);
    let target = surface::delegate();
    let target: &objc2::runtime::AnyObject = target.as_ref();
    // SAFETY: the delegate is never freed, and readoutClicked: takes the sender as its one argument.
    unsafe {
        button.setTarget(Some(target));
        button.setAction(Some(sel!(readoutClicked:)));
    }
    button.sendActionOn(NSEventMask::LeftMouseUp | NSEventMask::RightMouseUp);
    (Some(item), Some(view))
}

/// Returns the shared usage and settings. No app borrow is held once this returns.
fn shared_state() -> Option<(Rc<RefCell<UsageStore>>, Rc<RefCell<Settings>>)> {
    let mut shared = None;
    with_app(|app| shared = Some((Rc::clone(&app.usage), Rc::clone(&app.settings))));
    shared
}

/// Returns the menu bar item's area on screen.
pub fn anchor_frame() -> NSRect {
    let mut frame = None;
    with_app(|app| {
        if let Some(window) = app.view.as_ref().and_then(|view| view.window()) {
            frame = Some(window.frame());
        }
    });
    let (x, y, width, height) = FALLBACK_ANCHOR;
    frame.unwrap_or(NSRect::new(NSPoint::new(x, y), NSSize::new(width, height)))
}

fn open_usage(opened_by: OpenedBy) {
    if let Some((usage, settings)) = shared_state() {
        popup_ui::close();
        let keep_on_top = settings.borrow().is_on(Field::KeepOnTop);
        usage_ui::open(usage, keep_on_top, opened_by);
    }
}

fn open_settings() {
    if let Some((_, settings)) = shared_state() {
        popup_ui::close();
        let keep_on_top = settings.borrow().is_on(Field::KeepOnTop);
        settings_ui::open(settings, keep_on_top);
    }
}

/// Paints the menu bar item.
pub fn draw_readout(height: i32, appearance: &NSAppearance) {
    with_app(|app| {
        let automatic = if is_dark_appearance(appearance) { TEXT_ON_DARK } else { TEXT_ON_LIGHT };
        if automatic != app.automatic_text {
            app.automatic_text = automatic;
            app.colors = readout_colors(&app.settings.borrow(), automatic, app.up_speed, app.down_speed);
        }
        app.readout.draw(0, 0, height, &app.up_text, &app.down_text, &app.colors);
    });
}

/// Starts waiting to see whether the mouse rests on the numbers.
pub fn on_readout_enter() {
    with_app(|app| {
        if let Some(timer) = app.hover_timer.take() {
            timer.invalidate();
        }
        app.hover_timer = Some(surface::start_timer(HOVER_DELAY_SECONDS, sel!(hoverFired:), false));
    });
}

/// Closes what hovering opened when the mouse leaves. A hovered usage window watches the mouse itself.
pub fn on_readout_exit() {
    with_app(|app| {
        if let Some(timer) = app.hover_timer.take() {
            timer.invalidate();
        }
    });
    popup_ui::hide_hover_card();
}

/// Does what the user chose for resting on the numbers. It closes again when the mouse leaves.
pub fn on_hover_fired() {
    with_app(|app| app.hover_timer = None);
    let Some((usage, settings)) = shared_state() else {
        return;
    };
    let action = settings.borrow().meter_action(Field::HoverAction);
    match action {
        MeterAction::Nothing => {}
        MeterAction::Card => popup_ui::show_hover_card(&usage.borrow()),
        // Passing over the numbers must not disturb anything already open.
        MeterAction::UsageWindow if usage_ui::opened_by().is_some() || popup_ui::is_open() => {}
        MeterAction::UsageWindow => open_usage(OpenedBy::Hover),
    }
}

/// Remembers that a window just closed for losing focus.
pub fn note_auto_close() {
    LAST_AUTO_CLOSE.with(|cell| *cell.borrow_mut() = Some(Instant::now()));
}

fn was_just_auto_closed() -> bool {
    LAST_AUTO_CLOSE.with(|cell| cell.borrow().is_some_and(|closed| closed.elapsed().as_secs_f64() < REOPEN_GUARD_SECONDS))
}

/// Does what the user chose for a click. It stays until the next click, and a click pins what a hover opened.
pub fn on_readout_click() {
    with_app(|app| {
        if let Some(timer) = app.hover_timer.take() {
            timer.invalidate();
        }
    });
    let Some((usage, settings)) = shared_state() else {
        return;
    };
    if was_just_auto_closed() {
        return;
    }
    let action = settings.borrow().meter_action(Field::ClickAction);
    match action {
        MeterAction::Nothing => {}
        MeterAction::Card if popup_ui::is_pinned_card_open() => popup_ui::close(),
        MeterAction::Card => {
            if usage_ui::opened_by() == Some(OpenedBy::Hover) {
                usage_ui::close();
            }
            popup_ui::show_pinned_card(&usage.borrow());
        }
        MeterAction::UsageWindow => match usage_ui::opened_by() {
            None | Some(OpenedBy::Hover) => open_usage(OpenedBy::Click),
            Some(_) => usage_ui::close(),
        },
    }
}

/// Builds what the Mac's own menu shows, just before it opens. Nothing stays borrowed while it is open.
pub fn on_system_menu_opening() {
    with_app(|app| {
        if let Some(timer) = app.hover_timer.take() {
            timer.invalidate();
        }
    });
    if let Some((usage, _)) = shared_state() {
        popup_ui::fill_system_menu(&usage.borrow(), surface::current_event_asks_for_menu());
    }
}

/// Opens the Mac's own menu for a right click when a left click does something else.
pub fn open_item_menu() {
    let mut item = None;
    with_app(|app| item = app.status_item.clone());
    let Some(item) = item else {
        return;
    };
    let Some(button) = item.button(main_thread()) else {
        return;
    };
    item.setMenu(Some(&popup_ui::system_menu()));
    // SAFETY: a nil sender is allowed, and the button stays alive through the call.
    unsafe { button.performClick(None) };
    item.setMenu(None);
}

/// Opens the menu.
pub fn on_readout_right_click() {
    if let Some((usage, _)) = shared_state() {
        popup_ui::show_menu(&usage.borrow());
    }
}

/// Follows a change between light and dark.
pub fn on_appearance_change() {
    with_app(|app| app.redraw());
    usage_ui::apply_theme();
    settings_ui::apply_theme();
    popup_ui::sync_system_menu_look();
}

/// Runs one timer tick.
pub fn on_tick() {
    surface::release_closed_windows();
    with_app(App::tick);
}

/// Saves usage now, ahead of the Mac sleeping or the app quitting.
pub fn save_usage() {
    with_app(App::save_usage);
}

/// Holds a lock only one running copy can have. It is released when the program ends.
fn take_instance_lock() -> Option<std::fs::File> {
    let folder = data_folder();
    std::fs::create_dir_all(&folder).ok()?;
    let file = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(folder.join(LOCK_FILE_NAME)).ok()?;
    // SAFETY: the descriptor is valid for as long as `file`.
    let status = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    (status == 0).then_some(file)
}

/// Starts the app state with no menu bar item, for tests and captures.
pub fn start_without_menu_bar_item() {
    let app = App::create(false);
    APP.with(|cell| *cell.borrow_mut() = Some(app));
}

/// Returns the shared usage and settings to tests and captures.
pub fn state_for_checks() -> Option<(Rc<RefCell<UsageStore>>, Rc<RefCell<Settings>>)> {
    shared_state()
}

/// Runs the app until the user quits.
pub fn run() {
    let Some(_instance_lock) = take_instance_lock() else {
        return;
    };
    let application = NSApplication::sharedApplication(main_thread());
    application.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    application.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(&*surface::delegate())));
    application.run();
}

/// Creates the menu bar item once the app has finished launching.
pub fn finish_startup() {
    startup::refresh_path();
    surface::install_main_menu();
    let app = App::create(true);
    APP.with(|cell| *cell.borrow_mut() = Some(app));
    with_app(|app| {
        popup_ui::sync_system_menu_look();
        app.sync_item_menu();
        app.start_timer();
        app.redraw();
    });
    surface::watch_for_sleep();
}

/// Returns true while a window the user opened on purpose is showing. A hovered window comes and goes too quickly.
pub fn wants_dock_entry() -> bool {
    settings_ui::is_open() || usage_ui::opened_by().is_some_and(|opened_by| opened_by != OpenedBy::Hover)
}

/**
 * Puts the app in the Dock while a window is open, so its windows can be found again, and takes it
 * out when the last one closes. The menu bar item keeps running until the user quits.
 */
pub fn sync_dock_entry() {
    if surface::is_headless() {
        return;
    }
    let application = NSApplication::sharedApplication(main_thread());
    let wanted = if wants_dock_entry() { NSApplicationActivationPolicy::Regular } else { NSApplicationActivationPolicy::Accessory };
    if application.activationPolicy() == wanted {
        return;
    }
    application.setActivationPolicy(wanted);
    if wanted == NSApplicationActivationPolicy::Accessory {
        application.deactivate();
    } else {
        // Changing to having a Dock entry drops the focus the new window has just asked for.
        // SAFETY: the selector exists on every macOS version.
        let _: () = unsafe { objc2::msg_send![&*application, activateIgnoringOtherApps: true] };
    }
}

/// Returns where a window of this size goes: just below the menu bar item, kept on the item's screen.
pub fn origin_below_item(anchor: NSRect, screen: NSRect, size: NSSize) -> NSPoint {
    let highest_x = screen.origin.x + screen.size.width - size.width - SCREEN_MARGIN;
    let wanted_x = anchor.origin.x + anchor.size.width - size.width;
    let x = wanted_x.min(highest_x).max(screen.origin.x + SCREEN_MARGIN);
    let y = (anchor.origin.y - size.height - GAP_BELOW_ITEM).max(screen.origin.y + SCREEN_MARGIN);
    NSPoint::new(x, y)
}

/// Moves a window to just below the menu bar item.
pub fn place_below_item(window: &objc2_app_kit::NSWindow) {
    window.setFrameOrigin(origin_below_item(anchor_frame(), screen_area(), window.frame().size));
}

/// Returns the usable area of the screen holding the menu bar item.
pub fn screen_area() -> NSRect {
    let mut area = None;
    with_app(|app| {
        area = app.view.as_ref().and_then(|view| view.window()).and_then(|window| window.screen()).map(|screen| screen.visibleFrame());
    });
    area.or_else(|| objc2_app_kit::NSScreen::mainScreen(main_thread()).map(|screen| screen.visibleFrame()))
        .unwrap_or(NSRect::new(NSPoint::ZERO, NSSize::new(1440.0, 900.0)))
}

/// Starts the app state with a real menu bar item, for checks.
pub fn start_with_menu_bar_item() {
    let app = App::create(true);
    APP.with(|cell| *cell.borrow_mut() = Some(app));
    with_app(|app| app.redraw());
}

/// Returns the menu bar item's view, for checks.
pub fn readout_view_for_checks() -> Option<Retained<GaugeView>> {
    let mut found = None;
    with_app(|app| found = app.view.clone());
    found
}
