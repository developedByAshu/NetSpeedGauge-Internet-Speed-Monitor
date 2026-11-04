//! Draws the two speed rows into a see-through window using plain Windows drawing.

use crate::canvas::{mix, round_rect_coverage};
use crate::format::widest_speed_text;
use crate::settings::{Field, Settings};
use crate::taskbar_dock::to_wide;
use windows_sys::Win32::Foundation::{HWND, POINT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, GdiFlush,
    GetTextExtentPoint32W, GetTextMetricsW, SelectObject, SetBkMode, SetTextAlign, SetTextColor, TextOutW,
    AC_SRC_ALPHA, AC_SRC_OVER, ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, FF_DONTCARE, HBITMAP, HDC, HFONT,
    OUT_DEFAULT_PRECIS, TA_LEFT, TA_RIGHT, TEXTMETRICW, TRANSPARENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};

const FONT_WEIGHT: i32 = 400;
const POINTS_PER_INCH: i32 = 72;
const SIDE_PADDING: i32 = 2;
const BASE_DPI: i32 = 96;

// The gap between an arrow and its number is the width of this text.
const ARROW_SPACING: &str = "    ";

// Space between the upload and download rows, at 100% scaling.
const ROW_GAP: i32 = 3;

// The optional background's padding and corner radius, at 100% scaling.
const BOX_SIDE_PADDING: i32 = 7;
const BOX_TOP_PADDING: i32 = 3;
const BOX_RADIUS: i32 = 6;

// Near-invisible background so the empty parts of the window still receive mouse clicks.
const CLICKABLE_ALPHA: u32 = 1;

// The background behind the numbers while their menu or card is open. Measured from Windows' own selected
// tray highlight on a dark taskbar: colour 41 over 28, which is 6% of white, and 40 pixels tall at 100% scaling.
const SELECTED_ALPHA: u32 = 15;
const SELECTED_MIX: f32 = 0.06;
const SELECTED_HEIGHT: i32 = 40;

/// The colours for one drawing of the readout, as 0xRRGGBB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadoutColors {
    pub up_arrow: u32,
    pub down_arrow: u32,
    pub up_value: u32,
    pub down_value: u32,
    pub background: u32,
    pub background_alpha: u32,
}

/// The see-through drawing surface behind the readout window.
pub struct Readout {
    window: HWND,
    surface: HDC,
    bitmap: HBITMAP,
    font: HFONT,
    arrow_font: HFONT,
    pixels: *mut u32,
    pub width: i32,
    pub height: i32,
    row_step: i32,
    value_right: i32,
    arrow_gap: i32,
    arrow_stroke: i32,
    arrow_width: i32,
    top_offset: i32,
    box_padding: i32,
    box_radius: f32,
    arrows: (&'static str, &'static str),
    is_swapped: bool,
    dpi: u32,
    room_for_selected: i32,
}

/// Returns the pixel size of a string in the current font.
fn measure(surface: HDC, text: &str) -> SIZE {
    let wide: Vec<u16> = text.encode_utf16().collect();
    let mut size = SIZE { cx: 0, cy: 0 };
    // SAFETY: `wide` and `size` are valid for the duration of the call.
    unsafe { GetTextExtentPoint32W(surface, wide.as_ptr(), wide.len() as i32, &mut size) };
    size
}

/// Creates a font at the given weight and screen scaling.
fn create_font(family: &str, points: i32, weight: i32, dpi: u32) -> HFONT {
    let family = to_wide(family);
    // SAFETY: `family` is null-terminated and outlives the call. The caller owns the returned font.
    unsafe {
        CreateFontW(
            -(points * dpi as i32 / POINTS_PER_INCH),
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            u32::from(DEFAULT_CHARSET),
            u32::from(OUT_DEFAULT_PRECIS),
            u32::from(CLIP_DEFAULT_PRECIS),
            u32::from(ANTIALIASED_QUALITY),
            u32::from(DEFAULT_PITCH) | u32::from(FF_DONTCARE),
            family.as_ptr(),
        )
    }
}

/**
 * Returns where the arrows start. Both rows share this column, set by the wider number, so the two
 * arrows always line up. The taskbar readout and the Settings preview both use it.
 */
pub fn arrow_column_left(value_right: i32, widest_value: i32, gap: i32, stroke: i32, arrow_width: i32) -> i32 {
    value_right - widest_value - gap - stroke - arrow_width
}

/// Draws one string at the given position in white.
fn draw_text(surface: HDC, x: i32, y: i32, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    // SAFETY: `wide` is valid for the duration of the call.
    unsafe { TextOutW(surface, x, y, wide.as_ptr(), wide.len() as i32) };
}

/**
 * Returns the colours for numbers whose menu or card is open. Only the background changes: with none
 * set it gets a soft one in the taskbar's text colour, and a chosen one shifts towards that colour.
 */
pub fn selected_colors(colors: &ReadoutColors, taskbar_text: u32) -> ReadoutColors {
    let (background, background_alpha) = if colors.background_alpha == 0 {
        (taskbar_text, SELECTED_ALPHA)
    } else {
        (mix(colors.background, taskbar_text, SELECTED_MIX), colors.background_alpha)
    };
    ReadoutColors { background, background_alpha, ..*colors }
}

/**
 * Converts one drawn pixel to its final colour. Windows' text drawing has no transparency, so text
 * is drawn white on black and its brightness becomes how see-through the pixel is, over an optional
 * background colour.
 */
pub fn tint_pixel(drawn: u32, color: u32, background: u32, background_alpha: u32) -> u32 {
    let coverage = (drawn & 0xFF).max((drawn >> 8) & 0xFF).max((drawn >> 16) & 0xFF);
    let under = background_alpha.max(CLICKABLE_ALPHA);
    let remaining = 255 - coverage;
    let blend = |shift: u32| {
        let top = ((color >> shift) & 0xFF) * coverage;
        let bottom = ((background >> shift) & 0xFF) * under * remaining / 255;
        (top + bottom) / 255
    };
    let alpha = coverage + under * remaining / 255;
    (alpha << 24) | (blend(16) << 16) | (blend(8) << 8) | blend(0)
}

impl Readout {
    /**
     * Creates the drawing surface for the window at the given screen scaling.
     * Numbers are right-aligned, and each arrow sits a fixed gap to the left of its own number.
     */
    pub fn new(window: HWND, dpi: u32, settings: &Settings) -> Self {
        let points = settings.get(Field::FontPoints) as i32;
        let font = create_font(settings.font_family(), points, FONT_WEIGHT, dpi);
        let arrow_font = create_font(settings.font_family(), points, settings.icon_weight(), dpi);
        let widest = widest_speed_text(&settings.speed_format());
        let arrows = settings.arrow_symbols();
        let row_gap = ROW_GAP * dpi as i32 / BASE_DPI;
        let arrow_stroke = settings.icon_stroke(dpi);
        // SAFETY: the surface and fonts are owned by this struct and released in Drop.
        let has_background = settings.background().is_some();
        // Reserved even with no background, so the selected look has the same room around the numbers.
        let box_padding = BOX_SIDE_PADDING * dpi as i32 / BASE_DPI;
        let box_radius = (BOX_RADIUS * dpi as i32 / BASE_DPI) as f32;
        let (surface, row_height, width, arrow_gap, arrow_width) = unsafe {
            let surface = CreateCompatibleDC(std::ptr::null_mut());
            SelectObject(surface, arrow_font);
            let arrow_width = measure(surface, arrows.0).cx.max(measure(surface, arrows.1).cx);
            SelectObject(surface, font);
            SetBkMode(surface, TRANSPARENT as i32);
            SetTextColor(surface, 0x00FF_FFFF);
            let mut metrics: TEXTMETRICW = std::mem::zeroed();
            GetTextMetricsW(surface, &mut metrics);
            let arrow_gap = if arrows.0.is_empty() { 0 } else { measure(surface, ARROW_SPACING).cx };
            let width = arrow_width + arrow_stroke + arrow_gap + measure(surface, &widest).cx + (SIDE_PADDING + box_padding) * 2;
            (surface, metrics.tmHeight, width, arrow_gap, arrow_width)
        };
        let rows_height = row_height * 2 + row_gap;
        let top_offset = if has_background {
            BOX_TOP_PADDING * dpi as i32 / BASE_DPI
        } else {
            ((SELECTED_HEIGHT * dpi as i32 / BASE_DPI - rows_height) / 2).max(0)
        };
        let height = rows_height + top_offset * 2;
        // SAFETY: the header describes a top-down 32-bit bitmap, and `pixels` receives its memory.
        let (bitmap, pixels) = unsafe {
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = width;
            info.bmiHeader.biHeight = -height;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut pixels = std::ptr::null_mut();
            let bitmap = CreateDIBSection(surface, &info, DIB_RGB_COLORS, &mut pixels, std::ptr::null_mut(), 0);
            SelectObject(surface, bitmap);
            (bitmap, pixels.cast::<u32>())
        };
        let row_step = row_height + row_gap;
        let value_right = width - SIDE_PADDING - box_padding;
        let is_swapped = settings.is_on(Field::SwapRows);
        Self { window, surface, bitmap, font, arrow_font, pixels, width, height, row_step, value_right, arrow_gap, arrow_stroke, arrow_width, top_offset, box_padding, box_radius, arrows, is_swapped, dpi, room_for_selected: if has_background { 0 } else { box_padding } }
    }

    /// Returns the screen scaling this surface was built for.
    pub fn dpi(&self) -> u32 {
        self.dpi
    }

    /// Returns the side space reserved only for the selected look, so the numbers can stay put without it showing.
    pub fn room_for_selected(&self) -> i32 {
        self.room_for_selected
    }

    /// Draws both rows and puts them on screen.
    pub fn draw(&self, up_text: &str, down_text: &str, colors: &ReadoutColors) {
        let pixel_count = (self.width * self.height) as usize;
        // SAFETY: the bitmap holds exactly width * height 32-bit pixels and lives as long as self.
        let pixels = unsafe {
            GdiFlush();
            std::slice::from_raw_parts_mut(self.pixels, pixel_count)
        };
        pixels.fill(0);

        let up_row = (self.arrows.0, up_text, colors.up_arrow, colors.up_value);
        let down_row = (self.arrows.1, down_text, colors.down_arrow, colors.down_value);
        let rows = if self.is_swapped { [down_row, up_row] } else { [up_row, down_row] };
        // SAFETY: the surface and font are valid for the life of self.
        unsafe { SelectObject(self.surface, self.font) };
        let widest_value = rows.iter().map(|row| measure(self.surface, row.1).cx).max().unwrap_or(0);
        let arrow_left =
            arrow_column_left(self.value_right, widest_value, self.arrow_gap, self.arrow_stroke, self.arrow_width);
        let value_column = self.value_right - widest_value;
        for (position, (arrow, value, _, _)) in rows.iter().enumerate() {
            let y = self.top_offset + position as i32 * self.row_step;
            // SAFETY: the surface and fonts are valid for the life of self.
            unsafe {
                SelectObject(self.surface, self.arrow_font);
                SetTextAlign(self.surface, TA_LEFT);
            }
            for offset in 0..=self.arrow_stroke {
                draw_text(self.surface, arrow_left + offset, y, arrow);
            }
            // SAFETY: the surface and fonts are valid for the life of self.
            unsafe {
                SelectObject(self.surface, self.font);
                SetTextAlign(self.surface, TA_RIGHT);
            }
            draw_text(self.surface, self.value_right, y, value);
        }
        // SAFETY: this waits for Windows to finish drawing before the pixels are read.
        unsafe { GdiFlush() };

        // The background hugs the arrows and numbers, with padding and rounded corners.
        let box_left = (arrow_left - self.box_padding - SIDE_PADDING).max(0) as f32;
        let box_width = self.width as f32 - box_left;
        let box_height = self.height as f32;
        for (index, pixel) in pixels.iter_mut().enumerate() {
            let column = index as i32 % self.width;
            let row = index as i32 / self.width;
            let position = usize::from(row >= self.top_offset + self.row_step);
            let (_, _, arrow_color, value_color) = rows[position];
            let color = if column < value_column - self.arrow_gap / 2 { arrow_color } else { value_color };
            let under = if colors.background_alpha == 0 {
                0
            } else {
                let cover = round_rect_coverage(column as f32 + 0.5, row as f32 + 0.5, box_left, 0.0, box_width, box_height, self.box_radius);
                (colors.background_alpha as f32 * cover) as u32
            };
            *pixel = tint_pixel(*pixel, color, colors.background, under);
        }

        let size = SIZE { cx: self.width, cy: self.height };
        let source = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: all pointers refer to locals or self-owned objects valid during the call.
        unsafe {
            UpdateLayeredWindow(
                self.window,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
                self.surface,
                &source,
                0,
                &blend,
                ULW_ALPHA,
            );
        }
    }
}

impl Drop for Readout {
    fn drop(&mut self) {
        // SAFETY: these handles were created in new() and are released once here.
        unsafe {
            DeleteDC(self.surface);
            DeleteObject(self.bitmap);
            DeleteObject(self.font);
            DeleteObject(self.arrow_font);
        }
    }
}
