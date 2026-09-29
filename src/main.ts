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

type View = "balance" | "settings";

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
