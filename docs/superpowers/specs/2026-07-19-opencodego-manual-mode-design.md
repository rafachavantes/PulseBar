# OpenCode Go: 100% manual mode (workspace ID + cookie)

Date: 2026-07-19
Status: Approved (design)
Scope: `rust/src/providers/opencodego/mod.rs`, `rust/src/cli/usage.rs`.

## Problem

OpenCode Go does not work end-to-end for the user. Two independent root causes surfaced during diagnosis:

1. **ABE blocks cookie auto-extraction** (already analyzed this session): Chromium App-Bound Encryption (Chrome 127+, inherited by Edge/Comet) holds the active profile's Cookies DB with an exclusive OS handle and encrypts values with an app-bound key. No third-party process can read or decrypt it while the browser runs. Already mitigated with the actionable ABE warning; the real fix is manual cookie paste.

2. **The CLI `usage` command never passes per-provider settings** (`rust/src/cli/usage.rs:191-202`): `build_usage_fetch_context` hardcodes `manual_cookie_header: None, api_key: None, workspace_id: None, api_region: None`. Only `diagnose.rs` populates these from settings/manual_cookies/api_keys. Result: any provider that depends on these fields (OpenCode Go, Gemini, API-key providers) silently fails via `pulsebar usage`, even when the user configured them. This is why the OpenCode Go CLI test kept hitting the browser/ABE path instead of using the pasted cookie.

3. **The `WORKSPACES_SERVER_ID` is NOT stale.** Diffing against the local `Win-CodexBar` clone (the fork parent, where the user reports it works) shows the constant, the `_server` call, and the parse logic are byte-identical. The earlier "stale hash" hypothesis is disproven. The server-fn path is fragile in principle (SolidStart hashes change on deploy) but is not the current failure.

## Decision

Make OpenCode Go fully manual: user pastes **workspace ID** + **cookie header**. Drop the browser-extraction loop and the server-fn workspace-id fetch entirely. The only network call becomes the stable user-facing page `GET https://opencode.ai/workspace/<id>/go`, which is what the user visits in the browser and what the existing `fetch_usage_page` + `parse_usage_text` already scrape.

## Non-goals

- Adding a Preferences UI field for the workspace ID (deferred; v1 = edit `settings.json`). The per-provider `workspace_id` setting already exists and round-trips.
- Adding a CLI `config set-workspace-id` subcommand (deferred; `config set-api-key` exists as the pattern when we do).
- Removing the ABE surfacing in the shared extractor — it still serves Grok and any future cookie-based provider.
- Touching the `opencode` (non-Go) provider.

## Changes

### 1. `rust/src/cli/usage.rs` — wire per-provider settings into FetchContext

`build_usage_fetch_context` must load the same sources `diagnose.rs:166-181` loads, for the target provider:
- `manual_cookie_header` ← `manual_cookies.get(provider_id.cli_name())`
- `api_key` ← `api_keys.get(provider_id.cli_name())`
- `workspace_id` ← `settings.provider_config(provider_id).and_then(|c| c.workspace_id.clone())`
- `api_region` ← `settings.provider_config(provider_id).and_then(|c| c.api_region.clone())`

This requires the function (or its caller) to access `Settings`, `ManualCookies`, and the api-key store. Mirror the load paths already used in `diagnose.rs` and the CLI's settings-loading helpers. This fix benefits every provider, not just OpenCode Go.

### 2. `rust/src/providers/opencodego/mod.rs` — rewrite fetch_usage

New flow:
```
fetch_usage(ctx):
    let workspace_id = ctx.workspace_id.as_deref()
        .ok_or(ProviderError::Other(<clear message: configure workspace_id in settings.json>))?;
    let cookie_header = ctx.manual_cookie_header.as_deref()
        .ok_or(ProviderError::AuthRequired)?;   // or Other with same actionable text
    let html = self.fetch_usage_page(workspace_id, cookie_header).await?;
    Self::parse_usage_text(&html).map(|snap| ProviderFetchResult::new(snap, "web"))
```

Remove:
- The `WORKSPACES_SERVER_ID` and `SERVER_URL` constants.
- `fetch_workspace_id` method and its `X-Server-Id` / `X-Server-Instance` request.
- The `#[cfg(windows)]` browser-extraction loop (and its ABE-surfacing block — no longer reachable for this provider).
- The `looks_signed_out` helper IF it is now unused (it is also used by `fetch_usage_page`; keep it there).

Keep:
- `fetch_usage_page` (the `GET /workspace/{id}/go` request) — unchanged.
- `parse_usage_text`, `extract_window`, `extract_number`, `parse_workspace_ids` (the last only if still referenced; likely becomes dead — remove if unused after the rewrite, the file-level `#![allow(dead_code)]` covers it if kept).
- The provider metadata (session_label "Rolling", weekly_label "Weekly", etc.).

### 3. UX (v1): set workspace_id via settings.json

The user adds `workspace_id` under the OpenCode Go entry in `%APPDATA%\PulseBar\settings.json`. The Settings schema already supports it (`types.rs:199`) with getter/setter. Document the exact JSON shape in the error message and in the release notes. UI/CLI command is a follow-up.

## Verification

- `cargo test --manifest-path rust/Cargo.toml` — including new unit tests (TDD):
  - `opencodego` fetch_usage returns the actionable "configure workspace_id" error when `ctx.workspace_id` is None.
  - `opencodego` fetch_usage returns AuthRequired when workspace_id is set but cookie is None.
  - The wiring fix: `build_usage_fetch_context` populates workspace_id from settings (assert via a small test that mirrors the existing settings round-trip test at `settings/tests.rs:508`).
- Real check on the user's machine:
  1. Set `workspace_id` in `settings.json` under the opencodego provider entry (paste the `wrk_...` from `https://opencode.ai/workspace/<id>/go`).
  2. Paste the cookie header under manual_cookies for opencodego (already done by the user).
  3. `cargo run -p pulsebar -- usage -p opencodego` → should print rolling/weekly/monthly usage.
- `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test` on both manifests.

## Risks

- The wiring fix changes behavior for all providers on the CLI: they now receive their configured `api_key`/`manual_cookie_header`/`workspace_id`/`api_region`. This is strictly a fix (they were being ignored); no regression expected, but worth a regression smoke (codex, grok, gemini via CLI) before releasing.
- `parse_workspace_ids` may become dead code; rely on the file-level `#![allow(dead_code)]` or remove it — confirm at implementation time.
