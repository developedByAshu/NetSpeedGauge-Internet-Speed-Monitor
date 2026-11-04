//! The one view class every surface draws into, the windows that hold it, and the object AppKit calls back.

use crate::{app, popup_ui, settings_ui, usage_ui};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAppearanceCustomization, NSApplication, NSApplicationDelegate, NSBackingStoreType, NSBitmapImageFileType, NSCursor, NSEvent, NSEventModifierFlags, NSEventType,
    NSMenu, NSMenuItem, NSPanel, NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindow, NSWindowButton, NSWindowCollectionBehavior,
    NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{NSDictionary, NSNotification, NSPoint, NSRect, NSSize, NSString, NSTimer};
use std::cell::OnceCell;
use std::sync::atomic::{AtomicBool, Ordering};

pub const WINDOW_LEVEL_NORMAL: isize = 0;
pub const WINDOW_LEVEL_FLOATING: isize = 3;
pub const WINDOW_LEVEL_POPUP: isize = 25;

pub const KEY_RETURN: u16 = 36;
pub const KEY_ESCAPE: u16 = 53;
pub const KEY_LEFT: u16 = 123;
pub const KEY_RIGHT: u16 = 124;
pub const KEY_DOWN: u16 = 125;
pub const KEY_UP: u16 = 126;

// True in tests and captures: windows are built and painted, but never shown on the screen.
static IS_HEADLESS: AtomicBool = AtomicBool::new(false);

/// Stops windows appearing on screen, for tests and captures.
pub fn set_headless() {
    IS_HEADLESS.store(true, Ordering::Relaxed);
}

/// Returns true while windows must stay off screen.
pub fn is_headless() -> bool {
    IS_HEADLESS.load(Ordering::Relaxed)
}

/// Returns the proof of running on the main thread, which is the only thread this app uses.
pub fn main_thread() -> MainThreadMarker {
    MainThreadMarker::new().expect("NetSpeedGauge only runs on the main thread")
}

/// Which surface a view paints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Readout,
    Popup,
    Usage,
    Settings,
}

pub struct ViewState {
    surface: Surface,
}

define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[name = "NetSpeedGaugeView"]
    #[ivars = ViewState]
    pub struct GaugeView;

    impl GaugeView {
        // The menu bar item's button takes the clicks itself. This view only paints and watches for hover.
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> *mut NSView {
            if self.ivars().surface == Surface::Readout {
                return std::ptr::null_mut();
            }
            // SAFETY: this calls NSView's own hit test with the same point.
            unsafe { msg_send![super(self), hitTest: point] }
        }

        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let size = self.frame().size;
            match self.ivars().surface {
                Surface::Readout => app::draw_readout(size.height as i32, &self.effectiveAppearance()),
                Surface::Popup => popup_ui::draw(),
                Surface::Usage => usage_ui::draw(),
                Surface::Settings => settings_ui::draw(),
            }
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            if self.ivars().surface == Surface::Readout {
                app::on_readout_enter();
            }
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            match self.ivars().surface {
                Surface::Readout => app::on_readout_exit(),
                Surface::Popup => popup_ui::on_mouse_exit(),
                Surface::Usage => usage_ui::on_mouse_exit(),
                Surface::Settings => settings_ui::on_mouse_exit(),
            }
            NSCursor::arrowCursor().set();
            app::flush_commands();
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            let (x, y) = self.position_of(event);
            let is_clickable = match self.ivars().surface {
                Surface::Readout => return,
                Surface::Popup => popup_ui::on_mouse_move(x, y),
                Surface::Usage => usage_ui::on_mouse_move(x, y),
                Surface::Settings => settings_ui::on_mouse_move(x, y),
            };
            if is_clickable { NSCursor::pointingHandCursor() } else { NSCursor::arrowCursor() }.set();
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {}

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            let (x, y) = self.position_of(event);
            match self.ivars().surface {
                Surface::Readout => {}
                Surface::Popup => popup_ui::on_click(x, y),
                Surface::Usage => usage_ui::on_click(x, y),
                Surface::Settings => settings_ui::on_click(x, y),
            }
            app::flush_commands();
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            if self.ivars().surface == Surface::Settings {
                settings_ui::on_scroll(event.scrollingDeltaY() as i32);
            }
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            let code = event.keyCode();
            match self.ivars().surface {
                Surface::Readout => {}
                Surface::Popup => popup_ui::on_key(code),
                Surface::Usage => usage_ui::on_key(code),
                Surface::Settings => settings_ui::on_key(code),
            }
            app::flush_commands();
        }

        #[unsafe(method(viewDidChangeEffectiveAppearance))]
        fn appearance_changed(&self) {
            if self.ivars().surface == Surface::Readout {
                app::on_appearance_change();
            }
        }
    }
);

impl GaugeView {
    /// Creates a view that paints a surface and reports the mouse whichever app is in front.
    pub fn create(surface: Surface, width: i32, height: i32) -> Retained<Self> {
        let mtm = main_thread();
        let frame = NSRect::new(NSPoint::ZERO, NSSize::new(f64::from(width), f64::from(height)));
        let this = Self::alloc(mtm).set_ivars(ViewState { surface });
        // SAFETY: initWithFrame: is NSView's designated initialiser.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        let options = NSTrackingAreaOptions::MouseEnteredAndExited
            | NSTrackingAreaOptions::MouseMoved
            | NSTrackingAreaOptions::ActiveAlways
            | NSTrackingAreaOptions::InVisibleRect;
        let owner: &AnyObject = view.as_ref();
        // SAFETY: the view owns the area, and AppKit keeps its owner reference only while the area is attached.
        let area = unsafe {
            NSTrackingArea::initWithRect_options_owner_userInfo(NSTrackingArea::alloc(), NSRect::ZERO, options, Some(owner), None)
        };
        view.addTrackingArea(&area);
        // Since macOS 14 a view can paint outside its own area, which let scrolled rows show over the title bar.
        if view.respondsToSelector(sel!(setClipsToBounds:)) {
            // SAFETY: the check above shows this macOS has the method.
            let _: () = unsafe { msg_send![&*view, setClipsToBounds: true] };
        }
        view
    }

    fn position_of(&self, event: &NSEvent) -> (i32, i32) {
        let point = self.convertPoint_fromView(event.locationInWindow(), None);
        (point.x as i32, point.y as i32)
    }
}

define_class!(
    // A window with no title bar can only take the keyboard if it says so.
    #[unsafe(super = NSPanel)]
    #[thread_kind = MainThreadOnly]
    #[name = "NetSpeedGaugePanel"]
    pub struct GaugePanel;

    impl GaugePanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool {
            true
        }
    }
);

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[name = "NetSpeedGaugeDelegate"]
    pub struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            app::finish_startup();
        }

        #[unsafe(method(applicationWillTerminate:))]
        fn will_terminate(&self, _notification: &NSNotification) {
            app::save_usage();
        }
    }

    unsafe impl NSWindowDelegate for AppDelegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, notification: &NSNotification) {
            if let Some(window) = window_of(notification) {
                popup_ui::on_window_closed(&window);
                usage_ui::on_window_closed(&window);
                settings_ui::on_window_closed(&window);
            }
            app::flush_commands();
        }

        #[unsafe(method(windowDidResignKey:))]
        fn window_did_resign_key(&self, notification: &NSNotification) {
            if let Some(window) = window_of(notification) {
                popup_ui::on_window_resigned(&window);
                usage_ui::on_window_resigned(&window);
            }
            app::flush_commands();
        }
    }

    impl AppDelegate {
        #[unsafe(method(readoutClicked:))]
        fn readout_clicked(&self, _sender: Option<&AnyObject>) {
            let event = NSApplication::sharedApplication(main_thread()).currentEvent();
            let is_menu_click = event.is_some_and(|event| {
                event.r#type() == NSEventType::RightMouseUp || event.modifierFlags().contains(NSEventModifierFlags::Control)
            });
            if is_menu_click {
                app::on_readout_right_click();
            } else {
                app::on_readout_click();
            }
            app::flush_commands();
        }

        #[unsafe(method(tick:))]
        fn tick(&self, _timer: &NSTimer) {
            app::on_tick();
        }

        #[unsafe(method(hoverFired:))]
        fn hover_fired(&self, _timer: &NSTimer) {
            app::on_hover_fired();
            app::flush_commands();
        }

        #[unsafe(method(mouseCheck:))]
        fn mouse_check(&self, _timer: &NSTimer) {
            usage_ui::check_mouse();
            app::flush_commands();
        }

        #[unsafe(method(willSleep:))]
        fn will_sleep(&self, _notification: &NSNotification) {
            app::save_usage();
        }
    }
);

fn window_of(notification: &NSNotification) -> Option<Retained<NSWindow>> {
    notification.object()?.downcast::<NSWindow>().ok()
}

thread_local! {
    static DELEGATE: OnceCell<Retained<AppDelegate>> = const { OnceCell::new() };
    // Closed windows wait here, since AppKit may still be using one when its close notice arrives.
    static CLOSED_WINDOWS: std::cell::RefCell<Vec<Retained<NSWindow>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Returns the one object AppKit calls back.
pub fn delegate() -> Retained<AppDelegate> {
    DELEGATE.with(|cell| {
        cell.get_or_init(|| {
            let this = AppDelegate::alloc(main_thread()).set_ivars(());
            // SAFETY: init is NSObject's designated initialiser.
            unsafe { msg_send![super(this), init] }
        })
        .clone()
    })
}

/// Keeps a closed window alive until the next tick.
pub fn hold_closed_window(window: Retained<NSWindow>) {
    CLOSED_WINDOWS.with(|cell| cell.borrow_mut().push(window));
}

/// Frees windows closed since the last tick.
pub fn release_closed_windows() {
    let closed = CLOSED_WINDOWS.with(|cell| cell.take());
    drop(closed);
}

fn configure_window(window: &NSWindow, view: &GaugeView, level: isize) {
    // SAFETY: the app keeps its own reference, so AppKit must not free the window on close.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setContentView(Some(view));
    window.setInitialFirstResponder(Some(view));
    window.setDelegate(Some(ProtocolObject::from_ref(&*delegate())));
    window.setLevel(level);
    window.setAcceptsMouseMovedEvents(true);
    window.setCollectionBehavior(NSWindowCollectionBehavior::MoveToActiveSpace | NSWindowCollectionBehavior::FullScreenAuxiliary);
}

/// Creates a normal titled window with close and minimize, sized by its content.
pub fn create_window(surface: Surface, width: i32, height: i32, title: &str, level: isize) -> (Retained<NSWindow>, Retained<GaugeView>) {
    let view = GaugeView::create(surface, width, height);
    let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable;
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(main_thread()),
            view.frame(),
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    window.setTitle(&NSString::from_str(title));
    if let Some(zoom) = window.standardWindowButton(NSWindowButton::ZoomButton) {
        zoom.setHidden(true);
    }
    configure_window(&window, &view, level);
    (window, view)
}

/// Creates a borderless popup with a soft shadow. Its corners come from what the view paints.
pub fn create_popup(width: i32, height: i32) -> (Retained<NSWindow>, Retained<GaugeView>) {
    let view = GaugeView::create(Surface::Popup, width, height);
    let this = GaugePanel::alloc(main_thread()).set_ivars(());
    let frame = view.frame();
    // SAFETY: this is NSPanel's inherited designated initialiser.
    let panel: Retained<GaugePanel> = unsafe {
        msg_send![
            super(this),
            initWithContentRect: frame,
            styleMask: NSWindowStyleMask::Borderless,
            backing: NSBackingStoreType::Buffered,
            defer: false
        ]
    };
    let window: Retained<NSWindow> = panel.into_super().into_super();
    window.setOpaque(false);
    window.setBackgroundColor(Some(&objc2_app_kit::NSColor::clearColor()));
    window.setHasShadow(true);
    configure_window(&window, &view, WINDOW_LEVEL_POPUP);
    (window, view)
}

/// Shows a window and gives it the keyboard, unless running headless.
pub fn show_with_focus(window: &NSWindow, view: &GaugeView) {
    if is_headless() {
        return;
    }
    let application = NSApplication::sharedApplication(main_thread());
    // SAFETY: the selector exists on every macOS version. This older call works without macOS 14's replacement.
    let _: () = unsafe { msg_send![&*application, activateIgnoringOtherApps: true] };
    window.makeKeyAndOrderFront(None);
    window.makeFirstResponder(Some(view));
}

/// Shows a window without taking the keyboard from the app in front, unless running headless.
pub fn show_without_focus(window: &NSWindow) {
    if !is_headless() {
        window.orderFrontRegardless();
    }
}

/// Starts a repeating or one-shot timer that calls the delegate, also while a menu or a drag is running.
pub fn start_timer(seconds: f64, selector: objc2::runtime::Sel, repeats: bool) -> Retained<NSTimer> {
    let target = delegate();
    let target: &AnyObject = target.as_ref();
    // SAFETY: the delegate is never freed, and every selector passed here takes the timer as its one argument.
    let timer = unsafe { NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(seconds, target, selector, None, repeats) };
    timer.setTolerance(seconds / 10.0);
    // SAFETY: the common modes constant is Foundation's own.
    unsafe { objc2_foundation::NSRunLoop::mainRunLoop().addTimer_forMode(&timer, objc2_foundation::NSRunLoopCommonModes) };
    timer
}

/// Subscribes to the Mac going to sleep.
pub fn watch_for_sleep() {
    let target = delegate();
    let target: &AnyObject = target.as_ref();
    let center = objc2_app_kit::NSWorkspace::sharedWorkspace().notificationCenter();
    // SAFETY: the delegate is never freed, and willSleep: takes the notification as its one argument.
    unsafe {
        center.addObserver_selector_name_object(target, sel!(willSleep:), Some(objc2_app_kit::NSWorkspaceWillSleepNotification), None);
    }
}

/// Builds the Mac menus shown while a window has the Dock entry: Quit, Minimize and Close, with their usual keys.
pub fn install_main_menu() {
    let mtm = main_thread();
    let submenu = |title: &str, items: &[(&str, objc2::runtime::Sel, &str)]| {
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
        for (label, action, key) in items {
            // SAFETY: each selector is one AppKit answers along the normal chain of windows and the app.
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &NSString::from_str(label), Some(*action), &NSString::from_str(key))
            };
            menu.addItem(&item);
        }
        let holder = NSMenuItem::new(mtm);
        holder.setSubmenu(Some(&menu));
        holder
    };
    let bar = NSMenu::new(mtm);
    bar.addItem(&submenu("NetSpeedGauge", &[("Quit NetSpeedGauge", sel!(terminate:), "q")]));
    bar.addItem(&submenu("Window", &[("Minimize", sel!(performMiniaturize:), "m"), ("Close", sel!(performClose:), "w")]));
    NSApplication::sharedApplication(mtm).setMainMenu(Some(&bar));
}

/// Paints a view to a PNG file without showing it, so checks never touch the screen.
pub fn capture_view(view: &NSView, path: &str) -> bool {
    let bounds = view.bounds();
    let Some(image) = view.bitmapImageRepForCachingDisplayInRect(bounds) else {
        return false;
    };
    view.cacheDisplayInRect_toBitmapImageRep(bounds, &image);
    let no_options = NSDictionary::new();
    // SAFETY: an empty dictionary is valid for the options.
    let Some(data) = (unsafe { image.representationUsingType_properties(NSBitmapImageFileType::PNG, &no_options) }) else {
        return false;
    };
    data.writeToFile_atomically(&NSString::from_str(path), true)
}

