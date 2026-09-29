import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Mirrors `core::types::AppInfo` on the Rust side. */
interface AppInfo {
  name: string;
  version: string;
  identifier: string;
}

/** Mirrors `core::preferences::Preferences` on the Rust side. */
interface Preferences {
  openWindowOnStart: boolean;
}

/** Mirrors `ui::ProviderView` on the Rust side. */
interface ProviderView {
  id: string;
  enabled: boolean;
  keyType: "management" | "standard" | null;
  hasKey: boolean;
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

type View = "balance" | "settings";

const PROVIDER_LABELS: Record<string, string> = {
  openrouter: "OpenRouter",
  deepseek: "DeepSeek",
};

const KEY_TYPE_LABELS: Record<string, string> = {
  management: "Management key",
  standard: "Standard key",
};

function query<T extends HTMLElement>(selector: string): T | null {
  return document.querySelector<T>(selector);
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

function label(provider: ProviderView): string {
  return PROVIDER_LABELS[provider.id] ?? provider.id;
}

function formatAmount(amount: number, currency: string): string {
  try {
    return new Intl.NumberFormat(undefined, { style: "currency", currency }).format(amount);
  } catch {
    return `${amount.toFixed(2)} ${currency}`;
  }
}

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
    .catch(() => setStatus("Could not read the current setting."));

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
  const row = document.createElement("li");
  row.className = "provider";

  const head = document.createElement("div");
  head.className = "provider__head";

  const text = document.createElement("span");
  text.className = "provider__text";

  const name = document.createElement("span");
  name.className = "provider__name";
  name.textContent = label(provider);

  const meta = document.createElement("span");
  meta.className = "provider__meta";
  const keyKind = provider.keyType ? (KEY_TYPE_LABELS[provider.keyType] ?? provider.keyType) : null;
  const keyState = provider.hasKey ? "key configured" : "no key stored";
  meta.textContent = keyKind ? `${keyKind} · ${keyState}` : keyState;

  text.append(name, meta);

  const enabled = document.createElement("input");
  enabled.type = "checkbox";
  enabled.className = "switch";
  enabled.checked = provider.enabled;
  enabled.setAttribute("aria-label", `Enable ${label(provider)}`);
  enabled.addEventListener("change", async () => {
    enabled.disabled = true;
    try {
      await invoke("set_provider_enabled", { providerId: provider.id, value: enabled.checked });
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
  input.placeholder = provider.hasKey ? "Key stored — type to replace" : "Paste the API key";
  input.setAttribute("aria-label", `${label(provider)} API key`);

  const save = document.createElement("button");
  save.type = "button";
  save.className = "link";
  save.textContent = "Save";
  save.addEventListener("click", async () => {
    const key = input.value.trim();
    if (!key) {
      setStatus("Enter a key before saving.");
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
  remove.textContent = "Remove";
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
  test.textContent = "Test";
  test.addEventListener("click", async () => {
    test.disabled = true;
    delete result.dataset.state;
    result.textContent = "Checking…";
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
  row.append(head, keyRow, testRow);

  return row;
}

window.addEventListener("DOMContentLoaded", async () => {
  document.querySelectorAll<HTMLElement>("[data-navigate]").forEach((element) => {
    element.addEventListener("click", () => navigate(element.dataset.navigate as View));
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

    const balanceInfo = query<HTMLParagraphElement>("#app-info");
    if (balanceInfo) {
      balanceInfo.textContent = summary;
      balanceInfo.title = info.identifier;
    }

    const settingsInfo = query<HTMLSpanElement>("#settings-app-info");
    if (settingsInfo) {
      settingsInfo.textContent = `${summary} · ${info.identifier}`;
    }
  } catch (error) {
    const balanceInfo = query<HTMLParagraphElement>("#app-info");
    if (balanceInfo) {
      balanceInfo.textContent = "The backend did not respond.";
    }
    console.error(error);
  }

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

  await listen<string>("navigate", (event) => {
    if (event.payload === "balance" || event.payload === "settings") {
      navigate(event.payload);
    }
  });
});
