//! z.ai provider implementation
//!
//! Fetches usage data from z.ai's quota API
//! Uses API token stored in Windows Credential Manager

pub mod mcp_details;

// Re-exports for MCP details menu
#[allow(unused_imports)]
pub use mcp_details::{
    McpDetailsMenu, ZaiLimitEntry, ZaiLimitType, ZaiLimitUnit, ZaiUsageDetail, ZaiUsageSnapshot,
};

use async_trait::async_trait;
use serde::Deserialize;

use crate::core::{
    FetchContext, Provider, ProviderError, ProviderFetchResult, ProviderId, ProviderMetadata,
    RateWindow, SourceMode, UsageSnapshot,
};

/// z.ai API endpoint for quota/usage
const ZAI_API_URL: &str = "https://api.z.ai/api/monitor/usage/quota/limit";

/// Windows Credential Manager target for z.ai API token
const ZAI_CREDENTIAL_TARGET: &str = "pulsebar-zai";

/// z.ai quota response structure (live `quota/limit` endpoint)
#[derive(Debug, Deserialize)]
struct ZaiQuotaResponse {
    #[serde(default)]
    data: Option<ZaiQuotaData>,
}

#[derive(Debug, Deserialize)]
struct ZaiQuotaData {
    #[serde(default)]
    limits: Vec<ZaiLimit>,
    /// Account plan tier (e.g. "pro")
    #[serde(default)]
    level: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ZaiLimit {
    /// Limit type: "TOKENS_LIMIT" or "TIME_LIMIT"
    #[serde(rename = "type")]
    limit_type: Option<String>,
    /// Usage percentage (0-100) provided directly by z.ai
    #[serde(default)]
    percentage: Option<f64>,
    /// Reset time as Unix epoch milliseconds
    #[serde(rename = "nextResetTime", default)]
    next_reset_time: Option<i64>,
    /// Time unit enum: 3=hours, 5=minutes, 6=weeks
    #[serde(default)]
    unit: Option<i32>,
    /// Number of time units in the window
    #[serde(default)]
    number: Option<i32>,
}

/// z.ai provider
pub struct ZaiProvider {
    metadata: ProviderMetadata,
}

impl ZaiProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                id: ProviderId::Zai,
                display_name: "z.ai",
                session_label: "5-Hour",
                weekly_label: "Weekly",
                supports_opus: false,
                supports_credits: true,
                default_enabled: false,
                is_primary: false,
                dashboard_url: Some("https://z.ai/dashboard"),
                status_page_url: None,
            },
        }
    }

    /// Get API token from ctx, Windows Credential Manager, or env
    fn get_api_token(api_key: Option<&str>) -> Result<String, ProviderError> {
        // Check ctx.api_key first (from settings)
        if let Some(key) = api_key
            && !key.is_empty()
        {
            return Ok(key.to_string());
        }

        // Try Windows Credential Manager
        match keyring::Entry::new(ZAI_CREDENTIAL_TARGET, "api_token") {
            Ok(entry) => match entry.get_password() {
                Ok(token) => Ok(token),
                Err(_) => {
                    // Try environment variable as fallback
                    std::env::var("ZAI_API_TOKEN").map_err(|_| {
                        ProviderError::NotInstalled(
                            "z.ai API token not found. Set in Preferences → Providers or ZAI_API_TOKEN environment variable.".to_string()
                        )
                    })
                }
            },
            Err(_) => {
                // Try environment variable as fallback
                std::env::var("ZAI_API_TOKEN").map_err(|_| {
                    ProviderError::NotInstalled(
                        "z.ai API token not found. Set in Preferences → Providers or ZAI_API_TOKEN environment variable.".to_string()
                    )
                })
            }
        }
    }

    /// Fetch usage from z.ai API
    async fn fetch_usage_api(&self, ctx: &FetchContext) -> Result<UsageSnapshot, ProviderError> {
        let api_token = Self::get_api_token(ctx.api_key.as_deref())?;

        let client = crate::core::credentialed_http_client_builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| ProviderError::Other(e.to_string()))?;

        let resp = client
            .get(ZAI_API_URL)
            .header("Authorization", format!("Bearer {}", api_token))
            .header("Accept", "application/json")
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ProviderError::AuthRequired);
        }

        if !resp.status().is_success() {
            return Err(ProviderError::Other(format!(
                "z.ai API returned status {}",
                resp.status()
            )));
        }

        let resp_bytes = resp
            .bytes()
            .await
            .map_err(|e| ProviderError::Other(e.to_string()))?;

        // Handle empty response body (can happen with wrong region/endpoint)
        if resp_bytes.is_empty() {
            return Err(ProviderError::Parse(
                "Empty response body from z.ai API. Check API region and token.".to_string(),
            ));
        }

        let quota: ZaiQuotaResponse =
            serde_json::from_slice(&resp_bytes).map_err(|e| ProviderError::Parse(e.to_string()))?;

        self.parse_quota_response(&quota)
    }

    fn parse_quota_response(
        &self,
        quota: &ZaiQuotaResponse,
    ) -> Result<UsageSnapshot, ProviderError> {
        let data = quota.data.as_ref().ok_or_else(|| {
            ProviderError::Parse("z.ai response missing 'data' object".to_string())
        })?;
        let limits = &data.limits;
        let plan_name = data.level.clone().unwrap_or_else(|| "z.ai".to_string());

        // TOKENS_LIMIT entries (sorted shortest window → longest window)
        let mut token_limits: Vec<&ZaiLimit> = limits
            .iter()
            .filter(|l| l.limit_type.as_deref() == Some("TOKENS_LIMIT"))
            .collect();
        token_limits.sort_by_key(|l| Self::window_minutes(l).unwrap_or(0));

        // TIME_LIMIT entry (if present)
        let time_limit = limits
            .iter()
            .find(|l| l.limit_type.as_deref() == Some("TIME_LIMIT"));

        // Window layout: shortest TOKENS_LIMIT = primary (5-hour session),
        // longest = secondary (weekly); TIME_LIMIT = labeled "Monthly Web" extra.
        let (primary, secondary) = match token_limits.len() {
            0 => {
                let p = time_limit
                    .map(Self::make_window)
                    .unwrap_or_else(|| RateWindow::new(0.0));
                (p, None)
            }
            1 => (Self::make_window(token_limits[0]), None),
            _ => {
                let session = token_limits.first().unwrap();
                let weekly = token_limits.last().unwrap();
                (Self::make_window(session), Some(Self::make_window(weekly)))
            }
        };

        let mut usage = UsageSnapshot::new(primary).with_login_method(plan_name);
        if let Some(sec) = secondary {
            usage = usage.with_secondary(sec);
        }
        if let Some(time) = time_limit {
            usage =
                usage.with_extra_rate_window("monthly-web", "Monthly Web", Self::make_window(time));
        }

        Ok(usage)
    }

    /// Build a `RateWindow` from a z.ai limit using the provider-supplied
    /// usage percentage, converting `nextResetTime` (epoch ms) to a reset
    /// timestamp.
    fn make_window(l: &ZaiLimit) -> RateWindow {
        let pct = l.percentage.unwrap_or(0.0).clamp(0.0, 100.0);
        let resets_at = l
            .next_reset_time
            .and_then(chrono::DateTime::from_timestamp_millis);
        RateWindow::with_details(pct, Self::window_minutes(l), resets_at, None)
    }

    /// Compute window_minutes from a limit's unit + number fields
    fn window_minutes(l: &ZaiLimit) -> Option<u32> {
        let unit = l.unit?;
        let number = l.number.unwrap_or(1) as u32;
        let minutes_per_unit = match unit {
            1 => 1440,  // days
            3 => 60,    // hours
            5 => 1,     // minutes
            6 => 10080, // weeks
            _ => return None,
        };
        Some(number * minutes_per_unit)
    }
}

impl Default for ZaiProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for ZaiProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Zai
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    async fn fetch_usage(&self, ctx: &FetchContext) -> Result<ProviderFetchResult, ProviderError> {
        tracing::debug!("Fetching z.ai usage");

        // z.ai authenticates with a personal API token (no OAuth/CLI/web).
        match ctx.source_mode {
            SourceMode::Auto | SourceMode::OAuth => {
                let usage = self.fetch_usage_api(ctx).await?;
                Ok(ProviderFetchResult::new(usage, "api"))
            }
            SourceMode::Web | SourceMode::Cli => {
                // z.ai doesn't support web cookies or CLI
                Err(ProviderError::UnsupportedSource(ctx.source_mode))
            }
        }
    }

    fn available_sources(&self) -> Vec<SourceMode> {
        vec![SourceMode::Auto, SourceMode::OAuth]
    }

    fn supports_web(&self) -> bool {
        false
    }

    fn supports_cli(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_RESPONSE: &str = r#"{"code":200,"msg":"Operation successful","data":{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":9,"nextResetTime":1782409692760},{"type":"TOKENS_LIMIT","unit":6,"number":1,"percentage":61,"nextResetTime":1782469879990},{"type":"TIME_LIMIT","unit":5,"number":1,"usage":1000,"currentValue":0,"remaining":1000,"percentage":0,"nextResetTime":1783247479994,"usageDetails":[]}],"level":"pro"},"success":true}"#;

    #[test]
    fn parses_live_quota_response() {
        let quota: ZaiQuotaResponse =
            serde_json::from_str(REAL_RESPONSE).expect("deserialize z.ai response");
        let usage = ZaiProvider::new()
            .parse_quota_response(&quota)
            .expect("parse");

        // 5-hour token limit (shortest window) is the primary / session metric.
        assert_eq!(usage.primary.used_percent.round(), 9.0);
        // Weekly token limit (longest window) is the secondary metric.
        let weekly = usage.secondary.as_ref().expect("weekly window");
        assert_eq!(weekly.used_percent.round(), 61.0);
        // TIME_LIMIT (monthly web search) is exposed as a labeled extra window.
        let monthly = usage
            .extra_rate_windows
            .iter()
            .find(|e| e.id == "monthly-web")
            .expect("monthly web extra");
        assert_eq!(monthly.title, "Monthly Web");
        assert_eq!(monthly.window.used_percent.round(), 0.0);
        // Plan tier comes from `data.level`.
        assert_eq!(usage.login_method.as_deref(), Some("pro"));
        assert!(usage.model_specific.is_none());
        assert!(usage.tertiary.is_none());
    }

    #[test]
    fn next_reset_time_converts_to_datetime() {
        let quota: ZaiQuotaResponse = serde_json::from_str(REAL_RESPONSE).unwrap();
        let usage = ZaiProvider::new().parse_quota_response(&quota).unwrap();
        // nextResetTime (epoch ms) should populate the structured reset timestamp.
        assert!(usage.primary.resets_at.is_some());
        assert!(usage.secondary.as_ref().unwrap().resets_at.is_some());
    }

    #[test]
    fn missing_data_is_parse_error() {
        let quota: ZaiQuotaResponse = serde_json::from_str(r#"{"success":true}"#).unwrap();
        let err = ZaiProvider::new().parse_quota_response(&quota).unwrap_err();
        assert!(matches!(err, ProviderError::Parse(_)));
    }
}
