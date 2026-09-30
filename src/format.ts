/**
 * Number and time formatting shared by the main window and the tray popover.
 *
 * Kept in one place so the two surfaces never disagree about how an amount or a
 * timestamp reads.
 */

import { strings } from "./strings";

/**
 * Formats an amount the way the tray tooltip does, so they never disagree:
 * `$13.13`, not the `US$13.13` a non-US locale would produce.
 *
 * The locale is pinned because the interface is English-only for now. When
 * other languages arrive, this becomes the active language's locale.
 */
const AMOUNT_LOCALE = "en";

export function formatAmount(amount: number, currency: string): string {
  try {
    return new Intl.NumberFormat(AMOUNT_LOCALE, { style: "currency", currency }).format(amount);
  } catch {
    return `${amount.toFixed(2)} ${currency}`;
  }
}

/** "updated just now" / "updated 4 min ago" / "updated 2 h ago". */
export function relativeTime(timestamp: number): string {
  const elapsed = Math.max(0, Math.floor(Date.now() / 1000) - timestamp);

  if (elapsed < 60) {
    return strings.balance.updated.justNow;
  }

  const minutes = Math.floor(elapsed / 60);
  if (minutes < 60) {
    return strings.balance.updated.minutes(minutes);
  }

  return strings.balance.updated.hours(Math.floor(minutes / 60));
}
