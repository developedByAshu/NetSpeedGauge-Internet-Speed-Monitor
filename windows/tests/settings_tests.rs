//! Tests for which Settings page shows which choice.

use netspeedgauge_windows::settings::{Field, Settings};
use netspeedgauge_windows::settings_ui::page_fields;

#[test]
fn download_on_top_belongs_to_the_appearance_page() {
    assert!(page_fields(1).contains(&Field::SwapRows));
    assert!(!page_fields(2).contains(&Field::SwapRows));
}

#[test]
fn reset_restores_defaults_for_a_page_without_touching_others() {
    let mut settings = Settings::default();
    settings.set(Field::ArrowStyle, 2);
    settings.set(Field::UpdateRate, 3);
    settings.set(Field::ShowIcons, 0);
    settings.set(Field::ShowBackground, 1);
    settings.set(Field::ColorCoding, 1);
    page_fields(1).into_iter().for_each(|field| settings.reset(field));
    assert_eq!(settings.get(Field::ArrowStyle), 0);
    assert_eq!(settings.get(Field::UpdateRate), 3);
    assert!(settings.is_on(Field::ShowIcons));
    assert!(!settings.is_on(Field::ShowBackground));
    assert!(!settings.is_on(Field::ColorCoding));
}
