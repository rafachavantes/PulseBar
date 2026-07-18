# OpenCode Go: Comet browser support + locked cookie DB fallback

Date: 2026-07-18
Status: Approved (design)
Scope: `rust/src/browser/` + Tauri browser-import bridge. Fixes OpenCode Go cookie auth on machines where the signed-in browser is Comet and/or the browser is running (locked Cookies DB).

## Problem

`pulsebar usage -p opencodego` fails with `Authentication required` on the user's machine. Debug logs show two independent causes in the shared Chromium cookie extractor:

1. **Comet is not detected.** The user is signed in to opencode.ai in the Comet browser (Perplexity, Chromium-based). `BrowserDetector::detect_all()` only scans Chrome, Edge, Brave, Arc, Firefox, Chromium — Comet's user data dir (`%LOCALAPPDATA%\Perplexity\Comet\User Data`) is never scanned.
2. **Locked DB is fatal to a profile.** Edge's `Default` profile was skipped entirely because `copy_to_temp` failed with `os error 32` (sharing violation) while the browser was running — even though the copy already opens the source with `FILE_SHARE_READ|WRITE|DELETE`. A browser that is running can therefore make its profile unreadable.

Both causes affect every cookie-based provider (OpenCode Go, Grok web mode); the fix lands in the shared extractor.

## Non-goals

- Reworking the OpenCode Go scrape itself (workspace server-fn id, `/go` page parsing). If a *valid* cookie still fails, that is a separate item.
- WSL detection path for Comet (Arc/Chromium have no WSL entry either; existing precedent).
- Frontend changes — the browser dropdown is populated dynamically from `list_detected_browsers`.

## Changes

### 1. `rust/src/browser/detection.rs` — add Comet

- New `BrowserType::Comet` variant: `display_name() == "Comet"`, chromium-based (default via `is_chromium_based`).
- Insert `Comet` into `BrowserType::all()` after `Brave`.
- Native user data dir: `dirs::data_local_dir()/Perplexity/Comet/User Data`.
- No WSL arm (see non-goals).

### 2. `apps/desktop-tauri/src-tauri/src/commands/browser_import.rs` — bridge key

- `browser_type_key`: add `BrowserType::Comet => "comet"`. This is the only bridge change needed — the match is exhaustive (compiler enforces it), and `import_browser_cookies` resolves browsers by comparing keys from the same function, so no reverse mapping exists.

### 3. `rust/src/browser/cookies.rs` — immutable fallback for locked DBs

Current flow in `extract_chromium_cookies`: `copy_to_temp` → `Connection::open(temp)` → query.

New flow: if `copy_to_temp` fails, fall back to opening the **original** DB read-only via URI:

```
file:<path>?immutable=1   with SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_URI
```

(path converted to a proper `file:` URI — forward slashes, percent-encoded spaces/drive colon handled per rusqlite URI rules.)

Then run the same cookie query on that connection. No temp file, no locking.

Mark with a `ponytail:` comment: immutable mode skips the WAL, so cookies written but not yet checkpointed can be missed on one poll; the next refresh picks them up. Upgrade path if it ever matters: read-only + WAL open with retry.

### 4. Error flow

Unchanged: zero usable cookies across all detected browsers → `ProviderError::AuthRequired`. Existing debug logs already state per-browser/per-profile failure reasons; keep them.

## Verification

- Primary (real): `cargo run -p pulsebar -- usage -p opencodego` **with Comet running** — proves detection + lock fallback in one shot.
- Regression: `cargo run -p pulsebar -- usage -p grok` and `usage -p codex` (shared extractor / unrelated provider sanity).
- Desktop: browser dropdown in Preferences lists "Comet"; manual import via `import_browser_cookies` with `comet` works.
- `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test` on both manifests (`rust/`, `apps/desktop-tauri/src-tauri/`).

## Risks

- Comet may enable Chromium App-Bound Encryption; if so, decryption fails the same way it does for Chrome 127+ and the existing ABE warning path already covers the UX. Out of scope to bypass ABE.
- URI open of a DB that Chromium is actively checkpointing can theoretically read a torn page; `immutable=1` accepts that risk by contract, and a failed/garbled read surfaces as "0 cookies" → next poll retries.
