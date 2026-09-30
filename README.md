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

## Configuration

Two files live in the OS app config directory — `~/Library/Application Support/com.openquotebar.app/`
on macOS, `%APPDATA%\com.openquotebar.app\` on Windows:

| File               | Written by | Purpose                                       |
| ------------------ | ---------- | --------------------------------------------- |
| `config.toml`      | you        | Providers the app reads, and their options.   |
| `preferences.json` | the app    | Window and startup preferences set in the UI. |

`config.toml` is created on first run:

```toml
[[providers]]
id = "openrouter"
enabled = true
key_type = "management"

[[providers]]
id = "deepseek"
enabled = true
```

`id` must match an adapter bundled with the app, and `enabled` decides whether the app reads that
provider's balance. `key_type` is OpenRouter-specific: `management` reads the account-wide credits
endpoint, `standard` reads the per-key limit instead.

**API keys are never written to that file.** They are stored in the OS credential store — macOS
Keychain, Windows Credential Manager — under the service `com.openquotebar.app`, one entry per
provider id. Add, replace or remove them from the Settings view; the backend only ever tells the UI
_whether_ a key is stored, never the key itself. On macOS you can inspect an entry directly:

```bash
security find-generic-password -s com.openquotebar.app -a openrouter
```

Editing `config.toml` by hand is supported. Note that toggling a provider from the app rewrites the
file from its parsed values: the documented header is preserved, but comments added inside the
provider entries are not.

Each provider row in Settings also has a **Test** button, which fetches the current balance using the
stored key and reports either the amount or the reason it failed. The provider adapters live in
`src-tauri/src/adapters/`; each one is behind the same `ProviderAdapter` trait and returns a
normalized `BalanceSnapshot`, so the UI never has to know how a provider expresses "balance". A
snapshot carries one amount per currency — DeepSeek can report both USD and CNY — and amounts in
different currencies are never summed together.

## Refreshing

Balances are refreshed by a background loop that runs a cycle as soon as the app starts and then
every 5 to 15 minutes, according to the **Refresh interval** in Settings (default: 10). Providers are
fetched concurrently, and the tray menu's **Refresh now** triggers a cycle immediately. The tray
tooltip mirrors the state — updating, how many providers, how many are failing, and when the values
were last refreshed.

A failed fetch never clears the last value it managed to read: the cache keeps the previous snapshot
and records the error alongside it, so a provider that is briefly unreachable keeps showing the
balance you already knew about.

## Building

```bash
pnpm install
pnpm tauri build
```

That produces the installers for the machine you are on:

| Platform | Artifacts                                                              |
| -------- | ---------------------------------------------------------------------- |
| macOS    | `src-tauri/target/release/bundle/dmg/*.dmg` and `.../macos/*.app`      |
| Windows  | `src-tauri/target/release/bundle/msi/*.msi` and `.../nsis/*-setup.exe` |

The macOS bundle is signed **ad-hoc** (`bundle.macOS.signingIdentity = "-"`), which is what stops
Apple Silicon builds downloaded from a release from being reported as damaged. It is not a real
signature: macOS still asks the user to allow the app the first time (System Settings → Privacy &
Security → Open Anyway), and Windows SmartScreen warns about an unknown publisher. Real signing is
deliberately out of scope for now, but the workflow is ready for the certificates once they exist.

Linux is not supported.

## Releases

Releases are built by GitHub Actions:

- `.github/workflows/ci.yml` runs the checks and tests on macOS and compiles and tests the backend on
  Windows, on every push to `main` and every pull request. It does not exercise the tray or the
  autostart integration, which need a real desktop.
- `.github/workflows/release.yml` runs on a `v*` tag, builds a universal macOS DMG plus the Windows
  installers, and opens a **draft** release with the artifacts and the changelog.

To cut a release:

1. Bump `version` in `package.json` **and** `src-tauri/Cargo.toml`. The app itself reads the version
   from `package.json`, through `tauri.conf.json`.
2. Regenerate the changelog with `pnpm changelog` (needs [`git-cliff`](https://git-cliff.org)).
3. Commit, then tag and push:

   ```bash
   git tag v0.2.0
   git push origin main --tags
   ```

4. Review the draft release and publish it.

The workflow fails the run when the tag does not match the version in `package.json` and
`Cargo.toml`, so a release can never claim a version the app does not report.

## Icons

The artwork lives in `design/` as two SVGs, and those are the source of truth:

| File                   | What it is                                            |
| ---------------------- | ----------------------------------------------------- |
| `design/app-icon.svg`  | The app mark: a dollar sign on a blue rounded square. |
| `design/tray-icon.svg` | The same sign, monochrome, for the macOS menu bar.    |

Regenerate the platform assets after editing either of them:

```bash
pnpm tauri icon design/app-icon.svg
pnpm tauri icon design/tray-icon.svg -p 88 -o src-tauri/icons/tray
```

The second command produces the PNG the tray embeds. It is 88 px rather than 22 or 44 because the
tray layer normalises the image to 18 points of height and scales the width to match, so a
generously sized square is what keeps it crisp at every display density.

On macOS the tray icon is a _template image_: the system uses its alpha channel as a mask and tints
it for light and dark menu bars, which is why the tray source is monochrome. Windows has no
equivalent, so it gets the app mark, which has its own background and reads on any taskbar colour.

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
