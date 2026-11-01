//! The menu-bar / tray widget, on every desktop within what its tray can show ([`model`]):
//!
//! - the icon, with an attention badge, and on macOS stacked mini readings beside it
//!   ([`menubar`]); Linux shows the readings as a panel label;
//! - a popup panel with live readings, histories and lists ([`panel`]), opened by a click on
//!   macOS and Windows and from the menu on Linux;
//! - a native menu with the same rows as text.
//!
//! Nothing is stopped from here: a stop opens the window's reviewed plan.

mod badge;
#[cfg(target_os = "macos")]
mod menubar;
mod model;
pub mod panel;

use crate::state::AppState;
use badge::Pixels;
use holdmap_analyzers::Insights;
use holdmap_core::Snapshot;
use model::{Action, Platform, Readings, Style, TrayItem, TrayView, Trends};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

const TRAY_ID: &str = "holdmap-tray";
/// Analyzer results change only when an analyzer runs, so the widget rereads them at most
/// this often.
const INSIGHTS_MAX_AGE: Duration = Duration::from_secs(60);
/// The agents report reuses the scan the watcher just took; this only has to outlast the
/// moment between the two reads, so it never starts a second scan.
const SHARED_SCAN: Duration = Duration::from_secs(5);

/// macOS menu-bar icons are one-colour templates that follow the menu bar's appearance.
#[cfg(target_os = "macos")]
const ICON: &[u8] = include_bytes!("../../icons/tray-template.png");
#[cfg(not(target_os = "macos"))]
const ICON: &[u8] = include_bytes!("../../icons/tray.png");

/// A native panel row the widget can change in place.
enum Handle {
    Item(MenuItem<Wry>),
    Check(CheckMenuItem<Wry>),
    Submenu(Submenu<Wry>),
}

impl Handle {
    fn set_text(&self, text: &str) -> tauri::Result<()> {
        match self {
            Handle::Item(m) => m.set_text(text),
            Handle::Check(m) => m.set_text(text),
            Handle::Submenu(m) => m.set_text(text),
        }
    }
}

struct Widget {
    base: Pixels,
    readings: Option<Readings>,
    trends: Trends,
    shown: TrayView,
    /// One per row of `shown.items`, in [`model::flatten`] order.
    handles: Vec<Handle>,
    /// Whether macOS shows the menu-bar item (`None`: can't tell, or not macOS).
    on_screen: Option<bool>,
    /// The item's place is remembered (macOS), set once the tray is up.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    placed: bool,
}

/// The widget's state. Only background threads lock it: tray calls wait for the main thread,
/// so menu handlers (which run there) hand their work to a thread instead.
pub struct TrayWidget {
    widget: Mutex<Widget>,
    insights: Mutex<Option<(Instant, Insights)>>,
}

impl TrayWidget {
    fn insights(&self, app: &AppHandle) -> Insights {
        let mut cached = self.insights.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, insights)) = cached.as_ref() {
            if at.elapsed() < INSIGHTS_MAX_AGE {
                return insights.clone();
            }
        }
        let fresh = crate::analyzers::service(app).insights();
        *cached = Some((Instant::now(), fresh.clone()));
        fresh
    }
}

/// Where a row goes: the panel itself or one of its submenus.
trait Rows {
    fn add(&self, item: &dyn IsMenuItem<Wry>) -> tauri::Result<()>;
}

impl Rows for Menu<Wry> {
    fn add(&self, item: &dyn IsMenuItem<Wry>) -> tauri::Result<()> {
        self.append(item)
    }
}

impl Rows for Submenu<Wry> {
    fn add(&self, item: &dyn IsMenuItem<Wry>) -> tauri::Result<()> {
        self.append(item)
    }
}

fn fill(
    app: &AppHandle,
    into: &impl Rows,
    items: &[TrayItem],
    handles: &mut Vec<Handle>,
) -> tauri::Result<()> {
    for i in items {
        match i {
            TrayItem::Separator => into.add(&PredefinedMenuItem::separator(app)?)?,
            TrayItem::Item {
                id,
                label,
                enabled,
                accel,
            } => {
                let m = MenuItem::with_id(app, id.as_str(), label, *enabled, *accel)?;
                into.add(&m)?;
                handles.push(Handle::Item(m));
            }
            TrayItem::Check { id, label, checked } => {
                let m =
                    CheckMenuItem::with_id(app, id.as_str(), label, true, *checked, None::<&str>)?;
                into.add(&m)?;
                handles.push(Handle::Check(m));
            }
            TrayItem::Submenu { id, label, items } => {
                let m = Submenu::with_id(app, id.as_str(), label, true)?;
                handles.push(Handle::Submenu(m.clone()));
                fill(app, &m, items, handles)?;
                into.add(&m)?;
            }
        }
    }
    Ok(())
}

fn build_menu(app: &AppHandle, items: &[TrayItem]) -> tauri::Result<(Menu<Wry>, Vec<Handle>)> {
    let menu = Menu::new(app)?;
    let mut handles = Vec::new();
    fill(app, &menu, items, &mut handles)?;
    Ok((menu, handles))
}

fn icon(base: &Pixels, b: model::Badge, bar: &model::MenuBar) -> Image<'static> {
    #[cfg(target_os = "macos")]
    let p = menubar::render(&badge::with_dot(base, b), bar);
    #[cfg(not(target_os = "macos"))]
    let p = {
        let _ = bar;
        badge::with_count(base, b)
    };
    Image::new_owned(p.rgba, p.width, p.height)
}

/// Show `next`, touching only what changed. Same rows: the panel's text and check marks change
/// in place, so an open panel stays open and updates live. Different rows: a new panel.
/// `resync` resets every check mark (the OS toggles one when it's chosen).
fn apply(app: &AppHandle, tray: &TrayIcon, w: &mut Widget, next: TrayView, resync: bool) {
    let TrayView {
        items,
        bar,
        title,
        tooltip,
        badge,
    } = next;
    if model::same_rows(&w.shown.items, &items) {
        let rows = model::flatten(&w.shown.items)
            .into_iter()
            .zip(model::flatten(&items));
        for (h, (was, now)) in w.handles.iter().zip(rows) {
            if was.label() != now.label() {
                let _ = h.set_text(now.label());
            }
            if let (Handle::Check(m), TrayItem::Check { checked, .. }) = (h, now) {
                if resync || !matches!(was, TrayItem::Check { checked: c, .. } if c == checked) {
                    let _ = m.set_checked(*checked);
                }
            }
        }
        w.shown.items = items;
    } else {
        match build_menu(app, &items)
            .and_then(|(menu, handles)| tray.set_menu(Some(menu)).map(|()| handles))
        {
            Ok(handles) => {
                w.handles = handles;
                w.shown.items = items;
            }
            // What's shown stays recorded as shown, so the next refresh tries again.
            Err(error) => eprintln!("holdmap: couldn't update the tray panel ({error})"),
        }
    }
    if w.shown.title != title && tray.set_title(title.as_deref()).is_ok() {
        w.shown.title = title;
    }
    if w.shown.tooltip != tooltip && tray.set_tooltip(Some(&tooltip)).is_ok() {
        w.shown.tooltip = tooltip;
    }
    if (w.shown.badge != badge || w.shown.bar != bar)
        && tray
            .set_icon_with_as_template(
                Some(icon(&w.base, badge, &bar)),
                Platform::CURRENT == Platform::MacOs,
            )
            .is_ok()
    {
        w.shown.badge = badge;
        w.shown.bar = bar;
    }
}

fn style(app: &AppHandle) -> Style {
    Style::parse(app.state::<AppState>().store.config().tray_style())
}

/// Record `readings` (if any) and redraw the widget.
fn render(app: &AppHandle, readings: Option<Readings>, resync: bool) {
    let (Some(widget), Some(tray)) = (app.try_state::<TrayWidget>(), app.tray_by_id(TRAY_ID))
    else {
        return;
    };
    let style = style(app);
    let mut w = widget.widget.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = readings {
        w.trends.record(&r);
        w.readings = Some(r);
    }
    let next = model::view(w.readings.as_ref(), &w.trends, style, Platform::CURRENT);
    apply(app, &tray, &mut w, next, resync);
    #[cfg(target_os = "macos")]
    {
        let placed = w.placed;
        let on_screen = tray
            .with_inner_tray_icon(move |t| {
                let mtm = objc2::MainThreadMarker::new()?;
                let item = t.ns_status_item()?;
                if !placed {
                    menubar::remember_place(&item);
                }
                menubar::shown(&item, mtm)
            })
            .ok()
            .flatten();
        w.placed = true;
        if on_screen != w.on_screen {
            w.on_screen = on_screen;
            // The window says so: a full menu bar hides new items without telling anyone.
            let _ = app.emit("tray-status", TrayStatus { on_screen });
        }
    }
}

/// Whether macOS shows holdmap's menu-bar item.
#[derive(Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayStatus {
    /// `false` when macOS hides it for lack of room; `null` when that can't be told.
    on_screen: Option<bool>,
}

/// The menu-bar item's status, for the window's Settings.
#[tauri::command]
pub async fn tray_status(app: AppHandle) -> TrayStatus {
    let on_screen = app.try_state::<TrayWidget>().and_then(|widget| {
        widget
            .widget
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_screen
    });
    TrayStatus { on_screen }
}

/// Update the widget from a scan: the agents come from the same scan, the needs from the same
/// rules as the window's "Needs you" (running agents, ports and approved analyzers).
/// Blocking: call it off the main thread.
pub fn refresh_tray(app: &AppHandle, snapshot: &Snapshot) {
    let Some(widget) = app.try_state::<TrayWidget>() else {
        return;
    };
    if app.tray_by_id(TRAY_ID).is_none() {
        return;
    }
    let report = app
        .state::<AppState>()
        .with_engine(SHARED_SCAN, |e| e.agents())
        .ok();
    let needs = report.as_ref().map(|r| {
        let insights = widget.insights(app);
        crate::needs::items(Some(&snapshot.entries), Some(r), Some(&insights))
    });
    render(
        app,
        Some(Readings::of(snapshot, report.as_ref(), needs.as_deref())),
        false,
    );
}

/// Redraw after the style changed (from the panel or Settings). Blocking: call it off the
/// main thread.
pub fn restyle(app: &AppHandle) {
    render(app, None, true);
}

/// What the popup panel shows now.
fn panel_data(app: &AppHandle) -> model::PanelData {
    let style = style(app);
    match app.try_state::<TrayWidget>() {
        Some(widget) => {
            let w = widget.widget.lock().unwrap_or_else(|e| e.into_inner());
            model::panel_data(w.readings.as_ref(), &w.trends, style)
        }
        None => model::panel_data(None, &Trends::default(), style),
    }
}

fn set_style(app: &AppHandle, style: Style) {
    match app
        .state::<AppState>()
        .store
        .update_config(|c| c.tray_style = style.as_str().into())
    {
        // Settings, if open, follows the change.
        Ok((config, _)) => {
            let _ = app.emit("config-changed", &config);
        }
        Err(error) => eprintln!("holdmap: couldn't save the menu bar style ({error})"),
    }
    restyle(app);
}

pub fn show_main(app: &AppHandle) {
    if let Err(error) = crate::window_lifecycle::show_main(app) {
        eprintln!("holdmap: couldn't show the app window ({error})");
    }
}

fn on_menu(app: &AppHandle, id: &str) {
    if let Some(action) = Action::parse(id) {
        run(app, action);
    }
}

/// What a menu or panel row does. Blocking work goes to a thread: menu handlers run on the main
/// thread, which tray calls wait for.
fn run(app: &AppHandle, action: Action) {
    match action {
        Action::Show => show_main(app),
        Action::ShowPanel => {
            let app = app.clone();
            std::thread::spawn(move || panel::toggle(&app, None));
        }
        Action::HidePanel => panel::hide(app),
        Action::Quit => app.exit(0),
        // Confirmation happens in the window, with the full plan.
        Action::StopAllDev => {
            show_main(app);
            let _ = app.emit("stop-all-dev", ());
        }
        Action::Refresh => {
            let _ = app.emit("refresh", ());
            let app = app.clone();
            std::thread::spawn(move || tray_tick(&app));
        }
        Action::FocusPort(port) => {
            show_main(app);
            let _ = app.emit("focus-port", port);
        }
        Action::OpenAgent(agent) => {
            show_main(app);
            let _ = app.emit("open-agent", agent);
        }
        Action::OpenEntry(entry) => {
            show_main(app);
            let _ = app.emit("open-entry", entry);
        }
        Action::Style(style) => {
            let app = app.clone();
            std::thread::spawn(move || set_style(&app, style));
        }
    }
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let image = Image::from_bytes(ICON)?;
    let base = Pixels {
        rgba: image.rgba().to_vec(),
        width: image.width(),
        height: image.height(),
    };
    let first = model::view(None, &Trends::default(), style(app), Platform::CURRENT);
    let (menu, handles) = build_menu(app, &first.items)?;
    let builder = TrayIconBuilder::with_id(TRAY_ID)
        .icon(image)
        .tooltip(&first.tooltip)
        .menu(&menu);
    #[cfg(target_os = "macos")]
    let builder = builder.icon_as_template(true);
    builder
        // macOS and Windows open the popup panel on a click and the menu on the right button;
        // Windows also opens the window on a double click. Linux trays report no clicks and
        // open the menu, which starts with "Show panel".
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } => {
                let app = tray.app_handle().clone();
                std::thread::spawn(move || panel::toggle(&app, Some(rect)));
            }
            TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } if Platform::CURRENT == Platform::Windows => show_main(tray.app_handle()),
            _ => {}
        })
        .build(app)?;
    app.manage(TrayWidget {
        widget: Mutex::new(Widget {
            base,
            readings: None,
            trends: Trends::default(),
            shown: first,
            handles,
            on_screen: None,
            placed: false,
        }),
        insights: Mutex::new(None),
    });
    Ok(())
}

/// A fresh scan for the panel's Refresh, so it stays useful while the window is hidden.
pub fn tray_tick(app: &AppHandle) {
    if let Ok(snapshot) = app
        .state::<AppState>()
        .snapshot(false, std::time::Duration::ZERO)
    {
        refresh_tray(app, &snapshot);
    }
}
