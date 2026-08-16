//! Grok provider implementation.
//!
//! Uses the grok.com billing gRPC-web endpoint via either browser cookies or
//! `~/.grok/auth.json` produced by `grok login`.

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use reqwest::Client;
use serde_json::Value;
use std::path::PathBuf;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::core::{
    FetchContext, Provider, ProviderError, ProviderFetchResult, ProviderId, ProviderMetadata,
    RateWindow, SourceMode, UsageSnapshot,
};

const BILLING_ENDPOINT: &str = "https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig";
/// OIDC issuer used by `grok login` when auth.json omits it.
const DEFAULT_ISSUER: &str = "https://auth.x.ai";
/// Refresh this many minutes before the token actually expires.
const EXPIRY_SKEW_MINUTES: i64 = 5;

pub struct GrokProvider {
    metadata: ProviderMetadata,
    client: Client,
}

impl GrokProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                id: ProviderId::Grok,
                display_name: "Grok",
                session_label: "Monthly",
                weekly_label: "On-demand",
                supports_opus: false,
                supports_credits: false,
                default_enabled: false,
                is_primary: false,
                dashboard_url: Some("https://grok.com/?_s=usage"),
                status_page_url: Some("https://status.x.ai"),
            },
            client: crate::core::credentialed_http_client_builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    fn auth_file_path() -> Option<PathBuf> {
        if let Ok(home) = std::env::var("GROK_HOME")
            && !home.trim().is_empty()
        {
            return Some(PathBuf::from(home).join("auth.json"));
        }
        dirs::home_dir().map(|home| home.join(".grok").join("auth.json"))
    }

    fn load_credentials() -> Result<GrokCredentials, ProviderError> {
        let path = Self::auth_file_path()
            .ok_or_else(|| ProviderError::NotInstalled("Grok auth path not found".to_string()))?;
        let text = std::fs::read_to_string(&path).map_err(|_| {
            ProviderError::NotInstalled("Grok auth.json not found. Run `grok login`.".to_string())
        })?;
        GrokCredentials::parse(&text)
    }

    async fn fetch_with_auth(
        &self,
        credentials: &GrokCredentials,
    ) -> Result<ProviderFetchResult, ProviderError> {
        let mut credentials = credentials.clone();
        let mut refreshed = false;

        // Tokens from `grok login` last ~6h; renew before spending a request.
        if credentials.is_expired() && credentials.can_refresh() {
            credentials = self.refresh_credentials(&credentials).await?;
            refreshed = true;
        }

        loop {
            let attempt = self
                .fetch_billing(Some(format!("Bearer {}", credentials.access_token)), None)
                .await;
            let billing = match attempt {
                Ok(billing) => billing,
                Err(error) if should_retry_after_refresh(&error, &credentials, refreshed) => {
                    tracing::debug!("Grok billing rejected the token; refreshing and retrying");
                    credentials = self.refresh_credentials(&credentials).await?;
                    refreshed = true;
                    continue;
                }
                Err(error) => return Err(error),
            };
            return Ok(result_from_billing(
                billing,
                "grok-web",
                credentials.email.clone(),
                credentials.team_id.clone(),
                credentials.login_method(),
            ));
        }
    }

    /// Refresh the access token and write the result back to auth.json.
    async fn refresh_credentials(
        &self,
        credentials: &GrokCredentials,
    ) -> Result<GrokCredentials, ProviderError> {
        let client_id = credentials
            .client_id
            .as_deref()
            .ok_or(ProviderError::AuthRequired)?;
        let refresh_token = credentials
            .refresh_token
            .as_deref()
            .ok_or(ProviderError::AuthRequired)?;

        let tokens =
            request_token_refresh(&self.client, &credentials.issuer, client_id, refresh_token)
                .await?;

        if let Some(path) = Self::auth_file_path()
            && let Err(e) = persist_refreshed_tokens(&path, &credentials.scope_key, &tokens)
        {
            // A refreshed token still works for this fetch even if the write
            // fails; only the Grok CLI loses the rotation.
            tracing::warn!("Failed to persist refreshed Grok token: {e}");
        }

        let mut refreshed = credentials.clone();
        refreshed.access_token = tokens.access_token;
        if let Some(refresh_token) = tokens.refresh_token {
            refreshed.refresh_token = Some(refresh_token);
        }
        refreshed.expires_at = tokens.expires_at;
        tracing::info!("Grok token refreshed successfully");
        Ok(refreshed)
    }

    async fn fetch_with_cookie(
        &self,
        cookie_header: &str,
    ) -> Result<ProviderFetchResult, ProviderError> {
        let billing = self
            .fetch_billing(None, Some(cookie_header.to_string()))
            .await?;
        Ok(result_from_billing(
            billing,
            "grok-browser",
            None,
            None,
            None,
        ))
    }

    async fn fetch_billing(
        &self,
        authorization: Option<String>,
        cookie_header: Option<String>,
    ) -> Result<GrokBillingSnapshot, ProviderError> {
        let mut request = self
            .client
            .post(BILLING_ENDPOINT)
            .body(vec![0, 0, 0, 0, 0])
            .header("Origin", "https://grok.com")
            .header("Referer", "https://grok.com/?_s=usage")
            .header("Accept", "*/*")
            .header("Content-Type", "application/grpc-web+proto")
            .header("x-grpc-web", "1")
            .header("x-user-agent", "connect-es/2.1.1")
            .header("User-Agent", "PulseBar");
        if let Some(auth) = authorization {
            request = request.header("Authorization", auth);
        }
        if let Some(cookie) = cookie_header {
            request = request.header("Cookie", cookie);
        }

        let response = request.send().await?;
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.bytes().await?;
        if !status.is_success() {
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                return Err(ProviderError::AuthRequired);
            }
            return Err(ProviderError::Other(format!(
                "Grok web billing returned status {status}"
            )));
        }
        validate_grpc_headers(&headers)?;
        parse_grpc_web_response(&bytes)
    }

    fn detect_cli_version() -> Option<String> {
        let mut command = std::process::Command::new("grok");
        command.arg("--version");
        hide_windows_console(&mut command);
        let output = command.output().ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        let trimmed = text
            .lines()
            .next()?
            .trim()
            .strip_prefix("grok ")
            .unwrap_or(text.trim());
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }
}

#[cfg(windows)]
fn hide_windows_console(command: &mut std::process::Command) {
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_windows_console(_command: &mut std::process::Command) {}

impl Default for GrokProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for GrokProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Grok
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    async fn fetch_usage(&self, ctx: &FetchContext) -> Result<ProviderFetchResult, ProviderError> {
        match ctx.source_mode {
            SourceMode::Auto | SourceMode::Web => {
                if let Some(ref cookie_header) = ctx.manual_cookie_header {
                    return self.fetch_with_cookie(cookie_header).await;
                }
                #[cfg_attr(not(windows), allow(unused_mut))]
                let mut abe_seen = false;

                #[cfg(windows)]
                {
                    use crate::browser::cookies::{Cookie, CookieExtractor};
                    use crate::browser::detection::BrowserDetector;

                    for browser in BrowserDetector::detect_all() {
                        match CookieExtractor::extract_for_domain(&browser, "grok.com") {
                            Ok(cookies) if !cookies.is_empty() => {
                                let cookie_header = cookies
                                    .iter()
                                    .map(|c: &Cookie| format!("{}={}", c.name, c.value))
                                    .collect::<Vec<_>>()
                                    .join("; ");
                                if !cookie_header.is_empty()
                                    && let Ok(result) = self.fetch_with_cookie(&cookie_header).await
                                {
                                    return Ok(result);
                                }
                            }
                            Ok(_) => {}
                            Err(crate::browser::cookies::CookieError::AppBoundEncryption) => {
                                abe_seen = true;
                            }
                            Err(e) => {
                                tracing::debug!(
                                    "Failed to extract cookies from {}: {}",
                                    browser.browser_type.display_name(),
                                    e
                                );
                            }
                        }
                    }
                }

                // auth.json (from `grok login`) is the primary fallback; only
                // surface ABE if it is missing and the cookie path was blocked.
                match Self::load_credentials() {
                    Ok(credentials) => self.fetch_with_auth(&credentials).await,
                    Err(_) if abe_seen => Err(ProviderError::Other(
                        crate::browser::cookies::CookieError::AppBoundEncryption.to_string(),
                    )),
                    Err(e) => Err(e),
                }
            }
            SourceMode::Cli => Err(ProviderError::UnsupportedSource(SourceMode::Cli)),
            SourceMode::OAuth => Err(ProviderError::UnsupportedSource(SourceMode::OAuth)),
        }
    }

    fn available_sources(&self) -> Vec<SourceMode> {
        vec![SourceMode::Auto, SourceMode::Web]
    }

    fn supports_web(&self) -> bool {
        true
    }

    fn detect_version(&self) -> Option<String> {
        Self::detect_cli_version()
    }
}

#[derive(Debug, Clone)]
struct GrokCredentials {
    access_token: String,
    refresh_token: Option<String>,
    client_id: Option<String>,
    issuer: String,
    scope_key: String,
    auth_mode: Option<String>,
    email: Option<String>,
    team_id: Option<String>,
    expires_at: Option<DateTime<Utc>>,
}

/// Tokens returned by the x.ai OIDC token endpoint after a refresh.
#[derive(Debug, Clone)]
struct RefreshedTokens {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<DateTime<Utc>>,
}

impl GrokCredentials {
    fn parse(text: &str) -> Result<Self, ProviderError> {
        let root: Value = serde_json::from_str(text)
            .map_err(|e| ProviderError::Parse(format!("Failed to decode Grok auth.json: {e}")))?;
        let map = root
            .as_object()
            .ok_or_else(|| ProviderError::Parse("Invalid Grok auth.json".to_string()))?;
        let mut selected: Option<(&String, &Value)> = None;
        for (scope, entry) in map {
            if entry
                .get("key")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
                && (scope.starts_with("https://auth.x.ai::")
                    || selected.is_none()
                    || scope.contains("/sign-in"))
            {
                selected = Some((scope, entry));
                if scope.starts_with("https://auth.x.ai::") {
                    break;
                }
            }
        }
        let (scope, entry) = selected.ok_or(ProviderError::AuthRequired)?;
        let access_token = entry
            .get("key")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or(ProviderError::AuthRequired)?
            .to_string();
        let expires_at = entry
            .get("expires_at")
            .and_then(Value::as_str)
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .map(|dt| dt.with_timezone(&Utc));
        let refresh_token = text_field(entry, "refresh_token");
        let client_id = text_field(entry, "oidc_client_id")
            .or_else(|| scope.split_once("::").map(|(_, id)| id.to_string()));
        let issuer = text_field(entry, "oidc_issuer").unwrap_or_else(|| DEFAULT_ISSUER.to_string());
        // A token past its lifetime is only fatal when nothing can renew it;
        // otherwise the caller refreshes it before the billing request.
        if expires_at.is_some_and(|dt| dt <= Utc::now())
            && (refresh_token.is_none() || client_id.is_none())
        {
            return Err(ProviderError::AuthRequired);
        }
        Ok(Self {
            access_token,
            refresh_token,
            client_id,
            issuer,
            scope_key: scope.clone(),
            auth_mode: text_field(entry, "auth_mode"),
            email: text_field(entry, "email"),
            team_id: text_field(entry, "team_id"),
            expires_at,
        })
    }

    /// True when the access token is expired or close enough that a fetch
    /// would likely race the expiry.
    fn is_expired(&self) -> bool {
        self.expires_at
            .is_some_and(|dt| dt <= Utc::now() + chrono::Duration::minutes(EXPIRY_SKEW_MINUTES))
    }

    fn can_refresh(&self) -> bool {
        self.refresh_token.is_some() && self.client_id.is_some()
    }

    fn login_method(&self) -> Option<String> {
        match self.auth_mode.as_deref().map(str::to_lowercase).as_deref() {
            Some("oidc") => Some("SuperGrok".to_string()),
            Some("session") => Some("session".to_string()),
            Some(other) => Some(other.to_string()),
            None if self.expires_at.is_some() => Some("Grok".to_string()),
            None => None,
        }
    }
}

fn text_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

/// Whether a failed billing request is worth retrying after a token refresh.
/// Only auth failures qualify, and only once per fetch.
fn should_retry_after_refresh(
    error: &ProviderError,
    credentials: &GrokCredentials,
    already_refreshed: bool,
) -> bool {
    matches!(error, ProviderError::AuthRequired) && credentials.can_refresh() && !already_refreshed
}

/// Exchange a refresh token for a fresh access token at the OIDC token
/// endpoint. `grok login` registers a public client, so no client secret is
/// sent (`token_endpoint_auth_methods_supported` includes "none").
async fn request_token_refresh(
    client: &Client,
    issuer: &str,
    client_id: &str,
    refresh_token: &str,
) -> Result<RefreshedTokens, ProviderError> {
    let endpoint = format!("{}/oauth2/token", issuer.trim_end_matches('/'));
    let response = client
        .post(&endpoint)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
        ])
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?;

    if !response.status().is_success() {
        tracing::debug!(status = %response.status(), "Grok token refresh rejected");
        return Err(ProviderError::AuthRequired);
    }

    let body: Value = response
        .json()
        .await
        .map_err(|e| ProviderError::Parse(format!("Failed to decode Grok token response: {e}")))?;
    let access_token = body
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or(ProviderError::AuthRequired)?
        .to_string();
    let expires_at = body
        .get("expires_in")
        .and_then(Value::as_i64)
        .and_then(|secs| Utc::now().checked_add_signed(chrono::Duration::seconds(secs)));

    Ok(RefreshedTokens {
        access_token,
        refresh_token: text_field(&body, "refresh_token"),
        expires_at,
    })
}

/// Write refreshed tokens back into auth.json so the Grok CLI keeps working.
/// The endpoint rotates refresh tokens, so dropping the response would
/// invalidate the CLI's stored credentials. Other entries and unknown fields
/// are preserved, and the file is replaced atomically.
fn persist_refreshed_tokens(
    path: &std::path::Path,
    scope_key: &str,
    tokens: &RefreshedTokens,
) -> Result<(), ProviderError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| ProviderError::Other(format!("Failed to read Grok auth.json: {e}")))?;
    let mut root: Value = serde_json::from_str(&text)
        .map_err(|e| ProviderError::Parse(format!("Failed to decode Grok auth.json: {e}")))?;
    let entry = root
        .get_mut(scope_key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            ProviderError::Other("Grok auth.json no longer has the refreshed scope".to_string())
        })?;

    entry.insert(
        "key".to_string(),
        Value::String(tokens.access_token.clone()),
    );
    if let Some(ref refresh_token) = tokens.refresh_token {
        entry.insert(
            "refresh_token".to_string(),
            Value::String(refresh_token.clone()),
        );
    }
    if let Some(expires_at) = tokens.expires_at {
        entry.insert(
            "expires_at".to_string(),
            Value::String(expires_at.to_rfc3339_opts(SecondsFormat::Secs, true)),
        );
    }

    let serialized = serde_json::to_string_pretty(&root)
        .map_err(|e| ProviderError::Parse(format!("Failed to encode Grok auth.json: {e}")))?;
    let temp_path = path.with_extension("json.pulsebar-tmp");
    std::fs::write(&temp_path, serialized)
        .map_err(|e| ProviderError::Other(format!("Failed to stage Grok auth.json: {e}")))?;
    std::fs::rename(&temp_path, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        ProviderError::Other(format!("Failed to update Grok auth.json: {e}"))
    })?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct GrokBillingSnapshot {
    used_percent: f64,
    resets_at: Option<DateTime<Utc>>,
}

fn result_from_billing(
    billing: GrokBillingSnapshot,
    source_label: &str,
    email: Option<String>,
    team_id: Option<String>,
    login_method: Option<String>,
) -> ProviderFetchResult {
    let mut usage = UsageSnapshot::new(RateWindow::with_details(
        billing.used_percent,
        None,
        billing.resets_at,
        None,
    ));
    usage.account_email = email;
    usage.account_organization = team_id;
    usage.login_method = login_method;
    ProviderFetchResult::new(usage, source_label)
}

fn validate_grpc_headers(headers: &reqwest::header::HeaderMap) -> Result<(), ProviderError> {
    if let Some(status) = headers
        .get("grpc-status")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u16>().ok())
        && status != 0
    {
        if status == 16 {
            return Err(ProviderError::AuthRequired);
        }
        return Err(ProviderError::Other(format!(
            "Grok RPC failed with status {status}"
        )));
    }
    Ok(())
}

fn parse_grpc_web_response(data: &[u8]) -> Result<GrokBillingSnapshot, ProviderError> {
    let frames = grpc_web_data_frames(data);
    if frames.is_empty() {
        return Err(ProviderError::Parse(
            "Grok web billing returned no payload".to_string(),
        ));
    }
    let mut scan = ProtoScan::default();
    for frame in frames {
        scan.scan_message(&frame, &mut Vec::new(), 0);
    }
    let used_percent = scan
        .fixed32
        .iter()
        .filter(|field| {
            field.path.last() == Some(&1)
                && field.value.is_finite()
                && field.value >= 0.0
                && field.value <= 100.0
        })
        .min_by(|a, b| {
            a.path
                .len()
                .cmp(&b.path.len())
                .then_with(|| a.order.cmp(&b.order))
        })
        .map(|field| field.value as f64)
        .ok_or_else(|| ProviderError::Parse("Could not parse Grok billing percent".to_string()))?;

    let resets_at = scan
        .varints
        .iter()
        .filter_map(|field| {
            (1_700_000_000..=2_100_000_000)
                .contains(&field.value)
                .then(|| Utc.timestamp_opt(field.value as i64, 0).single())
                .flatten()
        })
        .filter(|dt| *dt > Utc::now())
        .min();
    Ok(GrokBillingSnapshot {
        used_percent,
        resets_at,
    })
}

fn grpc_web_data_frames(data: &[u8]) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut index = 0;
    while index + 5 <= data.len() {
        let flags = data[index];
        let len = ((data[index + 1] as usize) << 24)
            | ((data[index + 2] as usize) << 16)
            | ((data[index + 3] as usize) << 8)
            | (data[index + 4] as usize);
        let start = index + 5;
        let end = start.saturating_add(len);
        if end > data.len() {
            break;
        }
        if flags & 0x80 == 0 {
            frames.push(data[start..end].to_vec());
        }
        index = end;
    }
    frames
}

#[derive(Default)]
struct ProtoScan {
    fixed32: Vec<Fixed32Field>,
    varints: Vec<VarintField>,
    order: usize,
}

struct Fixed32Field {
    path: Vec<u64>,
    value: f32,
    order: usize,
}

struct VarintField {
    value: u64,
}

impl ProtoScan {
    fn scan_message(&mut self, data: &[u8], path: &mut Vec<u64>, depth: usize) {
        if depth > 8 {
            return;
        }
        let mut i = 0;
        while i < data.len() {
            let Some((field, wire, next)) = read_key(data, i) else {
                break;
            };
            i = next;
            path.push(field);
            let Some(next) = self.scan_field(data, i, path, depth, wire) else {
                path.pop();
                break;
            };
            i = next;
            path.pop();
        }
    }

    fn scan_field(
        &mut self,
        data: &[u8],
        i: usize,
        path: &mut Vec<u64>,
        depth: usize,
        wire: u64,
    ) -> Option<usize> {
        match wire {
            0 => self.scan_varint(data, i),
            2 => self.scan_length_delimited(data, i, path, depth),
            5 => self.scan_fixed32(data, i, path),
            1 => Some(i.saturating_add(8)),
            _ => None,
        }
    }

    fn scan_varint(&mut self, data: &[u8], i: usize) -> Option<usize> {
        let (value, next) = read_varint(data, i)?;
        self.varints.push(VarintField { value });
        Some(next)
    }

    fn scan_length_delimited(
        &mut self,
        data: &[u8],
        i: usize,
        path: &mut Vec<u64>,
        depth: usize,
    ) -> Option<usize> {
        let (len, next) = read_varint(data, i)?;
        let start = next;
        let end = start.saturating_add(len as usize);
        if end <= data.len() {
            self.scan_message(&data[start..end], path, depth + 1);
            Some(end)
        } else {
            None
        }
    }

    fn scan_fixed32(&mut self, data: &[u8], i: usize, path: &[u64]) -> Option<usize> {
        if i + 4 > data.len() {
            return None;
        }
        let bytes = [data[i], data[i + 1], data[i + 2], data[i + 3]];
        self.fixed32.push(Fixed32Field {
            path: path.to_vec(),
            value: f32::from_le_bytes(bytes),
            order: self.order,
        });
        self.order += 1;
        Some(i + 4)
    }
}

fn read_key(data: &[u8], i: usize) -> Option<(u64, u64, usize)> {
    let (key, next) = read_varint(data, i)?;
    Some((key >> 3, key & 0x07, next))
}

fn read_varint(data: &[u8], mut i: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0;
    while i < data.len() && shift < 64 {
        let b = data[i];
        i += 1;
        value |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some((value, i));
        }
        shift += 7;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_auth_file_prefer_oidc() {
        let auth = r#"{
          "https://accounts.x.ai/sign-in": {"key": "legacy"},
          "https://auth.x.ai::abc": {"key": "oidc", "auth_mode": "oidc", "email": "u@example.com"}
        }"#;
        let parsed = GrokCredentials::parse(auth).unwrap();
        assert_eq!(parsed.access_token, "oidc");
        assert_eq!(parsed.login_method().as_deref(), Some("SuperGrok"));
    }

    #[test]
    fn splits_grpc_web_data_frames() {
        let data = [0, 0, 0, 0, 2, 1, 2, 0x80, 0, 0, 0, 1, b'x'];
        assert_eq!(grpc_web_data_frames(&data), vec![vec![1, 2]]);
    }

    #[test]
    fn parses_refresh_metadata_from_auth_file() {
        let auth = r#"{
          "https://auth.x.ai::client-123": {
            "key": "access",
            "auth_mode": "oidc",
            "refresh_token": "refresh-abc",
            "oidc_issuer": "https://auth.x.ai",
            "oidc_client_id": "client-123",
            "expires_at": "2999-01-01T00:00:00Z"
          }
        }"#;
        let parsed = GrokCredentials::parse(auth).unwrap();
        assert_eq!(parsed.refresh_token.as_deref(), Some("refresh-abc"));
        assert_eq!(parsed.client_id.as_deref(), Some("client-123"));
        assert_eq!(parsed.issuer, "https://auth.x.ai");
        assert_eq!(parsed.scope_key, "https://auth.x.ai::client-123");
        assert!(!parsed.is_expired());
        assert!(parsed.can_refresh());
    }

    #[test]
    fn client_id_falls_back_to_scope_suffix() {
        let auth = r#"{
          "https://auth.x.ai::scope-client": {"key": "access", "refresh_token": "r"}
        }"#;
        let parsed = GrokCredentials::parse(auth).unwrap();
        assert_eq!(parsed.client_id.as_deref(), Some("scope-client"));
    }

    #[test]
    fn expired_credentials_with_refresh_token_are_kept_for_refresh() {
        let auth = r#"{
          "https://auth.x.ai::c": {
            "key": "stale",
            "refresh_token": "refresh-abc",
            "expires_at": "2000-01-01T00:00:00Z"
          }
        }"#;
        let parsed = GrokCredentials::parse(auth).unwrap();
        assert!(parsed.is_expired());
        assert!(parsed.can_refresh());
    }

    #[test]
    fn expired_credentials_without_refresh_token_still_require_auth() {
        let auth = r#"{
          "https://auth.x.ai::c": {"key": "stale", "expires_at": "2000-01-01T00:00:00Z"}
        }"#;
        assert!(matches!(
            GrokCredentials::parse(auth),
            Err(ProviderError::AuthRequired)
        ));
    }

    #[tokio::test]
    async fn token_refresh_returns_rotated_tokens() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/oauth2/token")
            .match_body(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("grant_type".into(), "refresh_token".into()),
                mockito::Matcher::UrlEncoded("refresh_token".into(), "old-refresh".into()),
                mockito::Matcher::UrlEncoded("client_id".into(), "client-123".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"access_token":"new-access","refresh_token":"new-refresh","expires_in":21600}"#,
            )
            .create_async()
            .await;

        let tokens =
            request_token_refresh(&Client::new(), &server.url(), "client-123", "old-refresh")
                .await
                .unwrap();

        mock.assert_async().await;
        assert_eq!(tokens.access_token, "new-access");
        assert_eq!(tokens.refresh_token.as_deref(), Some("new-refresh"));
        let expires_at = tokens.expires_at.expect("expires_at");
        let delta = (expires_at - Utc::now()).num_seconds();
        assert!((21_000..=21_600).contains(&delta), "delta was {delta}");
    }

    #[tokio::test]
    async fn token_refresh_rejection_maps_to_auth_required() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/oauth2/token")
            .with_status(400)
            .with_body(r#"{"error":"invalid_grant"}"#)
            .create_async()
            .await;

        let error = request_token_refresh(&Client::new(), &server.url(), "c", "bad")
            .await
            .unwrap_err();
        assert!(matches!(error, ProviderError::AuthRequired));
    }

    #[test]
    fn persisting_refreshed_tokens_rotates_entry_and_preserves_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        std::fs::write(
            &path,
            r#"{
              "https://accounts.x.ai/sign-in": {"key": "legacy"},
              "https://auth.x.ai::c": {
                "key": "stale",
                "refresh_token": "old-refresh",
                "email": "u@example.com",
                "expires_at": "2000-01-01T00:00:00Z"
              }
            }"#,
        )
        .unwrap();

        let tokens = RefreshedTokens {
            access_token: "new-access".to_string(),
            refresh_token: Some("new-refresh".to_string()),
            expires_at: Some(Utc.timestamp_opt(2_000_000_000, 0).single().unwrap()),
        };
        persist_refreshed_tokens(&path, "https://auth.x.ai::c", &tokens).unwrap();

        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let entry = &saved["https://auth.x.ai::c"];
        assert_eq!(entry["key"], "new-access");
        assert_eq!(entry["refresh_token"], "new-refresh");
        assert_eq!(entry["expires_at"], "2033-05-18T03:33:20Z");
        assert_eq!(entry["email"], "u@example.com");
        assert_eq!(saved["https://accounts.x.ai/sign-in"]["key"], "legacy");
    }

    #[test]
    fn persisting_without_rotation_keeps_previous_refresh_token() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        std::fs::write(
            &path,
            r#"{"https://auth.x.ai::c": {"key": "stale", "refresh_token": "old-refresh"}}"#,
        )
        .unwrap();

        let tokens = RefreshedTokens {
            access_token: "new-access".to_string(),
            refresh_token: None,
            expires_at: None,
        };
        persist_refreshed_tokens(&path, "https://auth.x.ai::c", &tokens).unwrap();

        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["https://auth.x.ai::c"]["key"], "new-access");
        assert_eq!(
            saved["https://auth.x.ai::c"]["refresh_token"],
            "old-refresh"
        );
    }

    #[test]
    fn auth_failure_triggers_a_single_refresh_retry() {
        let creds = GrokCredentials::parse(
            r#"{"https://auth.x.ai::c": {"key": "k", "refresh_token": "r"}}"#,
        )
        .unwrap();
        assert!(should_retry_after_refresh(
            &ProviderError::AuthRequired,
            &creds,
            false
        ));
        assert!(!should_retry_after_refresh(
            &ProviderError::AuthRequired,
            &creds,
            true
        ));
    }

    #[test]
    fn non_auth_failures_do_not_trigger_a_refresh() {
        let creds = GrokCredentials::parse(
            r#"{"https://auth.x.ai::c": {"key": "k", "refresh_token": "r"}}"#,
        )
        .unwrap();
        assert!(!should_retry_after_refresh(
            &ProviderError::Other("boom".to_string()),
            &creds,
            false
        ));
    }

    #[test]
    fn credentials_without_refresh_token_never_retry() {
        let creds = GrokCredentials::parse(r#"{"https://auth.x.ai::c": {"key": "k"}}"#).unwrap();
        assert!(!should_retry_after_refresh(
            &ProviderError::AuthRequired,
            &creds,
            false
        ));
    }

    #[test]
    fn abe_error_message_is_actionable() {
        use crate::browser::cookies::CookieError;
        let msg = CookieError::AppBoundEncryption.to_string();
        assert!(msg.contains("App-Bound Encryption"));
        assert!(msg.contains("Chrome/Edge"));
    }
}
