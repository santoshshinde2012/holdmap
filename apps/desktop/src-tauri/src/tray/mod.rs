//! The menu-bar / tray widget, on every desktop within what its tray can show ([`model`]):
//!
//! - macOS: one small menu-bar item per chosen reading (or all of them as one item), drawn like
//!   a system monitor's ([`menubar`]); Windows: the icon with an attention badge; Linux: the
//!   icon with the readings as its panel label;
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
#[cfg(all(test, target_os = "macos"))]
mod preview;

use crate::state::AppState;
use badge::Pixels;
use holdmap_analyzers::Insights;
use holdmap_core::Snapshot;
use model::{Action, BarChoice, Item, Platform, Readings, TrayItem, TrayView, Trends};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

/// The one tray icon on Windows and Linux; macOS has one per menu-bar item (`holdmap-<item>`).
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

/// A native menu row the widget can change in place.
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

/// A macOS menu-bar item: its tray icon, what it shows and whether macOS shows it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct Slot {
    id: &'static str,
    tray: TrayIcon,
    /// What it was last drawn with (parts, hover text, a dark menu bar).
    drawn: Option<(Vec<model::Draw>, String, bool)>,
    /// Its place is remembered (set once it's up).
    placed: bool,
    on_screen: Option<bool>,
}

struct Widget {
    base: Pixels,
    readings: Option<Readings>,
    trends: Trends,
    shown: TrayView,
    /// One per row of `shown.items`, in [`model::flatten`] order.
    handles: Vec<Handle>,
    /// The native menu, shared by every tray icon.
    menu: Menu<Wry>,
    /// The menu-bar items (macOS), in the order shown.
    slots: Vec<Slot>,
    /// The menu bar is dark (macOS), as last read from it.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    dark: bool,
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

/// Where a row goes: the menu itself or one of its submenus.
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

/// The Windows and Linux tray icon: holdmap's icon with the attention count.
#[cfg(not(target_os = "macos"))]
fn badge_icon(base: &Pixels, b: model::Badge) -> Image<'static> {
    let p = badge::with_count(base, b);
    Image::new_owned(p.rgba, p.width, p.height)
}

/// The tray id and the place name of a macOS menu-bar item.
#[cfg(target_os = "macos")]
fn tray_id(item: &str) -> String {
    format!("holdmap-{item}")
}

#[cfg(target_os = "macos")]
fn place_name(item: &str) -> String {
    format!("holdmap_{item}")
}

/// Every tray icon the menu is attached to.
fn trays(app: &AppHandle, w: &Widget) -> Vec<TrayIcon> {
    let mut out: Vec<TrayIcon> = w.slots.iter().map(|s| s.tray.clone()).collect();
    out.extend(app.tray_by_id(TRAY_ID));
    out
}

/// A click on an item opens the panel under it, at that item's section; the right button opens
/// the menu; on Windows a double click opens the window.
fn tray_events(builder: TrayIconBuilder<Wry>, section: &'static str) -> TrayIconBuilder<Wry> {
    builder
        .show_menu_on_left_click(false)
        .on_tray_icon_event(move |tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } => {
                let app = tray.app_handle().clone();
                std::thread::spawn(move || panel::toggle(&app, Some(rect), Some(section)));
            }
            TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } if Platform::CURRENT == Platform::Windows => show_main(tray.app_handle()),
            _ => {}
        })
}

/// Make the macOS menu-bar items match `bar`: turned-off items go (their place kept), new ones
/// come (back where they were), changed ones are redrawn.
#[cfg(target_os = "macos")]
fn sync_slots(app: &AppHandle, w: &mut Widget, bar: &[model::BarItem]) {
    let (kept, gone): (Vec<Slot>, Vec<Slot>) = std::mem::take(&mut w.slots)
        .into_iter()
        .partition(|slot| bar.iter().any(|b| b.id == slot.id));
    w.slots = kept;
    if !gone.is_empty() {
        let removed: Vec<_> = gone
            .into_iter()
            .map(|slot| {
                menubar::save_place(&place_name(slot.id));
                let registered = app.remove_tray_by_id(&tray_id(slot.id));
                (slot, registered)
            })
            .collect();
        // AppKit removes a status item when its last handle is dropped, which must happen on
        // the main thread (this runs on a background thread from Settings and the menu).
        let _ = app.run_on_main_thread(move || drop(removed));
    }
    let mut slots = Vec::with_capacity(bar.len());
    for b in bar {
        let key = (b.parts.clone(), b.tooltip.clone(), w.dark);
        if let Some(i) = w.slots.iter().position(|s| s.id == b.id) {
            let mut slot = w.slots.swap_remove(i);
            if slot.drawn.as_ref() != Some(&key) {
                let drawn = menubar::render(&w.base, &b.parts, w.dark);
                let image =
                    Image::new_owned(drawn.pixels.rgba, drawn.pixels.width, drawn.pixels.height);
                if slot
                    .tray
                    .set_icon_with_as_template(Some(image), drawn.template)
                    .is_ok()
                    && slot.tray.set_tooltip(Some(&b.tooltip)).is_ok()
                {
                    slot.drawn = Some(key);
                }
            }
            slots.push(slot);
            continue;
        }
        menubar::restore_place(&place_name(b.id));
        let drawn = menubar::render(&w.base, &b.parts, w.dark);
        let image = Image::new_owned(drawn.pixels.rgba, drawn.pixels.width, drawn.pixels.height);
        let builder = TrayIconBuilder::with_id(tray_id(b.id))
            .icon(image)
            .icon_as_template(drawn.template)
            .tooltip(&b.tooltip)
            .menu(&w.menu);
        match tray_events(builder, b.section).build(app) {
            Ok(tray) => slots.push(Slot {
                id: b.id,
                tray,
                drawn: Some(key),
                placed: false,
                on_screen: None,
            }),
            Err(error) => eprintln!("holdmap: couldn't add a menu-bar item ({error})"),
        }
    }
    w.slots = slots;
}

/// Show `next`, touching only what changed. Same rows: the menu's text and check marks change
/// in place, so an open menu stays open and updates live. Different rows: a new menu.
/// `resync` resets every check mark (the OS toggles one when it's chosen).
fn apply(app: &AppHandle, w: &mut Widget, next: TrayView, resync: bool) {
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
        match build_menu(app, &items) {
            Ok((menu, handles)) => {
                if trays(app, w)
                    .iter()
                    .all(|t| t.set_menu(Some(menu.clone())).is_ok())
                {
                    w.menu = menu;
                    w.handles = handles;
                    w.shown.items = items;
                }
            }
            // What's shown stays recorded as shown, so the next refresh tries again.
            Err(error) => eprintln!("holdmap: couldn't update the tray menu ({error})"),
        }
    }
    #[cfg(target_os = "macos")]
    sync_slots(app, w, &bar);
    w.shown.bar = bar;
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if w.shown.title != title && tray.set_title(title.as_deref()).is_ok() {
            w.shown.title = title;
        }
        if w.shown.tooltip != tooltip && tray.set_tooltip(Some(&tooltip)).is_ok() {
            w.shown.tooltip = tooltip;
        }
        #[cfg(not(target_os = "macos"))]
        if w.shown.badge != badge && tray.set_icon(Some(badge_icon(&w.base, badge))).is_ok() {
            w.shown.badge = badge;
        }
    }
    #[cfg(target_os = "macos")]
    let _ = badge;
}

/// What the menu bar shows, from the settings.
fn choice(app: &AppHandle) -> BarChoice {
    let config = app.state::<AppState>().store.config();
    BarChoice::from_config(
        &config.tray_items(),
        config.tray_combined,
        config.tray_compact,
    )
}

/// Record `readings` (if any) and redraw the widget.
fn render(app: &AppHandle, readings: Option<Readings>, resync: bool) {
    let Some(widget) = app.try_state::<TrayWidget>() else {
        return;
    };
    let choice = choice(app);
    let mut w = widget.widget.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = readings {
        w.trends.record(&r);
        w.readings = Some(r);
    }
    #[cfg(target_os = "macos")]
    if let Some(first) = w.slots.first() {
        // Colour needs the menu bar's own look (on macOS 26 it follows the wallpaper).
        if let Ok(Some(dark)) = first.tray.with_inner_tray_icon(|t| {
            let mtm = objc2::MainThreadMarker::new()?;
            let item = t.ns_status_item()?;
            Some(menubar::dark_menu_bar(&item, mtm))
        }) {
            w.dark = dark;
        }
    }
    let next = model::view(w.readings.as_ref(), &w.trends, &choice, Platform::CURRENT);
    apply(app, &mut w, next, resync);
    #[cfg(target_os = "macos")]
    check_slots(app, &mut w);
}

/// Whether macOS shows each item; the window hears when that changes.
#[cfg(target_os = "macos")]
fn check_slots(app: &AppHandle, w: &mut Widget) {
    let before = status(w);
    for slot in &mut w.slots {
        let (placed, name) = (slot.placed, place_name(slot.id));
        slot.on_screen = slot
            .tray
            .with_inner_tray_icon(move |t| {
                let mtm = objc2::MainThreadMarker::new()?;
                let item = t.ns_status_item()?;
                if !placed {
                    menubar::remember_place(&item, &name);
                }
                menubar::shown(&item, mtm)
            })
            .ok()
            .flatten();
        slot.placed = true;
    }
    let after = status(w);
    if after != before {
        // A full menu bar hides new items without telling anyone, so the window says so.
        let _ = app.emit("tray-status", &after);
    }
}

/// Whether macOS shows holdmap's menu-bar items.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayStatus {
    /// `false` when macOS hides any of them for lack of room; `null` when that can't be told.
    on_screen: Option<bool>,
    /// The names of the items macOS hides.
    hidden: Vec<&'static str>,
}

fn status(w: &Widget) -> TrayStatus {
    let hidden: Vec<&'static str> = w
        .slots
        .iter()
        .filter(|s| s.on_screen == Some(false))
        .map(|s| Item::parse(s.id).map_or("holdmap", Item::name))
        .collect();
    let known = w.slots.iter().any(|s| s.on_screen.is_some());
    TrayStatus {
        on_screen: (known || !hidden.is_empty()).then_some(hidden.is_empty()),
        hidden,
    }
}

/// The menu-bar items' status, for the window's Settings.
#[tauri::command]
pub async fn tray_status(app: AppHandle) -> TrayStatus {
    match app.try_state::<TrayWidget>() {
        Some(widget) => status(&widget.widget.lock().unwrap_or_else(|e| e.into_inner())),
        None => TrayStatus {
            on_screen: None,
            hidden: Vec::new(),
        },
    }
}

/// Update the widget from a scan: the agents come from the same scan, the needs from the same
/// rules as the window's "Needs you" (running agents, ports and approved analyzers).
/// Blocking: call it off the main thread.
pub fn refresh_tray(app: &AppHandle, snapshot: &Snapshot) {
    let Some(widget) = app.try_state::<TrayWidget>() else {
        return;
    };
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

/// Redraw after the menu-bar choice changed (from the menu or Settings). Blocking: call it off
/// the main thread.
pub fn restyle(app: &AppHandle) {
    render(app, None, true);
}

/// What the popup panel shows now.
fn panel_data(app: &AppHandle) -> model::PanelData {
    let state = panel::state();
    let config = app.state::<AppState>().store.config();
    let settings = model::panel_settings(
        &BarChoice::from_config(
            &config.tray_items(),
            config.tray_combined,
            config.tray_compact,
        ),
        config.show_in_dock,
        config.notify,
        Platform::CURRENT,
    );
    match app.try_state::<TrayWidget>() {
        Some(widget) => {
            let w = widget.widget.lock().unwrap_or_else(|e| e.into_inner());
            model::panel_data(w.readings.as_ref(), &w.trends, state, settings)
        }
        None => model::panel_data(None, &Trends::default(), state, settings),
    }
}

/// Persist quick settings before acknowledging them to the panel.
fn save_setting(app: &AppHandle, action: Action) -> Result<(), String> {
    let (config, _) = app
        .state::<AppState>()
        .store
        .update_config(|c| match action {
            Action::ToggleItem(item) => {
                let mut items: Vec<String> = c.tray_items().into_iter().map(String::from).collect();
                if let Some(i) = items.iter().position(|i| i == item.id()) {
                    items.remove(i);
                } else {
                    items.push(item.id().into());
                }
                c.tray_items = items;
            }
            Action::ToggleCombined => c.tray_combined = !c.tray_combined,
            Action::ToggleCompact => c.tray_compact = !c.tray_compact,
            Action::ToggleDock => c.show_in_dock = !c.show_in_dock,
            Action::ToggleNotify => c.notify = !c.notify,
            _ => unreachable!("only setting actions are persisted"),
        })
        .map_err(|e| e.to_string())?;
    let _ = app.emit("config-changed", &config);
    if matches!(action, Action::ToggleDock) {
        apply_dock(app, config.show_in_dock);
    }
    if matches!(
        action,
        Action::ToggleItem(_) | Action::ToggleCombined | Action::ToggleCompact
    ) {
        restyle(app);
    }
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Err(error) = crate::window_lifecycle::show_main(app) {
        eprintln!("holdmap: couldn't show the app window ({error})");
    }
}

/// Keep holdmap in the Dock, or run it as a menu-bar app (macOS).
pub fn apply_dock(app: &AppHandle, show: bool) {
    #[cfg(target_os = "macos")]
    {
        let policy = if show {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        };
        if let Err(error) = app.set_activation_policy(policy) {
            eprintln!("holdmap: couldn't change the Dock setting ({error})");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, show);
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
            std::thread::spawn(move || panel::toggle(&app, None, None));
        }
        Action::HidePanel => panel::hide(app),
        Action::PinPanel => panel::toggle_pin(),
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
        Action::ToggleItem(_)
        | Action::ToggleCombined
        | Action::ToggleCompact
        | Action::ToggleDock
        | Action::ToggleNotify => {
            let app = app.clone();
            std::thread::spawn(move || {
                if let Err(error) = save_setting(&app, action) {
                    eprintln!("holdmap: couldn't save the setting ({error})");
                }
            });
        }
        Action::OpenSettings => {
            show_main(app);
            let _ = app.emit("open-settings", ());
        }
        // The theme is the window's own choice; it saves it and the panel follows.
        Action::SetTheme(theme) => {
            let _ = app.emit("set-theme", theme);
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
    let first = model::view(None, &Trends::default(), &choice(app), Platform::CURRENT);
    let (menu, handles) = build_menu(app, &first.items)?;
    // One handler for every tray icon's menu (several icons share it on macOS).
    app.on_menu_event(|app, event| on_menu(app, event.id.as_ref()));
    // macOS: the items claim their places at launch, before the first scan fills them in.
    #[cfg(target_os = "macos")]
    let slots = {
        let mut slots = Vec::new();
        for b in &first.bar {
            let drawn = menubar::render(&base, &b.parts, false);
            let builder = TrayIconBuilder::with_id(tray_id(b.id))
                .icon(Image::new_owned(
                    drawn.pixels.rgba,
                    drawn.pixels.width,
                    drawn.pixels.height,
                ))
                .icon_as_template(drawn.template)
                .tooltip(&b.tooltip)
                .menu(&menu);
            menubar::restore_place(&place_name(b.id));
            slots.push(Slot {
                id: b.id,
                tray: tray_events(builder, b.section).build(app)?,
                drawn: Some((b.parts.clone(), b.tooltip.clone(), false)),
                placed: false,
                on_screen: None,
            });
        }
        slots
    };
    #[cfg(not(target_os = "macos"))]
    let slots = {
        let builder = TrayIconBuilder::with_id(TRAY_ID)
            .icon(image)
            .tooltip(&first.tooltip)
            .menu(&menu);
        tray_events(builder, "top").build(app)?;
        Vec::new()
    };
    app.manage(TrayWidget {
        widget: Mutex::new(Widget {
            base,
            readings: None,
            trends: Trends::default(),
            shown: first,
            handles,
            menu,
            slots,
            dark: false,
        }),
        insights: Mutex::new(None),
    });
    Ok(())
}

/// A fresh scan for the menu's Refresh, so it stays useful while the window is hidden.
pub fn tray_tick(app: &AppHandle) {
    if let Ok(snapshot) = app
        .state::<AppState>()
        .snapshot(false, std::time::Duration::ZERO)
    {
        refresh_tray(app, &snapshot);
    }
}
