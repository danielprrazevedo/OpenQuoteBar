/**
 * The tray popover: the rich balance card anchored under the menu bar icon.
 *
 * Loaded by the `popup` window, which shares this bundle with the main window
 * and is told apart by its window label. It renders from the same `balances`
 * event the rest of the app uses.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { formatAmount, relativeTime } from "./format";
import { providerNames, strings } from "./strings";

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
  showInTray: boolean;
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

/** Mirrors `core::preferences::Preferences` on the Rust side. */
interface Preferences {
  openWindowOnStart: boolean;
  pollIntervalMinutes: number;
}

/** Padding around the card, in CSS pixels, so its shadow is not clipped. */
const SHADOW_PADDING = 24;

/** Per-provider avatar colour, matching the mockups. */
const AVATAR_COLORS: Record<string, string> = {
  openrouter: "#5b6472",
  deepseek: "#4d6bfe",
};

const REFRESH_ICON = `<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M13 8a5 5 0 1 1-1.46-3.54" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/><path d="M13.1 2.7v2.7H10.4" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>`;

function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) {
    node.className = className;
  }
  if (text !== undefined) {
    node.textContent = text;
  }
  return node;
}

function displayName(providerId: string, fallback: string): string {
  return providerNames[providerId] ?? fallback;
}

/** A status pill, mirroring the one the main window uses. */
function statusBadge(provider: ProviderBalance): { label: string; tone: string } {
  if (!provider.enabled) {
    return { label: strings.provider.status.disabled, tone: "muted" };
  }

  switch (provider.status) {
    case "ok":
      return { label: strings.provider.status.ok, tone: "ok" };
    case "loading":
      return { label: strings.provider.status.loading, tone: "busy" };
    case "error":
      return provider.errorKind === "missingKey"
        ? { label: strings.provider.status.missingKey, tone: "muted" }
        : { label: strings.provider.status.error, tone: "error" };
    default:
      return { label: strings.provider.status.idle, tone: "muted" };
  }
}

/**
 * Sums the shown providers per currency, dropping a zero currency when another
 * one has money. The same rule the Rust side applies to the window's totals.
 */
function totalsOf(providers: ProviderBalance[]): CurrencyTotal[] {
  const sums = new Map<string, number>();

  for (const provider of providers) {
    for (const amount of provider.snapshot?.amounts ?? []) {
      sums.set(amount.currency, (sums.get(amount.currency) ?? 0) + amount.amount);
    }
  }

  const all = [...sums.entries()]
    .map(([currency, amount]) => ({ currency, amount }))
    .sort((left, right) => left.currency.localeCompare(right.currency));

  const nonZero = all.filter((total) => Math.abs(total.amount) >= 0.005);
  return nonZero.length > 0 ? nonZero : all;
}

/** The one-line status under the app name. */
function statusLine(enabled: ProviderBalance[]): string {
  if (enabled.length === 0) {
    return strings.balance.noProviders;
  }

  if (enabled.some((provider) => provider.status === "loading")) {
    return strings.balance.refreshing;
  }

  const parts = [strings.balance.providers(enabled.length)];

  const failures = enabled.filter(
    (provider) => provider.errorKind !== null && provider.errorKind !== "missingKey",
  ).length;
  if (failures > 0) {
    parts.push(strings.balance.errors(failures));
  }

  const updatedAt = Math.max(...enabled.map((provider) => provider.updatedAt ?? 0));
  parts.push(updatedAt > 0 ? relativeTime(updatedAt) : strings.balance.updated.never);

  return parts.join(" · ");
}

/** Builds one provider tile. */
function providerTile(provider: ProviderBalance): HTMLElement {
  const tile = element("div", "popup__tile");

  const avatar = element(
    "span",
    "popup__avatar",
    displayName(provider.providerId, provider.displayName).charAt(0),
  );
  avatar.style.backgroundColor = AVATAR_COLORS[provider.providerId] ?? "#5b6472";

  const text = element("span", "popup__tile-text");
  text.append(
    element("span", "popup__tile-name", displayName(provider.providerId, provider.displayName)),
  );

  const amounts = provider.snapshot?.amounts ?? [];
  const detail =
    provider.error ??
    (amounts.length === 1
      ? amounts[0].label
      : amounts.length > 1
        ? strings.balance.perCurrency
        : "");
  text.append(element("span", "popup__tile-sub", detail));

  const right = element("span", "popup__tile-right");
  right.append(
    element(
      "span",
      "popup__tile-amount",
      amounts.length > 0
        ? amounts.map((amount) => formatAmount(amount.amount, amount.currency)).join(" · ")
        : strings.balance.noValue,
    ),
  );

  const badge = statusBadge(provider);
  right.append(element("span", `popup__badge popup__badge--${badge.tone}`, badge.label));

  tile.append(avatar, text, right);
  return tile;
}

/** Renders the card into the popup root and resizes the window to fit it. */
function render(root: HTMLElement, report: BalancesReport, pollMinutes: number): void {
  const enabled = report.providers.filter((provider) => provider.enabled);
  const shown = enabled.filter((provider) => provider.showInTray);

  const card = element("div", "popup");

  // Header.
  const header = element("div", "popup__header");
  const titles = element("span", "popup__titles");
  titles.append(element("span", "popup__name", strings.appName));
  titles.append(element("span", "popup__status", statusLine(enabled)));

  const refresh = element("button", "popup__icon-btn");
  refresh.type = "button";
  refresh.setAttribute("aria-label", strings.balance.refresh);
  refresh.innerHTML = REFRESH_ICON;
  refresh.addEventListener("click", () => {
    void invoke("refresh_balances");
  });

  header.append(element("span", "popup__mark", "$"), titles, refresh);

  // Hero.
  const hero = element("div", "popup__hero");
  hero.append(element("span", "popup__hero-label", strings.balance.total));
  const totals = element("div", "popup__totals");
  const values = totalsOf(shown);
  if (values.length === 0) {
    totals.append(element("span", "popup__total", strings.balance.noValue));
  } else {
    for (const total of values) {
      totals.append(element("span", "popup__total", formatAmount(total.amount, total.currency)));
    }
  }
  hero.append(totals);

  // Providers.
  const list = element("div", "popup__providers");
  if (shown.length === 0) {
    list.append(element("p", "popup__empty", strings.balance.noTrayProviders));
  } else {
    for (const provider of shown) {
      list.append(providerTile(provider));
    }
  }

  // Footer.
  const footer = element("div", "popup__footer");
  footer.append(element("span", "popup__footer-meta", strings.balance.autoRefresh(pollMinutes)));
  const settings = element("button", "popup__link", strings.balance.settings);
  settings.type = "button";
  settings.addEventListener("click", () => {
    void invoke("show_window", { view: "settings" });
  });
  footer.append(settings);

  card.append(header, hero, list, footer);
  root.replaceChildren(card);

  // Let the browser lay the card out before measuring it.
  requestAnimationFrame(() => {
    const height = Math.ceil(card.getBoundingClientRect().height) + SHADOW_PADDING * 2;
    void invoke("set_popup_size", { height });
  });
}

/** Boots the popover. Called instead of the main window's setup. */
export async function startPopup(): Promise<void> {
  document.documentElement.classList.add("popup-window");

  const root = document.getElementById("popup-root");
  if (!root) {
    return;
  }

  let pollMinutes = 10;
  try {
    pollMinutes = (await invoke<Preferences>("get_preferences")).pollIntervalMinutes;
  } catch {
    // The default is fine; the interval only feeds the footer line.
  }

  await listen<BalancesReport>("balances", (event) => render(root, event.payload, pollMinutes));

  try {
    render(root, await invoke<BalancesReport>("get_balances"), pollMinutes);
  } catch {
    // Nothing to show yet; the next balances event will fill it in.
  }

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      void invoke("hide_popup");
    }
  });
}
