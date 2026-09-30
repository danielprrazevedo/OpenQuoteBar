//! The tray / menu bar icon and its menu.

use tauri::{
    image::Image,
    menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    App, AppHandle, Emitter, Listener, Manager, Runtime,
};

use crate::core::{
    balances::{
        renders_as_zero, BalancesReport, CurrencyTotal, FailureKind, ProviderBalance,
        ProviderStatus,
    },
    time::unix_now,
    types::BalanceAmount,
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

/// Prefix of the per-provider preview row ids.
const BALANCE_PREFIX: &str = "tray-balance:";

/// Ids of the preview rows are stable per provider, so the rebuild is cheap.
fn balance_id(provider_id: &str) -> String {
    format!("{BALANCE_PREFIX}{provider_id}")
}

/// Creates the tray icon, its tooltip and its menu.
pub fn build(app: &App) -> tauri::Result<()> {
    let handle = app.handle().clone();

    // The initial menu already carries whatever the cache knows; the polling
    // loop keeps it current from here on.
    let report = crate::poller::report(&handle);
    let menu = build_menu(&handle, &report)?;

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

    // A menu bar icon should be a template image: monochrome and alpha-only, so
    // the system tints it to match light and dark menu bars. Windows has no such
    // concept, so it gets the app mark, which reads on any taskbar colour.
    #[cfg(target_os = "macos")]
    const TRAY_ICON: &[u8] = include_bytes!("../../icons/tray/88x88.png");
    #[cfg(not(target_os = "macos"))]
    const TRAY_ICON: &[u8] = include_bytes!("../../icons/icon.png");

    match Image::from_bytes(TRAY_ICON) {
        Ok(icon) => {
            builder = builder
                .icon(icon)
                .icon_as_template(cfg!(target_os = "macos"));
        }
        // A tray without an icon is a poor tray, but not worth taking the app
        // down for: everything else keeps working.
        Err(error) => eprintln!("could not load the tray icon: {error}"),
    }

    builder.build(app)?;

    if let Some(tray) = handle.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(&summarize(&report)));
    }

    // The tooltip and the menu both follow the cache, so the tray stays current
    // whether the window is open or not.
    app.listen(crate::poller::BALANCES_EVENT, move |event| {
        let Ok(report) = serde_json::from_str::<BalancesReport>(event.payload()) else {
            return;
        };

        render(&handle, &report);
    });

    Ok(())
}

/// Rebuilds the tray from the cache, e.g. after a preference changed.
///
/// The menu rows depend on `show_in_tray`, which lives in `config.toml` rather
/// than in the balance event, so a config change has to ask for a fresh render
/// instead of waiting for the next polling cycle.
pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    render(app, &crate::poller::report(app));
}

/// Rebuilds the tray menu and tooltip from a report.
fn render<R: Runtime>(app: &AppHandle<R>, report: &BalancesReport) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };

    match build_menu(app, report) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(error) => eprintln!("could not rebuild the tray menu: {error}"),
    }

    let _ = tray.set_tooltip(Some(&summarize(report)));
}

/// Builds the whole tray menu: the balance preview, if any, then the actions.
fn build_menu<R: Runtime>(app: &AppHandle<R>, report: &BalancesReport) -> tauri::Result<Menu<R>> {
    // Keep the rows alive for as long as the menu borrows them.
    let mut preview: Vec<MenuItem<R>> = Vec::new();

    for row in preview_rows(report) {
        preview.push(MenuItem::with_id(
            app,
            row.id,
            row.text,
            false,
            None::<&str>,
        )?);
    }

    let open = MenuItem::with_id(app, OPEN, "Open", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, REFRESH, "Refresh now", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, SETTINGS, "Settings…", true, None::<&str>)?;
    let preview_separator = PredefinedMenuItem::separator(app)?;
    let quit_separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit OpenQuoteBar", true, None::<&str>)?;

    let mut items: Vec<&dyn IsMenuItem<R>> = Vec::new();

    if !preview.is_empty() {
        items.extend(preview.iter().map(|item| item as &dyn IsMenuItem<R>));
        items.push(&preview_separator);
    }

    items.push(&open);
    items.push(&refresh);
    items.push(&settings);
    items.push(&quit_separator);
    items.push(&quit);

    Menu::with_items(app, &items)
}

/// One provider row in the tray preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewRow {
    /// Menu item id, stable per provider.
    pub id: String,
    /// Text shown in the menu.
    pub text: String,
}

/// Builds the preview rows: the providers enabled and opted into the tray.
///
/// Order follows the configuration, so the menu does not reshuffle between
/// refreshes.
pub fn preview_rows(report: &BalancesReport) -> Vec<PreviewRow> {
    report
        .providers
        .iter()
        .filter(|provider| provider.enabled && provider.show_in_tray)
        .map(|provider| PreviewRow {
            id: balance_id(&provider.provider_id),
            text: preview_text(provider),
        })
        .collect()
}

/// Renders one provider's preview row.
fn preview_text(provider: &ProviderBalance) -> String {
    let name = &provider.display_name;

    match &provider.snapshot {
        Some(snapshot) if !snapshot.amounts.is_empty() => {
            let amounts = visible_amounts(&snapshot.amounts)
                .into_iter()
                .map(|amount| format_amount(amount.amount, &amount.currency))
                .collect::<Vec<_>>()
                .join(" · ");

            format!("{name}: {amounts}")
        }
        _ if provider.status == ProviderStatus::Loading => format!("{name}: updating…"),
        _ => format!("{name}: —"),
    }
}

/// The amounts worth showing for one provider.
///
/// A currency that reads as zero is dropped when another one has money, since
/// `CN¥0.00` next to `$10.00` is noise. A lone zero is kept: an account at zero
/// should say so. Mirrors the rule the window's totals already follow.
fn visible_amounts(amounts: &[BalanceAmount]) -> Vec<&BalanceAmount> {
    let non_zero: Vec<&BalanceAmount> = amounts
        .iter()
        .filter(|amount| !renders_as_zero(amount.amount))
        .collect();

    if non_zero.is_empty() {
        amounts.iter().collect()
    } else {
        non_zero
    }
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

    /// Marks a provider as opted into the tray preview.
    fn shown(mut entry: ProviderBalance) -> ProviderBalance {
        entry.show_in_tray = true;
        entry
    }

    #[test]
    fn only_providers_opted_into_the_tray_are_previewed() {
        let opted_in = shown(provider_with_value("openrouter", 10.0, "USD"));
        let not_opted_in = provider_with_value("deepseek", 5.0, "USD");

        let mut disabled = provider_with_value("acme", 3.0, "USD");
        disabled.enabled = false;
        disabled.show_in_tray = true;

        let rows = preview_rows(&report(vec![opted_in, not_opted_in, disabled]));

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "tray-balance:openrouter");
        assert_eq!(rows[0].text, "openrouter: $10.00");
    }

    #[test]
    fn preview_rows_keep_the_report_order() {
        let rows = preview_rows(&report(vec![
            shown(provider_with_value("openrouter", 10.0, "USD")),
            shown(provider_with_value("deepseek", 5.0, "USD")),
        ]));

        assert_eq!(rows[0].id, "tray-balance:openrouter");
        assert_eq!(rows[1].id, "tray-balance:deepseek");
    }

    #[test]
    fn a_provider_without_a_value_shows_a_placeholder() {
        let rows = preview_rows(&report(vec![
            shown(provider("openrouter", true, ProviderStatus::Loading)),
            shown(provider("deepseek", true, ProviderStatus::Idle)),
        ]));

        assert_eq!(rows[0].text, "openrouter: updating…");
        assert_eq!(rows[1].text, "deepseek: —");
    }

    /// A provider opted into the tray, reporting one amount per currency.
    fn tray_provider(
        provider_id: &str,
        display_name: &str,
        amounts: &[(f64, &str)],
    ) -> ProviderBalance {
        let mut entry = ProviderBalance::new(provider_id, display_name, true);
        entry.show_in_tray = true;
        entry.apply_success(BalanceSnapshot {
            provider_id: provider_id.to_string(),
            display_name: display_name.to_string(),
            amounts: amounts
                .iter()
                .map(|(amount, currency)| BalanceAmount {
                    amount: *amount,
                    currency: (*currency).to_string(),
                    label: format!("{currency} balance"),
                })
                .collect(),
            fetched_at: unix_now(),
        });
        entry
    }

    /// The preview text for a single provider row.
    fn row_text(entry: ProviderBalance) -> String {
        preview_rows(&report(vec![entry])).remove(0).text
    }

    #[test]
    fn every_currency_fits_on_one_provider_row() {
        let row = row_text(tray_provider(
            "deepseek",
            "DeepSeek",
            &[(72.0, "CNY"), (10.0, "USD")],
        ));

        assert_eq!(row, "DeepSeek: CN¥72.00 · $10.00");
    }

    #[test]
    fn a_zero_currency_is_hidden_when_another_one_has_money() {
        let row = row_text(tray_provider(
            "deepseek",
            "DeepSeek",
            &[(0.0, "CNY"), (9.42, "USD")],
        ));

        // `CN¥0.00` next to real money is noise.
        assert_eq!(row, "DeepSeek: $9.42");
    }

    #[test]
    fn an_amount_that_rounds_to_zero_is_hidden_too() {
        let row = row_text(tray_provider(
            "deepseek",
            "DeepSeek",
            &[(0.001, "CNY"), (9.42, "USD")],
        ));

        assert_eq!(row, "DeepSeek: $9.42");
    }

    #[test]
    fn a_lone_zero_is_still_shown() {
        let row = row_text(tray_provider("deepseek", "DeepSeek", &[(0.0, "USD")]));

        // Nothing else to show, so the zero is the answer.
        assert_eq!(row, "DeepSeek: $0.00");
    }

    #[test]
    fn every_currency_is_kept_when_they_are_all_zero() {
        let row = row_text(tray_provider(
            "deepseek",
            "DeepSeek",
            &[(0.0, "CNY"), (0.0, "USD")],
        ));

        // Dropping both would leave the row with nothing at all.
        assert_eq!(row, "DeepSeek: CN¥0.00 · $0.00");
    }
}
