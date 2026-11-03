//! NetSpeedGauge: live upload and download speed on the Windows taskbar.

#![windows_subsystem = "windows"]

use netspeedgauge::calendar;
use netspeedgauge::format::{format_speed, SpeedFormat};
use netspeedgauge::net_counters::{list_adapters, read_counters, Counter, DeltaTracker, InterfaceMode};
use netspeedgauge::popup_ui;
use netspeedgauge::readout::{Readout, ReadoutColors};
use netspeedgauge::settings::{
    Field, Settings, WM_APP_EXIT, WM_APP_OPEN_SETTINGS, WM_APP_OPEN_USAGE, WM_APP_SETTINGS_CHANGED,
};
use netspeedgauge::settings_ui::{self, speed_color};
use netspeedgauge::taskbar_dock::{
    find_taskbar, is_fullscreen_active, is_taskbar_collapsed, place_readout, to_wide,
};
use netspeedgauge::theme::{app_icon, set_icon_colors, set_theme_choice};
use netspeedgauge::usage_store::{data_folder, UsageStore};
use netspeedgauge::usage_ui;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::System::SystemInformation::GetTickCount64;
use windows_sys::Win32::System::Threading::{CreateMutexW, GetCurrentProcess, SetProcessWorkingSetSize};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows_sys::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Input::Ime::ImmDisableIME;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    TrackMouseEvent, TME_HOVER, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, LoadCursorW,
    PostMessageW, PostQuitMessage, RegisterClassW, SetCoalescableTimer, SetWindowPos, ShowWindow,
    TranslateMessage, EVENT_SYSTEM_FOREGROUND, HWND_TOPMOST, IDC_ARROW, MSG, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_APP,
    WM_DESTROY, WM_ENDSESSION, WM_LBUTTONUP, WM_MOUSEMOVE, WM_POWERBROADCAST, WM_QUERYENDSESSION,
    WM_RBUTTONUP, WM_SETTINGCHANGE, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

// The windows-sys crate does not export these messages, so they are declared here.
const WM_MOUSEHOVER: u32 = 0x02A1;
const WM_MOUSELEAVE: u32 = 0x02A3;

const SINGLE_INSTANCE_NAME: &str = "Local\\NetSpeedGaugeSingleInstance";
const WINDOW_CLASS_NAME: &str = "NetSpeedGaugeReadout";
const TIMER_ID: usize = 1;
const TICK_TOLERANCE_MS: u32 = 100;
const WM_APP_RESYNC: u32 = WM_APP + 1;
const SAVE_EVERY_SECONDS: f64 = 60.0;
const HOVER_DELAY_MS: u32 = 450;
// With no traffic for this many readings, smart updates slow the readout down.
const IDLE_TICKS: u32 = 10;
const IDLE_SLOWDOWN: u32 = 2;
const BASE_DPI: f64 = 96.0;
const TEXT_ON_DARK: u32 = 0x00FF_FFFF;
const TEXT_ON_LIGHT: u32 = 0x0000_0000;
const ALL_THREADS: u32 = u32::MAX;
// Two start times this close together mean the computer has not restarted in between.
const SAME_BOOT_TOLERANCE_SECS: i64 = 120;

/// Everything the readout keeps between ticks.
struct App {
    window: HWND,
    readout: Readout,
    tracker: DeltaTracker,
    counters: Vec<Counter>,
    mode: InterfaceMode,
    format: SpeedFormat,
    settings: Rc<RefCell<Settings>>,
    last_tick: Instant,
    last_save: Instant,
    up_speed: f64,
    down_speed: f64,
    up_text: String,
    down_text: String,
    colors: ReadoutColors,
    taskbar_text: u32,
    is_shown: bool,
    is_tracking_mouse: bool,
    position: (i32, i32),
    usage: Rc<RefCell<UsageStore>>,
    idle_ticks: u32,
    timer_ms: u32,
}

/// Returns the current time in seconds since 1970.
fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// Returns when the computer last started, in seconds since 1970.
fn boot_time() -> i64 {
    // SAFETY: GetTickCount64 takes no input and cannot fail.
    unix_now() - (unsafe { GetTickCount64() } / 1000) as i64
}

/// Adds moved bytes to today's usage, learning the adapter's name the first time it is seen.
fn record_usage(usage: &mut UsageStore, luid: u64, up: u64, down: u64) {
    if !usage.knows_adapter(luid) {
        let adapters = list_adapters();
        let name = adapters.iter().find(|row| row.luid == luid).map(|row| row.alias.clone());
        usage.add_adapter(luid, name.unwrap_or_else(|| "Adapter".to_string()));
    }
    usage.record(luid, calendar::today(), up, down);
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static WINDOW: RefCell<HWND> = const { RefCell::new(std::ptr::null_mut()) };
}

/// Returns the text colour that suits the taskbar's light or dark theme.
fn taskbar_text_color() -> u32 {
    let path = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let name = to_wide("SystemUsesLightTheme");
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
    if status == 0 && value == 1 { TEXT_ON_LIGHT } else { TEXT_ON_DARK }
}

/// Works out every colour for the readout from the settings and the current speeds.
fn readout_colors(settings: &Settings, automatic: u32, up_speed: f64, down_speed: f64) -> ReadoutColors {
    let text = settings.color_or(Field::TextColor, automatic);
    let up_value = speed_color(settings, up_speed, text);
    let down_value = speed_color(settings, down_speed, text);
    let background = u32::try_from(settings.get(Field::BackgroundColor)).ok();
    ReadoutColors {
        up_arrow: settings.color_or(Field::UpColor, up_value),
        down_arrow: settings.color_or(Field::DownColor, down_value),
        up_value,
        down_value,
        background: background.unwrap_or(0),
        background_alpha: background.map_or(0, |_| settings.get(Field::BackgroundOpacity) as u32 * 255 / 100),
    }
}

impl App {
    /// Moves the readout onto the taskbar, hides it when it must not show, and keeps it on top.
    fn sync_position(&mut self) {
        let Some(info) = find_taskbar() else {
            self.set_shown(false);
            return;
        };
        // SAFETY: GetDpiForWindow returns 0 for a stale window, handled just below.
        let dpi = unsafe { GetDpiForWindow(info.taskbar) };
        if dpi != 0 && dpi != self.readout.dpi() {
            self.readout = Readout::new(self.window, dpi, &self.settings.borrow());
            self.readout.draw(&self.up_text, &self.down_text, &self.colors);
        }
        let (width, height) = (self.readout.width, self.readout.height);
        if is_taskbar_collapsed(&info.taskbar_rect, width, height) || is_fullscreen_active() {
            self.set_shown(false);
            return;
        }
        let scale = f64::from(self.readout.dpi()) / BASE_DPI;
        let placed = place_readout(&info.taskbar_rect, &info.tray_rect, width, height, scale);
        let (offset_x, offset_y) = {
            let settings = self.settings.borrow();
            (settings.get(Field::OffsetX) as i32, settings.get(Field::OffsetY) as i32)
        };
        let position = (placed.0 + offset_x, placed.1 + offset_y);
        let flags = if position == self.position {
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE
        } else {
            SWP_NOSIZE | SWP_NOACTIVATE
        };
        self.position = position;
        // SAFETY: self.window is valid until the app exits.
        unsafe { SetWindowPos(self.window, HWND_TOPMOST, position.0, position.1, 0, 0, flags) };
        self.set_shown(true);
    }

    /// Shows or hides the window only when that actually changes.
    fn set_shown(&mut self, shown: bool) {
        if shown != self.is_shown {
            self.is_shown = shown;
            // SAFETY: self.window is valid until the app exits.
            unsafe { ShowWindow(self.window, if shown { SW_SHOWNOACTIVATE } else { SW_HIDE }) };
        }
    }

    /// Redraws the readout only when its text or colours changed.
    fn update_readout(&mut self) {
        let up_text = format_speed(self.up_speed, &self.format);
        let down_text = format_speed(self.down_speed, &self.format);
        let colors = readout_colors(&self.settings.borrow(), self.taskbar_text, self.up_speed, self.down_speed);
        if up_text != self.up_text || down_text != self.down_text || colors != self.colors {
            self.up_text = up_text;
            self.down_text = down_text;
            self.colors = colors;
            self.readout.draw(&self.up_text, &self.down_text, &self.colors);
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
            // SAFETY: self.window is valid. Setting the same timer again replaces its interval.
            unsafe { SetCoalescableTimer(self.window, TIMER_ID, wanted, None, TICK_TOLERANCE_MS) };
        }
    }

    /// Reads the counters and updates the readout and usage.
    fn tick(&mut self) {
        let elapsed = self.last_tick.elapsed().as_secs_f64();
        self.last_tick = Instant::now();
        if read_counters(&self.mode, &mut self.counters) {
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
        self.sync_position();
    }

    /// Saves usage if anything changed, along with the counters that let the next start catch up.
    fn save_usage(&mut self) {
        self.last_save = Instant::now();
        let mut usage = self.usage.borrow_mut();
        if !usage.is_dirty {
            return;
        }
        usage.prune(calendar::today());
        usage.baseline.clone_from(&self.counters);
        usage.saved_unix = unix_now();
        usage.boot_unix = boot_time();
        if usage.save_to(&data_folder()).is_ok() {
            usage.is_dirty = false;
        }
    }

    /// Follows a change between the light and dark theme.
    fn refresh_color(&mut self) {
        self.taskbar_text = taskbar_text_color();
        self.update_readout();
    }

    /// Applies changed settings at once.
    fn apply_settings(&mut self) {
        let settings = self.settings.borrow().clone();
        self.format = settings.speed_format();
        self.mode = settings.interface_mode();
        self.usage.borrow_mut().keep_days = settings.keep_days();
        self.readout = Readout::new(self.window, self.readout.dpi(), &settings);
        self.colors = readout_colors(&settings, self.taskbar_text, self.up_speed, self.down_speed);
        self.up_text = format_speed(self.up_speed, &self.format);
        self.down_text = format_speed(self.down_speed, &self.format);
        self.readout.draw(&self.up_text, &self.down_text, &self.colors);
        self.position = (i32::MIN, i32::MIN);
        self.update_timer();
        self.sync_position();
        set_theme_choice(settings.get(Field::WindowTheme));
        set_icon_colors(settings.get(Field::UpColor), settings.get(Field::DownColor));
        usage_ui::apply_theme();
    }

    /// Asks Windows to report when the mouse rests on the readout or leaves it.
    fn track_mouse(&mut self) {
        if self.is_tracking_mouse {
            return;
        }
        let mut request = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_HOVER | TME_LEAVE,
            hwndTrack: self.window,
            dwHoverTime: HOVER_DELAY_MS,
        };
        // SAFETY: `request` is valid for the duration of the call.
        unsafe { TrackMouseEvent(&mut request) };
        self.is_tracking_mouse = true;
    }
}

/// Runs a closure on the app state, if it has been created.
fn with_app(action: impl FnOnce(&mut App)) {
    APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            action(app);
        }
    });
}

/// Returns the shared usage and settings. No app borrow is held once this returns.
fn shared_state() -> Option<(Rc<RefCell<UsageStore>>, Rc<RefCell<Settings>>)> {
    let mut shared = None;
    with_app(|app| shared = Some((Rc::clone(&app.usage), Rc::clone(&app.settings))));
    shared
}

/// Opens the usage window.
fn open_usage(window: HWND) {
    if let Some((usage, settings)) = shared_state() {
        popup_ui::close();
        let keep_on_top = settings.borrow().is_on(Field::KeepOnTop);
        usage_ui::open(usage, window, keep_on_top);
    }
}

/// Opens the Settings window.
fn open_settings(window: HWND) {
    if let Some((_, settings)) = shared_state() {
        popup_ui::close();
        let keep_on_top = settings.borrow().is_on(Field::KeepOnTop);
        settings_ui::open(settings, window, keep_on_top);
    }
}

/// Handles the window's messages.
unsafe extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_TIMER => with_app(App::tick),
        WM_APP_RESYNC => with_app(App::sync_position),
        WM_SETTINGCHANGE => with_app(App::refresh_color),
        WM_RBUTTONUP => {
            if let Some((usage, _)) = shared_state() {
                popup_ui::show_menu(&usage.borrow(), window);
            }
        }
        WM_LBUTTONUP => {
            if let Some((_, settings)) = shared_state() {
                if settings.borrow().is_on(Field::ClickOpensUsage) {
                    open_usage(window);
                }
            }
        }
        WM_APP_OPEN_USAGE => open_usage(window),
        WM_APP_OPEN_SETTINGS => open_settings(window),
        WM_APP_SETTINGS_CHANGED => with_app(App::apply_settings),
        WM_APP_EXIT => {
            DestroyWindow(window);
        }
        WM_MOUSEMOVE => with_app(App::track_mouse),
        WM_MOUSEHOVER => {
            with_app(|app| app.is_tracking_mouse = false);
            if let Some((usage, settings)) = shared_state() {
                if settings.borrow().is_on(Field::HoverCard) {
                    popup_ui::show_hover_card(&usage.borrow(), window);
                }
            }
        }
        WM_MOUSELEAVE => {
            with_app(|app| app.is_tracking_mouse = false);
            popup_ui::hide_hover_card();
        }
        WM_POWERBROADCAST | WM_ENDSESSION => with_app(App::save_usage),
        WM_QUERYENDSESSION => {
            with_app(App::save_usage);
            return 1;
        }
        WM_DESTROY => {
            with_app(App::save_usage);
            usage_ui::close();
            settings_ui::close();
            popup_ui::close();
            PostQuitMessage(0);
        }
        _ => return DefWindowProcW(window, message, wparam, lparam),
    }
    0
}

/// Asks for a re-sync when another window comes to the front, since the taskbar may now cover the readout.
unsafe extern "system" fn on_foreground_change(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _source: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    let window = WINDOW.with(|cell| *cell.borrow());
    PostMessageW(window, WM_APP_RESYNC, 0, 0);
}

fn main() {
    let instance_name = to_wide(SINGLE_INSTANCE_NAME);
    let class_name = to_wide(WINDOW_CLASS_NAME);
    let title = to_wide("NetSpeedGauge");
    // SAFETY: Win32 setup on the main thread. All strings outlive the calls that read them.
    unsafe {
        let _instance_guard = CreateMutexW(std::ptr::null(), 0, instance_name.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // Nothing in the app takes typed text, so skip loading the text input libraries.
        ImmDisableIME(ALL_THREADS);

        let module = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: module,
            hIcon: app_icon(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&class);
        let window = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            module,
            std::ptr::null(),
        );
        WINDOW.with(|cell| *cell.borrow_mut() = window);

        let settings = Settings::load();
        set_theme_choice(settings.get(Field::WindowTheme));
        set_icon_colors(settings.get(Field::UpColor), settings.get(Field::DownColor));
        let dpi = find_taskbar().map_or(BASE_DPI as u32, |info| GetDpiForWindow(info.taskbar));
        let format = settings.speed_format();
        let taskbar_text = taskbar_text_color();
        let mode = settings.interface_mode();
        let mut counters = Vec::new();
        read_counters(&mode, &mut counters);

        // If the computer has not restarted, Windows' counters kept running, so catch up on what was missed.
        let mut usage = UsageStore::load_from(&data_folder());
        usage.keep_days = settings.keep_days();
        let is_same_boot = (usage.boot_unix - boot_time()).abs() <= SAME_BOOT_TOLERANCE_SECS;
        let mut tracker = if is_same_boot {
            DeltaTracker::with_baseline(std::mem::take(&mut usage.baseline))
        } else {
            DeltaTracker::default()
        };
        let gap_secs = if is_same_boot { (unix_now() - usage.saved_unix).max(0) as f64 } else { 0.0 };
        tracker.update_each(&counters, gap_secs, |luid, up, down| {
            record_usage(&mut usage, luid, up, down);
        });

        let timer_ms = settings.update_milliseconds();
        let mut app = App {
            window,
            readout: Readout::new(window, dpi, &settings),
            tracker,
            counters,
            mode,
            format,
            colors: readout_colors(&settings, taskbar_text, 0.0, 0.0),
            settings: Rc::new(RefCell::new(settings)),
            last_tick: Instant::now(),
            last_save: Instant::now(),
            up_speed: 0.0,
            down_speed: 0.0,
            up_text: format_speed(0.0, &format),
            down_text: format_speed(0.0, &format),
            taskbar_text,
            is_shown: false,
            is_tracking_mouse: false,
            position: (i32::MIN, i32::MIN),
            usage: Rc::new(RefCell::new(usage)),
            idle_ticks: 0,
            timer_ms,
        };
        app.readout.draw(&app.up_text, &app.down_text, &app.colors);
        app.sync_position();
        APP.with(|cell| *cell.borrow_mut() = Some(app));

        SetCoalescableTimer(window, TIMER_ID, timer_ms, None, TICK_TOLERANCE_MS);
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            std::ptr::null_mut(),
            Some(on_foreground_change),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        // Release memory only startup needed. Anything still in use comes straight back.
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);

        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
