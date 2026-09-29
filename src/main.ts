import { invoke } from "@tauri-apps/api/core";

/** Mirrors `core::types::AppInfo` on the Rust side. */
interface AppInfo {
  name: string;
  version: string;
  identifier: string;
}

window.addEventListener("DOMContentLoaded", async () => {
  const target = document.querySelector<HTMLParagraphElement>("#app-info");
  if (!target) {
    return;
  }

  try {
    const info = await invoke<AppInfo>("app_info");
    target.textContent = `${info.name} v${info.version}`;
    target.title = info.identifier;
  } catch (error) {
    target.textContent = "The backend did not respond.";
    console.error(error);
  }
});
