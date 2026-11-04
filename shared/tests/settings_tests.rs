//! Tests for saving settings and for what each one changes.

use netspeedgauge_shared::format::{Scale, Unit};
use netspeedgauge_shared::net_counters::InterfaceMode;
use netspeedgauge_shared::palette::figure_color;
use netspeedgauge_shared::settings::{speed_color, Field, MeterAction, Settings, COLOR_AUTO, FIELDS};

const BASE: u32 = 0x123456;

#[test]
fn field_table_lines_up_with_the_field_list() {
    for (position, spec) in FIELDS.iter().enumerate() {
        assert_eq!(spec.field as usize, position, "{} is out of order", spec.key);
        assert!(spec.default >= spec.min && spec.default <= spec.max, "{} default is out of range", spec.key);
    }
}

#[test]
fn saving_and_loading_keeps_every_choice() {
    let mut settings = Settings::default();
    settings.set(Field::ArrowStyle, 2);
    settings.set(Field::Unit, 1);
    settings.set(Field::UpColor, 0x4ADE80);
    settings.set(Field::OffsetX, -40);
    settings.chosen_adapters = vec![11, 22];
    assert_eq!(Settings::from_text(&settings.to_text()), settings);
}

#[test]
fn damaged_lines_fall_back_to_defaults() {
    let settings = Settings::from_text("font_points=banana\nnonsense\nunknown_key=4\narrow_style=1\n");
    assert_eq!(settings.get(Field::FontPoints), 9);
    assert_eq!(settings.get(Field::ArrowStyle), 1);
}

#[test]
fn values_are_kept_within_their_limits() {
    let mut settings = Settings::default();
    settings.set(Field::FontPoints, 500);
    assert_eq!(settings.get(Field::FontPoints), 12);
    settings.set(Field::FontPoints, -3);
    assert_eq!(settings.get(Field::FontPoints), 7);
    assert_eq!(Settings::from_text("arrow_style=99\n").get(Field::ArrowStyle), 2);
}

#[test]
fn unit_choices_map_to_megabytes_and_megabits() {
    let mut settings = Settings::default();
    assert_eq!(settings.speed_format().unit, Unit::BytesDecimal);
    settings.set(Field::Unit, 1);
    assert_eq!(settings.speed_format().unit, Unit::BitsDecimal);
    settings.set(Field::Scale, 0);
    assert_eq!(settings.speed_format().scale, Scale::Auto);
}

#[test]
fn icon_styles_include_arrows_and_letters_and_the_section_can_turn_them_off() {
    let mut settings = Settings::default();
    assert_eq!(settings.arrow_symbols(), ("\u{2191}", "\u{2193}"));
    settings.set(Field::ArrowStyle, 2);
    assert_eq!(settings.arrow_symbols(), ("U", "D"));
    settings.set(Field::ShowIcons, 0);
    assert_eq!(settings.arrow_symbols(), ("", ""));
    settings.set(Field::ShowIcons, 1);
    assert_eq!(settings.arrow_symbols(), ("U", "D"));
}

#[test]
fn icon_thickness_gets_heavier_at_every_level_and_stays_in_range() {
    let mut settings = Settings::default();
    assert_eq!(settings.get(Field::IconThickness), 2);
    assert_eq!(settings.icon_stroke(96), 1);
    let strokes: Vec<i32> = (1..=4)
        .map(|level| {
            settings.set(Field::IconThickness, level);
            settings.icon_stroke(144)
        })
        .collect();
    assert!(strokes.windows(2).all(|pair| pair[0] < pair[1]), "{strokes:?}");
    settings.set(Field::IconThickness, 1);
    assert_eq!((settings.icon_stroke(96), settings.icon_weight()), (0, 400));
    settings.set(Field::IconThickness, 3);
    assert_eq!((settings.icon_stroke(96), settings.icon_weight()), (2, 900));
    settings.set(Field::IconThickness, 99);
    assert_eq!(settings.get(Field::IconThickness), 4);
    settings.set(Field::IconThickness, 0);
    assert_eq!(settings.get(Field::IconThickness), 1);
}

#[test]
fn turning_a_section_off_and_on_keeps_its_choices() {
    let mut settings = Settings::default();
    settings.set(Field::IconThickness, 3);
    settings.set(Field::ShowIcons, 0);
    assert_eq!(settings.icon_stroke(96), 0);
    assert_eq!(settings.get(Field::IconThickness), 3);
    settings.set(Field::ShowIcons, 1);
    assert_eq!(settings.icon_stroke(96), 2);

    assert_eq!(settings.background(), None);
    settings.set(Field::ShowBackground, 1);
    assert_eq!(settings.background(), Some(0x60A5FA));
    settings.set(Field::BackgroundColor, 0xF87171);
    settings.set(Field::ShowBackground, 0);
    assert_eq!(settings.background(), None);
    settings.set(Field::ShowBackground, 1);
    assert_eq!(settings.background(), Some(0xF87171));
    assert_eq!(Settings::from_text(&settings.to_text()), settings);
    settings.set(Field::ShowBackground, 0);
    assert_eq!(Settings::from_text(&settings.to_text()), settings);
}

#[test]
fn every_row_under_a_toggle_heading_names_that_toggle() {
    for field in [Field::ArrowStyle, Field::IconThickness, Field::UpColor, Field::DownColor] {
        assert_eq!(Settings::section_switch(field), Some(Field::ShowIcons));
    }
    for field in [Field::BackgroundColor, Field::BackgroundOpacity] {
        assert_eq!(Settings::section_switch(field), Some(Field::ShowBackground));
    }
    for field in [Field::LowLimit, Field::LowColor, Field::HighLimit, Field::HighColor] {
        assert_eq!(Settings::section_switch(field), Some(Field::ColorCoding));
    }
    for field in [Field::FontFamily, Field::FontPoints, Field::TextColor, Field::SwapRows] {
        assert_eq!(Settings::section_switch(field), None);
    }
}

#[test]
fn old_files_with_no_icons_or_no_background_carry_over() {
    let old = Settings::from_text("arrow_style=3\nbackground_color=-1\n");
    assert!(!old.is_on(Field::ShowIcons));
    assert_eq!(old.get(Field::ArrowStyle), 0);
    assert_eq!(old.background(), None);
    assert_eq!(old.get(Field::BackgroundColor), 0x60A5FA);

    let with_colour = Settings::from_text("arrow_style=1\nbackground_color=16218294\n");
    assert!(with_colour.is_on(Field::ShowIcons));
    assert_eq!(with_colour.get(Field::ArrowStyle), 1);
    assert_eq!(with_colour.background(), Some(16218294));
}

#[test]
fn only_short_labels_grey_out_now_that_sections_collapse() {
    let mut settings = Settings::default();
    assert!(settings.is_relevant(Field::ShortLabels));
    settings.set(Field::HideSuffix, 1);
    assert!(!settings.is_relevant(Field::ShortLabels));
    assert!(settings.is_relevant(Field::Decimals));
}

#[test]
fn hover_shows_the_card_and_click_opens_usage_by_default() {
    let settings = Settings::default();
    assert_eq!(settings.meter_action(Field::HoverAction), MeterAction::Card);
    assert_eq!(settings.meter_action(Field::ClickAction), MeterAction::UsageWindow);
}

#[test]
fn old_on_off_choices_carry_over() {
    let both_off = Settings::from_text("hover_card=0\nclick_opens_usage=0\n");
    assert_eq!(both_off.meter_action(Field::HoverAction), MeterAction::Nothing);
    assert_eq!(both_off.meter_action(Field::ClickAction), MeterAction::Nothing);
    let both_on = Settings::from_text("hover_card=1\nclick_opens_usage=1\n");
    assert_eq!(both_on, Settings::default());
}

#[test]
fn any_action_can_go_on_hover_or_click_and_is_saved() {
    let mut settings = Settings::default();
    settings.set(Field::HoverAction, 0);
    settings.set(Field::ClickAction, 1);
    let reloaded = Settings::from_text(&settings.to_text());
    assert_eq!(reloaded.meter_action(Field::HoverAction), MeterAction::Nothing);
    assert_eq!(reloaded.meter_action(Field::ClickAction), MeterAction::Card);
    assert!(!settings.to_text().contains("click_opens_usage"));
}

#[test]
fn an_action_out_of_range_stays_in_range() {
    let settings = Settings::from_text("hover_action=9\nclick_action=-3\n");
    assert_eq!(settings.meter_action(Field::HoverAction), MeterAction::UsageWindow);
    assert_eq!(settings.meter_action(Field::ClickAction), MeterAction::Nothing);
}

#[test]
fn usage_and_hover_figures_use_the_chosen_icon_colour() {
    const AUTOMATIC: u32 = 0x4C9AFF;
    const DARK_CARD: u32 = 0x2B2B2B;
    const WHITE_CARD: u32 = 0xFFFFFF;
    assert_eq!(figure_color(COLOR_AUTO, AUTOMATIC, DARK_CARD), AUTOMATIC);
    assert_eq!(figure_color(0xF472B6, AUTOMATIC, DARK_CARD), 0xF472B6);
    assert_eq!(figure_color(0xFFFFFF, AUTOMATIC, DARK_CARD), 0xFFFFFF);
    let white_on_white = figure_color(0xFFFFFF, AUTOMATIC, WHITE_CARD);
    assert_ne!(white_on_white, WHITE_CARD);
    assert_ne!(white_on_white, AUTOMATIC);
}

#[test]
fn network_choice_selects_the_adapters_counted() {
    let mut settings = Settings::default();
    assert_eq!(settings.interface_mode(), InterfaceMode::AllPhysical);
    settings.set(Field::InterfaceMode, 1);
    assert_eq!(settings.interface_mode(), InterfaceMode::Auto);
    settings.set(Field::InterfaceMode, 2);
    settings.chosen_adapters = vec![7];
    assert_eq!(settings.interface_mode(), InterfaceMode::Selected(vec![7]));
}

#[test]
fn automatic_colour_falls_back_and_a_chosen_colour_wins() {
    let mut settings = Settings::default();
    assert_eq!(settings.get(Field::TextColor), COLOR_AUTO);
    assert_eq!(settings.color_or(Field::TextColor, BASE), BASE);
    settings.set(Field::TextColor, 0xF87171);
    assert_eq!(settings.color_or(Field::TextColor, BASE), 0xF87171);
}

#[test]
fn colour_by_speed_uses_the_limits_only_when_on() {
    let mut settings = Settings::default();
    settings.set(Field::LowColor, 0x111111);
    settings.set(Field::HighColor, 0x222222);
    assert_eq!(speed_color(&settings, 50_000_000.0, BASE), BASE);
    settings.set(Field::ColorCoding, 1);
    assert_eq!(speed_color(&settings, 50_000_000.0, BASE), 0x222222);
    assert_eq!(speed_color(&settings, 500_000.0, BASE), 0x111111);
    assert_eq!(speed_color(&settings, 5_000_000.0, BASE), BASE);
    assert_eq!(speed_color(&settings, 0.0, BASE), BASE);
}

#[test]
fn history_choice_sets_how_long_usage_is_kept() {
    let mut settings = Settings::default();
    assert_eq!(settings.keep_days(), 731);
    settings.set(Field::KeepHistory, 0);
    assert_eq!(settings.keep_days(), 93);
}
