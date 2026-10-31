//! macOS: the menu-bar image. The icon (with its attention dot) and stacked mini readings, a
//! small label over each value as system monitors show them, drawn with the system font into a
//! template image so it follows the menu bar's light or dark look. `tray-icon` shows the image
//! 18 pt high and as wide as its aspect ratio.

use super::badge::Pixels;
use super::model::MenuBar;
use std::ffi::c_void;

type Ref = *const c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    static kCFBooleanTrue: Ref;
    fn CFStringCreateWithBytes(a: Ref, bytes: *const u8, len: isize, enc: u32, bom: u8) -> Ref;
    fn CFDictionaryCreate(
        a: Ref,
        keys: *const Ref,
        values: *const Ref,
        count: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> Ref;
    fn CFAttributedStringCreate(a: Ref, s: Ref, attributes: Ref) -> Ref;
    fn CFRelease(r: Ref);
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGColorSpaceCreateDeviceRGB() -> Ref;
    fn CGColorSpaceRelease(space: Ref);
    fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        space: Ref,
        info: u32,
    ) -> Ref;
    fn CGContextRelease(context: Ref);
    fn CGContextScaleCTM(context: Ref, sx: f64, sy: f64);
    fn CGContextSetRGBFillColor(context: Ref, r: f64, g: f64, b: f64, a: f64);
    fn CGContextFillRect(context: Ref, rect: CGRect);
    fn CGContextSetTextPosition(context: Ref, x: f64, y: f64);
}

#[link(name = "CoreText", kind = "framework")]
extern "C" {
    static kCTFontAttributeName: Ref;
    static kCTForegroundColorFromContextAttributeName: Ref;
    fn CTFontCreateUIFontForLanguage(kind: u32, size: f64, language: Ref) -> Ref;
    fn CTFontCreateCopyWithSymbolicTraits(
        font: Ref,
        size: f64,
        matrix: *const c_void,
        value: u32,
        mask: u32,
    ) -> Ref;
    fn CTLineCreateWithAttributedString(s: Ref) -> Ref;
    fn CTLineGetTypographicBounds(line: Ref, a: *mut f64, d: *mut f64, l: *mut f64) -> f64;
    fn CTLineDraw(line: Ref, context: Ref);
}

/// Pixels per point: drawn at 3× and shown at 18 pt, so it stays sharp on Retina displays.
const SCALE: f64 = 3.0;
/// The menu bar item's height in points.
const HEIGHT: f64 = 18.0;
const LABEL_SIZE: f64 = 7.0;
const VALUE_SIZE: f64 = 11.5;
/// Baselines from the bottom of the item, in points.
const VALUE_BASELINE: f64 = 1.6;
const LABEL_BASELINE: f64 = 11.9;
const ICON_GAP: f64 = 5.0;
const COLUMN_GAP: f64 = 7.0;
const CHART_WIDTH: f64 = 30.0;
const CHART_HEIGHT: f64 = 13.0;
/// A chart's full height is at least this much CPU, so idle stays low.
const CHART_FLOOR: f32 = 10.0;
/// `kCTFontUIFontSystem`, `kCTFontBoldTrait`, `kCFStringEncodingUTF8`.
const UI_FONT_SYSTEM: u32 = 2;
const BOLD_TRAIT: u32 = 1 << 1;
const UTF8: u32 = 0x0800_0100;
/// `kCGImageAlphaPremultipliedLast` with 32-bit big-endian RGBA.
const RGBA_PREMULTIPLIED: u32 = 1 | (4 << 12);

/// A CoreText line in one font, drawn in the context's fill colour.
struct Line(Ref);

impl Line {
    fn new(text: &str, font: Ref) -> Line {
        unsafe {
            let string = CFStringCreateWithBytes(
                std::ptr::null(),
                text.as_ptr(),
                text.len() as isize,
                UTF8,
                0,
            );
            let keys = [
                kCTFontAttributeName,
                kCTForegroundColorFromContextAttributeName,
            ];
            let values = [font, kCFBooleanTrue];
            let attributes = CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                keys.len() as isize,
                &raw const kCFTypeDictionaryKeyCallBacks,
                &raw const kCFTypeDictionaryValueCallBacks,
            );
            let attributed = CFAttributedStringCreate(std::ptr::null(), string, attributes);
            let line = CTLineCreateWithAttributedString(attributed);
            CFRelease(attributed);
            CFRelease(attributes);
            CFRelease(string);
            Line(line)
        }
    }

    fn width(&self) -> f64 {
        unsafe {
            CTLineGetTypographicBounds(
                self.0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        }
    }

    fn draw(&self, context: Ref, x: f64, baseline: f64) {
        unsafe {
            CGContextSetTextPosition(context, x, baseline);
            CTLineDraw(self.0, context);
        }
    }
}

impl Drop for Line {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}

/// The system font at `size`, bold when asked.
struct Font(Ref);

impl Font {
    fn system(size: f64, bold: bool) -> Font {
        unsafe {
            let regular = CTFontCreateUIFontForLanguage(UI_FONT_SYSTEM, size, std::ptr::null());
            if !bold {
                return Font(regular);
            }
            let heavy = CTFontCreateCopyWithSymbolicTraits(
                regular,
                size,
                std::ptr::null(),
                BOLD_TRAIT,
                BOLD_TRAIT,
            );
            if heavy.is_null() {
                Font(regular)
            } else {
                CFRelease(regular);
                Font(heavy)
            }
        }
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}

/// The value's width with every digit as wide as `8`, so a reading keeps its column while the
/// number changes and the menu bar item doesn't shift on each scan.
fn steady(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_ascii_digit() { '8' } else { c })
        .collect()
}

/// Alpha of `icon` (square) scaled to `size` × `size`, sampled bilinearly.
fn icon_alpha(icon: &Pixels, size: usize) -> Vec<f32> {
    let (w, h) = (icon.width as usize, icon.height as usize);
    let at = |x: usize, y: usize| icon.rgba[(y.min(h - 1) * w + x.min(w - 1)) * 4 + 3] as f32;
    let mut out = vec![0.0; size * size];
    for y in 0..size {
        for x in 0..size {
            let sx = ((x as f32 + 0.5) * w as f32 / size as f32 - 0.5).max(0.0);
            let sy = ((y as f32 + 0.5) * h as f32 / size as f32 - 0.5).max(0.0);
            let (x0, y0) = (sx.floor() as usize, sy.floor() as usize);
            let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
            let top = at(x0, y0) * (1.0 - fx) + at(x0 + 1, y0) * fx;
            let bottom = at(x0, y0 + 1) * (1.0 - fx) + at(x0 + 1, y0 + 1) * fx;
            out[y * size + x] = top * (1.0 - fy) + bottom * fy;
        }
    }
    out
}

/// The menu-bar template image: `icon` (already badged) and then `bar`'s readings and chart.
/// With nothing in `bar`, the icon alone.
pub fn render(icon: &Pixels, bar: &MenuBar) -> Pixels {
    if bar.readings.is_empty() && bar.chart.is_empty() {
        return icon.clone();
    }
    let label_font = Font::system(LABEL_SIZE, true);
    let value_font = Font::system(VALUE_SIZE, false);
    let columns: Vec<(Line, Line, f64)> = bar
        .readings
        .iter()
        .map(|m| {
            let label = Line::new(m.label, label_font.0);
            let value = Line::new(&m.value, value_font.0);
            let width = label
                .width()
                .max(Line::new(&steady(&m.value), value_font.0).width());
            (label, value, width)
        })
        .collect();
    let mut width = HEIGHT + ICON_GAP;
    for (_, _, w) in &columns {
        width += w + COLUMN_GAP;
    }
    if !bar.chart.is_empty() {
        width += CHART_WIDTH + COLUMN_GAP;
    }
    width -= COLUMN_GAP;
    let (pw, ph) = ((width * SCALE).ceil() as usize, (HEIGHT * SCALE) as usize);
    let mut rgba = vec![0u8; pw * ph * 4];
    unsafe {
        let space = CGColorSpaceCreateDeviceRGB();
        let context = CGBitmapContextCreate(
            rgba.as_mut_ptr().cast(),
            pw,
            ph,
            8,
            pw * 4,
            space,
            RGBA_PREMULTIPLIED,
        );
        CGColorSpaceRelease(space);
        if context.is_null() {
            return icon.clone();
        }
        CGContextScaleCTM(context, SCALE, SCALE);
        CGContextSetRGBFillColor(context, 0.0, 0.0, 0.0, 1.0);
        let mut x = HEIGHT + ICON_GAP;
        for (label, value, w) in &columns {
            label.draw(context, x, LABEL_BASELINE);
            value.draw(context, x, VALUE_BASELINE);
            x += w + COLUMN_GAP;
        }
        if !bar.chart.is_empty() {
            let top = bar.chart.iter().copied().fold(CHART_FLOOR, f32::max) as f64;
            let n = bar.chart.len() as f64;
            let gap = 1.0;
            let bar_width = ((CHART_WIDTH - gap * (n - 1.0)) / n).max(0.5);
            let bottom = (HEIGHT - CHART_HEIGHT) / 2.0;
            for (i, v) in bar.chart.iter().enumerate() {
                let height = (*v as f64 / top * CHART_HEIGHT).clamp(1.0, CHART_HEIGHT);
                CGContextFillRect(
                    context,
                    CGRect {
                        origin: CGPoint {
                            x: x + i as f64 * (bar_width + gap),
                            y: bottom,
                        },
                        size: CGSize {
                            width: bar_width,
                            height,
                        },
                    },
                );
            }
        }
        // `rgba` is the context's backing store, filled by now.
        CGContextRelease(context);
    }
    // The icon, square at the left, over the drawn readings (template: alpha is all that counts).
    let side = ph;
    let alpha = icon_alpha(icon, side);
    for y in 0..side {
        for x in 0..side.min(pw) {
            let i = (y * pw + x) * 4;
            let a = (alpha[y * side + x] + rgba[i + 3] as f32 * (1.0 - alpha[y * side + x] / 255.0))
                .round()
                .min(255.0) as u8;
            rgba[i..i + 4].copy_from_slice(&[0, 0, 0, a]);
        }
    }
    Pixels {
        rgba,
        width: pw as u32,
        height: ph as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray::model::Mini;

    fn icon() -> Pixels {
        let mut rgba = vec![0u8; 64 * 64 * 4];
        for px in rgba.chunks_mut(4).take(64 * 32) {
            px[3] = 255;
        }
        Pixels {
            rgba,
            width: 64,
            height: 64,
        }
    }

    fn mini(label: &'static str, value: &str) -> Mini {
        Mini {
            label,
            value: value.into(),
            words: String::new(),
        }
    }

    fn ink(p: &Pixels, x0: u32, x1: u32) -> usize {
        (0..p.height)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|(x, y)| p.rgba[((y * p.width + x) * 4 + 3) as usize] > 0)
            .count()
    }

    #[test]
    fn the_icon_alone_stays_the_icon() {
        assert_eq!(render(&icon(), &MenuBar::default()), icon());
    }

    #[test]
    fn readings_sit_beside_the_icon_as_a_template_image() {
        let bar = MenuBar {
            readings: vec![mini("AGENTS", "8"), mini("CPU", "7%"), mini("DEV", "3")],
            chart: Vec::new(),
        };
        let p = render(&icon(), &bar);
        assert_eq!(p.height, 54, "18 pt at 3×");
        assert!(
            p.width > 54 * 3,
            "three columns beside the icon: {}",
            p.width
        );
        // Ink beside the icon (the readings), and only black: a template carries alpha.
        assert!(ink(&p, 70, p.width) > 200);
        assert!(p.rgba.chunks(4).all(|px| px[..3] == [0, 0, 0]));
        // The icon's top half is opaque, its bottom half clear, scaled to the item's height.
        assert_eq!(p.rgba[((5 * p.width + 20) * 4 + 3) as usize], 255);
        assert_eq!(p.rgba[((50 * p.width + 20) * 4 + 3) as usize], 0);
    }

    #[test]
    fn columns_keep_their_width_while_numbers_change() {
        let at = |cpu: &str| {
            render(
                &icon(),
                &MenuBar {
                    readings: vec![mini("CPU", cpu), mini("DEV", "3")],
                    chart: Vec::new(),
                },
            )
            .width
        };
        assert_eq!(at("1%"), at("8%"));
        assert_eq!(at("11%"), at("88%"));
    }

    #[test]
    fn the_chart_adds_bars_after_the_readings() {
        let readings = vec![mini("CPU", "7%")];
        let plain = render(
            &icon(),
            &MenuBar {
                readings: readings.clone(),
                chart: Vec::new(),
            },
        );
        let charted = render(
            &icon(),
            &MenuBar {
                readings,
                chart: vec![1.0, 20.0, 5.0, 40.0],
            },
        );
        assert!(charted.width > plain.width);
        assert!(ink(&charted, plain.width + 6, charted.width) > 50);
    }
}
