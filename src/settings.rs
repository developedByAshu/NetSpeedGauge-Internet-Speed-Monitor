//! Every user choice, with its default and limits in one table, saved as a small text file.

use crate::format::{Scale, SpeedFormat, Unit};
use crate::net_counters::InterfaceMode;
use crate::usage_store::data_folder;
use windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;

// Messages other windows send to the readout window.
pub const WM_APP_OPEN_USAGE: u32 = WM_APP + 2;
pub const WM_APP_OPEN_SETTINGS: u32 = WM_APP + 3;
pub const WM_APP_EXIT: u32 = WM_APP + 4;
pub const WM_APP_SETTINGS_CHANGED: u32 = WM_APP + 5;

const FILE_NAME: &str = "settings.ini";
const TEMP_FILE_NAME: &str = "settings.ini.tmp";
const CHOSEN_ADAPTERS_KEY: &str = "chosen_adapters";

/// A colour value meaning "follow the default" rather than a specific colour.
pub const COLOR_AUTO: i64 = -1;

/// Colours offered in the colour pickers.
pub const PALETTE: [u32; 10] = [
    0xFFFFFF, 0xA6A6A6, 0xF87171, 0xFB923C, 0xFACC15, 0x4ADE80, 0x2DD4BF, 0x60A5FA, 0xC084FC, 0xF472B6,
];

pub const FONT_FAMILIES: [&str; 4] = ["Segoe UI", "Segoe UI Semibold", "Bahnschrift", "Consolas"];

/// The shapes offered for the upload and download markers, as (up, down).
pub const ARROW_STYLES: [(&str, &str); 3] = [("\u{2191}", "\u{2193}"), ("\u{25B2}", "\u{25BC}"), ("U", "D")];
const NO_ARROWS: (&str, &str) = ("", "");
// Older versions saved "no icons" as this icon style and "no background" as a negative colour.
const OLD_NO_ICONS_STYLE: &str = "3";
const OLD_NO_BACKGROUND_SIGN: char = '-';
// The blue in the palette: visible behind white text, unlike white itself.
const FIRST_BACKGROUND_COLOR: i64 = 0x60A5FA;

const BASE_DPI: i64 = 96;
const REGULAR_WEIGHT: i32 = 400;
const HEAVY_WEIGHT: i32 = 900;
const THINNEST_ICON: i64 = 1;
const UPDATE_MILLISECONDS: [u32; 4] = [500, 1000, 2000, 5000];
const KEEP_DAYS_CHOICES: [i32; 4] = [93, 186, 366, 731];
const UNITS: [Unit; 4] = [Unit::BytesDecimal, Unit::BitsDecimal, Unit::BytesBinary, Unit::BitsBinary];

/// One saved choice. The order matches `FIELDS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    ArrowStyle,
    IconThickness,
    FontFamily,
    FontPoints,
    TextColor,
    UpColor,
    DownColor,
    BackgroundColor,
    BackgroundOpacity,
    ColorCoding,
    LowLimit,
    HighLimit,
    LowColor,
    HighColor,
    Unit,
    Scale,
    Decimals,
    HideSuffix,
    ShortLabels,
    SwapRows,
    UpdateRate,
    SmartUpdates,
    OffsetX,
    OffsetY,
    InterfaceMode,
    KeepHistory,
    HoverAction,
    KeepOnTop,
    WindowTheme,
    ClickAction,
    ShowIcons,
    ShowBackground,
}

/// What resting on or clicking the numbers does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeterAction {
    Nothing,
    Card,
    UsageWindow,
}

// Older versions saved these as on or off. On matches today's defaults, so only off carries over.
const OLD_ON_OFF_KEYS: [(&str, Field); 2] = [("hover_card", Field::HoverAction), ("click_opens_usage", Field::ClickAction)];

/// A choice's name in the file, starting value and limits.
pub struct FieldSpec {
    pub field: Field,
    pub key: &'static str,
    pub default: i64,
    pub min: i64,
    pub max: i64,
}

const fn spec(field: Field, key: &'static str, default: i64, min: i64, max: i64) -> FieldSpec {
    FieldSpec { field, key, default, min, max }
}

const MAX_COLOR: i64 = 0xFF_FFFF;

pub const FIELDS: [FieldSpec; 32] = [
    spec(Field::ArrowStyle, "arrow_style", 0, 0, ARROW_STYLES.len() as i64 - 1),
    spec(Field::IconThickness, "icon_thickness", 2, THINNEST_ICON, 4),
    spec(Field::FontFamily, "font_family", 0, 0, 3),
    spec(Field::FontPoints, "font_points", 9, 7, 12),
    spec(Field::TextColor, "text_color", COLOR_AUTO, COLOR_AUTO, MAX_COLOR),
    spec(Field::UpColor, "up_color", COLOR_AUTO, COLOR_AUTO, MAX_COLOR),
    spec(Field::DownColor, "down_color", COLOR_AUTO, COLOR_AUTO, MAX_COLOR),
    spec(Field::BackgroundColor, "background_color", FIRST_BACKGROUND_COLOR, 0, MAX_COLOR),
    spec(Field::BackgroundOpacity, "background_opacity", 60, 10, 100),
    spec(Field::ColorCoding, "color_coding", 0, 0, 1),
    spec(Field::LowLimit, "low_limit", 1, 1, 100),
    spec(Field::HighLimit, "high_limit", 10, 1, 1000),
    spec(Field::LowColor, "low_color", 0xFB923C, 0, MAX_COLOR),
    spec(Field::HighColor, "high_color", 0x4ADE80, 0, MAX_COLOR),
    spec(Field::Unit, "unit", 0, 0, 3),
    spec(Field::Scale, "scale", 1, 0, 1),
    spec(Field::Decimals, "decimals", 1, 0, 2),
    spec(Field::HideSuffix, "hide_suffix", 0, 0, 1),
    spec(Field::ShortLabels, "short_labels", 0, 0, 1),
    spec(Field::SwapRows, "swap_rows", 0, 0, 1),
    spec(Field::UpdateRate, "update_rate", 1, 0, 3),
    spec(Field::SmartUpdates, "smart_updates", 1, 0, 1),
    spec(Field::OffsetX, "offset_x", 0, -200, 200),
    spec(Field::OffsetY, "offset_y", 0, -20, 20),
    spec(Field::InterfaceMode, "interface_mode", 0, 0, 2),
    spec(Field::KeepHistory, "keep_history", 3, 0, 3),
    spec(Field::HoverAction, "hover_action", 1, 0, 2),
    spec(Field::KeepOnTop, "keep_on_top", 0, 0, 1),
    spec(Field::WindowTheme, "window_theme", 0, 0, 2),
    spec(Field::ClickAction, "click_action", 2, 0, 2),
    spec(Field::ShowIcons, "show_icons", 1, 0, 1),
    spec(Field::ShowBackground, "show_background", 0, 0, 1),
];

/// All the user's choices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    values: [i64; FIELDS.len()],
    pub chosen_adapters: Vec<u64>,
    // False for previews, so they never overwrite the user's real choices.
    pub is_saved_to_disk: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let mut values = [0; FIELDS.len()];
        FIELDS.iter().enumerate().for_each(|(position, spec)| values[position] = spec.default);
        Self { values, chosen_adapters: Vec::new(), is_saved_to_disk: true }
    }
}

impl Settings {
    pub fn get(&self, field: Field) -> i64 {
        self.values[field as usize]
    }

    pub fn is_on(&self, field: Field) -> bool {
        self.get(field) != 0
    }

    /// Sets a choice, keeping it within its limits.
    pub fn set(&mut self, field: Field, value: i64) {
        let spec = &FIELDS[field as usize];
        self.values[field as usize] = value.clamp(spec.min, spec.max);
    }

    /// Puts one choice back to its default.
    pub fn reset(&mut self, field: Field) {
        self.values[field as usize] = FIELDS[field as usize].default;
    }

    /// Returns what the hover or click choice asks for.
    pub fn meter_action(&self, field: Field) -> MeterAction {
        match self.get(field) {
            1 => MeterAction::Card,
            2 => MeterAction::UsageWindow,
            _ => MeterAction::Nothing,
        }
    }

    /// Returns a colour choice, or the given colour when it is on automatic.
    pub fn color_or(&self, field: Field, automatic: u32) -> u32 {
        u32::try_from(self.get(field)).unwrap_or(automatic)
    }

    pub fn arrow_symbols(&self) -> (&'static str, &'static str) {
        if self.is_on(Field::ShowIcons) { ARROW_STYLES[self.get(Field::ArrowStyle) as usize] } else { NO_ARROWS }
    }

    /// Returns the colour behind the numbers, or None while the background is off.
    pub fn background(&self) -> Option<u32> {
        if self.is_on(Field::ShowBackground) { u32::try_from(self.get(Field::BackgroundColor)).ok() } else { None }
    }

    /// Returns the heading toggle that shows or collapses a setting's row, if it has one.
    pub fn section_switch(field: Field) -> Option<Field> {
        match field {
            Field::ArrowStyle | Field::IconThickness | Field::UpColor | Field::DownColor => Some(Field::ShowIcons),
            Field::BackgroundColor | Field::BackgroundOpacity => Some(Field::ShowBackground),
            Field::LowLimit | Field::LowColor | Field::HighLimit | Field::HighColor => Some(Field::ColorCoding),
            _ => None,
        }
    }

    /// Returns the font weight for the icons. Only the thinnest level uses the regular weight.
    pub fn icon_weight(&self) -> i32 {
        if self.get(Field::IconThickness) > THINNEST_ICON { HEAVY_WEIGHT } else { REGULAR_WEIGHT }
    }

    /**
     * Returns how many extra pixels wide the icons are at a screen scaling. This font's arrows stay
     * thin even at its heaviest weight, so thicker levels are drawn several times side by side.
     * The taskbar readout and the Settings preview both use it.
     */
    pub fn icon_stroke(&self, dpi: u32) -> i32 {
        if !self.is_on(Field::ShowIcons) {
            return 0;
        }
        ((self.get(Field::IconThickness) - THINNEST_ICON) * i64::from(dpi) / BASE_DPI) as i32
    }

    /// Returns false when another choice leaves this one with no effect. Its value is kept for when it applies again.
    pub fn is_relevant(&self, field: Field) -> bool {
        match field {
            Field::ShortLabels => !self.is_on(Field::HideSuffix),
            _ => true,
        }
    }

    pub fn font_family(&self) -> &'static str {
        FONT_FAMILIES[self.get(Field::FontFamily) as usize]
    }

    pub fn speed_format(&self) -> SpeedFormat {
        SpeedFormat {
            unit: UNITS[self.get(Field::Unit) as usize],
            scale: if self.is_on(Field::Scale) { Scale::AlwaysMega } else { Scale::Auto },
            decimals: self.get(Field::Decimals) as usize,
            hide_suffix: self.is_on(Field::HideSuffix),
            short_labels: self.is_on(Field::ShortLabels),
        }
    }

    pub fn interface_mode(&self) -> InterfaceMode {
        match self.get(Field::InterfaceMode) {
            1 => InterfaceMode::Auto,
            2 => InterfaceMode::Selected(self.chosen_adapters.clone()),
            _ => InterfaceMode::AllPhysical,
        }
    }

    pub fn update_milliseconds(&self) -> u32 {
        UPDATE_MILLISECONDS[self.get(Field::UpdateRate) as usize]
    }

    pub fn keep_days(&self) -> i32 {
        KEEP_DAYS_CHOICES[self.get(Field::KeepHistory) as usize]
    }

    /// Converts the choices to the text saved on disk.
    pub fn to_text(&self) -> String {
        let mut text: String = FIELDS
            .iter()
            .enumerate()
            .map(|(position, spec)| format!("{}={}\n", spec.key, self.values[position]))
            .collect();
        let chosen: Vec<String> = self.chosen_adapters.iter().map(u64::to_string).collect();
        text.push_str(&format!("{CHOSEN_ADAPTERS_KEY}={}\n", chosen.join(",")));
        text
    }

    /// Reads choices from saved text. Unknown lines and bad values fall back to the defaults.
    pub fn from_text(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key == CHOSEN_ADAPTERS_KEY {
                settings.chosen_adapters = value.split(',').filter_map(|luid| luid.parse().ok()).collect();
            } else if let Some((_, field)) = OLD_ON_OFF_KEYS.iter().find(|(old_key, _)| *old_key == key) {
                if value.trim() == "0" {
                    settings.set(*field, 0);
                }
            } else if key == "arrow_style" && value.trim() == OLD_NO_ICONS_STYLE {
                settings.set(Field::ShowIcons, 0);
            } else if key == "background_color" && value.trim().starts_with(OLD_NO_BACKGROUND_SIGN) {
                // Off is already the default, and this keeps the first colour for when it is turned on.
            } else if let (Some(spec), Ok(number)) =
                (FIELDS.iter().find(|spec| spec.key == key), value.trim().parse::<i64>())
            {
                settings.set(spec.field, number);
                // An older file with a colour had the background on. A newer file's own switch comes later and wins.
                if spec.field == Field::BackgroundColor {
                    settings.set(Field::ShowBackground, 1);
                }
            }
        }
        settings
    }

    /// Loads the saved choices, or the defaults if there are none yet.
    pub fn load() -> Self {
        std::fs::read_to_string(data_folder().join(FILE_NAME)).map_or_else(|_| Self::default(), |text| Self::from_text(&text))
    }

    /// Saves the choices through a temporary file, so a crash cannot damage them. Previews skip this.
    pub fn save(&self) {
        if !self.is_saved_to_disk {
            return;
        }
        let folder = data_folder();
        let temp_path = folder.join(TEMP_FILE_NAME);
        let wrote = std::fs::create_dir_all(&folder).and_then(|()| std::fs::write(&temp_path, self.to_text()));
        if wrote.is_ok() {
            let _ = std::fs::rename(&temp_path, folder.join(FILE_NAME));
        }
    }
}
