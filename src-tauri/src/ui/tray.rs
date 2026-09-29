//! The tray / menu bar icon and its menu.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    App, AppHandle, Emitter, Manager, Runtime,
};

/// Id of the main window, as declared in `tauri.conf.json`.
pub const MAIN_WINDOW: &str = "main";

/// Id of the tray icon.
const TRAY_ID: &str = "tray";

/// Tray menu item ids.
const OPEN: &str = "open";
const SETTINGS: &str = "settings";
const QUIT: &str = "quit";

/// Creates the tray icon, its tooltip and its menu.
pub fn build(app: &App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, OPEN, "Open", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, SETTINGS, "Settings…", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit OpenQuoteBar", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &settings, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("OpenQuoteBar")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            OPEN => show_main_window(app, None),
            SETTINGS => show_main_window(app, Some("settings")),
            QUIT => app.exit(0),
            _ => {}
        });

    // Without a window icon there is nothing to show in the tray; the caller
    // keeps running regardless so the rest of the app still works.
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;

    Ok(())
}

/// Shows and focuses the main window, optionally asking the frontend to switch
/// to a given view.
pub fn show_main_window<R: Runtime>(app: &AppHandle<R>, view: Option<&str>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };

    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();

    if let Some(view) = view {
        let _ = window.emit("navigate", view);
    }
}
