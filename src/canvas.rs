//! A drawing surface with smooth rounded shapes and crisp text, using only built-in Windows drawing.

use crate::taskbar_dock::to_wide;
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, GdiFlush,
    GetTextExtentPoint32W, SelectObject, SetBkMode, SetTextAlign, SetTextColor, TextOutW, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH,
    DIB_RGB_COLORS, FF_DONTCARE, HBITMAP, HDC, HFONT, OUT_DEFAULT_PRECIS, SRCCOPY, TA_CENTER, TA_LEFT,
    TA_RIGHT, TRANSPARENT,
};
use windows_sys::Win32::Foundation::SIZE;

const POINTS_PER_INCH: f32 = 72.0;

/// Which side of the given position the text sits on.
#[derive(Clone, Copy)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Blends two 0xRRGGBB colours. An `amount` of 0 gives `from`, and 1 gives `to`.
pub fn mix(from: u32, to: u32, amount: f32) -> u32 {
    let channel = |shift: u32| {
        let start = ((from >> shift) & 0xFF) as f32;
        let end = ((to >> shift) & 0xFF) as f32;
        ((start + (end - start) * amount).round() as u32).min(255) << shift
    };
    channel(16) | channel(8) | channel(0)
}

/**
 * Returns how much of a pixel a rounded rectangle covers, from 0 to 1. Partly covered edge pixels
 * are what make corners look smooth rather than jagged.
 */
pub fn round_rect_coverage(px: f32, py: f32, x: f32, y: f32, width: f32, height: f32, radius: f32) -> f32 {
    let half_width = width / 2.0;
    let half_height = height / 2.0;
    let offset_x = (px - (x + half_width)).abs() - (half_width - radius);
    let offset_y = (py - (y + half_height)).abs() - (half_height - radius);
    let outside = offset_x.max(0.0).hypot(offset_y.max(0.0));
    let inside = offset_x.max(offset_y).min(0.0);
    (0.5 - (outside + inside - radius)).clamp(0.0, 1.0)
}

/// Creates a font at a size in points for the given screen scaling.
pub fn create_font(family: &str, points: f32, weight: i32, dpi: u32) -> HFONT {
    let family = to_wide(family);
    // SAFETY: `family` is null-terminated and outlives the call. The caller owns the returned font.
    unsafe {
        CreateFontW(
            -((points * dpi as f32 / POINTS_PER_INCH).round() as i32),
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
            u32::from(CLEARTYPE_QUALITY),
            u32::from(DEFAULT_PITCH) | u32::from(FF_DONTCARE),
            family.as_ptr(),
        )
    }
}

/// An off-screen picture of the window. It is painted fully, then shown in one step, so it never flickers.
pub struct Canvas {
    surface: HDC,
    bitmap: HBITMAP,
    pixels: *mut u32,
    width: i32,
    height: i32,
}

impl Canvas {
    /// Creates a canvas of the given pixel size.
    pub fn new(width: i32, height: i32) -> Self {
        // SAFETY: the header describes a top-down 32-bit bitmap, and `pixels` receives its memory.
        unsafe {
            let surface = CreateCompatibleDC(std::ptr::null_mut());
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
            SetBkMode(surface, TRANSPARENT as i32);
            Self { surface, bitmap, pixels: pixels.cast(), width, height }
        }
    }

    /// Returns the bitmap's pixels.
    fn pixels(&mut self) -> &mut [u32] {
        // SAFETY: Windows has finished its queued drawing after GdiFlush, and the bitmap holds
        // exactly width * height pixels for the life of self.
        unsafe {
            GdiFlush();
            std::slice::from_raw_parts_mut(self.pixels, (self.width * self.height) as usize)
        }
    }

    /// Fills the whole canvas with one colour.
    pub fn clear(&mut self, color: u32) {
        self.pixels().fill(color);
    }

    /// Draws a filled rounded rectangle with smooth edges.
    pub fn round_rect(&mut self, x: i32, y: i32, width: i32, height: i32, radius: f32, color: u32) {
        let (canvas_width, canvas_height) = (self.width, self.height);
        let radius = radius.min(width as f32 / 2.0).min(height as f32 / 2.0);
        let pixels = self.pixels();
        for row in y.max(0)..(y + height).min(canvas_height) {
            for column in x.max(0)..(x + width).min(canvas_width) {
                let coverage = round_rect_coverage(
                    column as f32 + 0.5,
                    row as f32 + 0.5,
                    x as f32,
                    y as f32,
                    width as f32,
                    height as f32,
                    radius,
                );
                if coverage > 0.0 {
                    let pixel = &mut pixels[(row * canvas_width + column) as usize];
                    *pixel = if coverage >= 1.0 { color } else { mix(*pixel, color, coverage) };
                }
            }
        }
    }

    /// Fades what is already drawn in a rounded rectangle towards a colour. An `amount` of 1 covers it fully.
    pub fn fade_round_rect(&mut self, x: i32, y: i32, width: i32, height: i32, radius: f32, color: u32, amount: f32) {
        let (canvas_width, canvas_height) = (self.width, self.height);
        let pixels = self.pixels();
        for row in y.max(0)..(y + height).min(canvas_height) {
            for column in x.max(0)..(x + width).min(canvas_width) {
                let coverage = round_rect_coverage(
                    column as f32 + 0.5,
                    row as f32 + 0.5,
                    x as f32,
                    y as f32,
                    width as f32,
                    height as f32,
                    radius,
                );
                let pixel = &mut pixels[(row * canvas_width + column) as usize];
                *pixel = mix(*pixel, color, coverage * amount);
            }
        }
    }

    /// Draws text in a 0xRRGGBB colour.
    pub fn text(&mut self, x: i32, y: i32, text: &str, font: HFONT, color: u32, align: Align) {
        let wide: Vec<u16> = text.encode_utf16().collect();
        let align_flag = match align {
            Align::Left => TA_LEFT,
            Align::Center => TA_CENTER,
            Align::Right => TA_RIGHT,
        };
        let windows_color = ((color & 0xFF) << 16) | (color & 0xFF00) | ((color >> 16) & 0xFF);
        // SAFETY: the surface and font are valid, and `wide` outlives the call.
        unsafe {
            SelectObject(self.surface, font);
            SetTextAlign(self.surface, align_flag);
            SetTextColor(self.surface, windows_color);
            TextOutW(self.surface, x, y, wide.as_ptr(), wide.len() as i32);
        }
    }

    /// Returns the pixel width of a string in a font.
    pub fn text_width(&self, text: &str, font: HFONT) -> i32 {
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut size = SIZE { cx: 0, cy: 0 };
        // SAFETY: `wide` and `size` are valid for the duration of the calls.
        unsafe {
            SelectObject(self.surface, font);
            GetTextExtentPoint32W(self.surface, wide.as_ptr(), wide.len() as i32, &mut size);
        }
        size.cx
    }

    /// Returns the pixel height of a line of text in a font.
    pub fn text_height(&self, font: HFONT) -> i32 {
        let sample = [u16::from(b'A')];
        let mut size = SIZE { cx: 0, cy: 0 };
        // SAFETY: `sample` and `size` are valid for the duration of the calls.
        unsafe {
            SelectObject(self.surface, font);
            GetTextExtentPoint32W(self.surface, sample.as_ptr(), 1, &mut size);
        }
        size.cy
    }

    /// Copies the finished picture to the screen.
    pub fn present(&self, target: HDC) {
        // SAFETY: both surfaces are valid for the duration of the call.
        unsafe { BitBlt(target, 0, 0, self.width, self.height, self.surface, 0, 0, SRCCOPY) };
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        // SAFETY: these handles were created in new() and are released once here.
        unsafe {
            DeleteDC(self.surface);
            DeleteObject(self.bitmap);
        }
    }
}
