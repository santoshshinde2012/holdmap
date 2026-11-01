//! macOS: the menu-bar items. Each item is drawn the way system monitors draw theirs: a small
//! label over one value in a fixed column, a tiny line of recent values, or one bar per agent,
//! with the system font. An item with nothing to flag is a template image, so macOS tints it for
//! a light or dark menu bar; one that flags a state (something needs you, CPU running high) is
//! drawn in colour, with ink read from the menu bar's own appearance. `tray-icon` shows each
//! image 18 pt high and as wide as its aspect ratio.

use super::badge::{self, Pixels};
use super::model::{Draw, Tone};
use holdmap_core::attention::Urgency;
use objc2::runtime::NSObjectProtocol;
use objc2::{sel, MainThreadMarker};
use objc2_app_kit::{NSAppearanceCustomization, NSMenu, NSStatusItem, NSWindowOcclusionState};
use objc2_foundation::{NSString, NSUserDefaults};
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
    fn CGContextSetRGBStrokeColor(context: Ref, r: f64, g: f64, b: f64, a: f64);
    fn CGContextSetLineWidth(context: Ref, width: f64);
    fn CGContextBeginPath(context: Ref);
    fn CGContextMoveToPoint(context: Ref, x: f64, y: f64);
    fn CGContextAddLineToPoint(context: Ref, x: f64, y: f64);
    fn CGContextClosePath(context: Ref);
    fn CGContextFillPath(context: Ref);
    fn CGContextStrokePath(context: Ref);
    fn CGContextFillRect(context: Ref, rect: CGRect);
    fn CGContextSetTextPosition(context: Ref, x: f64, y: f64);
}

#[link(name = "CoreText", kind = "framework")]
extern "C" {
    static kCTFontAttributeName: Ref;
    static kCTForegroundColorFromContextAttributeName: Ref;
    fn CTFontCreateUIFontForLanguage(kind: u32, size: f64, language: Ref) -> Ref;
    fn CTLineCreateWithAttributedString(s: Ref) -> Ref;
    fn CTLineGetTypographicBounds(line: Ref, a: *mut f64, d: *mut f64, l: *mut f64) -> f64;
    fn CTLineDraw(line: Ref, context: Ref);
}

/// Pixels per point: drawn at 3× and shown at 18 pt, so it stays sharp on Retina displays.
pub(super) const SCALE: f64 = 3.0;
/// The menu bar item's height in points.
const HEIGHT: f64 = 18.0;
/// A reading: a 7 pt label over a 12 pt value, as system monitors set them.
const LABEL_SIZE: f64 = 7.0;
const VALUE_SIZE: f64 = 12.0;
/// Baselines from the bottom of the item, in points.
const VALUE_BASELINE: f64 = 1.5;
const LABEL_BASELINE: f64 = 12.2;
/// A reading's column is at least this wide, so short values don't make a skinny item.
const MIN_READING: f64 = 28.0;
/// Charts: 32 pt wide, 14 pt tall above a 2 pt foot.
const CHART_WIDTH: f64 = 32.0;
const CHART_HEIGHT: f64 = 14.0;
const CHART_FOOT: f64 = 2.0;
/// The line chart's slots: the last scans, one point apart, filling in from the right. (The
/// panel's chart keeps all 60; at half a point each, a new line here would be a sliver.)
const CHART_SLOTS: usize = 33;
const MAX_BAR: f64 = 4.0;
const BAR_GAP: f64 = 1.5;
/// A chart's full height is at least this much CPU, so idle stays low.
const CHART_FLOOR: f32 = 10.0;
/// Between the parts of one item (an item showing several readings), and at its edges.
const PART_GAP: f64 = 6.0;
const PAD: f64 = 1.0;
/// `kCTFontUIFontSystem`, `kCFStringEncodingUTF8`.
const UI_FONT_SYSTEM: u32 = 2;
const UTF8: u32 = 0x0800_0100;
/// `kCGImageAlphaPremultipliedLast` with 32-bit big-endian RGBA.
const RGBA_PREMULTIPLIED: u32 = 1 | (4 << 12);

/// A colour as CoreGraphics takes it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ink(f64, f64, f64, f64);

impl Ink {
    /// The text colour: black for a template (macOS tints it), else the menu bar's label colour.
    fn text(template: bool, dark: bool) -> Ink {
        match (template, dark) {
            (true, _) => Ink(0.0, 0.0, 0.0, 1.0),
            (false, true) => Ink(1.0, 1.0, 1.0, 0.92),
            (false, false) => Ink(0.0, 0.0, 0.0, 0.85),
        }
    }

    /// The system's orange and red, in their light and dark variants.
    fn tone(tone: Tone, dark: bool) -> Option<Ink> {
        match (tone, dark) {
            (Tone::Plain, _) => None,
            (Tone::Warning, false) => Some(Ink(1.0, 0.584, 0.0, 1.0)),
            (Tone::Warning, true) => Some(Ink(1.0, 0.624, 0.039, 1.0)),
            (Tone::Critical, false) => Some(Ink(1.0, 0.231, 0.188, 1.0)),
            (Tone::Critical, true) => Some(Ink(1.0, 0.271, 0.227, 1.0)),
        }
    }

    fn faded(self, alpha: f64) -> Ink {
        Ink(self.0, self.1, self.2, self.3 * alpha)
    }

    fn fill(self, context: Ref) {
        unsafe { CGContextSetRGBFillColor(context, self.0, self.1, self.2, self.3) }
    }

    fn stroke(self, context: Ref) {
        unsafe { CGContextSetRGBStrokeColor(context, self.0, self.1, self.2, self.3) }
    }
}

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

/// The system font at `size`.
struct Font(Ref);

impl Font {
    fn system(size: f64) -> Font {
        Font(unsafe { CTFontCreateUIFontForLanguage(UI_FONT_SYSTEM, size, std::ptr::null()) })
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

/// A part's width in points, with the text it draws (readings only).
enum Placed {
    Icon,
    Reading {
        label: Line,
        value: Line,
        width: f64,
    },
    Chart(f64),
}

fn bars_width(n: usize) -> f64 {
    if n == 0 {
        return 8.0;
    }
    let bar = ((CHART_WIDTH - BAR_GAP * (n as f64 - 1.0)) / n as f64).min(MAX_BAR);
    bar * n as f64 + BAR_GAP * (n as f64 - 1.0)
}

/// One menu-bar item, drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub pixels: Pixels,
    /// A template image: macOS tints it for the menu bar. Otherwise it's in colour.
    pub template: bool,
}

/// The image for an item made of `parts` (`icon` = holdmap's template glyph; `dark` = the menu
/// bar's appearance, used only when the item needs colour).
pub fn render(icon: &Pixels, parts: &[Draw], dark: bool) -> Rendered {
    let template = !parts.iter().any(Draw::coloured);
    let ink = Ink::text(template, dark);
    let label_font = Font::system(LABEL_SIZE);
    let value_font = Font::system(VALUE_SIZE);
    let placed: Vec<Placed> = parts
        .iter()
        .map(|part| match part {
            Draw::Icon(_) => Placed::Icon,
            Draw::Reading { label, value, .. } => {
                let label = Line::new(label, label_font.0);
                let width = label
                    .width()
                    .max(Line::new(&steady(value), value_font.0).width())
                    .max(MIN_READING);
                Placed::Reading {
                    label,
                    value: Line::new(value, value_font.0),
                    width,
                }
            }
            Draw::Line(_) => Placed::Chart(CHART_WIDTH),
            Draw::Bars(values) => Placed::Chart(bars_width(values.len())),
        })
        .collect();
    let width_of = |p: &Placed| match p {
        Placed::Icon => HEIGHT,
        Placed::Reading { width, .. } => *width,
        Placed::Chart(w) => *w,
    };
    let width = PAD * 2.0
        + placed.iter().map(width_of).sum::<f64>()
        + PART_GAP * (placed.len().saturating_sub(1)) as f64;
    let (pw, ph) = ((width * SCALE).ceil() as usize, (HEIGHT * SCALE) as usize);
    let mut rgba = vec![0u8; pw * ph * 4];
    let mut icons = Vec::new();
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
            return Rendered {
                pixels: icon.clone(),
                template: true,
            };
        }
        CGContextScaleCTM(context, SCALE, SCALE);
        let mut x = PAD;
        for (part, place) in parts.iter().zip(&placed) {
            let w = width_of(place);
            match (part, place) {
                (Draw::Icon(b), Placed::Icon) => icons.push((x, *b)),
                (
                    Draw::Reading { tone, .. },
                    Placed::Reading {
                        label,
                        value,
                        width,
                    },
                ) => {
                    ink.fill(context);
                    label.draw(context, x + (width - label.width()) / 2.0, LABEL_BASELINE);
                    Ink::tone(*tone, dark).unwrap_or(ink).fill(context);
                    value.draw(context, x + (width - value.width()) / 2.0, VALUE_BASELINE);
                }
                (Draw::Line(samples), _) => line_chart(context, x, samples, ink),
                (Draw::Bars(values), Placed::Chart(w)) => bar_chart(context, x, *w, values, ink),
                _ => {}
            }
            x += w + PART_GAP;
        }
        CGContextRelease(context);
    }
    // Straight alpha for the PNG `tray-icon` writes (CoreGraphics drew premultiplied).
    for px in rgba.chunks_mut(4) {
        let a = px[3] as u32;
        if a > 0 && a < 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    for (x, b) in icons {
        draw_icon(
            &mut rgba,
            pw,
            icon,
            b,
            (x * SCALE).round() as usize,
            ink,
            dark,
        );
    }
    Rendered {
        pixels: Pixels {
            rgba,
            width: pw as u32,
            height: ph as u32,
        },
        template,
    }
}

/// The glyph in `ink` at `left`, with its attention dot in the urgency's colour.
fn draw_icon(
    rgba: &mut [u8],
    pw: usize,
    icon: &Pixels,
    b: super::model::Badge,
    left: usize,
    ink: Ink,
    dark: bool,
) {
    let side = (HEIGHT * SCALE) as usize;
    let alpha = icon_alpha(&badge::with_dot(icon, b), side);
    let tone = match b.urgency {
        Some(Urgency::High) => Tone::Critical,
        Some(Urgency::Medium) if b.count > 0 => Tone::Warning,
        _ => Tone::Plain,
    };
    let dot = Ink::tone(tone, dark);
    // The dot's place, as `badge::with_dot` draws it on the glyph.
    let r = side as f32 * 0.16;
    let (cx, cy) = (side as f32 - r, r);
    for y in 0..side {
        for x in 0..side.min(pw.saturating_sub(left)) {
            let a = alpha[y * side + x] / 255.0;
            if a <= 0.0 {
                continue;
            }
            let in_dot =
                ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt() <= r + 0.5;
            let c = if in_dot { dot.unwrap_or(ink) } else { ink };
            let i = (y * pw + left + x) * 4;
            rgba[i..i + 4].copy_from_slice(&[
                (c.0 * 255.0).round() as u8,
                (c.1 * 255.0).round() as u8,
                (c.2 * 255.0).round() as u8,
                (a as f64 * c.3 * 255.0).round() as u8,
            ]);
        }
    }
}

/// The last scans as a filled line, newest at the right, over a faint foot the width of the
/// chart, so a short history reads as filling in rather than as a stray mark.
fn line_chart(context: Ref, x: f64, samples: &[f32], ink: Ink) {
    // SAFETY: CoreGraphics calls on the live bitmap context `render` made.
    unsafe {
        let samples = &samples[samples.len().saturating_sub(CHART_SLOTS)..];
        ink.faded(0.35).fill(context);
        CGContextFillRect(
            context,
            CGRect {
                origin: CGPoint { x, y: CHART_FOOT },
                size: CGSize {
                    width: CHART_WIDTH,
                    height: 1.0,
                },
            },
        );
        if samples.len() < 2 {
            return;
        }
        let top = samples.iter().copied().fold(CHART_FLOOR, f32::max) as f64;
        let step = CHART_WIDTH / (CHART_SLOTS - 1) as f64;
        let x0 = x + CHART_WIDTH - step * (samples.len() - 1) as f64;
        let at = |i: usize, v: f32| {
            (
                x0 + step * i as f64,
                CHART_FOOT + (v.max(0.0) as f64 / top * CHART_HEIGHT).min(CHART_HEIGHT),
            )
        };
        CGContextBeginPath(context);
        CGContextMoveToPoint(context, x0, CHART_FOOT);
        for (i, v) in samples.iter().enumerate() {
            let (px, py) = at(i, *v);
            CGContextAddLineToPoint(context, px, py);
        }
        CGContextAddLineToPoint(context, x + CHART_WIDTH, CHART_FOOT);
        CGContextClosePath(context);
        ink.faded(0.3).fill(context);
        CGContextFillPath(context);
        CGContextBeginPath(context);
        for (i, v) in samples.iter().enumerate() {
            let (px, py) = at(i, *v);
            if i == 0 {
                CGContextMoveToPoint(context, px, py);
            } else {
                CGContextAddLineToPoint(context, px, py);
            }
        }
        ink.stroke(context);
        CGContextSetLineWidth(context, 1.0);
        CGContextStrokePath(context);
    }
}

/// One bar per agent, each at least 1 pt so an idle agent still shows; a faint foot with none.
fn bar_chart(context: Ref, x: f64, width: f64, values: &[f32], ink: Ink) {
    // SAFETY: CoreGraphics calls on the live bitmap context `render` made.
    unsafe {
        if values.is_empty() {
            ink.faded(0.35).fill(context);
            CGContextFillRect(
                context,
                CGRect {
                    origin: CGPoint { x, y: CHART_FOOT },
                    size: CGSize { width, height: 1.0 },
                },
            );
            return;
        }
        let n = values.len() as f64;
        let bar = (width - BAR_GAP * (n - 1.0)) / n;
        let top = values.iter().copied().fold(CHART_FLOOR, f32::max) as f64;
        ink.fill(context);
        for (i, v) in values.iter().enumerate() {
            let height = (v.max(0.0) as f64 / top * CHART_HEIGHT).clamp(1.0, CHART_HEIGHT);
            CGContextFillRect(
                context,
                CGRect {
                    origin: CGPoint {
                        x: x + i as f64 * (bar + BAR_GAP),
                        y: CHART_FOOT,
                    },
                    size: CGSize { width: bar, height },
                },
            );
        }
    }
}

/// Whether the menu bar under this item is dark (on macOS 26 it follows the wallpaper).
pub fn dark_menu_bar(item: &NSStatusItem, mtm: MainThreadMarker) -> bool {
    item.button(mtm)
        .is_some_and(|b| b.effectiveAppearance().name().to_string().contains("Dark"))
}

/// The name macOS keeps an item's place under: a ⌘-dragged place survives restarts. Without a
/// name macOS forgets the place, and a new item starts at the far left, the first place it
/// runs out of room.
pub fn remember_place(item: &NSStatusItem, name: &str) {
    item.setAutosaveName(Some(&NSString::from_str(name)));
}

fn position_key(name: &str) -> objc2::rc::Retained<NSString> {
    NSString::from_str(&format!("NSStatusItem Preferred Position {name}"))
}

fn kept_key(name: &str) -> objc2::rc::Retained<NSString> {
    NSString::from_str(&format!("holdmap kept position {name}"))
}

/// Before an item is turned off: macOS drops its place when it goes, so keep a copy.
pub fn save_place(name: &str) {
    let defaults = NSUserDefaults::standardUserDefaults();
    if let Some(place) = defaults.objectForKey(&position_key(name)) {
        // SAFETY: a property-list value read from the same defaults.
        unsafe { defaults.setObject_forKey(Some(&place), &kept_key(name)) };
    }
}

/// Before an item is turned on again: put its kept place back, so it returns where it was.
pub fn restore_place(name: &str) {
    let defaults = NSUserDefaults::standardUserDefaults();
    if let Some(place) = defaults.objectForKey(&kept_key(name)) {
        // SAFETY: a property-list value read from the same defaults.
        unsafe { defaults.setObject_forKey(Some(&place), &position_key(name)) };
        defaults.removeObjectForKey(&kept_key(name));
    }
}

/// Whether macOS shows the item. A full menu bar pushes new items left, behind the frontmost
/// app's menus, and on a screen with a camera housing nothing left of it is shown. `None` while
/// the menu bar itself is hidden (fullscreen, or set to hide), when it can't be told.
pub fn shown(item: &NSStatusItem, mtm: MainThreadMarker) -> Option<bool> {
    if !NSMenu::menuBarVisible(mtm) {
        return None;
    }
    let window = item.button(mtm)?.window()?;
    let frame = window.frame();
    if let Some(screen) = window.screen() {
        // macOS 12+: the menu bar to the right of the camera housing (empty without one).
        if screen.respondsToSelector(sel!(auxiliaryTopRightArea)) {
            let right = screen.auxiliaryTopRightArea();
            if right.size.width > 0.0 {
                return Some(frame.origin.x >= right.origin.x - 1.0);
            }
        }
    }
    Some(
        window
            .occlusionState()
            .contains(NSWindowOcclusionState::Visible),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray::model::Badge;

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

    fn reading(label: &'static str, value: &str, tone: Tone) -> Draw {
        Draw::Reading {
            label,
            value: value.into(),
            tone,
        }
    }

    fn ink(p: &Pixels, x0: u32, x1: u32) -> usize {
        (0..p.height)
            .flat_map(|y| (x0..x1.min(p.width)).map(move |x| (x, y)))
            .filter(|(x, y)| p.rgba[((y * p.width + x) * 4 + 3) as usize] > 0)
            .count()
    }

    #[test]
    fn a_plain_reading_is_a_small_template_like_a_system_monitor_s() {
        let r = render(&icon(), &[reading("AGENTS", "8", Tone::Plain)], false);
        assert!(r.template, "nothing to flag: macOS tints it");
        assert_eq!(r.pixels.height, 54, "18 pt at 3×");
        let pt = r.pixels.width as f64 / SCALE;
        assert!(
            (28.0..=36.0).contains(&pt),
            "about 32 pt, like a Mini widget: {pt}"
        );
        assert!(ink(&r.pixels, 0, r.pixels.width) > 100);
        assert!(r.pixels.rgba.chunks(4).all(|px| px[..3] == [0, 0, 0]));
    }

    #[test]
    fn a_flagged_reading_is_drawn_in_colour_for_the_menu_bar_it_sits_on() {
        let light = render(&icon(), &[reading("NEEDS", "2", Tone::Critical)], false);
        let dark = render(&icon(), &[reading("NEEDS", "2", Tone::Critical)], true);
        assert!(!light.template && !dark.template);
        let red = |p: &Pixels| {
            p.rgba
                .chunks(4)
                .any(|px| px[3] > 200 && px[0] > 200 && px[1] < 120)
        };
        assert!(red(&light.pixels) && red(&dark.pixels), "the value is red");
        // The label stays label-coloured: dark ink on a light bar, light ink on a dark one.
        let label_ink = |p: &Pixels| {
            p.rgba
                .chunks(4)
                .take(p.width as usize * 12)
                .filter(|px| px[3] > 128)
                .map(|px| px[0])
                .max()
        };
        assert!(label_ink(&light.pixels) < Some(60));
        assert!(label_ink(&dark.pixels) > Some(200));
    }

    #[test]
    fn readings_keep_their_width_while_numbers_change() {
        let at = |v: &str| {
            render(&icon(), &[reading("CPU", v, Tone::Plain)], false)
                .pixels
                .width
        };
        assert_eq!(at("1%"), at("8%"));
        assert_eq!(at("11%"), at("88%"));
    }

    #[test]
    fn charts_fit_32_pt_and_show_their_shape() {
        let line = render(
            &icon(),
            &[Draw::Line(vec![1.0, 12.0, 4.0, 20.0, 6.0])],
            false,
        );
        assert!(line.template);
        assert_eq!(
            line.pixels.width,
            ((CHART_WIDTH + 2.0 * PAD) * SCALE).ceil() as u32
        );
        assert!(ink(&line.pixels, 0, line.pixels.width) > 100);
        // A short history is one point per scan from the right, over the full-width foot.
        let short = render(
            &icon(),
            &[Draw::Line(vec![12.0, 4.0, 20.0, 6.0, 9.0])],
            false,
        )
        .pixels;
        let above_foot = short.height - ((CHART_FOOT + 1.0) * SCALE) as u32;
        let line_cols = (0..short.width)
            .filter(|&x| {
                (0..above_foot).any(|y| short.rgba[((y * short.width + x) * 4 + 3) as usize] > 100)
            })
            .count() as f64;
        assert!((3.5..=5.5).contains(&(line_cols / SCALE)), "{line_cols}");
        assert!(
            ink(&short, 0, (2.0 * SCALE) as u32) > 0,
            "the foot runs the full width"
        );
        let bars = render(&icon(), &[Draw::Bars(vec![3.0, 9.0, 1.0])], false);
        let pt = bars.pixels.width as f64 / SCALE;
        assert!(pt <= CHART_WIDTH + 2.0 * PAD, "{pt}");
        // An empty chart still draws its foot, so the item isn't invisible.
        let empty = render(&icon(), &[Draw::Bars(vec![])], false);
        assert!(ink(&empty.pixels, 0, empty.pixels.width) > 0);
    }

    #[test]
    fn the_icon_s_dot_takes_the_urgency_colour() {
        let calm = render(&icon(), &[Draw::Icon(Badge::default())], false);
        assert!(calm.template);
        assert_eq!(
            calm.pixels.width,
            ((HEIGHT + 2.0 * PAD) * SCALE).ceil() as u32
        );
        let alert = render(
            &icon(),
            &[Draw::Icon(Badge {
                count: 1,
                urgency: Some(Urgency::Medium),
            })],
            false,
        );
        assert!(!alert.template);
        let orange = alert
            .pixels
            .rgba
            .chunks(4)
            .any(|px| px[3] > 200 && px[0] > 240 && (120..180).contains(&px[1]) && px[2] < 40);
        assert!(orange, "an amber dot");
    }

    #[test]
    fn one_item_lays_its_parts_side_by_side() {
        let one = render(&icon(), &[reading("AGENTS", "8", Tone::Plain)], false);
        let both = render(
            &icon(),
            &[
                reading("AGENTS", "8", Tone::Plain),
                reading("DEV", "3", Tone::Plain),
            ],
            false,
        );
        assert!(both.pixels.width > one.pixels.width + (MIN_READING * SCALE) as u32);
    }
}
