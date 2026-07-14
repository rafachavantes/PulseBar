//! Status page polling for AI providers
//!
//! Fetches operational status from provider status pages

#![allow(dead_code)]
#![allow(unused_imports)]

pub mod indicators;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export indicator types for convenience
pub use indicators::{
    OverlayPosition, ProviderStatus as IndicatorProviderStatus,
    StatusLevel as IndicatorStatusLevel, StatusOverlayConfig, StatuspageIncident,
    StatuspageResponse, StatuspageStatus,
};

/// Status level for a provider
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StatusLevel {
    /// All systems operational
    Operational,
    /// Degraded performance
    Degraded,
    /// Partial outage
    Partial,
    /// Major outage
    Major,
    /// Unknown status
    #[default]
    Unknown,
}

impl StatusLevel {
    /// Create from a string indicator
    pub fn from_indicator(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "operational" | "none" | "green" | "ok" => StatusLevel::Operational,
            "degraded" | "degraded_performance" | "yellow" => StatusLevel::Degraded,
            "partial" | "partial_outage" | "orange" => StatusLevel::Partial,
            "major" | "major_outage" | "critical" | "red" => StatusLevel::Major,
            _ => StatusLevel::Unknown,
        }
    }

    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            StatusLevel::Operational => "All Systems Operational",
            StatusLevel::Degraded => "Degraded Performance",
            StatusLevel::Partial => "Partial Outage",
            StatusLevel::Major => "Major Outage",
            StatusLevel::Unknown => "Status Unknown",
        }
    }

    /// Severity rank used for component roll-up comparisons. Higher is worse.
    ///
    /// This is deliberately NOT the raw enum discriminant: the `Unknown`
    /// variant is declared last (so it stays the serde/`Default` value) but a
    /// real outage must always outrank an unrecognized/maintenance status. We
    /// therefore rank `Unknown` just above `Operational` — it surfaces over
    /// "all clear" but can never hide a `Major`/`Partial`/`Degraded` outage.
    pub fn severity(&self) -> u8 {
        match self {
            StatusLevel::Operational => 0,
            StatusLevel::Unknown => 1,
            StatusLevel::Degraded => 2,
            StatusLevel::Partial => 3,
            StatusLevel::Major => 4,
        }
    }
}

/// Provider status information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderStatus {
    pub level: StatusLevel,
    pub description: String,
    pub last_updated: Option<String>,
    pub components: Vec<ComponentStatus>,
}

/// Individual component status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub name: String,
    pub status: StatusLevel,
}

/// Status page URLs for known providers
pub fn get_status_page_url(provider: &str) -> Option<&'static str> {
    match provider.to_lowercase().as_str() {
        "claude" | "anthropic" => Some("https://status.anthropic.com"),
        "codex" | "openai" => Some("https://status.openai.com"),
        "gemini" | "google" => Some("https://status.cloud.google.com"),
        "zai" | "z.ai" => None, // z.ai doesn't have a public status page
        _ => None,
    }
}

/// Fetch status from a Statuspage.io-based status page
pub async fn fetch_statuspage_io(url: &str) -> Result<ProviderStatus, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    // Statuspage.io API endpoint
    let api_url = format!("{}/api/v2/status.json", url.trim_end_matches('/'));

    let resp = client
        .get(&api_url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    // Parse Statuspage.io format
    let status = json
        .get("status")
        .and_then(|s| s.get("indicator"))
        .and_then(|i| i.as_str())
        .map(StatusLevel::from_indicator)
        .unwrap_or(StatusLevel::Unknown);

    let description = json
        .get("status")
        .and_then(|s| s.get("description"))
        .and_then(|d| d.as_str())
        .unwrap_or("Unknown")
        .to_string();

    let last_updated = json
        .get("page")
        .and_then(|p| p.get("updated_at"))
        .and_then(|u| u.as_str())
        .map(|s| s.to_string());

    Ok(ProviderStatus {
        level: status,
        description,
        last_updated,
        components: Vec::new(),
    })
}

/// Fetch status with components from a Statuspage.io-based status page
pub async fn fetch_statuspage_io_components(url: &str) -> Result<ProviderStatus, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    // Statuspage.io components endpoint
    let api_url = format!("{}/api/v2/components.json", url.trim_end_matches('/'));

    let resp = client
        .get(&api_url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let mut components = Vec::new();

    if let Some(comps) = json.get("components").and_then(|c| c.as_array()) {
        for comp in comps {
            let name = comp
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("Unknown");
            let status_str = comp
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            let status = StatusLevel::from_indicator(status_str);

            components.push(ComponentStatus {
                name: name.to_string(),
                status,
            });
        }
    }

    // Roll up to the worst component by SEVERITY, not by raw enum discriminant,
    // otherwise an unrecognized/maintenance status (mapped to `Unknown`) would
    // outrank a real `Major` outage.
    let overall_status = rollup_status(
        &components
            .iter()
            .map(|component| component.status)
            .collect::<Vec<_>>(),
    );

    Ok(ProviderStatus {
        level: overall_status,
        description: overall_status.description().to_string(),
        last_updated: None,
        components,
    })
}

/// Fetch status for a specific provider
pub async fn fetch_provider_status(provider: &str) -> Option<ProviderStatus> {
    let url = get_status_page_url(provider)?;

    // Try the simple status endpoint first
    match fetch_statuspage_io(url).await {
        Ok(status) => Some(status),
        Err(_) => {
            // Fall back to components endpoint
            fetch_statuspage_io_components(url).await.ok()
        }
    }
}

/// Fetch status for all providers in parallel
pub async fn fetch_all_statuses(providers: &[&str]) -> HashMap<String, ProviderStatus> {
    let futures: Vec<_> = providers
        .iter()
        .map(|&p| async move {
            let status = fetch_provider_status(p).await;
            (p.to_string(), status)
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    results
        .into_iter()
        .filter_map(|(provider, status)| status.map(|s| (provider, s)))
        .collect()
}

/// Roll up a set of component statuses to the single worst level, mirroring the
/// logic in [`fetch_statuspage_io_components`]. Extracted so the severity
/// ordering is unit-testable without a live HTTP fetch.
fn rollup_status(components: &[StatusLevel]) -> StatusLevel {
    let mut overall = StatusLevel::Operational;
    for status in components {
        if status.severity() > overall.severity() {
            overall = *status;
        }
    }
    overall
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_ranks_major_above_unknown() {
        // The core bug: `Unknown` must never outrank a real outage.
        assert!(StatusLevel::Major.severity() > StatusLevel::Unknown.severity());
        assert!(StatusLevel::Partial.severity() > StatusLevel::Unknown.severity());
        assert!(StatusLevel::Degraded.severity() > StatusLevel::Unknown.severity());
        // Unknown still surfaces over "all clear".
        assert!(StatusLevel::Unknown.severity() > StatusLevel::Operational.severity());
    }

    #[test]
    fn severity_is_monotonic_for_real_outages() {
        assert!(StatusLevel::Degraded.severity() < StatusLevel::Partial.severity());
        assert!(StatusLevel::Partial.severity() < StatusLevel::Major.severity());
    }

    #[test]
    fn rollup_keeps_major_when_a_later_component_is_unknown() {
        // A `Major` outage followed by an `under_maintenance` (→ Unknown)
        // component must remain `Major`, not flip to `Status Unknown`.
        let maintenance = StatusLevel::from_indicator("under_maintenance");
        assert_eq!(maintenance, StatusLevel::Unknown);
        assert_eq!(
            rollup_status(&[StatusLevel::Major, maintenance]),
            StatusLevel::Major
        );
        assert_eq!(
            rollup_status(&[maintenance, StatusLevel::Major]),
            StatusLevel::Major
        );
    }

    #[test]
    fn rollup_all_operational_stays_operational() {
        assert_eq!(
            rollup_status(&[StatusLevel::Operational, StatusLevel::Operational]),
            StatusLevel::Operational
        );
    }

    #[test]
    fn rollup_unknown_over_operational_surfaces_unknown() {
        assert_eq!(
            rollup_status(&[StatusLevel::Operational, StatusLevel::Unknown]),
            StatusLevel::Unknown
        );
    }
}
