//! The tray / menu bar icon and its menu.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    App, AppHandle, Emitter, Listener, Manager, Runtime,
};

use crate::core::{
    balances::{ProviderBalance, ProviderStatus},
    time::unix_now,
};

/// Id of the main window, as declared in `tauri.conf.json`.
pub const MAIN_WINDOW: &str = "main";

/// Id of the tray icon.
const TRAY_ID: &str = "tray";

/// Name shown in the tooltip and used as its prefix.
const APP_NAME: &str = "OpenQuoteBar";

/// Tray menu item ids.
const OPEN: &str = "open";
const REFRESH: &str = "refresh";
const SETTINGS: &str = "settings";
const QUIT: &str = "quit";

/// Creates the tray icon, its tooltip and its menu.
pub fn build(app: &App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, OPEN, "Open", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, REFRESH, "Refresh now", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, SETTINGS, "Settings…", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit OpenQuoteBar", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &refresh, &settings, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(APP_NAME)
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            OPEN => show_main_window(app, None),
            REFRESH => crate::poller::request_refresh(app),
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

    // The tooltip is the one surface that is always visible, so it doubles as
    // the status indicator while the window is closed.
    let handle = app.handle().clone();
    app.listen(crate::poller::BALANCES_EVENT, move |event| {
        let Ok(balances) = serde_json::from_str::<Vec<ProviderBalance>>(event.payload()) else {
            return;
        };

        if let Some(tray) = handle.tray_by_id(TRAY_ID) {
            let _ = tray.set_tooltip(Some(&summarize(&balances)));
        }
    });

    Ok(())
}

/// One line describing the cache, for the tray tooltip.
pub fn summarize(balances: &[ProviderBalance]) -> String {
    let enabled: Vec<&ProviderBalance> = balances.iter().filter(|it| it.enabled).collect();

    if enabled.is_empty() {
        return format!("{APP_NAME} · no providers enabled");
    }

    if enabled
        .iter()
        .any(|it| it.status == ProviderStatus::Loading)
    {
        return format!("{APP_NAME} · updating…");
    }

    let count = enabled.len();
    let failed = enabled
        .iter()
        .filter(|it| it.status == ProviderStatus::Error)
        .count();

    let mut summary = format!(
        "{APP_NAME} · {count} provider{}",
        if count == 1 { "" } else { "s" }
    );

    if failed > 0 {
        summary.push_str(&format!(
            " · {failed} error{}",
            if failed == 1 { "" } else { "s" }
        ));
    }

    match enabled.iter().filter_map(|it| it.updated_at).max() {
        Some(updated_at) => summary.push_str(&format!(" · {}", relative(updated_at))),
        None => summary.push_str(" · not updated yet"),
    }

    summary
}

/// Turns a timestamp into "just now" / "4 min ago" / "2 h ago".
fn relative(timestamp: i64) -> String {
    let elapsed = unix_now().saturating_sub(timestamp);

    if elapsed < 60 {
        return "updated just now".to_string();
    }

    let minutes = elapsed / 60;
    if minutes < 60 {
        return format!("updated {minutes} min ago");
    }

    format!("updated {} h ago", minutes / 60)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(id: &str, enabled: bool, status: ProviderStatus) -> ProviderBalance {
        let mut entry = ProviderBalance::new(id, id, enabled);
        entry.status = status;
        entry
    }

    #[test]
    fn nothing_enabled_says_so() {
        assert_eq!(summarize(&[]), "OpenQuoteBar · no providers enabled");
        assert_eq!(
            summarize(&[provider("openrouter", false, ProviderStatus::Idle)]),
            "OpenQuoteBar · no providers enabled"
        );
    }

    #[test]
    fn a_cycle_in_flight_says_it_is_updating() {
        let balances = [
            provider("openrouter", true, ProviderStatus::Ok),
            provider("deepseek", true, ProviderStatus::Loading),
        ];

        assert_eq!(summarize(&balances), "OpenQuoteBar · updating…");
    }

    #[test]
    fn a_successful_cycle_counts_providers_and_says_when() {
        let mut openrouter = provider("openrouter", true, ProviderStatus::Ok);
        openrouter.updated_at = Some(unix_now());

        let summary = summarize(&[openrouter]);

        assert!(
            summary.starts_with("OpenQuoteBar · 1 provider · "),
            "{summary}"
        );
        assert!(summary.ends_with("updated just now"), "{summary}");
    }

    #[test]
    fn failures_are_counted_and_the_last_success_is_still_reported() {
        let mut openrouter = provider("openrouter", true, ProviderStatus::Ok);
        openrouter.updated_at = Some(unix_now());

        let summary = summarize(&[
            openrouter,
            provider("deepseek", true, ProviderStatus::Error),
        ]);

        assert!(summary.contains("2 providers"), "{summary}");
        assert!(summary.contains("1 error"), "{summary}");
        assert!(summary.contains("updated just now"), "{summary}");
    }

    #[test]
    fn a_failure_before_any_success_says_it_has_not_updated_yet() {
        let summary = summarize(&[provider("deepseek", true, ProviderStatus::Error)]);

        assert!(summary.contains("1 error"), "{summary}");
        assert!(summary.ends_with("not updated yet"), "{summary}");
    }

    #[test]
    fn elapsed_time_is_phrased_in_minutes_then_hours() {
        let now = unix_now();

        assert_eq!(relative(now), "updated just now");
        assert_eq!(relative(now - 240), "updated 4 min ago");
        assert_eq!(relative(now - 7200), "updated 2 h ago");
    }
}
