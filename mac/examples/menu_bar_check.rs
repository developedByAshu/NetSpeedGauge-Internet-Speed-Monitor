//! Adds a real menu bar item and sends real click events straight to its window. The pointer never moves.
//! Popups stay off screen, and data goes to a throwaway home folder. Exits with 1 if a click does not land.
//! Give a data folder to start from a copy of its settings and usage.

#[cfg(target_os = "macos")]
fn main() {
    use netspeedgauge_mac::{app, popup_ui, surface, usage_ui};
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSEvent, NSEventModifierFlags, NSEventType, NSWindow};
    use objc2_foundation::{NSDate, NSPoint, NSRunLoop};

    let home = std::env::temp_dir().join(format!("netspeedgauge-bar-{}", std::process::id()));
    std::fs::create_dir_all(&home).expect("temp folder");
    std::env::set_var("HOME", &home);
    if let Some(source) = std::env::args().nth(1) {
        let copy = netspeedgauge_shared::usage_store::data_folder();
        std::fs::create_dir_all(&copy).expect("data folder");
        for name in ["settings.ini", "usage.bin"] {
            let _ = std::fs::copy(std::path::Path::new(&source).join(name), copy.join(name));
        }
    }
    surface::set_headless();
    let mtm = MainThreadMarker::new().expect("runs on the main thread");
    NSApplication::sharedApplication(mtm).setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app::start_with_menu_bar_item();
    let wait = |seconds: f64| NSRunLoop::mainRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(seconds));
    wait(1.0);

    let view = app::readout_view_for_checks().expect("menu bar item has a view");
    let window = view.window().expect("menu bar item has a window");
    let frame = view.frame();
    println!("view {} x {}, bar window {} x {}", frame.size.width, frame.size.height, window.frame().size.width, window.frame().size.height);
    let center = view.convertPoint_toView(NSPoint::new(frame.size.width / 2.0, frame.size.height / 2.0), None);
    let application = NSApplication::sharedApplication(mtm);
    let event_of = |window: &NSWindow, kind: NSEventType| {
        NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind,
            center,
            NSEventModifierFlags::empty(),
            0.0,
            window.windowNumber(),
            None,
            0,
            1,
            1.0,
        )
        .expect("event")
    };
    // The button follows the press until the release, so the release must already be waiting.
    let click = |down: NSEventType, up: NSEventType| {
        application.postEvent_atStart(&event_of(&window, up), false);
        application.sendEvent(&event_of(&window, down));
    };
    // SAFETY: alarm only schedules the signal that ends this check if a click ever hangs.
    unsafe { libc::alarm(20) };

    click(NSEventType::RightMouseDown, NSEventType::RightMouseUp);
    wait(0.1);
    let right_click_landed = popup_ui::is_menu_open();
    println!("right click opens the menu: {right_click_landed}");
    popup_ui::close();
    wait(0.5);

    click(NSEventType::LeftMouseDown, NSEventType::LeftMouseUp);
    wait(0.1);
    let click_landed = popup_ui::is_open() || usage_ui::opened_by().is_some();
    println!("left click opens its choice: {click_landed}");
    popup_ui::close();
    usage_ui::close();
    let _ = std::fs::remove_dir_all(&home);
    std::process::exit(i32::from(!(right_click_landed && click_landed)));
}

#[cfg(not(target_os = "macos"))]
fn main() {}
