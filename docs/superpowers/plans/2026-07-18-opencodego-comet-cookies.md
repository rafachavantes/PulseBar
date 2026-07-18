# OpenCode Go: Comet Browser Support + Locked Cookie DB Fallback — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make OpenCode Go (and every cookie-based provider) work when the signed-in browser is Comet and/or the browser is running with a locked Cookies DB.

**Architecture:** Add `BrowserType::Comet` to the shared Windows browser detector (Chromium layout at `%LOCALAPPDATA%\Perplexity\Comet\User Data`), expose it through the Tauri browser-import bridge, and add an `immutable=1` read-only SQLite fallback in the Chromium cookie extractor for when the temp-copy of a locked DB fails.

**Tech Stack:** Rust, rusqlite 0.32 (bundled), Tauri IPC bridge.

**Spec:** `docs/superpowers/specs/2026-07-18-opencodego-comet-cookies-design.md`

---

### Task 1: Add `BrowserType::Comet` to detection + bridge key

**Files:**
- Modify: `rust/src/browser/detection.rs` (enum at :13-21, `all()` at :25-34, `display_name()` at :42-52, WSL match at :141-164, native match at :175-191, tests at :264-281)
- Modify: `apps/desktop-tauri/src-tauri/src/commands/browser_import.rs:112-122`

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `rust/src/browser/detection.rs`:

```rust
    #[test]
    fn test_comet_browser_registration() {
        assert!(BrowserType::all().contains(&BrowserType::Comet));
        assert!(BrowserType::Comet.is_chromium_based());
        assert_eq!(BrowserType::Comet.display_name(), "Comet");

        let dir = BrowserDetector::get_user_data_dir(BrowserType::Comet)
            .expect("Comet user data dir should resolve");
        let normalized = dir.to_string_lossy().replace('\\', "/");
        assert!(
            normalized.ends_with("Perplexity/Comet/User Data"),
            "unexpected Comet user data dir: {normalized}"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path rust/Cargo.toml browser::detection`
Expected: FAIL to compile — `BrowserType::Comet` does not exist.

- [ ] **Step 3: Implement Comet in detection.rs**

In the enum (after `Brave`):

```rust
pub enum BrowserType {
    Chrome,
    Edge,
    Brave,
    Comet,
    Arc,
    Firefox,
    Chromium,
}
```

In `all()`:

```rust
    pub fn all() -> &'static [BrowserType] {
        &[
            BrowserType::Chrome,
            BrowserType::Edge,
            BrowserType::Brave,
            BrowserType::Comet,
            BrowserType::Arc,
            BrowserType::Firefox,
            BrowserType::Chromium,
        ]
    }
```

In `display_name()`:

```rust
            BrowserType::Comet => "Comet",
```

In the WSL branch match of `get_user_data_dir` (after the `Brave` arm):

```rust
                BrowserType::Comet => Some(
                    appdata_local
                        .join("Perplexity")
                        .join("Comet")
                        .join("User Data"),
                ),
```

In the native branch match (after the `Brave` arm):

```rust
            BrowserType::Comet => local_app_data
                .join("Perplexity")
                .join("Comet")
                .join("User Data"),
```

- [ ] **Step 4: Add bridge key in browser_import.rs**

In `browser_type_key` (the match is exhaustive — it will not compile without this):

```rust
        BrowserType::Comet => "comet",
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path rust/Cargo.toml browser::detection`
Expected: PASS (including `test_comet_browser_registration`)
Also: `cargo check --manifest-path apps/desktop-tauri/src-tauri/Cargo.toml`
Expected: compiles (exhaustive match arm present).

- [ ] **Step 6: Commit**

```bash
git add rust/src/browser/detection.rs apps/desktop-tauri/src-tauri/src/commands/browser_import.rs
git commit -m "Add Comet browser detection for cookie import"
```

---

### Task 2: `chromium_immutable_uri` helper (TDD)

**Files:**
- Modify: `rust/src/browser/cookies.rs` (new helper near `copy_to_temp` at :556; tests in `mod tests` at :682)

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `rust/src/browser/cookies.rs`:

```rust
    /// Windows paths must become valid SQLite `file:` URIs with `immutable=1`.
    #[test]
    fn test_chromium_immutable_uri_escapes_windows_path() {
        let path = std::path::Path::new(r"C:\Users\ra fa\Cookies#x");
        let uri = CookieExtractor::chromium_immutable_uri(path);
        assert_eq!(uri, "file:///C:/Users/ra%20fa/Cookies%23x?immutable=1");
    }

    /// An immutable read-only open must work against a real SQLite file,
    /// including one another connection holds open (the locked-DB scenario).
    #[test]
    fn test_immutable_open_reads_db_held_by_another_connection() {
        let dir = std::env::temp_dir();
        let db_path = dir.join(format!("pulsebar_test_immutable_{}.db", uuid::Uuid::new_v4()));

        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch("CREATE TABLE t(v TEXT); INSERT INTO t VALUES ('hello');")
                .unwrap();
            // Deliberately keep `conn` open while the immutable reader opens the file.
            let uri = CookieExtractor::chromium_immutable_uri(&db_path);
            let reader = Connection::open_with_flags(
                &uri,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
            )
            .unwrap();
            let value: String = reader.query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap();
            assert_eq!(value, "hello");
        }

        let _ = std::fs::remove_file(&db_path);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path rust/Cargo.toml browser::cookies`
Expected: FAIL to compile — `chromium_immutable_uri` does not exist.

- [ ] **Step 3: Implement the helper**

Add inside `impl CookieExtractor` in `rust/src/browser/cookies.rs`, directly above `copy_to_temp`:

```rust
    /// Build a SQLite `file:` URI that opens the DB read-only in immutable
    /// mode (no locks, no WAL), used as a fallback when the temp copy of a
    /// locked Chromium Cookies DB fails.
    fn chromium_immutable_uri(path: &Path) -> String {
        let escaped = path
            .to_string_lossy()
            .replace('\\', "/")
            .replace('%', "%25")
            .replace(' ', "%20")
            .replace('#', "%23")
            .replace('?', "%3F");
        format!("file:///{escaped}?immutable=1")
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path rust/Cargo.toml browser::cookies`
Expected: PASS (both new tests plus existing ones).

- [ ] **Step 5: Commit**

```bash
git add rust/src/browser/cookies.rs
git commit -m "Add immutable SQLite URI helper for locked cookie DBs"
```

---

### Task 3: Wire the immutable fallback into `extract_chromium_cookies`

**Files:**
- Modify: `rust/src/browser/cookies.rs:160-280` (`extract_chromium_cookies`)

Context: today the function does `copy_to_temp` → `Connection::open(&temp_db)` → inline query+decrypt loop → delete temp. The query/decrypt loop is inlined at :199-258. We extract it into a shared associated fn so both the temp-copy path and the fallback path run the identical query.

- [ ] **Step 1: Extract the query/decrypt loop into `query_chromium_cookies`**

Add inside `impl CookieExtractor`:

```rust
    /// Run the cookie query + decryption against an open Chromium Cookies DB.
    /// Returns (cookies, decrypt_failures, abe_decrypt_failures).
    fn query_chromium_cookies(
        conn: &Connection,
        domain: &str,
        encryption_key: &[u8],
    ) -> Result<(Vec<Cookie>, u32, u32), CookieError> {
        let domain_pattern = format!("%{}", domain);
        let dot_domain_pattern = format!(".{}", domain);

        let mut cookies = Vec::new();
        let mut decrypt_failures: u32 = 0;
        let mut abe_decrypt_failures: u32 = 0;

        let mut stmt = conn.prepare(
            "SELECT name, encrypted_value, host_key, path, expires_utc, is_secure, is_httponly
             FROM cookies
             WHERE host_key LIKE ?1 OR host_key LIKE ?2",
        )?;

        let rows = stmt.query_map([&domain_pattern, &dot_domain_pattern], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i32>(5)? != 0,
                row.get::<_, i32>(6)? != 0,
            ))
        })?;

        for row in rows {
            let (name, encrypted_value, host_key, path, expires_utc, is_secure, is_http_only) =
                row?;

            let value = match Self::decrypt_chromium_cookie(&encrypted_value, encryption_key) {
                Ok(v) => v,
                Err(CookieError::AppBoundEncryption) => {
                    tracing::debug!("Candidate cookie uses Chromium App-Bound Encryption");
                    decrypt_failures += 1;
                    abe_decrypt_failures += 1;
                    continue;
                }
                Err(e) => {
                    tracing::debug!("Failed to decrypt a candidate cookie: {}", e);
                    decrypt_failures += 1;
                    continue;
                }
            };

            cookies.push(Cookie {
                name,
                value,
                domain: host_key,
                path,
                expires: if expires_utc > 0 {
                    Some(expires_utc)
                } else {
                    None
                },
                is_secure,
                is_http_only,
            });
        }

        Ok((cookies, decrypt_failures, abe_decrypt_failures))
    }
```

- [ ] **Step 2: Rewire `extract_chromium_cookies` to use it, with the fallback**

Replace the body of `extract_chromium_cookies` (from the `// Copy the database to a temp file` comment through the temp-file cleanup) with:

```rust
        // Copy the database to a temp file (browser may have it locked)
        tracing::debug!("Copying cookies DB to temp...");
        let temp_db = match Self::copy_to_temp(&cookies_db) {
            Ok(path) => Some(path),
            Err(e) => {
                tracing::debug!("Failed to copy cookies DB: {}", e);
                None
            }
        };

        tracing::debug!("Searching for cookies for domain {}", domain);

        // ponytail: immutable mode skips the WAL, so just-written cookies can be
        // missed on one poll; the next refresh picks them up. Upgrade path if
        // that ever bites: read-only + WAL open with retry.
        let (cookies, decrypt_failures, abe_decrypt_failures) = match &temp_db {
            Some(path) => {
                let conn = Connection::open(path)?;
                Self::query_chromium_cookies(&conn, domain, &encryption_key)?
            }
            None => {
                tracing::debug!(
                    "Falling back to immutable read-only open of the original cookies DB"
                );
                let uri = Self::chromium_immutable_uri(&cookies_db);
                let conn = Connection::open_with_flags(
                    &uri,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                        | rusqlite::OpenFlags::SQLITE_OPEN_URI,
                )?;
                Self::query_chromium_cookies(&conn, domain, &encryption_key)?
            }
        };

        tracing::debug!(
            "Found {} cookies for {} ({} failed to decrypt)",
            cookies.len(),
            domain,
            decrypt_failures
        );

        // Clean up temp file
        if let Some(path) = &temp_db {
            let _ = std::fs::remove_file(path);
        }
```

Keep everything after that point (the ABE warning block that uses `abe_decrypt_failures` / `decrypt_failures`, and the `Ok(cookies)` return) unchanged — it consumes the same variable names.

Do **not** touch `extract_firefox_cookies` (it also calls `copy_to_temp`; the fallback is Chromium-only per the spec).

- [ ] **Step 3: Run tests**

Run: `cargo test --manifest-path rust/Cargo.toml browser::cookies`
Expected: PASS (existing + new tests; behavior on unlocked DBs unchanged).

- [ ] **Step 4: Commit**

```bash
git add rust/src/browser/cookies.rs
git commit -m "Fall back to immutable SQLite open when cookie DB copy fails"
```

---

### Task 4: End-to-end verification + lint

**Files:** none (verification only)

- [ ] **Step 1: Primary verification — OpenCode Go with Comet RUNNING**

Keep Comet open (this proves both the detection and the lock fallback). Run:

```bash
cargo run -p pulsebar -- usage -p opencodego
```

Expected: usage percentages print (no `Authentication required`). If it still fails:
- `RUST_LOG=debug` output showing `0 failed to decrypt` + cookies found but server rejecting → out of scope (likely stale `WORKSPACES_SERVER_ID`); stop and report.
- ABE decrypt failures → out of scope (existing ABE UX path); stop and report.

- [ ] **Step 2: Regression — Grok + Codex**

```bash
cargo run -p pulsebar -- usage -p grok
cargo run -p pulsebar -- usage -p codex
```

Expected: both behave as before this plan (Grok via cookies or `~/.grok/auth.json`; Codex via OAuth).

- [ ] **Step 3: Desktop bridge sanity**

```bash
cargo test --manifest-path apps/desktop-tauri/src-tauri/Cargo.toml
```

Expected: PASS. (The Comet entry in the Preferences dropdown is data-driven from `list_detected_browsers`; manual UI check optional.)

- [ ] **Step 4: fmt + clippy + full tests on both manifests**

```bash
cargo fmt --all
cargo clippy --all-targets --manifest-path rust/Cargo.toml -- -D warnings
cargo clippy --all-targets --manifest-path apps/desktop-tauri/src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path rust/Cargo.toml
cargo test --manifest-path apps/desktop-tauri/src-tauri/Cargo.toml
```

Expected: no fmt diff, zero clippy warnings, all tests pass.

- [ ] **Step 5: Commit (only if fmt/clippy produced changes)**

```bash
git add -A
git commit -m "Format and lint Comet cookie support"
```
