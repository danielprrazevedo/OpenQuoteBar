//! The tray / menu bar icon and its menu.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    App, AppHandle, Emitter, Listener, Manager, Runtime,
};

use crate::core::{
    balances::{BalancesReport, CurrencyTotal, FailureKind, ProviderBalance, ProviderStatus},
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
        let Ok(report) = serde_json::from_str::<BalancesReport>(event.payload()) else {
            return;
        };

        if let Some(tray) = handle.tray_by_id(TRAY_ID) {
            let _ = tray.set_tooltip(Some(&summarize(&report)));
        }
    });

    Ok(())
}

/// One line describing the cache, for the tray tooltip.
pub fn summarize(report: &BalancesReport) -> String {
    let enabled: Vec<&ProviderBalance> = report.providers.iter().filter(|it| it.enabled).collect();

    if enabled.is_empty() {
        return format!("{APP_NAME} · no providers enabled");
    }

    if enabled
        .iter()
        .any(|it| it.status == ProviderStatus::Loading)
    {
        return format!("{APP_NAME} · updating…");
    }

    if report.totals.is_empty() {
        // Nothing has ever answered. Say that plainly rather than shouting
        // about keys that were never configured.
        return format!("{APP_NAME} · no balances yet");
    }

    let mut summary = format!("{APP_NAME} · {}", format_totals(&report.totals));

    let failures = enabled
        .iter()
        .filter(|it| it.error_kind.is_some_and(FailureKind::is_failure))
        .count();

    if failures > 0 {
        summary.push_str(&format!(
            " · {failures} error{}",
            if failures == 1 { "" } else { "s" }
        ));
    }

    match enabled.iter().filter_map(|it| it.updated_at).max() {
        Some(updated_at) => summary.push_str(&format!(" · {}", relative(updated_at))),
        None => summary.push_str(" · not updated yet"),
    }

    summary
}

/// Renders every currency total, e.g. `$142.18` or `$10.00 · CN¥72.00`.
fn format_totals(totals: &[CurrencyTotal]) -> String {
    totals
        .iter()
        .map(|total| format_amount(total.amount, &total.currency))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Formats an amount with its currency symbol, falling back to the code.
///
/// The window uses `Intl.NumberFormat`; this is the Rust counterpart, so the
/// tooltip does not depend on the webview being awake (WebKit throttles timers
/// in hidden windows, and the tooltip has to keep working there).
fn format_amount(amount: f64, currency: &str) -> String {
    let symbol = match currency {
        "USD" => "$".to_string(),
        "EUR" => "€".to_string(),
        "GBP" => "£".to_string(),
        "CNY" => "CN¥".to_string(),
        "JPY" => "¥".to_string(),
        other => format!("{other} "),
    };

    format!("{symbol}{amount:.2}")
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
    use crate::core::{
        balances::totals,
        time::unix_now,
        types::{BalanceAmount, BalanceSnapshot},
    };

    fn provider(id: &str, enabled: bool, status: ProviderStatus) -> ProviderBalance {
        let mut entry = ProviderBalance::new(id, id, enabled);
        entry.status = status;
        entry
    }

    /// A provider that answered with a single amount in `currency`.
    fn provider_with_value(id: &str, amount: f64, currency: &str) -> ProviderBalance {
        let mut entry = ProviderBalance::new(id, id, true);
        entry.apply_success(BalanceSnapshot {
            provider_id: id.to_string(),
            display_name: id.to_string(),
            amounts: vec![BalanceAmount {
                amount,
                currency: currency.to_string(),
                label: format!("{currency} balance"),
            }],
            fetched_at: unix_now(),
        });
        entry
    }

    fn report(providers: Vec<ProviderBalance>) -> BalancesReport {
        BalancesReport {
            totals: totals(&providers),
            providers,
        }
    }

    #[test]
    fn nothing_enabled_says_so() {
        assert_eq!(
            summarize(&report(vec![])),
            "OpenQuoteBar · no providers enabled"
        );
        assert_eq!(
            summarize(&report(vec![provider(
                "openrouter",
                false,
                ProviderStatus::Idle
            )])),
            "OpenQuoteBar · no providers enabled"
        );
    }

    #[test]
    fn a_cycle_in_flight_says_it_is_updating() {
        let balances = report(vec![
            provider("openrouter", true, ProviderStatus::Ok),
            provider("deepseek", true, ProviderStatus::Loading),
        ]);

        assert_eq!(summarize(&balances), "OpenQuoteBar · updating…");
    }

    #[test]
    fn a_fresh_install_without_any_value_says_so_plainly() {
        let mut unconfigured = provider("openrouter", true, ProviderStatus::Error);
        unconfigured.apply_failure(FailureKind::MissingKey, "no API key is stored");

        let summary = summarize(&report(vec![unconfigured]));

        assert_eq!(summary, "OpenQuoteBar · no balances yet");
    }

    #[test]
    fn the_total_is_reported_with_its_currency() {
        let summary = summarize(&report(vec![provider_with_value(
            "openrouter",
            142.18,
            "USD",
        )]));

        assert!(
            summary.starts_with("OpenQuoteBar · $142.18 · "),
            "{summary}"
        );
        assert!(summary.ends_with("updated just now"), "{summary}");
    }

    #[test]
    fn every_currency_gets_its_own_total() {
        let deepseek = {
            let mut entry = ProviderBalance::new("deepseek", "deepseek", true);
            entry.apply_success(BalanceSnapshot {
                provider_id: "deepseek".to_string(),
                display_name: "deepseek".to_string(),
                amounts: vec![
                    BalanceAmount {
                        amount: 10.0,
                        currency: "USD".to_string(),
                        label: "USD balance".to_string(),
                    },
                    BalanceAmount {
                        amount: 72.0,
                        currency: "CNY".to_string(),
                        label: "CNY balance".to_string(),
                    },
                ],
                fetched_at: unix_now(),
            });
            entry
        };

        let summary = summarize(&report(vec![deepseek]));

        // CNY sorts before USD.
        assert!(summary.contains("CN¥72.00 · $10.00"), "{summary}");
    }

    #[test]
    fn a_failure_is_counted_and_the_last_success_is_still_reported() {
        let mut failing = provider_with_value("deepseek", 5.0, "USD");
        failing.apply_failure(FailureKind::Network, "the request timed out");

        let summary = summarize(&report(vec![
            provider_with_value("openrouter", 10.0, "USD"),
            failing,
        ]));

        assert!(summary.contains("$15.00"), "{summary}");
        assert!(summary.contains("1 error"), "{summary}");
        assert!(summary.ends_with("updated just now"), "{summary}");
    }

    #[test]
    fn a_missing_key_is_not_counted_as_an_error() {
        let mut unconfigured = provider("deepseek", true, ProviderStatus::Error);
        unconfigured.apply_failure(FailureKind::MissingKey, "no API key is stored");
        // Give it a value so the summary does not collapse to "no balances yet".
        let mut configured = provider_with_value("openrouter", 10.0, "USD");

        let summary = summarize(&report(vec![configured, unconfigured.clone()]));

        assert!(!summary.contains("error"), "{summary}");

        // Now the same provider fails for a real reason.
        configured = provider_with_value("openrouter", 10.0, "USD");
        let mut broken = provider("deepseek", true, ProviderStatus::Error);
        broken.apply_failure(FailureKind::Unauthorized, "the key was rejected");

        let summary = summarize(&report(vec![configured, broken]));
        assert!(summary.contains("1 error"), "{summary}");
    }

    #[test]
    fn elapsed_time_is_phrased_in_minutes_then_hours() {
        let now = unix_now();

        assert_eq!(relative(now), "updated just now");
        assert_eq!(relative(now - 240), "updated 4 min ago");
        assert_eq!(relative(now - 7200), "updated 2 h ago");
    }

    #[test]
    fn amounts_are_formatted_with_a_symbol_and_two_decimals() {
        assert_eq!(format_amount(142.1, "USD"), "$142.10");
        assert_eq!(format_amount(72.456, "CNY"), "CN¥72.46");
        assert_eq!(format_amount(3.0, "CHF"), "CHF 3.00");
    }
}
