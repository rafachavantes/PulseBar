//! Claude OAuth token refresh.
//!
//! The Claude CLI rotates its own tokens, but PulseBar reads the credentials
//! file directly and would otherwise stall on an expired access token until
//! the user opened Claude Code again. The OAuth client id is not stored in the
//! credentials file, so it is extracted from the installed CLI bundle rather
//! than hardcoded (the Gemini provider does the same with its CLI).

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::core::ProviderError;

const TOKEN_ENDPOINT: &str = "https://platform.claude.com/v1/oauth/token";
/// The bundle is a single-file executable of a few hundred MB; scan it in
/// windows instead of loading it into memory.
const SCAN_CHUNK: usize = 1 << 20;
const CLIENT_ID_LEN: usize = 36;
/// Bytes of context kept before a match to tell the production config apart
/// from the local-environment one.
const LOOKBEHIND: usize = 128;
const NEEDLE: &[u8] = b"CLIENT_ID:\"";

/// Tokens returned by the Claude token endpoint after a refresh.
#[derive(Debug, Clone)]
pub struct RefreshedTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Scan a CLI bundle for the OAuth client id.
pub fn scan_client_id(reader: impl Read) -> Option<String> {
    scan_client_id_chunked(reader, SCAN_CHUNK)
}

fn scan_client_id_chunked(mut reader: impl Read, chunk_size: usize) -> Option<String> {
    // Retain enough of the tail for a match that straddles two reads: the byte
    // before the needle, the needle, the uuid and its closing quote.
    let overlap = LOOKBEHIND + NEEDLE.len() + CLIENT_ID_LEN + 2;
    let mut window: Vec<u8> = Vec::new();
    let mut buffer = vec![0u8; chunk_size.max(1)];

    loop {
        let read = reader.read(&mut buffer).ok()?;
        if read == 0 {
            return None;
        }
        window.extend_from_slice(&buffer[..read]);
        if let Some(found) = find_client_id(&window) {
            return Some(found);
        }
        if window.len() > overlap {
            window.drain(..window.len() - overlap);
        }
    }
}

fn find_client_id(haystack: &[u8]) -> Option<String> {
    let mut start = 0;
    while let Some(offset) = find_subslice(&haystack[start..], NEEDLE) {
        let at = start + offset;
        // `DESIGN_CLIENT_ID` also ends with the needle; reject it.
        let part_of_longer_key = at > 0 && is_key_char(haystack[at - 1]);
        let value_start = at + NEEDLE.len();
        let value_end = value_start + CLIENT_ID_LEN;
        if !part_of_longer_key
            && is_production_config(&haystack[at.saturating_sub(LOOKBEHIND)..at])
            && value_end < haystack.len()
            && haystack[value_end] == b'"'
            && let Ok(candidate) = std::str::from_utf8(&haystack[value_start..value_end])
            && is_uuid(candidate)
        {
            return Some(candidate.to_string());
        }
        start = at + NEEDLE.len();
    }
    None
}

/// The bundle carries a local-environment config whose URLs are unresolved
/// template literals (`${r}/oauth/code/callback`) alongside the production one
/// with absolute URLs. Only the latter holds the client id we can refresh with,
/// so require both an absolute URL and the production host nearby.
fn is_production_config(preceding: &[u8]) -> bool {
    find_subslice(preceding, b"https://").is_some()
        && find_subslice(preceding, b"claude.com").is_some()
}

fn is_key_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_uuid(value: &str) -> bool {
    value.len() == CLIENT_ID_LEN
        && value.chars().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Known install locations for the Claude Code CLI, most specific first.
fn claude_cli_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(path) = which::which("claude") {
        candidates.push(path);
    }
    if let Some(home) = dirs::home_dir() {
        candidates.push(
            home.join("AppData/Local/Microsoft/WinGet/Packages")
                .join("Anthropic.ClaudeCode_Microsoft.Winget.Source_8wekyb3d8bbwe")
                .join("claude.exe"),
        );
        candidates.push(home.join(".local/bin/claude.exe"));
        candidates.push(home.join(".claude/local/claude.exe"));
        candidates.push(home.join(".claude/local/node_modules/@anthropic-ai/claude-code/cli.js"));
        candidates
            .push(home.join("AppData/Roaming/npm/node_modules/@anthropic-ai/claude-code/cli.js"));
    }
    candidates
}

/// Scanning the bundle costs several seconds, and a revoked refresh token
/// would otherwise re-scan on every refresh cycle. The client id is fixed per
/// install, so cache the scan result for the life of the process; restarting
/// PulseBar picks up a reinstalled CLI.
static CLIENT_ID_CACHE: OnceLock<Option<String>> = OnceLock::new();

/// Locate the CLI bundle and extract the OAuth client id from it.
pub fn extract_client_id() -> Result<String, ProviderError> {
    CLIENT_ID_CACHE
        .get_or_init(scan_installed_cli)
        .clone()
        .ok_or_else(|| {
            ProviderError::OAuth(
                "Could not find the Claude Code CLI to renew the OAuth token. Run `claude` to refresh it."
                    .to_string(),
            )
        })
}

fn scan_installed_cli() -> Option<String> {
    for path in claude_cli_candidates() {
        if !path.is_file() {
            continue;
        }
        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };
        if let Some(client_id) = scan_client_id(std::io::BufReader::new(file)) {
            tracing::debug!("Extracted Claude OAuth client id from {}", path.display());
            return Some(client_id);
        }
    }
    None
}

/// Exchange a refresh token for a fresh access token.
pub async fn request_token_refresh(
    client: &Client,
    endpoint: &str,
    client_id: &str,
    refresh_token: &str,
) -> Result<RefreshedTokens, ProviderError> {
    let response = client
        .post(endpoint)
        .json(&serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": client_id,
        }))
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?;

    if !response.status().is_success() {
        tracing::debug!(status = %response.status(), "Claude token refresh rejected");
        return Err(ProviderError::OAuth(
            "Claude OAuth refresh was rejected. Run `claude` to sign in again.".to_string(),
        ));
    }

    let body: Value = response.json().await.map_err(|e| {
        ProviderError::Parse(format!("Failed to decode Claude token response: {e}"))
    })?;
    let access_token = body
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ProviderError::OAuth("Claude token response had no access token".to_string())
        })?
        .to_string();
    let expires_at = body
        .get("expires_in")
        .and_then(Value::as_i64)
        .and_then(|secs| Utc::now().checked_add_signed(chrono::Duration::seconds(secs)));

    Ok(RefreshedTokens {
        access_token,
        refresh_token: body
            .get("refresh_token")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned),
        expires_at,
    })
}

/// Write refreshed tokens back to the credentials file so the Claude CLI keeps
/// working. Unknown fields are preserved and the file is replaced atomically.
pub fn persist_refreshed_tokens(
    path: &Path,
    tokens: &RefreshedTokens,
) -> Result<(), ProviderError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| ProviderError::Other(format!("Failed to read Claude credentials: {e}")))?;
    let mut root: Value = serde_json::from_str(&text)
        .map_err(|e| ProviderError::Parse(format!("Failed to decode Claude credentials: {e}")))?;
    let oauth = root
        .get_mut("claudeAiOauth")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            ProviderError::OAuth("Claude credentials have no claudeAiOauth entry".to_string())
        })?;

    oauth.insert(
        "accessToken".to_string(),
        Value::String(tokens.access_token.clone()),
    );
    if let Some(ref refresh_token) = tokens.refresh_token {
        oauth.insert(
            "refreshToken".to_string(),
            Value::String(refresh_token.clone()),
        );
    }
    if let Some(expires_at) = tokens.expires_at {
        // The CLI stores this as epoch milliseconds.
        oauth.insert(
            "expiresAt".to_string(),
            Value::Number(expires_at.timestamp_millis().into()),
        );
    }

    let serialized = serde_json::to_string_pretty(&root)
        .map_err(|e| ProviderError::Parse(format!("Failed to encode Claude credentials: {e}")))?;
    let temp_path = path.with_extension("json.pulsebar-tmp");
    std::fs::write(&temp_path, serialized)
        .map_err(|e| ProviderError::Other(format!("Failed to stage Claude credentials: {e}")))?;
    std::fs::rename(&temp_path, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        ProviderError::Other(format!("Failed to update Claude credentials: {e}"))
    })?;
    Ok(())
}

/// Default token endpoint, separate so callers and tests can override it.
pub fn token_endpoint() -> String {
    TOKEN_ENDPOINT.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_BUNDLE_SNIPPET: &str = concat!(
        r#"REDIRECT_URL:"https://platform.claude.com/oauth/code/callback","#,
        r#"CLIENT_ID:"9d1c250a-e61b-44d9-88ed-5944d1962f5e","#,
        r#"DESIGN_CLIENT_ID:"59637612-477b-4836-a601-b0589e000000""#,
    );

    /// Smoke test against a real install; ignored because it needs the CLI.
    /// Run with: cargo test -- --ignored extracts_client_id_from_installed_cli
    #[test]
    #[ignore]
    fn extracts_client_id_from_installed_cli() {
        let started = std::time::Instant::now();
        let client_id = extract_client_id().expect("client id");
        println!("client_id={client_id} in {:?}", started.elapsed());
        assert!(is_uuid(&client_id));
    }

    #[test]
    fn scans_client_id_from_bundle_bytes() {
        let found = scan_client_id(REAL_BUNDLE_SNIPPET.as_bytes());
        assert_eq!(
            found.as_deref(),
            Some("9d1c250a-e61b-44d9-88ed-5944d1962f5e")
        );
    }

    /// The bundle ships a local-environment config before the production one;
    /// its redirect is an unresolved template literal instead of a real URL.
    const LOCAL_ENV_SNIPPET: &str = concat!(
        r#"CONSOLE_SUCCESS_URL:`${r}/oauth/code/success?app=claude-code`,"#,
        r#"MANUAL_REDIRECT_URL:`${r}/oauth/code/callback`,"#,
        r#"CLIENT_ID:"22422756-60c9-4084-8eb7-27705fd5cf9a","#,
        r#"DESIGN_CLIENT_ID:"00000000-0000-4000-8000-000000000000","#,
        r#"OAUTH_FILE_SUFFIX:"-local-oauth","#,
    );

    #[test]
    fn skips_the_local_environment_client_and_finds_the_production_one() {
        let payload = format!("{LOCAL_ENV_SNIPPET}{REAL_BUNDLE_SNIPPET}");
        assert_eq!(
            scan_client_id(payload.as_bytes()).as_deref(),
            Some("9d1c250a-e61b-44d9-88ed-5944d1962f5e")
        );
    }

    #[test]
    fn ignores_a_client_id_whose_config_has_no_absolute_url() {
        assert_eq!(scan_client_id(LOCAL_ENV_SNIPPET.as_bytes()), None);
    }

    #[test]
    fn does_not_mistake_design_client_id_for_the_oauth_client() {
        // Same production context, so only the longer key can reject it.
        let payload = concat!(
            r#"MANUAL_REDIRECT_URL:"https://platform.claude.com/oauth/code/callback","#,
            r#"DESIGN_CLIENT_ID:"59637612-477b-4836-a601-b0589e000000""#,
        );
        assert_eq!(scan_client_id(payload.as_bytes()), None);
    }

    #[test]
    fn scans_client_id_split_across_read_boundaries() {
        let payload = format!("{}{}", "x".repeat(40), REAL_BUNDLE_SNIPPET);
        // Chunks of 8 bytes guarantee the needle and uuid straddle several reads.
        let found = scan_client_id_chunked(payload.as_bytes(), 8);
        assert_eq!(
            found.as_deref(),
            Some("9d1c250a-e61b-44d9-88ed-5944d1962f5e")
        );
    }

    #[test]
    fn returns_none_when_bundle_has_no_client_id() {
        assert_eq!(scan_client_id(b"nothing to see here".as_slice()), None);
    }

    #[test]
    fn rejects_values_that_are_not_uuids() {
        let payload = r#",CLIENT_ID:"not-a-uuid-at-all-not-a-uuid-at-all""#;
        assert_eq!(scan_client_id(payload.as_bytes()), None);
    }

    #[tokio::test]
    async fn token_refresh_returns_rotated_tokens() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/oauth/token")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": "old-refresh",
                "client_id": "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"access_token":"new-access","refresh_token":"new-refresh","expires_in":28800}"#,
            )
            .create_async()
            .await;

        let tokens = request_token_refresh(
            &reqwest::Client::new(),
            &format!("{}/v1/oauth/token", server.url()),
            "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
            "old-refresh",
        )
        .await
        .unwrap();

        mock.assert_async().await;
        assert_eq!(tokens.access_token, "new-access");
        assert_eq!(tokens.refresh_token.as_deref(), Some("new-refresh"));
        let delta = (tokens.expires_at.expect("expires_at") - Utc::now()).num_seconds();
        assert!((28_200..=28_800).contains(&delta), "delta was {delta}");
    }

    #[tokio::test]
    async fn token_refresh_rejection_is_actionable() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/oauth/token")
            .with_status(401)
            .create_async()
            .await;

        let error = request_token_refresh(
            &reqwest::Client::new(),
            &format!("{}/v1/oauth/token", server.url()),
            "client",
            "bad",
        )
        .await
        .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("claude"), "message was {message}");
    }

    #[test]
    fn persisting_tokens_rotates_and_preserves_other_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".credentials.json");
        std::fs::write(
            &path,
            r#"{"claudeAiOauth":{
                "accessToken":"stale",
                "refreshToken":"old-refresh",
                "expiresAt":1786848946365,
                "refreshTokenExpiresAt":1788707782365,
                "scopes":["user:profile"],
                "subscriptionType":"max"
            }}"#,
        )
        .unwrap();

        let tokens = RefreshedTokens {
            access_token: "new-access".to_string(),
            refresh_token: Some("new-refresh".to_string()),
            expires_at: DateTime::from_timestamp(2_000_000_000, 0),
        };
        persist_refreshed_tokens(&path, &tokens).unwrap();

        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let oauth = &saved["claudeAiOauth"];
        assert_eq!(oauth["accessToken"], "new-access");
        assert_eq!(oauth["refreshToken"], "new-refresh");
        // expiresAt stays in milliseconds, the format the Claude CLI writes.
        assert_eq!(oauth["expiresAt"], 2_000_000_000_000i64);
        assert_eq!(oauth["refreshTokenExpiresAt"], 1788707782365i64);
        assert_eq!(oauth["subscriptionType"], "max");
        assert_eq!(oauth["scopes"][0], "user:profile");
    }

    #[test]
    fn persisting_without_rotation_keeps_previous_refresh_token() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".credentials.json");
        std::fs::write(
            &path,
            r#"{"claudeAiOauth":{"accessToken":"stale","refreshToken":"old-refresh"}}"#,
        )
        .unwrap();

        let tokens = RefreshedTokens {
            access_token: "new-access".to_string(),
            refresh_token: None,
            expires_at: None,
        };
        persist_refreshed_tokens(&path, &tokens).unwrap();

        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["claudeAiOauth"]["accessToken"], "new-access");
        assert_eq!(saved["claudeAiOauth"]["refreshToken"], "old-refresh");
    }
}
