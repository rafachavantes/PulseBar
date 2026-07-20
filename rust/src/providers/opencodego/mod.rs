//! OpenCode Go provider implementation
//!
//! Separate workspace surface that shares the `opencode.ai` cookie domain with
//! the OpenCode provider. 100% manual: requires `workspace_id` +
//! `manual_cookie_header` and scrapes the `/go` usage page for
//! rolling/weekly/monthly windows.

use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;

use crate::core::{
    FetchContext, Provider, ProviderError, ProviderFetchResult, ProviderId, ProviderMetadata,
    RateWindow, SourceMode, UsageSnapshot,
};

const BASE_URL: &str = "https://opencode.ai";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

pub struct OpenCodeGoProvider {
    metadata: ProviderMetadata,
    client: Client,
}

impl OpenCodeGoProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                id: ProviderId::OpenCodeGo,
                display_name: "OpenCode Go",
                session_label: "Rolling",
                weekly_label: "Weekly",
                supports_opus: false,
                supports_credits: false,
                default_enabled: false,
                is_primary: false,
                dashboard_url: Some("https://opencode.ai"),
                status_page_url: None,
            },
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    async fn fetch_usage_page(
        &self,
        workspace_id: &str,
        cookie_header: &str,
    ) -> Result<String, ProviderError> {
        let url = format!("{}/workspace/{}/go", BASE_URL, workspace_id);
        let response = self
            .client
            .get(&url)
            .header("Cookie", cookie_header)
            .header("User-Agent", USER_AGENT)
            .header("Referer", BASE_URL)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .send()
            .await?;

        // The server redirects unauthenticated requests to auth.opencode.ai.
        // reqwest follows the redirect, so the final URL is the clearest signal.
        if response.url().as_str().contains("auth.opencode.ai")
            || response.url().as_str().contains("/auth/authorize")
        {
            return Err(ProviderError::AuthRequired);
        }

        let status = response.status();
        if !status.is_success() {
            if status.as_u16() == 401 || status.as_u16() == 403 {
                return Err(ProviderError::AuthRequired);
            }
            return Err(ProviderError::Other(format!(
                "OpenCode Go usage page returned {}",
                status
            )));
        }

        let text = response.text().await?;
        if Self::looks_signed_out(&text) {
            return Err(ProviderError::AuthRequired);
        }
        Ok(text)
    }

    fn parse_usage_text(text: &str) -> Result<UsageSnapshot, ProviderError> {
        let now = Utc::now();

        let rolling = Self::extract_window(text, &["rollingUsage", "rolling_usage", "rolling"])
            .ok_or_else(|| ProviderError::Parse("Missing rolling usage window".to_string()))?;
        let weekly = Self::extract_window(text, &["weeklyUsage", "weekly_usage", "weekly"]);
        let monthly = Self::extract_window(text, &["monthlyUsage", "monthly_usage", "monthly"]);

        let primary = RateWindow::with_details(
            rolling.0,
            Some(300),
            Some(now + chrono::Duration::seconds(rolling.1)),
            None,
        );
        let mut snap = UsageSnapshot::new(primary).with_login_method("OpenCode Go");

        if let Some((pct, reset)) = weekly {
            snap = snap.with_secondary(RateWindow::with_details(
                pct,
                Some(10080),
                Some(now + chrono::Duration::seconds(reset)),
                None,
            ));
        }

        if let Some((pct, reset)) = monthly {
            snap = snap.with_tertiary(RateWindow::with_details(
                pct,
                Some(43200),
                Some(now + chrono::Duration::seconds(reset)),
                None,
            ));
        }

        Ok(snap)
    }

    /// Extract `(percent, resetInSec)` for a usage block by name.
    fn extract_window(text: &str, names: &[&str]) -> Option<(f64, i64)> {
        for name in names {
            let percent_pattern = format!(
                r#"{}[^}}]*?(?:usagePercent|usedPercent|percentUsed|percent)\s*[:=]\s*([0-9]+(?:\.[0-9]+)?)"#,
                name
            );
            let reset_pattern = format!(
                r#"{}[^}}]*?(?:resetInSec|resetInSeconds|resetSeconds|resetSec)\s*[:=]\s*([0-9]+)"#,
                name
            );

            let percent = Self::extract_number(&percent_pattern, text);
            if let Some(p) = percent {
                let reset = Self::extract_number(&reset_pattern, text)
                    .map(|n| n as i64)
                    .unwrap_or(0);
                let p = if p <= 1.0 { p * 100.0 } else { p };
                return Some((p.clamp(0.0, 100.0), reset.max(0)));
            }
        }
        None
    }

    fn extract_number(pattern: &str, text: &str) -> Option<f64> {
        let re = regex_lite::Regex::new(pattern).ok()?;
        re.captures(text)?.get(1)?.as_str().parse().ok()
    }

    fn looks_signed_out(text: &str) -> bool {
        let lower = text.to_lowercase();
        lower.contains("auth/authorize")
            || lower.contains("\"signin\"")
            || lower.contains("please sign in")
            || lower.contains("continue with github")
            || lower.contains("<title>openauth")
    }
}

impl Default for OpenCodeGoProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for OpenCodeGoProvider {
    fn id(&self) -> ProviderId {
        ProviderId::OpenCodeGo
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    async fn fetch_usage(&self, ctx: &FetchContext) -> Result<ProviderFetchResult, ProviderError> {
        tracing::debug!("Fetching OpenCode Go usage");

        match ctx.source_mode {
            SourceMode::Auto | SourceMode::Web => {
                let workspace_id = ctx.workspace_id.as_deref().ok_or_else(|| {
                    ProviderError::Other(
                        "OpenCode Go requires a workspace ID. Set `workspace_id` under the opencodego entry in settings.json (copy the `wrk_...` from https://opencode.ai/workspace/<id>/go).".to_string()
                    )
                })?;
                let raw_cookie = ctx.manual_cookie_header.as_deref().ok_or_else(|| {
                    ProviderError::Other(
                        "OpenCode Go requires a cookie header. Paste the Cookie header from opencode.ai (DevTools → Network → copy Cookie) under the opencodego entry in manual_cookies.json / Preferences.".to_string()
                    )
                })?;
                // Cookie headers must not contain internal whitespace. Paste/render
                // artifacts (wrapped displays, rich-text copies) can insert spaces
                // that corrupt Fe26.2 session values; strip all whitespace.
                let cookie_header: String =
                    raw_cookie.chars().filter(|c| !c.is_whitespace()).collect();
                let html = self.fetch_usage_page(workspace_id, &cookie_header).await?;
                let snap = Self::parse_usage_text(&html)?;
                Ok(ProviderFetchResult::new(snap, "web"))
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

    fn supports_cli(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usage_blocks() {
        let text = r#"
            rollingUsage: { usagePercent: 42.5, resetInSec: 3600 }
            weeklyUsage: { usagePercent: 0.13, resetInSec: 86400 }
            monthlyUsage: { usagePercent: 7, resetInSec: 2592000 }
        "#;
        let snap = OpenCodeGoProvider::parse_usage_text(text).unwrap();
        assert!((snap.primary.used_percent - 42.5).abs() < 0.001);
        let secondary = snap.secondary.expect("weekly");
        // 0.13 normalized as fraction → 13%
        assert!((secondary.used_percent - 13.0).abs() < 0.001);
        let tertiary = snap.tertiary.expect("monthly");
        assert!((tertiary.used_percent - 7.0).abs() < 0.001);
    }

    #[test]
    fn detects_openauth_login_page_as_signed_out() {
        let html = r#"<html><head><title>OpenAuth</title></head>
            <body><button>Continue with GitHub</button>
            <button>Continue with Google</button></body></html>"#;
        assert!(OpenCodeGoProvider::looks_signed_out(html));
    }

    #[test]
    fn does_not_flag_real_usage_page_as_signed_out() {
        let html = r#"<html><head><title>OpenCode Go</title></head>
            <body>rollingUsage: { usagePercent: 42, resetInSec: 3600 }</body></html>"#;
        assert!(!OpenCodeGoProvider::looks_signed_out(html));
    }

    /// Verifies the provider's real fetch path works with a live cookie.
    /// Run with: OPENCODE_GO_TEST_COOKIE="auth=...; oc_locale=en" cargo test -- --ignored
    #[tokio::test]
    #[ignore = "live network; set OPENCODE_GO_TEST_COOKIE and OPENCODE_GO_TEST_WORKSPACE"]
    async fn fetch_usage_page_with_live_cookie_returns_usage() {
        let cookie = std::env::var("OPENCODE_GO_TEST_COOKIE").unwrap_or_default();
        let workspace = std::env::var("OPENCODE_GO_TEST_WORKSPACE")
            .unwrap_or_else(|_| "wrk_01KXCFBCZMP3VDKGRPE47GZT23".to_string());
        if cookie.is_empty() {
            eprintln!("skipped: OPENCODE_GO_TEST_COOKIE not set");
            return;
        }
        let provider = OpenCodeGoProvider::new();
        match provider.fetch_usage_page(&workspace, &cookie).await {
            Ok(html) => {
                assert!(
                    html.contains("rollingUsage"),
                    "no rollingUsage in response (first 300 chars): {}",
                    &html[..300.min(html.len())]
                );
            }
            Err(e) => panic!("fetch_usage_page failed with a supposedly valid cookie: {e}"),
        }
    }

    #[tokio::test]
    async fn missing_workspace_id_returns_actionable_error() {
        let provider = OpenCodeGoProvider::new();
        let ctx = FetchContext {
            source_mode: SourceMode::Auto,
            include_credits: true,
            web_timeout: 60,
            verbose: false,
            manual_cookie_header: Some("k=v".to_string()),
            api_key: None,
            workspace_id: None,
            api_region: None,
        };
        let err = provider.fetch_usage(&ctx).await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("workspace ID"), "got: {msg}");
    }

    #[tokio::test]
    async fn missing_cookie_returns_actionable_error() {
        let provider = OpenCodeGoProvider::new();
        let ctx = FetchContext {
            source_mode: SourceMode::Auto,
            include_credits: true,
            web_timeout: 60,
            verbose: false,
            manual_cookie_header: None,
            api_key: None,
            workspace_id: Some("wrk_test".to_string()),
            api_region: None,
        };
        let err = provider.fetch_usage(&ctx).await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("cookie"), "got: {msg}");
    }
}
