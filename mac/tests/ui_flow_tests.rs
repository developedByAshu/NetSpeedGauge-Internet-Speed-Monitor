//! Drives the real open, click and close logic with windows kept off screen, in a throwaway home folder.

#[cfg(target_os = "macos")]
mod flow {
    use netspeedgauge_mac::usage_ui::OpenedBy;
    use netspeedgauge_mac::{app, popup_ui, settings_ui, surface, usage_ui};
    use netspeedgauge_shared::settings::Field;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    type Check = (&'static str, fn() -> Result<(), String>);

    fn expect(condition: bool, message: &str) -> Result<(), String> {
        if condition { Ok(()) } else { Err(message.to_string()) }
    }

    /// Closes everything and puts the actions back to their defaults.
    fn reset() {
        popup_ui::close();
        usage_ui::close();
        settings_ui::close();
        app::flush_commands();
        let (_, settings) = app::state_for_checks().expect("started");
        let mut settings = settings.borrow_mut();
        settings.reset(Field::HoverAction);
        settings.reset(Field::ClickAction);
    }

    fn set_action(field: Field, value: i64) {
        let (_, settings) = app::state_for_checks().expect("started");
        settings.borrow_mut().set(field, value);
    }

    fn hover_does_nothing_by_default() -> Result<(), String> {
        app::on_hover_fired();
        expect(!popup_ui::is_open() && usage_ui::opened_by().is_none(), "hover should open nothing by default")
    }

    fn click_shows_the_card_by_default_and_second_click_closes_it() -> Result<(), String> {
        app::on_readout_click();
        expect(popup_ui::is_pinned_card_open() && usage_ui::opened_by().is_none(), "click should show the card")?;
        app::on_readout_click();
        expect(!popup_ui::is_open(), "second click should close it")
    }

    fn hover_card_closes_when_the_mouse_leaves() -> Result<(), String> {
        set_action(Field::HoverAction, 1);
        app::on_hover_fired();
        expect(popup_ui::is_open() && !popup_ui::is_pinned_card_open(), "hover should open an unpinned card")?;
        app::on_readout_exit();
        app::flush_commands();
        expect(!popup_ui::is_open(), "leaving should close the hover card")
    }

    fn click_can_open_the_window_and_second_click_closes_it() -> Result<(), String> {
        set_action(Field::ClickAction, 2);
        app::on_readout_click();
        expect(usage_ui::opened_by() == Some(OpenedBy::Click), "click should open the usage window")?;
        app::on_readout_click();
        expect(usage_ui::opened_by().is_none(), "second click should close it")
    }

    fn click_pins_what_hover_opened() -> Result<(), String> {
        set_action(Field::HoverAction, 2);
        set_action(Field::ClickAction, 2);
        app::on_hover_fired();
        expect(usage_ui::opened_by() == Some(OpenedBy::Hover), "hover should open the window")?;
        app::on_readout_click();
        expect(usage_ui::opened_by() == Some(OpenedBy::Click), "click should pin it")?;
        app::on_readout_exit();
        app::flush_commands();
        expect(usage_ui::opened_by() == Some(OpenedBy::Click), "a pinned window stays when the mouse leaves")
    }

    fn pinned_card_stays_when_mouse_leaves_and_click_closes_it() -> Result<(), String> {
        set_action(Field::ClickAction, 1);
        app::on_readout_click();
        expect(popup_ui::is_pinned_card_open(), "click should open a pinned card")?;
        app::on_readout_exit();
        app::flush_commands();
        expect(popup_ui::is_pinned_card_open(), "a pinned card stays when the mouse leaves")?;
        app::on_readout_click();
        expect(!popup_ui::is_open(), "second click should close the card")
    }

    fn nothing_means_nothing() -> Result<(), String> {
        set_action(Field::HoverAction, 0);
        set_action(Field::ClickAction, 0);
        app::on_hover_fired();
        app::on_readout_click();
        expect(!popup_ui::is_open() && usage_ui::opened_by().is_none(), "Nothing should open nothing")
    }

    fn hover_does_not_disturb_an_open_window() -> Result<(), String> {
        set_action(Field::HoverAction, 2);
        app::post(app::Command::OpenUsage(OpenedBy::Menu));
        app::flush_commands();
        app::on_hover_fired();
        expect(usage_ui::opened_by() == Some(OpenedBy::Menu), "hover must not change how an open window closes")
    }

    fn right_click_opens_menu_and_escape_closes_it() -> Result<(), String> {
        app::on_readout_right_click();
        expect(popup_ui::is_menu_open(), "right click should open the menu")?;
        popup_ui::on_key(surface::KEY_ESCAPE);
        app::flush_commands();
        expect(!popup_ui::is_open(), "escape should close the menu")
    }

    fn menu_rows_open_usage_and_settings() -> Result<(), String> {
        app::on_readout_right_click();
        popup_ui::on_key(surface::KEY_DOWN);
        popup_ui::on_key(surface::KEY_RETURN);
        app::flush_commands();
        expect(usage_ui::opened_by() == Some(OpenedBy::Menu) && !popup_ui::is_open(), "first row opens Usage")?;
        app::on_readout_right_click();
        popup_ui::on_key(surface::KEY_DOWN);
        popup_ui::on_key(surface::KEY_DOWN);
        popup_ui::on_key(surface::KEY_RETURN);
        app::flush_commands();
        expect(settings_ui::is_open(), "second row opens Settings")
    }

    fn window_keeps_day_week_month_year_and_settings_button() -> Result<(), String> {
        set_action(Field::ClickAction, 2);
        app::on_readout_click();
        for position in 0..4 {
            usage_ui::click_for_checks("period", position);
            expect(usage_ui::period_for_checks() == Some(position), "period button should switch")?;
        }
        usage_ui::click_for_checks("settings", 0);
        app::flush_commands();
        expect(settings_ui::is_open(), "the gear should open Settings")
    }

    fn settings_click_applies_and_saves_at_once() -> Result<(), String> {
        app::post(app::Command::OpenSettings);
        app::flush_commands();
        settings_ui::show_page_for_checks(2);
        settings_ui::capture(&format!("{}/page.png", std::env::var("HOME").unwrap_or_default()));
        expect(settings_ui::click_setting_for_checks(Field::HideSuffix, 1), "the toggle should be clickable")?;
        app::flush_commands();
        let (_, settings) = app::state_for_checks().expect("started");
        expect(settings.borrow().is_on(Field::HideSuffix), "the choice should apply")?;
        let path = netspeedgauge_shared::usage_store::data_folder().join("settings.ini");
        let saved = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        settings.borrow_mut().reset(Field::HideSuffix);
        expect(saved.contains("hide_suffix=1"), "the choice should be saved")
    }

    fn closing_then_reopening_works_many_times() -> Result<(), String> {
        set_action(Field::ClickAction, 2);
        for _ in 0..50 {
            app::on_readout_click();
            app::on_readout_click();
            surface::release_closed_windows();
        }
        expect(usage_ui::opened_by().is_none(), "should end closed")
    }

    fn every_settings_row_fits() -> Result<(), String> {
        let problems = settings_ui::rows_that_do_not_fit();
        expect(problems.is_empty(), &format!("these do not fit: {problems:?}"))
    }

    fn dock_entry_only_while_a_window_is_open_on_purpose() -> Result<(), String> {
        expect(!app::wants_dock_entry(), "nothing open means no Dock entry")?;
        app::on_readout_click();
        expect(!app::wants_dock_entry(), "a card is not a window")?;
        popup_ui::close();
        set_action(Field::HoverAction, 2);
        app::on_hover_fired();
        expect(!app::wants_dock_entry(), "a hovered window comes and goes too quickly")?;
        usage_ui::close();
        app::post(app::Command::OpenUsage(OpenedBy::Menu));
        app::flush_commands();
        expect(app::wants_dock_entry(), "Usage should have a Dock entry")?;
        app::post(app::Command::OpenSettings);
        app::flush_commands();
        usage_ui::close();
        expect(app::wants_dock_entry(), "Settings still open keeps the Dock entry")?;
        settings_ui::close();
        expect(!app::wants_dock_entry(), "closing the last window removes it")
    }

    fn windows_open_just_below_the_item_and_stay_on_screen() -> Result<(), String> {
        let rect = |x, y, width, height| NSRect::new(NSPoint::new(x, y), NSSize::new(width, height));
        let screen = rect(0.0, 0.0, 1440.0, 875.0);
        let size = NSSize::new(440.0, 586.0);
        let middle = app::origin_below_item(rect(800.0, 875.0, 180.0, 25.0), screen, size);
        expect(middle.x == 540.0 && middle.y == 283.0, "should sit under the item, ending at its right edge")?;
        let far_right = app::origin_below_item(rect(1300.0, 875.0, 180.0, 25.0), screen, size);
        expect(far_right.x == 992.0, "should stay on screen on the right")?;
        let far_left = app::origin_below_item(rect(10.0, 875.0, 180.0, 25.0), screen, size);
        expect(far_left.x == 8.0, "should stay on screen on the left")?;
        let short_screen = app::origin_below_item(rect(800.0, 500.0, 180.0, 25.0), rect(0.0, 0.0, 1440.0, 500.0), size);
        expect(short_screen.y == 8.0, "should stay on screen at the bottom")
    }

    fn views_never_paint_outside_their_area() -> Result<(), String> {
        let view = surface::GaugeView::create(surface::Surface::Settings, 100, 100);
        // SAFETY: this macOS has the method, which is what this test relies on.
        let clips: bool = unsafe { objc2::msg_send![&*view, clipsToBounds] };
        expect(clips, "scrolled rows would paint over the title bar")
    }

    pub fn run() -> i32 {
        let home = std::env::temp_dir().join(format!("netspeedgauge-flow-{}", std::process::id()));
        std::fs::create_dir_all(&home).expect("temp folder");
        std::env::set_var("HOME", &home);
        surface::set_headless();
        let mtm = MainThreadMarker::new().expect("runs on the main thread");
        NSApplication::sharedApplication(mtm).setActivationPolicy(NSApplicationActivationPolicy::Prohibited);
        app::start_without_menu_bar_item();

        let checks: [Check; 17] = [
            ("hover_does_nothing_by_default", hover_does_nothing_by_default),
            ("click_shows_the_card_by_default_and_second_click_closes_it", click_shows_the_card_by_default_and_second_click_closes_it),
            ("hover_card_closes_when_the_mouse_leaves", hover_card_closes_when_the_mouse_leaves),
            ("click_can_open_the_window_and_second_click_closes_it", click_can_open_the_window_and_second_click_closes_it),
            ("click_pins_what_hover_opened", click_pins_what_hover_opened),
            ("pinned_card_stays_when_mouse_leaves_and_click_closes_it", pinned_card_stays_when_mouse_leaves_and_click_closes_it),
            ("nothing_means_nothing", nothing_means_nothing),
            ("hover_does_not_disturb_an_open_window", hover_does_not_disturb_an_open_window),
            ("right_click_opens_menu_and_escape_closes_it", right_click_opens_menu_and_escape_closes_it),
            ("menu_rows_open_usage_and_settings", menu_rows_open_usage_and_settings),
            ("window_keeps_day_week_month_year_and_settings_button", window_keeps_day_week_month_year_and_settings_button),
            ("settings_click_applies_and_saves_at_once", settings_click_applies_and_saves_at_once),
            ("closing_then_reopening_works_many_times", closing_then_reopening_works_many_times),
            ("every_settings_row_fits", every_settings_row_fits),
            ("dock_entry_only_while_a_window_is_open_on_purpose", dock_entry_only_while_a_window_is_open_on_purpose),
            ("windows_open_just_below_the_item_and_stay_on_screen", windows_open_just_below_the_item_and_stay_on_screen),
            ("views_never_paint_outside_their_area", views_never_paint_outside_their_area),
        ];
        let mut failed = 0;
        for (name, check) in checks {
            reset();
            match check() {
                Ok(()) => println!("test {name} ... ok"),
                Err(message) => {
                    failed += 1;
                    println!("test {name} ... FAILED: {message}");
                }
            }
        }
        let _ = std::fs::remove_dir_all(&home);
        println!("test result: {} passed; {failed} failed", checks.len() - failed);
        i32::from(failed > 0)
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    std::process::exit(flow::run());
}
