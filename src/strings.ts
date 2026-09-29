/**
 * Every user-visible string, in one place.
 *
 * English is the primary language; additional languages are out of scope for
 * now. Nothing in the markup carries text of its own, so adding one later
 * should only mean changing what this module returns.
 */
export const strings = {
  appName: "OpenQuoteBar",

  balance: {
    total: "Total available",
    refresh: "Refresh",
    refreshing: "Refreshing…",
    settings: "Settings",
    noProviders: "No providers enabled. Turn one on in Settings.",
    noBalances: "No balances yet. Add an API key in Settings.",
    noValue: "—",
    autoRefresh: (minutes: number) => `Auto-refresh every ${minutes} min`,
    providers: (count: number) => `${count} provider${count === 1 ? "" : "s"}`,
    errors: (count: number) => `${count} error${count === 1 ? "" : "s"}`,
    updated: {
      justNow: "updated just now",
      minutes: (minutes: number) => `updated ${minutes} min ago`,
      hours: (hours: number) => `updated ${hours} h ago`,
      never: "not updated yet",
    },
  },

  provider: {
    managementKey: "Management key",
    standardKey: "Standard key",
    keyConfigured: "key configured",
    noKeyStored: "no key stored",
    keyPlaceholder: {
      stored: "Key stored — type to replace",
      empty: "Paste the API key",
    },
    save: "Save",
    remove: "Remove",
    test: "Test",
    checking: "Checking…",
    status: {
      idle: "Waiting",
      loading: "Updating",
      ok: "OK",
      error: "Error",
      disabled: "Disabled",
      missingKey: "No key",
    },
  },

  settings: {
    title: "Settings",
    back: "Back",
    general: "General",
    providers: "Providers",
    openWindowOnLaunch: {
      label: "Open window on launch",
      hint: "Start with the details window instead of the menu bar icon alone.",
    },
    launchAtLogin: {
      label: "Launch at login",
      hint: "Start OpenQuoteBar automatically when you log in.",
    },
    refreshInterval: {
      label: "Refresh interval",
      hint: "How often balances are fetched.",
      option: (minutes: number) => `Every ${minutes} min`,
    },
  },

  status: {
    backendUnreachable: "The backend did not respond.",
    readSettingFailed: "Could not read the current setting.",
    readIntervalFailed: "Could not read the refresh interval.",
    keyRequired: "Enter a key before saving.",
  },
} as const;

/** Display names for the providers the app knows how to talk to. */
export const providerNames: Record<string, string> = {
  openrouter: "OpenRouter",
  deepseek: "DeepSeek",
};

/** Intervals the settings offer, in minutes. */
export const pollIntervals = [5, 10, 15] as const;
