import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { formatAmount, relativeTime } from "./format";
import { startPopup } from "./popup";
import { pollIntervals, providerNames, strings } from "./strings";

/** Mirrors `core::types::AppInfo` on the Rust side. */
interface AppInfo {
  name: string;
  version: string;
  identifier: string;
}

/** Mirrors `core::preferences::Preferences` on the Rust side. */
interface Preferences {
  openWindowOnStart: boolean;
  pollIntervalMinutes: number;
}

/** Mirrors `core::types::BalanceAmount` on the Rust side. */
interface BalanceAmount {
  amount: number;
  currency: string;
  label: string;
}

/** Mirrors `core::types::BalanceSnapshot` on the Rust side. */
interface BalanceSnapshot {
  providerId: string;
  displayName: string;
  amounts: BalanceAmount[];
  fetchedAt: number;
}

/** Mirrors `core::balances::ProviderStatus` on the Rust side. */
type ProviderStatus = "idle" | "loading" | "ok" | "error";

/** Mirrors `core::balances::FailureKind` on the Rust side. */
type FailureKind =
  "missingKey" | "unauthorized" | "rateLimited" | "network" | "invalid" | "unsupported" | "other";

/** Mirrors `core::balances::ProviderBalance` on the Rust side. */
interface ProviderBalance {
  providerId: string;
  displayName: string;
  enabled: boolean;
  status: ProviderStatus;
  snapshot: BalanceSnapshot | null;
  error: string | null;
  errorKind: FailureKind | null;
  updatedAt: number | null;
  checkedAt: number | null;
}

/** Mirrors `core::balances::CurrencyTotal` on the Rust side. */
interface CurrencyTotal {
  currency: string;
  amount: number;
}

/** Mirrors `core::balances::BalancesReport` on the Rust side. */
interface BalancesReport {
  totals: CurrencyTotal[];
  providers: ProviderBalance[];
}

/** Mirrors `ui::ProviderView` on the Rust side. */
interface ProviderView {
  id: string;
  enabled: boolean;
  showInTray: boolean;
  keyType: "management" | "standard" | null;
  hasKey: boolean;
}

type View = "balance" | "settings";

function query<T extends HTMLElement>(selector: string): T | null {
  return document.querySelector<T>(selector);
}

function displayName(providerId: string, fallback: string): string {
  return providerNames[providerId] ?? fallback;
}

/** Fills every static label from `strings`. */
function applyStrings(): void {
  document.title = strings.appName;

  const set = (selector: string, text: string) => {
    const element = query(selector);
    if (element) {
      element.textContent = text;
    }
  };

  set("#balance-title", strings.appName);
  set("#refresh", strings.balance.refresh);
  set("#total-label", strings.balance.total);
  set("#open-settings", strings.balance.settings);

  set("#settings-title", strings.settings.title);
  set("#close-settings", strings.settings.back);
  set("#settings-general", strings.settings.general);
  set("#settings-providers", strings.settings.providers);

  set("#open-window-on-start-label", strings.settings.openWindowOnLaunch.label);
  set("#open-window-on-start-hint", strings.settings.openWindowOnLaunch.hint);
  set("#launch-at-login-label", strings.settings.launchAtLogin.label);
  set("#launch-at-login-hint", strings.settings.launchAtLogin.hint);
  set("#poll-interval-label", strings.settings.refreshInterval.label);
  set("#poll-interval-hint", strings.settings.refreshInterval.hint);

  const labelled = (selector: string, label: string) =>
    query(selector)?.setAttribute("aria-label", label);
  labelled("#refresh", strings.balance.refresh);
  labelled("#open-window-on-start", strings.settings.openWindowOnLaunch.label);
  labelled("#launch-at-login", strings.settings.launchAtLogin.label);
  labelled("#poll-interval", strings.settings.refreshInterval.label);

  const interval = query<HTMLSelectElement>("#poll-interval");
  if (interval) {
    interval.replaceChildren(
      ...pollIntervals.map((minutes) => {
        const option = document.createElement("option");
        option.value = String(minutes);
        option.textContent = strings.settings.refreshInterval.option(minutes);
        return option;
      }),
    );
  }
}

const views: Record<View, HTMLElement | null> = {
  balance: query<HTMLElement>("#view-balance"),
  settings: query<HTMLElement>("#view-settings"),
};

function navigate(view: View): void {
  for (const [name, element] of Object.entries(views)) {
    if (element) {
      element.hidden = name !== view;
    }
  }
}

// ---------------------------------------------------------------------------
// Balance view
// ---------------------------------------------------------------------------

/** The last report, kept so the relative times can be refreshed on a timer. */
let latest: BalancesReport | null = null;
/** Minutes between refreshes, for the footer. */
let pollMinutes = 10;

function enabledProviders(report: BalancesReport): ProviderBalance[] {
  return report.providers.filter((it) => it.enabled);
}

/** A failure the user should act on, as opposed to a key they never added. */
function isFailure(provider: ProviderBalance): boolean {
  return provider.errorKind !== null && provider.errorKind !== "missingKey";
}

function statusBadge(provider: ProviderBalance): { label: string; state: string } {
  if (!provider.enabled) {
    return { label: strings.provider.status.disabled, state: "muted" };
  }

  switch (provider.status) {
    case "ok":
      return { label: strings.provider.status.ok, state: "ok" };
    case "loading":
      return { label: strings.provider.status.loading, state: "busy" };
    case "error":
      return provider.errorKind === "missingKey"
        ? { label: strings.provider.status.missingKey, state: "muted" }
        : { label: strings.provider.status.error, state: "error" };
    default:
      return { label: strings.provider.status.idle, state: "muted" };
  }
}

function metaText(report: BalancesReport): string {
  const enabled = enabledProviders(report);

  if (enabled.length === 0) {
    return strings.balance.noProviders;
  }

  if (report.totals.length === 0) {
    return strings.balance.noBalances;
  }

  if (enabled.some((it) => it.status === "loading")) {
    return strings.balance.refreshing;
  }

  const parts = [strings.balance.providers(enabled.length)];

  const failures = enabled.filter(isFailure).length;
  if (failures > 0) {
    parts.push(strings.balance.errors(failures));
  }

  const updatedAt = Math.max(...enabled.map((it) => it.updatedAt ?? 0));
  parts.push(updatedAt > 0 ? relativeTime(updatedAt) : strings.balance.updated.never);

  return parts.join(" · ");
}

function overlay(...parts: HTMLElement[]): HTMLElement {
  const wrapper = document.createElement("span");
  wrapper.className = "stack";
  wrapper.append(...parts);
  return wrapper;
}

function balanceRow(provider: ProviderBalance): HTMLLIElement {
  const badge = statusBadge(provider);

  const row = document.createElement("li");
  row.className = "balance";
  row.dataset.state = provider.enabled ? provider.status : "disabled";

  const avatar = document.createElement("span");
  avatar.className = "balance__avatar";
  avatar.textContent = displayName(provider.providerId, provider.displayName).charAt(0);

  const text = document.createElement("span");
  text.className = "balance__text";

  const name = document.createElement("span");
  name.className = "balance__name";
  name.textContent = displayName(provider.providerId, provider.displayName);

  const detail = document.createElement("span");
  detail.className = "balance__detail";
  // A failure is the most useful thing to show. Otherwise the label says what
  // the number is — unless the provider reports several currencies, in which
  // case no single label describes them all.
  const amountsOf = provider.snapshot?.amounts ?? [];
  detail.textContent =
    provider.error ??
    (amountsOf.length === 1
      ? amountsOf[0].label
      : amountsOf.length > 1
        ? strings.balance.perCurrency
        : "");

  text.append(name, detail);

  const right = document.createElement("span");
  right.className = "balance__right";

  const amounts = document.createElement("span");
  amounts.className = "balance__amounts";

  if (provider.snapshot) {
    for (const entry of provider.snapshot.amounts) {
      const value = document.createElement("span");
      value.className = "balance__amount";
      value.textContent = formatAmount(entry.amount, entry.currency);
      amounts.append(value);
    }
  } else if (provider.status === "loading" && provider.enabled) {
    const skeleton = document.createElement("span");
    skeleton.className = "skeleton";
    amounts.append(skeleton);
  } else {
    const empty = document.createElement("span");
    empty.className = "balance__amount balance__amount--empty";
    empty.textContent = strings.balance.noValue;
    amounts.append(empty);
  }

  const tag = document.createElement("span");
  tag.className = "badge";
  tag.dataset.state = badge.state;
  tag.textContent = badge.label;

  // When each value was read, so a stale one is recognisable at a glance.
  const stamp = document.createElement("span");
  stamp.className = "balance__stamp";
  stamp.textContent =
    provider.updatedAt !== null ? relativeTime(provider.updatedAt) : strings.balance.updated.never;

  right.append(overlay(amounts), overlay(tag, stamp));

  row.append(avatar, text, right);

  return row;
}

function renderBalances(report: BalancesReport): void {
  latest = report;

  const totals = query<HTMLDivElement>("#totals");
  if (totals) {
    totals.replaceChildren(
      ...report.totals.map((total) => {
        const value = document.createElement("span");
        value.className = "hero__total";
        value.textContent = formatAmount(total.amount, total.currency);
        return value;
      }),
    );
  }

  const meta = query<HTMLSpanElement>("#balance-meta");
  if (meta) {
    meta.textContent = metaText(report);
  }

  const list = query<HTMLUListElement>("#balance-providers");
  if (list) {
    list.replaceChildren(...report.providers.map(balanceRow));
  }

  renderFooter();
}

function renderFooter(): void {
  const footer = query<HTMLSpanElement>("#balance-footer");
  if (footer) {
    footer.textContent = strings.balance.autoRefresh(pollMinutes);
  }
}

// ---------------------------------------------------------------------------
// Settings view
// ---------------------------------------------------------------------------

/**
 * Wires a checkbox to a Rust getter/setter pair, reverting the toggle when the
 * backend rejects the change.
 */
function bindSwitch(
  input: HTMLInputElement | null,
  read: () => Promise<boolean>,
  write: (value: boolean) => Promise<unknown>,
  setStatus: (message: string) => void,
): void {
  if (!input) {
    return;
  }

  read()
    .then((value) => {
      input.checked = value;
    })
    .catch(() => setStatus(strings.status.readSettingFailed));

  input.addEventListener("change", async () => {
    input.disabled = true;
    try {
      await write(input.checked);
      setStatus("");
    } catch (error) {
      input.checked = !input.checked;
      setStatus(String(error));
    } finally {
      input.disabled = false;
    }
  });
}

/** Builds the settings row for one provider. */
function providerRow(
  provider: ProviderView,
  refresh: () => Promise<void>,
  setStatus: (message: string) => void,
): HTMLLIElement {
  const name = displayName(provider.id, provider.id);

  const row = document.createElement("li");
  row.className = "provider";

  const head = document.createElement("div");
  head.className = "provider__head";

  const text = document.createElement("span");
  text.className = "provider__text";

  const title = document.createElement("span");
  title.className = "provider__name";
  title.textContent = name;

  const meta = document.createElement("span");
  meta.className = "provider__meta";
  const keyKind =
    provider.keyType === "management"
      ? strings.provider.managementKey
      : provider.keyType === "standard"
        ? strings.provider.standardKey
        : null;
  const keyState = provider.hasKey ? strings.provider.keyConfigured : strings.provider.noKeyStored;
  meta.textContent = keyKind ? `${keyKind} · ${keyState}` : keyState;

  text.append(title, meta);

  const enabled = document.createElement("input");
  enabled.type = "checkbox";
  enabled.className = "switch";
  enabled.checked = provider.enabled;
  enabled.setAttribute("aria-label", name);

  const trayRow = document.createElement("div");
  trayRow.className = "provider__option";

  const trayLabel = document.createElement("label");
  trayLabel.className = "provider__option-label";
  trayLabel.textContent = strings.provider.showInTray;

  const tray = document.createElement("input");
  tray.type = "checkbox";
  tray.className = "switch";
  tray.checked = provider.showInTray;
  tray.disabled = !provider.enabled;
  tray.setAttribute("aria-label", `${name} ${strings.provider.showInTray}`);
  tray.addEventListener("change", async () => {
    tray.disabled = true;
    try {
      await invoke("set_provider_show_in_tray", {
        providerId: provider.id,
        value: tray.checked,
      });
      setStatus("");
    } catch (error) {
      tray.checked = !tray.checked;
      setStatus(String(error));
    } finally {
      // Toggling `enabled` above also gates this switch.
      tray.disabled = !enabled.checked;
    }
  });

  trayRow.append(trayLabel, tray);

  enabled.addEventListener("change", async () => {
    enabled.disabled = true;
    try {
      await invoke("set_provider_enabled", { providerId: provider.id, value: enabled.checked });
      // A disabled provider cannot be previewed.
      tray.disabled = !enabled.checked;
      setStatus("");
    } catch (error) {
      enabled.checked = !enabled.checked;
      setStatus(String(error));
    } finally {
      enabled.disabled = false;
    }
  });

  head.append(text, enabled);

  const keyRow = document.createElement("div");
  keyRow.className = "provider__key";

  const input = document.createElement("input");
  input.type = "password";
  input.className = "key-input";
  input.autocomplete = "off";
  input.spellcheck = false;
  input.placeholder = provider.hasKey
    ? strings.provider.keyPlaceholder.stored
    : strings.provider.keyPlaceholder.empty;
  input.setAttribute("aria-label", `${name} ${strings.provider.keyPlaceholder.empty}`);

  const save = document.createElement("button");
  save.type = "button";
  save.className = "link";
  save.textContent = strings.provider.save;
  save.addEventListener("click", async () => {
    const key = input.value.trim();
    if (!key) {
      setStatus(strings.status.keyRequired);
      return;
    }

    save.disabled = true;
    try {
      await invoke("set_provider_key", { providerId: provider.id, key });
      input.value = "";
      setStatus("");
      await refresh();
    } catch (error) {
      setStatus(String(error));
    } finally {
      save.disabled = false;
    }
  });

  const remove = document.createElement("button");
  remove.type = "button";
  remove.className = "link";
  remove.textContent = strings.provider.remove;
  remove.disabled = !provider.hasKey;
  remove.addEventListener("click", async () => {
    remove.disabled = true;
    try {
      await invoke("delete_provider_key", { providerId: provider.id });
      setStatus("");
      await refresh();
    } catch (error) {
      setStatus(String(error));
    } finally {
      remove.disabled = false;
    }
  });

  keyRow.append(input, save, remove);

  const testRow = document.createElement("div");
  testRow.className = "provider__actions";

  const result = document.createElement("span");
  result.className = "provider__result";

  const test = document.createElement("button");
  test.type = "button";
  test.className = "link";
  test.textContent = strings.provider.test;
  test.addEventListener("click", async () => {
    test.disabled = true;
    delete result.dataset.state;
    result.textContent = strings.provider.checking;
    try {
      const snapshot = await invoke<BalanceSnapshot>("fetch_provider_balance", {
        providerId: provider.id,
      });
      result.textContent = snapshot.amounts
        .map((it) => `${formatAmount(it.amount, it.currency)} ${it.label}`)
        .join(" · ");
      result.dataset.state = "ok";
    } catch (error) {
      result.textContent = String(error);
      result.dataset.state = "error";
    } finally {
      test.disabled = false;
    }
  });

  testRow.append(test, result);
  row.append(head, trayRow, keyRow, testRow);

  return row;
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

window.addEventListener("DOMContentLoaded", async () => {
  // The tray popover shares this bundle but renders its own card.
  if (getCurrentWindow().label === "popup") {
    await startPopup();
    return;
  }

  applyStrings();

  query("#open-settings")?.addEventListener("click", () => navigate("settings"));
  query("#close-settings")?.addEventListener("click", () => navigate("balance"));

  query("#refresh")?.addEventListener("click", () => {
    void invoke("refresh_balances");
  });

  const status = query<HTMLParagraphElement>("#settings-status");
  const setStatus = (message: string) => {
    if (status) {
      status.textContent = message;
    }
  };

  const providerList = query<HTMLUListElement>("#providers");
  const refreshProviders = async () => {
    if (!providerList) {
      return;
    }

    const providers = await invoke<ProviderView[]>("get_providers");
    providerList.replaceChildren(
      ...providers.map((provider) => providerRow(provider, refreshProviders, setStatus)),
    );
  };

  try {
    await refreshProviders();
  } catch (error) {
    setStatus(String(error));
  }

  try {
    const info = await invoke<AppInfo>("app_info");
    const summary = `${info.name} v${info.version}`;
    const settingsInfo = query<HTMLSpanElement>("#settings-app-info");
    if (settingsInfo) {
      settingsInfo.textContent = `${summary} · ${info.identifier}`;
    }
  } catch (error) {
    setStatus(strings.status.backendUnreachable);
    console.error(error);
  }

  // The balances arrive from the poller; the window only ever renders them.
  await listen<BalancesReport>("balances", (event) => renderBalances(event.payload));

  try {
    renderBalances(await invoke<BalancesReport>("get_balances"));
  } catch (error) {
    setStatus(String(error));
  }

  // Relative times would otherwise freeze at whatever they said on first paint.
  window.setInterval(() => {
    if (latest) {
      renderBalances(latest);
    }
  }, 30_000);

  bindSwitch(
    query<HTMLInputElement>("#open-window-on-start"),
    async () => (await invoke<Preferences>("get_preferences")).openWindowOnStart,
    (value) => invoke("set_open_window_on_start", { value }),
    setStatus,
  );

  bindSwitch(
    query<HTMLInputElement>("#launch-at-login"),
    () => invoke<boolean>("get_launch_at_login"),
    (value) => invoke("set_launch_at_login", { value }),
    setStatus,
  );

  const interval = query<HTMLSelectElement>("#poll-interval");
  if (interval) {
    invoke<Preferences>("get_preferences")
      .then((preferences) => {
        pollMinutes = preferences.pollIntervalMinutes;
        interval.value = String(pollMinutes);
        renderFooter();
      })
      .catch(() => setStatus(strings.status.readIntervalFailed));

    interval.addEventListener("change", async () => {
      interval.disabled = true;
      try {
        // The backend clamps, so it decides what the setting actually became.
        pollMinutes = await invoke<number>("set_poll_interval", {
          minutes: Number(interval.value),
        });
        interval.value = String(pollMinutes);
        renderFooter();
        setStatus("");
      } catch (error) {
        setStatus(String(error));
      } finally {
        interval.disabled = false;
      }
    });
  }

  await listen<string>("navigate", (event) => {
    if (event.payload === "balance" || event.payload === "settings") {
      navigate(event.payload);
    }
  });
});
