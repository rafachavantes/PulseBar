# CodexBar (personal fork)

A Windows system-tray app for keeping AI coding-tool usage visible without
opening a dozen dashboards. Tauri + React desktop shell backed by shared Rust
provider logic.

This is a **personal hard fork** of [Win-CodexBar](https://github.com/Finesssee/Win-CodexBar)
v0.37.2 (itself a Windows port of [CodexBar](https://github.com/steipete/CodexBar)
by Peter Steinberger). It is pruned to the providers I actually use and built for
personal use; there is no auto-update and no public release pipeline (yet).

## Supported providers

This fork supports **6 providers**:

| Provider | Auth | Tracks |
|---|---|---|
| Codex | OAuth / CLI | Session, Weekly, Credits |
| Claude | Cookies / OAuth fallback / CLI fallback | Session (5h), Weekly |
| Gemini | gcloud OAuth | Quota |
| z.ai | API Token | Quota |
| Grok | Cookies / auth.json | Billing |
| Synthetic | (test/proof harness only — hidden from the UI) | — |

## Build from source

Prerequisites: Node.js + pnpm, Rust (run `.\dev.ps1` / `./dev.sh` to install
Rust and MinGW when needed on Windows).

```powershell
git clone <this-fork> CodexBar
cd CodexBar
.\dev.ps1            # or: cd apps/desktop-tauri && npm run tauri:dev
```

Release/installer build:

```powershell
cd apps/desktop-tauri
npm run tauri:build          # installer + portable assets under src-tauri/target/release/bundle/
```

CLI (console binary `codexbar`):

```bash
cargo build -p codexbar --release
./rust/target/release/codexbar --help
./rust/target/release/codexbar usage -p claude
./rust/target/release/codexbar diagnose --pretty
```

> The installer ships `codexbar.exe` (console CLI) and `codexbar-desktop.exe`
> (tray app). Start Menu shortcuts launch the desktop app; terminal commands use
> `codexbar.exe`.

## First run

1. Launch **CodexBar** from the Start Menu or portable executable.
2. Click the tray icon to open the usage panel.
3. Open **Settings → Providers**.
4. Enable the providers you use and add the matching credential (OAuth/device
   login, API key, browser cookies, or local CLI login).

For Claude, browser cookies/sessionKey are preferred (they match Claude's
settings-page usage). For Codex and Gemini, sign in with the provider CLI first.
For z.ai, paste your API token in Preferences or set `ZAI_API_TOKEN`.

## Updates

**Disabled in this personal build.** The app does not check GitHub for new
versions and will not auto-update. When/if this fork goes public, re-enable the
updater by restoring `check_for_updates_network` in `rust/src/updater.rs` and
pointing `GITHUB_REPO` at the fork's own release feed.

## Privacy

- **On-device by default**: provider data is read from known local paths or
  provider APIs you configure.
- **Opt-in cookies**: browser-cookie extraction only runs for providers you
  enable.
- **Protected secrets**: API keys, manual cookies, and token accounts use the
  secure-file layer; Windows uses user-scoped DPAPI where available.
- **Safe diagnostics**: diagnostics expose provider/source/status metadata only,
  never raw cookies, API keys, bearer tokens, or OAuth values.

## Project layout

- `apps/desktop-tauri/` — Tauri desktop shell (React UI in `src/`, Rust bridge in
  `src-tauri/src/`).
- `rust/src/` — shared backend crate + CLI (`codexbar` binary): providers,
  settings, login, status, tray renderer, browser cookie extraction.
- `docs/superpowers/specs/` — design docs (e.g. the fork design).
- `docs/superpowers/plans/` — implementation plans.

See `AGENTS.md` for build/test commands, coding style, and conventions.

## Attribution & license

Derives from [Win-CodexBar](https://github.com/Finesssee/Win-CodexBar) (v0.37.2)
and the original macOS [CodexBar](https://github.com/steipete/CodexBar) by Peter
Steberger; inspired by [ccusage](https://github.com/ryoppippi/ccusage) for cost
tracking. MIT licensed — see [LICENSE](LICENSE).
