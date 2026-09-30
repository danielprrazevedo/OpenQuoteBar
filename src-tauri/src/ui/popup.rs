//! The custom tray popover.
//!
//! A borderless, transparent window anchored under the tray icon. It carries the
//! rich balance card, which the native menu cannot express. The menu is still
//! available on right click; the popover opens on left click.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, Position, Rect, Runtime, Size, WebviewWindow,
};

/// Label of the popup window, as declared in `tauri.conf.json`.
pub const POPUP_WINDOW: &str = "popup";

/// Width of the popup window: the card plus its shadow padding on both sides.
const POPUP_WIDTH: f64 = 388.0;

/// Gap between the tray icon and the popup, in logical pixels.
const GAP: f64 = 6.0;

/// When the popup last hid itself on focus loss.
///
/// Clicking the tray icon blurs (and hides) the popup before the click event
/// arrives, so without this a second click would only ever reopen it. A toggle
/// landing within this window is treated as "close instead".
static LAST_HIDDEN: Mutex<Option<Instant>> = Mutex::new(None);

fn recently_hidden() -> bool {
    let Ok(guard) = LAST_HIDDEN.lock() else {
        return false;
    };

    guard.is_some_and(|at| at.elapsed() < Duration::from_millis(300))
}

/// Records that the popup hid itself, so the toggle can tell a blur apart.
pub fn note_hidden() {
    if let Ok(mut guard) = LAST_HIDDEN.lock() {
        *guard = Some(Instant::now());
    }
}

/// Shows the popup under the tray icon, or hides it when already open.
pub fn toggle<R: Runtime>(app: &AppHandle<R>, at: PhysicalPosition<f64>, icon: Rect) {
    let Some(window) = app.get_webview_window(POPUP_WINDOW) else {
        return;
    };

    if window.is_visible().unwrap_or(false) || recently_hidden() {
        let _ = window.hide();
        return;
    }

    position(app, &window, at, icon);
    let _ = window.show();
    let _ = window.set_focus();
}

/// Hides the popup, if it is open.
pub fn hide<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(POPUP_WINDOW) {
        let _ = window.hide();
    }
}

/// Resizes the popup to fit its content. `height` is logical, like the CSS.
pub fn set_size<R: Runtime>(app: &AppHandle<R>, height: f64) {
    if let Some(window) = app.get_webview_window(POPUP_WINDOW) {
        let height = height.max(1.0);
        let _ = window.set_size(Size::Logical(LogicalSize::new(POPUP_WIDTH, height)));
    }
}

/// Moves the popup so it hangs under the tray icon, kept on screen.
fn position<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
    at: PhysicalPosition<f64>,
    icon: Rect,
) {
    // The event's rect may be in either scale; normalise it to physical pixels.
    let monitor = app.monitor_from_point(at.x, at.y).ok().flatten();
    let scale = monitor
        .as_ref()
        .map_or(1.0, |monitor| monitor.scale_factor());

    let (mut icon_x, mut icon_y) = match icon.position {
        Position::Physical(p) => (f64::from(p.x), f64::from(p.y)),
        Position::Logical(p) => (p.x * scale, p.y * scale),
    };
    let (mut icon_w, mut icon_h) = match icon.size {
        Size::Physical(s) => (f64::from(s.width), f64::from(s.height)),
        Size::Logical(s) => (s.width * scale, s.height * scale),
    };

    // Some platforms report no rect; fall back to the click point itself.
    if icon_w <= 0.0 || icon_h <= 0.0 {
        let side = 22.0 * scale;
        icon_x = at.x - side / 2.0;
        icon_y = at.y - side / 2.0;
        icon_w = side;
        icon_h = side;
    }

    let width = window
        .outer_size()
        .map_or(POPUP_WIDTH * scale, |size| f64::from(size.width));

    let mut x = icon_x + icon_w / 2.0 - width / 2.0;
    let y = icon_y + icon_h + GAP * scale;

    if let Some(monitor) = &monitor {
        let origin = monitor.position();
        let size = monitor.size();
        let min_x = f64::from(origin.x) + 4.0;
        let max_x = f64::from(origin.x) + f64::from(size.width) - width - 4.0;
        x = x.clamp(min_x, max_x.max(min_x));
    }

    let target = PhysicalPosition::new(x.round() as i32, y.round() as i32);
    let _ = window.set_position(Position::Physical(target));
}
