//! The docs' menu-bar pictures, drawn by the real renderer from sample data (the same agents as
//! the panel's `demoPanel`), so they can be regenerated like every other screenshot:
//!
//! `HOLDMAP_MENU_BAR_PREVIEW=docs/screenshots cargo test -p holdmap-desktop --lib menu_bar_preview -- --ignored`

use super::badge::Pixels;
use super::menubar;
use super::model::{
    self, AgentRow, BarChoice, DevServer, Item, NeedRow, Platform, Readings, Trends,
};
use holdmap_core::attention::{Target, Urgency};
use std::path::Path;
use tauri::image::Image;

/// Between items, and around the strip, in points.
const GAP: u32 = 14;
const MARGIN: u32 = 16;
/// A menu bar's height, in points.
const BAR: u32 = 30;

fn sample(scan: usize) -> Readings {
    // The panel demo's CPU wave, shared out between the agents in the same proportions.
    let i = scan as f32;
    let wave = 4.0
        + 3.0 * (i / 4.0).sin()
        + if scan.is_multiple_of(9) { 9.0 } else { 0.0 }
        + if scan % 13 == 5 { 5.0 } else { 0.0 };
    let agent = |id: &str, name: &str, place: Option<&str>, share: f32, memory: u64| AgentRow {
        id: id.into(),
        name: name.into(),
        place: place.map(Into::into),
        cpu: wave * share,
        memory,
    };
    Readings {
        ports: 37,
        port_ids: (0..37).map(|n| format!("tcp:{}", 3000 + n)).collect(),
        exposed: 4,
        dev: [
            (3000, "Next.js · storefront"),
            (5173, "Vite · dashboard"),
            (8000, "FastAPI · api"),
        ]
        .into_iter()
        .map(|(port, label)| DevServer {
            port,
            label: label.into(),
            stoppable: true,
            exposed: port == 5173,
        })
        .collect(),
        agents: Some(vec![
            agent(
                "agent:4242",
                "Claude Code",
                Some("storefront"),
                0.57,
                301_989_888,
            ),
            agent("agent:811", "Cursor", None, 0.28, 1_181_116_006),
            agent("agent:977", "Codex CLI", Some("api"), 0.12, 189_792_256),
            agent(
                "agent:1203",
                "Gemini CLI",
                Some("dashboard"),
                0.03,
                98_566_144,
            ),
        ]),
        needs: Some(vec![
            NeedRow {
                urgency: Urgency::High,
                title: "Claude Code listens on all interfaces".into(),
                target: Target::Agents {
                    agent_id: "agent:4242".into(),
                },
            },
            NeedRow {
                urgency: Urgency::Medium,
                title: ":5173 is open to the network".into(),
                target: Target::List {
                    entry_id: "tcp:5173".into(),
                },
            },
        ]),
    }
}

/// Blend `item` onto `canvas` at (`x`, `y`) pixels; a template takes the menu bar's ink first.
fn blend(canvas: &mut Pixels, item: &menubar::Rendered, x: u32, y: u32, dark: bool) {
    let p = &item.pixels;
    let ink = if dark {
        [255.0, 255.0, 255.0, 1.0]
    } else {
        [0.0, 0.0, 0.0, 0.85]
    };
    for row in 0..p.height {
        for col in 0..p.width {
            let s = ((row * p.width + col) * 4) as usize;
            let (rgb, alpha) = if item.template {
                (ink, p.rgba[s + 3] as f32 / 255.0 * ink[3])
            } else {
                let [r, g, b, a] = [0, 1, 2, 3].map(|k| p.rgba[s + k] as f32);
                ([r, g, b, 1.0], a / 255.0)
            };
            let d = (((y + row) * canvas.width + x + col) * 4) as usize;
            for (k, ink) in rgb.iter().take(3).enumerate() {
                let under = canvas.rgba[d + k] as f32;
                canvas.rgba[d + k] = (ink * alpha + under * (1.0 - alpha)).round() as u8;
            }
        }
    }
}

fn strip(icon: &Pixels, readings: &Readings, trends: &Trends, dark: bool) -> Pixels {
    let choice = BarChoice {
        items: Item::ALL.to_vec(),
        combined: false,
    };
    let view = model::view(Some(readings), trends, &choice, Platform::MacOs);
    let items: Vec<menubar::Rendered> = view
        .bar
        .iter()
        .map(|b| menubar::render(icon, &b.parts, dark))
        .collect();
    let scale = menubar::SCALE as u32;
    let width = items.iter().map(|r| r.pixels.width).sum::<u32>()
        + (GAP * (items.len() as u32 - 1) + 2 * MARGIN) * scale;
    let height = BAR * scale;
    let bg: [u8; 4] = if dark {
        [36, 36, 40, 255]
    } else {
        [236, 236, 238, 255]
    };
    let mut canvas = Pixels {
        rgba: bg.repeat((width * height) as usize),
        width,
        height,
    };
    let mut x = MARGIN * scale;
    for item in &items {
        blend(
            &mut canvas,
            item,
            x,
            (height - item.pixels.height) / 2,
            dark,
        );
        x += item.pixels.width + GAP * scale;
    }
    canvas
}

fn write_png(path: &Path, p: &Pixels) {
    let file = std::fs::File::create(path).expect("create the preview");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), p.width, p.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(&p.rgba).expect("png data");
}

#[test]
#[ignore = "writes the docs' menu-bar pictures; run on purpose"]
fn menu_bar_preview() {
    let dir = std::env::var_os("HOLDMAP_MENU_BAR_PREVIEW").expect("HOLDMAP_MENU_BAR_PREVIEW");
    let image = Image::from_bytes(super::ICON).expect("tray icon");
    let icon = Pixels {
        rgba: image.rgba().to_vec(),
        width: image.width(),
        height: image.height(),
    };
    let mut trends = Trends::default();
    let scans = 33;
    for scan in 0..scans {
        trends.record(&sample(scan));
    }
    let now = sample(scans - 1);
    for (look, dark) in [("light", false), ("dark", true)] {
        let path = Path::new(&dir).join(format!("desktop-menu-bar-{look}.png"));
        write_png(&path, &strip(&icon, &now, &trends, dark));
        println!("wrote {}", path.display());
    }
}
