//! Provider chart data commands and DTOs.
//!
//! Cost history comes from the shared JSONL cost scanner and is available for
//! every provider. Credits history + usage breakdowns currently only apply to
//! the Codex / OpenAI dashboard cache and require an `account_email` to scope
//! reads to the right cached bundle.

use pulsebar::core::OpenAIDashboardCacheStore;
use pulsebar::cost_scanner::{CostScanner, CostSummary, get_daily_cost_history};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

/// A single (date, value) point for cost or credits history charts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyCostPoint {
    pub date: String,
    pub value: f64,
}

/// A single service's usage within a day for the stacked usage breakdown chart.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceUsagePoint {
    pub service: String,
    pub credits_used: f64,
}

/// One day's stacked usage breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsageBreakdown {
    pub day: String,
    pub services: Vec<ServiceUsagePoint>,
    pub total_credits_used: f64,
}

/// Real local usage summary from Codex / Claude log files.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderLocalUsageSummary {
    pub today_cost: Option<f64>,
    pub thirty_day_cost: Option<f64>,
    pub thirty_day_tokens: Option<u64>,
    pub latest_tokens: Option<u64>,
    pub top_model: Option<String>,
    pub estimate_note: String,
}

/// Full chart data bundle for one provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderChartData {
    pub provider_id: String,
    pub cost_history: Vec<DailyCostPoint>,
    pub credits_history: Vec<DailyCostPoint>,
    pub usage_breakdown: Vec<DailyUsageBreakdown>,
    pub local_usage: Option<ProviderLocalUsageSummary>,
}

#[tauri::command]
pub async fn get_provider_chart_data(
    provider_id: String,
    account_email: Option<String>,
) -> ProviderChartData {
    // Opening the tray remounts a MenuCard per provider, each of which calls
    // this — up to several times per open. Serve a recent memoized result so a
    // reopen is instant instead of re-running full 30-day JSONL scans.
    let key = chart_cache_key(&provider_id, account_email.as_deref());
    if let Some(cached) = cached_chart_data_at(&key, Instant::now(), CHART_CACHE_TTL) {
        return cached;
    }

    let fallback_provider_id = provider_id.clone();
    let cancel = register_chart_scan(&provider_id);
    let cancel_flag = cancel.clone();
    match tauri::async_runtime::spawn_blocking(move || {
        build_provider_chart_data_with_cancel(provider_id, account_email, Some(cancel))
    })
    .await
    {
        Ok(data) => {
            // Only memoize a scan that wasn't superseded by a newer one; a
            // cancelled scan returns partial data (local_usage = None) and the
            // superseding scan will populate the cache instead.
            if !cancel_flag.load(Ordering::Relaxed) {
                store_chart_data_at(&key, data.clone(), Instant::now());
            }
            data
        }
        Err(err) => {
            tracing::warn!("Provider chart data worker failed: {}", err);
            ProviderChartData::empty(fallback_provider_id)
        }
    }
}

/// How long a memoized [`ProviderChartData`] is served before a rescan. Short
/// enough that fresh local usage shows up quickly, long enough that repeated
/// opens (and the multiple per-open remounts) reuse one scan.
const CHART_CACHE_TTL: Duration = Duration::from_secs(60);

struct ChartCacheEntry {
    data: ProviderChartData,
    stored_at: Instant,
}

fn chart_cache() -> &'static Mutex<HashMap<String, ChartCacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<String, ChartCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Cache key scoping a bundle to a provider and (optionally) an account email;
/// credits/usage history depends on the email, so keys must distinguish them.
fn chart_cache_key(provider_id: &str, account_email: Option<&str>) -> String {
    // `\u{1}` can't appear in a provider id or email, so it's a safe separator.
    format!("{provider_id}\u{1}{}", account_email.unwrap_or(""))
}

fn cached_chart_data_at(key: &str, now: Instant, ttl: Duration) -> Option<ProviderChartData> {
    let cache = chart_cache().lock().ok()?;
    let entry = cache.get(key)?;
    (now.saturating_duration_since(entry.stored_at) < ttl).then(|| entry.data.clone())
}

fn store_chart_data_at(key: &str, data: ProviderChartData, now: Instant) {
    if let Ok(mut cache) = chart_cache().lock() {
        cache.insert(
            key.to_string(),
            ChartCacheEntry {
                data,
                stored_at: now,
            },
        );
    }
}

/// Drop all memoized chart data, forcing the next open to rescan. Called when
/// settings change so a new usage/cookie source is reflected immediately.
pub fn invalidate_chart_cache() {
    if let Ok(mut cache) = chart_cache().lock() {
        cache.clear();
    }
}

#[cfg(test)]
pub(crate) fn build_provider_chart_data(
    provider_id: String,
    account_email: Option<String>,
) -> ProviderChartData {
    build_provider_chart_data_with_cancel(provider_id, account_email, None)
}

fn build_provider_chart_data_with_cancel(
    provider_id: String,
    account_email: Option<String>,
    cancel: Option<Arc<AtomicBool>>,
) -> ProviderChartData {
    let raw_cost = get_daily_cost_history(&provider_id, 30);
    let cost_history: Vec<DailyCostPoint> = raw_cost
        .into_iter()
        .map(|(date, value)| DailyCostPoint { date, value })
        .collect();

    let (credits_history, usage_breakdown) =
        load_openai_dashboard_chart_data(&provider_id, account_email.as_deref());
    let local_usage = if cancel
        .as_deref()
        .is_some_and(|flag| flag.load(Ordering::Relaxed))
    {
        None
    } else {
        load_local_usage_summary(&provider_id, cancel.as_deref())
    };

    ProviderChartData {
        provider_id,
        cost_history,
        credits_history,
        usage_breakdown,
        local_usage,
    }
}

impl ProviderChartData {
    fn empty(provider_id: String) -> Self {
        Self {
            provider_id,
            cost_history: Vec::new(),
            credits_history: Vec::new(),
            usage_breakdown: Vec::new(),
            local_usage: None,
        }
    }
}

fn active_chart_scans() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static ACTIVE: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_chart_scan(provider_id: &str) -> Arc<AtomicBool> {
    let next = Arc::new(AtomicBool::new(false));
    if let Ok(mut active) = active_chart_scans().lock()
        && let Some(previous) = active.insert(provider_id.to_string(), next.clone())
    {
        previous.store(true, Ordering::Relaxed);
    }
    next
}

fn load_local_usage_summary(
    provider_id: &str,
    cancel: Option<&AtomicBool>,
) -> Option<ProviderLocalUsageSummary> {
    let thirty_day = scan_local_cost(provider_id, 30, cancel)?;
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return None;
    }
    let today = scan_local_cost(provider_id, 1, cancel).unwrap_or_default();

    let thirty_day_tokens = total_tokens(&thirty_day);
    let latest_tokens = total_tokens(&today);
    let has_usage =
        thirty_day.sessions_count > 0 || thirty_day.total_cost_usd > 0.0 || thirty_day_tokens > 0;
    if !has_usage {
        return None;
    }

    Some(ProviderLocalUsageSummary {
        today_cost: non_zero_f64(today.total_cost_usd),
        thirty_day_cost: non_zero_f64(thirty_day.total_cost_usd),
        thirty_day_tokens: non_zero_u64(thirty_day_tokens),
        latest_tokens: non_zero_u64(latest_tokens),
        top_model: top_model(&thirty_day),
        estimate_note: match provider_id {
            "claude" => "Estimated from local Claude logs at API rates; token totals may differ from your bill",
            _ => "Estimated from local logs; may differ from your bill",
        }
        .to_string(),
    })
}

fn scan_local_cost(
    provider_id: &str,
    days: u32,
    cancel: Option<&AtomicBool>,
) -> Option<CostSummary> {
    let scanner = CostScanner::new(days);
    match provider_id {
        "codex" => Some(scanner.scan_codex_with_cancel(cancel)),
        "claude" => Some(scanner.scan_claude_with_cancel(cancel)),
        _ => None,
    }
}

fn total_tokens(summary: &CostSummary) -> u64 {
    summary.input_tokens + summary.output_tokens
}

fn non_zero_f64(value: f64) -> Option<f64> {
    (value > 0.0).then_some(value)
}

fn non_zero_u64(value: u64) -> Option<u64> {
    (value > 0).then_some(value)
}

fn top_model(summary: &CostSummary) -> Option<String> {
    summary
        .by_model_tokens
        .iter()
        .max_by_key(|(_, counts)| counts.total())
        .map(|(model, _)| model.clone())
        .or_else(|| {
            summary
                .by_model
                .iter()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map(|(model, _)| model.clone())
        })
}

fn load_openai_dashboard_chart_data(
    provider_id: &str,
    account_email: Option<&str>,
) -> (Vec<DailyCostPoint>, Vec<DailyUsageBreakdown>) {
    if provider_id != "codex" && provider_id != "openai" {
        return (Vec::new(), Vec::new());
    }

    let Some(account_email) = account_email else {
        return (Vec::new(), Vec::new());
    };

    let Some(cache) = OpenAIDashboardCacheStore::load() else {
        return (Vec::new(), Vec::new());
    };

    if !cache.account_email.eq_ignore_ascii_case(account_email) {
        return (Vec::new(), Vec::new());
    }

    let snapshot = &cache.snapshot;

    let breakdown_source = if !snapshot.daily_breakdown.is_empty() {
        &snapshot.daily_breakdown
    } else if !snapshot.usage_breakdown.is_empty() {
        &snapshot.usage_breakdown
    } else {
        return (Vec::new(), Vec::new());
    };

    let credits_history: Vec<DailyCostPoint> = breakdown_source
        .iter()
        .map(|d| DailyCostPoint {
            date: d.day.clone(),
            value: d.total_credits_used,
        })
        .collect();

    let usage_breakdown: Vec<DailyUsageBreakdown> = snapshot
        .usage_breakdown
        .iter()
        .map(|d| DailyUsageBreakdown {
            day: d.day.clone(),
            services: d
                .services
                .iter()
                .map(|s| ServiceUsagePoint {
                    service: s.service.clone(),
                    credits_used: s.credits_used,
                })
                .collect(),
            total_credits_used: d.total_credits_used,
        })
        .collect();

    (credits_history, usage_breakdown)
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn cache_key_distinguishes_provider_and_account() {
        assert_ne!(
            chart_cache_key("codex", None),
            chart_cache_key("codex", Some("a@b.com"))
        );
        assert_ne!(
            chart_cache_key("codex", Some("a@b.com")),
            chart_cache_key("claude", Some("a@b.com"))
        );
        assert_eq!(
            chart_cache_key("codex", None),
            chart_cache_key("codex", None)
        );
    }

    #[test]
    fn cache_stores_serves_expires_and_invalidates() {
        // Unique key so this test owns its cache slot under parallel runs.
        let key = chart_cache_key("codex", Some("cache-test@example.com"));
        invalidate_chart_cache();

        let t0 = Instant::now();
        assert!(
            cached_chart_data_at(&key, t0, CHART_CACHE_TTL).is_none(),
            "empty cache should miss"
        );

        store_chart_data_at(&key, ProviderChartData::empty("codex".to_string()), t0);
        assert!(
            cached_chart_data_at(&key, t0, CHART_CACHE_TTL).is_some(),
            "a freshly stored bundle should be served"
        );

        // Past the TTL the entry is stale and must be rescanned.
        let later = t0 + CHART_CACHE_TTL + Duration::from_secs(1);
        assert!(
            cached_chart_data_at(&key, later, CHART_CACHE_TTL).is_none(),
            "expired entry should miss"
        );

        // Explicit invalidation (e.g. settings change) clears it.
        store_chart_data_at(
            &key,
            ProviderChartData::empty("codex".to_string()),
            Instant::now(),
        );
        invalidate_chart_cache();
        assert!(
            cached_chart_data_at(&key, Instant::now(), CHART_CACHE_TTL).is_none(),
            "invalidated cache should miss"
        );
    }
}
