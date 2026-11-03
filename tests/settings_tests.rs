//! Tests for saving settings and for what each one changes.

use netspeedgauge::format::{Scale, Unit};
use netspeedgauge::net_counters::InterfaceMode;
use netspeedgauge::settings::{Field, Settings, COLOR_AUTO, FIELDS};
use netspeedgauge::settings_ui::{page_fields, speed_color};
use netspeedgauge::theme::figure_color;

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
    assert_eq!(Settings::from_text("arrow_style=99\n").get(Field::ArrowStyle), 3);
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
fn icon_styles_include_arrows_letters_and_none() {
    let mut settings = Settings::default();
    assert_eq!(settings.arrow_symbols(), ("\u{2191}", "\u{2193}"));
    settings.set(Field::ArrowStyle, 2);
    assert_eq!(settings.arrow_symbols(), ("U", "D"));
    settings.set(Field::ArrowStyle, 3);
    assert_eq!(settings.arrow_symbols(), ("", ""));
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
fn settings_with_no_effect_are_reported_and_keep_their_values() {
    let mut settings = Settings::default();
    assert!(settings.is_relevant(Field::IconThickness) && settings.is_relevant(Field::UpColor));
    settings.set(Field::IconThickness, 3);
    settings.set(Field::ArrowStyle, 3);
    for field in [Field::IconThickness, Field::UpColor, Field::DownColor] {
        assert!(!settings.is_relevant(field));
    }
    assert_eq!(settings.icon_stroke(96), 0);
    assert_eq!(settings.get(Field::IconThickness), 3);
    settings.set(Field::ArrowStyle, 0);
    assert_eq!(settings.icon_stroke(96), 2);

    assert!(!settings.is_relevant(Field::BackgroundOpacity));
    settings.set(Field::BackgroundColor, 0x60A5FA);
    assert!(settings.is_relevant(Field::BackgroundOpacity));

    for field in [Field::LowLimit, Field::LowColor, Field::HighLimit, Field::HighColor] {
        assert!(!settings.is_relevant(field));
    }
    settings.set(Field::ColorCoding, 1);
    assert!(settings.is_relevant(Field::HighColor));

    assert!(settings.is_relevant(Field::ShortLabels));
    settings.set(Field::HideSuffix, 1);
    assert!(!settings.is_relevant(Field::ShortLabels));
    assert!(settings.is_relevant(Field::Decimals));
}

#[test]
fn clicking_the_numbers_opens_usage_by_default_and_can_be_turned_off() {
    assert!(Settings::default().is_on(Field::ClickOpensUsage));
    assert!(!Settings::from_text("click_opens_usage=0\n").is_on(Field::ClickOpensUsage));
}

#[test]
fn download_on_top_belongs_to_the_appearance_page() {
    assert!(page_fields(1).contains(&Field::SwapRows));
    assert!(!page_fields(2).contains(&Field::SwapRows));
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

#[test]
fn reset_restores_defaults_for_a_page_without_touching_others() {
    let mut settings = Settings::default();
    settings.set(Field::ArrowStyle, 2);
    settings.set(Field::UpdateRate, 3);
    page_fields(1).into_iter().for_each(|field| settings.reset(field));
    assert_eq!(settings.get(Field::ArrowStyle), 0);
    assert_eq!(settings.get(Field::UpdateRate), 3);
}
