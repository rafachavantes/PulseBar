# PulseBar

> Every AI coding limit, in your Windows tray.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Windows](https://img.shields.io/badge/Platform-Windows-0078D4.svg)](https://github.com/rafachavantes/PulseBar/releases)
[![Tauri](https://img.shields.io/badge/Built%20with-Tauri-orange.svg)](https://tauri.app)

PulseBar keeps AI coding-tool usage visible without opening a dozen dashboards:
session windows, weekly quotas, burn pace, and token counts for every provider
you use — one glance at the tray.

Windows desktop app built with Tauri + React, backed by shared Rust provider
logic. Fork of [Win-CodexBar](https://github.com/Finesssee/Win-CodexBar)
(v0.37.2), which is a Windows port of
[CodexBar](https://github.com/steipete/CodexBar) by Peter Steinberger.

## Features

- **Multi-provider tray monitoring** — one icon per provider, or merge mode
  with a switcher.
- **Usage meters with reset countdowns** — session (5h), weekly, and monthly
  windows with time-to-reset.
- **Severity thresholds** — tray icon, panel cards, and float bar share one
  threshold set (70% warn / 90% critical), always colored by used percent.
- **Float bar** — an always-on-top usage strip that stays legible on any
  wallpaper.
- **Auto-update** — checks GitHub releases on launch, downloads and installs
  silently.
- **Privacy-first** — on-device parsing, opt-in browser cookies, DPAPI-protected
  secrets. No passwords stored.

## Supported providers

| Provider | Auth | Tracks |
|---|---|---|
| Codex | OAuth / CLI | Session, Weekly, Credits |
| Claude | Cookies / OAuth / CLI | Session (5h), Weekly |
| Gemini | gcloud OAuth | Quota |
| z.ai | API Token | Quota (5h, Weekly, Monthly Web) |
| Grok | Cookies / auth.json | Billing |
| OpenCode Go | Cookies (opencode.ai) | Rolling, Weekly, Monthly |

## Install

Download the latest installer from
[GitHub Releases](https://github.com/rafachavantes/PulseBar/releases).

## First run

1. Launch **PulseBar** from the Start Menu.
2. Click the tray icon — the panel shows supported providers.
3. Go to **Settings → Providers**, enable what you use, and add credentials:
   - **Codex / Gemini**: sign in with the provider CLI first (`codex login`,
     `gcloud auth`).
   - **Claude**: browser cookies preferred (matches the settings-page usage).
   - **Grok**: `grok login` is picked up automatically, or log in at grok.com.
   - **z.ai**: paste your API token in Preferences or set `ZAI_API_TOKEN`.
   - **OpenCode Go**: log in at opencode.ai in your browser.

## Build from source

Prerequisites: Node.js + pnpm, Rust.

```powershell
git clone https://github.com/rafachavantes/PulseBar.git
cd PulseBar
.\dev.ps1            # or: cd apps/desktop-tauri && npm run tauri:dev
```

Release build:

```powershell
cd apps/desktop-tauri
npm run tauri:build
```

CLI:

```bash
cargo build -p pulsebar --release
./rust/target/release/pulsebar --help
./rust/target/release/pulsebar usage -p claude
```

## Privacy

- **On-device by default**: provider data is read from known local paths or
  provider APIs you configure.
- **Opt-in cookies**: browser-cookie extraction only runs for providers you
  enable.
- **Protected secrets**: API keys, manual cookies, and token accounts use the
  secure-file layer; Windows uses user-scoped DPAPI where available.
- **Safe diagnostics**: diagnostics expose provider/source/status metadata only,
  never raw cookies, API keys, or tokens.

## Attribution & license

Derives from [Win-CodexBar](https://github.com/Finesssee/Win-CodexBar) (v0.37.2)
by NessZerra and the original macOS
[CodexBar](https://github.com/steipete/CodexBar) by Peter Steinberger. Inspired
by [ccusage](https://github.com/ryoppippi/ccusage) for cost tracking. MIT
licensed — see [LICENSE](LICENSE).
