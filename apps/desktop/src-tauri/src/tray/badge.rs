//! The tray icon with its attention badge, drawn into the RGBA pixels Tauri takes.
//!
//! Windows and Linux get the coloured icon with a count in a red (look now) or amber (worth a
//! look) disc. macOS menu-bar icons are one-colour template images that follow the menu bar's
//! appearance, so there the badge is a dot and the count goes in the text beside the icon.

use super::model::Badge;
use holdmap_core::attention::Urgency;

/// RGBA pixels, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixels {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Look now: white text on it has 5.6:1 contrast.
#[cfg_attr(target_os = "macos", allow(dead_code))]
const HIGH: [u8; 3] = [0xC5, 0x22, 0x1F];
/// Worth a look: white text on it has 5.0:1 contrast.
#[cfg_attr(target_os = "macos", allow(dead_code))]
const MEDIUM: [u8; 3] = [0xB4, 0x53, 0x09];

/// 3×5 digits and `+`, one row per byte (bit 2 is the left column). At 64 px and scale 4
/// they map onto whole pixels when Windows shows the icon at 16 px.
#[cfg_attr(target_os = "macos", allow(dead_code))]
const GLYPHS: [(char, [u8; 5]); 11] = [
    ('0', [0b111, 0b101, 0b101, 0b101, 0b111]),
    ('1', [0b010, 0b110, 0b010, 0b010, 0b111]),
    ('2', [0b111, 0b001, 0b111, 0b100, 0b111]),
    ('3', [0b111, 0b001, 0b111, 0b001, 0b111]),
    ('4', [0b101, 0b101, 0b111, 0b001, 0b001]),
    ('5', [0b111, 0b100, 0b111, 0b001, 0b111]),
    ('6', [0b111, 0b100, 0b111, 0b101, 0b111]),
    ('7', [0b111, 0b001, 0b001, 0b001, 0b001]),
    ('8', [0b111, 0b101, 0b111, 0b101, 0b111]),
    ('9', [0b111, 0b101, 0b111, 0b001, 0b111]),
    ('+', [0b000, 0b010, 0b111, 0b010, 0b000]),
];

/// How much of the pixel at (`x`, `y`) lies inside the circle, from 4×4 samples.
fn coverage(x: u32, y: u32, cx: f32, cy: f32, r: f32) -> f32 {
    let mut inside = 0;
    for sy in 0..4 {
        for sx in 0..4 {
            let px = x as f32 + (sx as f32 + 0.5) / 4.0 - cx;
            let py = y as f32 + (sy as f32 + 0.5) / 4.0 - cy;
            if px * px + py * py <= r * r {
                inside += 1;
            }
        }
    }
    inside as f32 / 16.0
}

/// Clear a ring around the badge so it reads apart from the icon, then paint the disc.
fn disc(p: &mut Pixels, cx: f32, cy: f32, r: f32, gap: f32, rgb: [u8; 3]) {
    for y in 0..p.height {
        for x in 0..p.width {
            let cut = coverage(x, y, cx, cy, r + gap);
            if cut == 0.0 {
                continue;
            }
            let i = ((y * p.width + x) * 4) as usize;
            let fill = coverage(x, y, cx, cy, r);
            let kept = p.rgba[i + 3] as f32 * (1.0 - cut) / 255.0;
            let alpha = fill + kept * (1.0 - fill);
            for (c, paint) in rgb.iter().enumerate() {
                let over = *paint as f32 * fill + p.rgba[i + c] as f32 * kept * (1.0 - fill);
                p.rgba[i + c] = if alpha > 0.0 {
                    (over / alpha).round() as u8
                } else {
                    0
                };
            }
            p.rgba[i + 3] = (alpha * 255.0).round() as u8;
        }
    }
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn text(p: &mut Pixels, s: &str, cx: f32, cy: f32, scale: u32) {
    let glyphs: Vec<[u8; 5]> = s
        .chars()
        .filter_map(|c| GLYPHS.iter().find(|(g, _)| *g == c).map(|(_, rows)| *rows))
        .collect();
    let n = glyphs.len() as u32;
    if n == 0 {
        return;
    }
    let w = n * 3 * scale + (n - 1) * scale;
    let left = (cx - w as f32 / 2.0).round() as i64;
    let top = (cy - (5 * scale) as f32 / 2.0).round() as i64;
    for (g, rows) in glyphs.iter().enumerate() {
        let gx = left + (g as u32 * 4 * scale) as i64;
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..3 {
                if bits & (0b100 >> col) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let x = gx + (col * scale + dx) as i64;
                        let y = top + (row as u32 * scale + dy) as i64;
                        if (0..p.width as i64).contains(&x) && (0..p.height as i64).contains(&y) {
                            let i = ((y as u32 * p.width + x as u32) * 4) as usize;
                            p.rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
                        }
                    }
                }
            }
        }
    }
}

/// Windows and Linux: `base` with the count in a disc in the top-right corner, or `base`
/// unchanged when nothing needs you.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn with_count(base: &Pixels, badge: Badge) -> Pixels {
    let mut p = base.clone();
    let Some(urgency) = badge.urgency.filter(|_| badge.count > 0) else {
        return p;
    };
    let size = p.width.min(p.height) as f32;
    let r = size * 0.23;
    let (cx, cy) = (p.width as f32 - r - size * 0.02, r + size * 0.02);
    let rgb = if urgency == Urgency::High {
        HIGH
    } else {
        MEDIUM
    };
    disc(&mut p, cx, cy, r, size * 0.04, rgb);
    let (label, glyph_scale) = if badge.count > 9 {
        ("9+".to_string(), size / 64.0 * 3.0)
    } else {
        (badge.count.to_string(), size / 64.0 * 4.0)
    };
    text(&mut p, &label, cx, cy, glyph_scale.round().max(1.0) as u32);
    p
}

/// macOS: the template glyph with a dot in the top-right corner when anything needs you.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn with_dot(base: &Pixels, badge: Badge) -> Pixels {
    let mut p = base.clone();
    if badge.count == 0 {
        return p;
    }
    let size = p.width.min(p.height) as f32;
    let r = size * 0.16;
    let cx = p.width as f32 - r;
    disc(&mut p, cx, r, r, size * 0.07, [0, 0, 0]);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 64×64 icon filled with one opaque colour.
    fn solid(rgb: [u8; 3]) -> Pixels {
        Pixels {
            rgba: [rgb[0], rgb[1], rgb[2], 255].repeat(64 * 64),
            width: 64,
            height: 64,
        }
    }

    fn at(p: &Pixels, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * p.width + x) * 4) as usize;
        p.rgba[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn nothing_to_show_leaves_the_icon_alone() {
        let base = solid([10, 20, 30]);
        assert_eq!(with_count(&base, Badge::default()), base);
        assert_eq!(with_dot(&base, Badge::default()), base);
    }

    #[test]
    fn the_count_disc_takes_the_urgency_colour_and_a_white_digit() {
        let base = solid([10, 20, 30]);
        let high = with_count(
            &base,
            Badge {
                count: 3,
                urgency: Some(Urgency::High),
            },
        );
        assert_eq!((high.width, high.height), (64, 64));
        // The disc's edge is the urgency colour, its centre is the digit, and the opposite
        // corner is untouched.
        let r = 64.0 * 0.23;
        let (cx, cy) = (64.0 - r - 1.28, r + 1.28);
        assert_eq!(
            at(&high, (cx - r + 2.0) as u32, cy as u32),
            [0xC5, 0x22, 0x1F, 255]
        );
        assert_eq!(
            at(&high, cx as u32, (cy - 9.0) as u32),
            [255, 255, 255, 255]
        );
        assert_eq!(at(&high, 2, 62), [10, 20, 30, 255]);
        let medium = with_count(
            &base,
            Badge {
                count: 12,
                urgency: Some(Urgency::Medium),
            },
        );
        assert_eq!(
            at(&medium, (cx - r + 2.0) as u32, cy as u32),
            [0xB4, 0x53, 0x09, 255]
        );
        assert_ne!(medium, high, "9+ draws differently from 3");
    }

    #[test]
    fn a_gap_separates_the_badge_from_the_icon() {
        let p = with_count(
            &solid([10, 20, 30]),
            Badge {
                count: 1,
                urgency: Some(Urgency::High),
            },
        );
        let r = 64.0 * 0.23;
        let (cx, cy) = (64.0 - r - 1.28, r + 1.28);
        // Just outside the disc, inside the gap: cleared.
        let x = (cx - r - 1.6) as u32;
        assert!(at(&p, x, cy as u32)[3] < 64, "{:?}", at(&p, x, cy as u32));
    }

    #[test]
    fn the_macos_dot_is_one_colour_for_a_template_image() {
        let base = Pixels {
            rgba: vec![0; 64 * 64 * 4],
            width: 64,
            height: 64,
        };
        let p = with_dot(
            &base,
            Badge {
                count: 1,
                urgency: Some(Urgency::Medium),
            },
        );
        let r = 64.0 * 0.16;
        assert_eq!(at(&p, (64.0 - r) as u32, r as u32), [0, 0, 0, 255]);
        assert!(
            p.rgba.chunks(4).all(|px| px[..3] == [0, 0, 0]),
            "template pixels carry alpha only"
        );
    }

    #[test]
    fn the_shipped_icons_decode_to_square_rgba() {
        for bytes in [
            &include_bytes!("../../icons/tray.png")[..],
            &include_bytes!("../../icons/tray-template.png")[..],
        ] {
            let image = tauri::image::Image::from_bytes(bytes).expect("tray icon");
            assert_eq!(image.width(), image.height());
            assert_eq!(
                image.rgba().len(),
                (image.width() * image.height() * 4) as usize
            );
        }
    }
}
