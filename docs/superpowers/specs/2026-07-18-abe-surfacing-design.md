# Surface App-Bound Encryption (ABE) warning from cookie-based provider loops

Date: 2026-07-18
Status: Approved (design)
Scope: `rust/src/providers/opencodego/mod.rs`, `rust/src/providers/grok/mod.rs`.

## Problem

When Chromium App-Bound Encryption (ABE) blocks cookie extraction from a running browser, the OpenCode Go and Grok provider loops return a bare `ProviderError::AuthRequired` ("Authentication required"). The codebase already detects ABE and has an actionable message ("Chrome/Edge App-Bound Encryption is blocking automatic browser import… paste manually"), but the provider loops swallow it.

Root cause: `CookieExtractor::extract_for_domain` (cookies.rs:80-121) already returns `Err(CookieError::AppBoundEncryption)` when all profiles of a browser fail due to ABE. The provider loops call it with `if let Ok(cookies) = …`, discarding that error.

Verification confirmed ABE is the real blocker on the user's machine: both Edge and Comet have `app_bound_encrypted_key` in `Local State`, and the OS denies all read access to the active profile's Cookies DB (os error 32) — no code fix can extract cookies from a running ABE browser. Surfacing the reason is the correct, honest UX.

## Non-goals

- Bypassing ABE (would require the IElevator COM broker or VSS; out of scope, security-sensitive).
- Changing provider loop semantics (they try each browser's cookies against the server until one authenticates — must be preserved).
- Touching `get_cookies_for_domain` or the shared extractor.
- UI/CLI changes — `ProviderError::Other(String)` is already Displayed by the CLI and Tauri error paths.

## Changes

For each provider (`opencodego/mod.rs`, `grok/mod.rs`), in the `#[cfg(windows)]` browser loop inside `fetch_usage`:

1. Before the loop, declare `let mut abe_seen = false;`.
2. Replace the `if let Ok(cookies) = CookieExtractor::extract_for_domain(&browser, <domain>) { … }` block with a `match`:
   - `Ok(cookies)` if non-empty → existing behavior (build header, attempt fetch, return on success).
   - `Err(CookieError::AppBoundEncryption)` → set `abe_seen = true;` (continue trying other browsers).
   - `Err(e)` → existing `tracing::debug!` log, continue.
3. After the loop, before the existing fall-through (`AuthRequired` for OpenCode Go; `load_credentials()` for Grok): if `abe_seen`, return `Err(ProviderError::Other(CookieError::AppBoundEncryption.to_string()))`.
4. Add `CookieError` to the `use crate::browser::cookies::{…}` import in each file.

The `CookieError::AppBoundEncryption` Display impl (cookies.rs:44-52) already produces the actionable user-facing message; `ProviderError::Other(String)` surfaces it verbatim.

## Verification

- Unit test (per provider, or one shared if cleaner): construct a `FetchContext` and assert that when extraction returns only ABE errors, the provider returns `ProviderError::Other(s)` where `s` contains "App-Bound Encryption". (Mock/stub the extractor if the existing test pattern allows; otherwise an integration-style test that asserts the error variant mapping on a machine without the target browser is acceptable.)
- Real check on the user's machine: `cargo run -p pulsebar -- usage -p opencodego` with Comet running → now prints the ABE message instead of "Authentication required".
- Regression: `cargo run -p pulsebar -- usage -p grok` (still falls back to `~/.grok/auth.json`).
- `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test` on both manifests.

## Risks

- None material. The change only adds an error-mapping path; happy-path (cookies found) and existing fallbacks are untouched.
