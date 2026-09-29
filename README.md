# OpenQuoteBar

Lightweight menu bar / system tray app (macOS and Windows) that shows the available balance of
your LLM API providers. The app is provider-agnostic: every provider is an adapter behind a single
interface. Milestone 1 ships **OpenRouter** and **DeepSeek**; more providers land later.

> Linux is out of scope for now.

## Status

Milestone 1 — scaffold only. The app boots, exposes `app_info` to the webview and already pulls in
the base Rust dependencies. Balances are not fetched yet; see the open issues for what comes next.

## Prerequisites

- **Rust** stable 1.88+ — via [rustup](https://rustup.rs/) or your package manager.
- **Node.js 20+** and **pnpm**.
- **macOS** — Xcode Command Line Tools (`xcode-select --install`).
- **Windows** — WebView2 (preinstalled on Windows 11) and the Microsoft C++ Build Tools with the
  "Desktop development with C++" workload.

## Getting started

```bash
pnpm install
pnpm tauri dev
```

## Scripts

| Command                            | What it does                                          |
| ---------------------------------- | ----------------------------------------------------- |
| `pnpm tauri dev`                   | Run the app in development (Vite + Tauri).            |
| `pnpm tauri build`                 | Build the macOS / Windows bundles.                    |
| `pnpm typecheck`                   | Type-check the frontend (`tsc --noEmit`).             |
| `pnpm lint`                        | Lint the frontend with ESLint.                        |
| `pnpm format` / `format:check`     | Write / verify Prettier formatting.                   |
| `pnpm rust:fmt` / `rust:fmt:check` | Format / verify the Rust code.                        |
| `pnpm rust:clippy`                 | Lint the Rust code with Clippy (warnings are errors). |
| `pnpm check`                       | Run every check above in one go.                      |

## Project layout

```
.
├── index.html            # webview entry point
├── src/                  # frontend: Vite + TypeScript (no framework)
└── src-tauri/
    ├── tauri.conf.json   # app configuration (identifier, window, bundle)
    ├── capabilities/     # Tauri 2 permissions
    └── src/
        ├── main.rs       # binary entry point
        ├── lib.rs        # Tauri builder + command registration
        ├── core/         # provider-agnostic domain types and state
        ├── adapters/     # provider integrations (one adapter per provider)
        └── ui/           # Tauri commands the webview calls
```

## IDE setup

[VS Code](https://code.visualstudio.com/) with
[Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) and
[rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).
