//! Rate window model - represents a usage limit window (e.g., 5-hour session, 7-day weekly)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a rate limit window with usage percentage and reset time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateWindow {
    /// Percentage of the window that has been used (0-100)
    pub used_percent: f64,

    /// Duration of the window in minutes (e.g., 300 for 5-hour, 10080 for 7-day)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_minutes: Option<u32>,

    /// When the window resets
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<DateTime<Utc>>,

    /// Human-readable reset description (e.g., "Jan 15 at 3:00pm")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_description: Option<String>,
}

/// A window at least this long is a weekly quota rather than a session one.
pub const WEEKLY_WINDOW_MINUTES: u32 = 7 * 24 * 60;

/// Label for a window slot, corrected against the duration the provider
/// actually reported. Codex moved its account-wide quota from a 5-hour window
/// to a weekly one while still sending it in the primary slot, which used to
/// render weekly numbers under a "Session" heading.
pub fn window_label(default_label: &'static str, window: &RateWindow) -> &'static str {
    if window.is_weekly() && default_label.starts_with("Session") {
        return "Weekly";
    }
    default_label
}

impl RateWindow {
    /// Whether this window spans a week or more.
    pub fn is_weekly(&self) -> bool {
        self.window_minutes
            .is_some_and(|minutes| minutes >= WEEKLY_WINDOW_MINUTES)
    }

    /// Whether the window carries no information at all. Providers sometimes
    /// send a placeholder window (Codex does for its secondary slot) that would
    /// otherwise render as a real "0% used" quota.
    pub fn is_empty(&self) -> bool {
        self.used_percent == 0.0
            && self.window_minutes.is_none()
            && self.resets_at.is_none()
            && self.reset_description.is_none()
    }

    /// Create a new rate window
    pub fn new(used_percent: f64) -> Self {
        Self {
            used_percent: used_percent.clamp(0.0, 100.0),
            window_minutes: None,
            resets_at: None,
            reset_description: None,
        }
    }

    /// Create a rate window with full details
    pub fn with_details(
        used_percent: f64,
        window_minutes: Option<u32>,
        resets_at: Option<DateTime<Utc>>,
        reset_description: Option<String>,
    ) -> Self {
        Self {
            used_percent: used_percent.clamp(0.0, 100.0),
            window_minutes,
            resets_at,
            reset_description,
        }
    }

    /// Get the remaining percentage (100 - used)
    pub fn remaining_percent(&self) -> f64 {
        100.0 - self.used_percent
    }

    /// Check if the window is exhausted (>= 100% used)
    pub fn is_exhausted(&self) -> bool {
        self.used_percent >= 100.0
    }

    /// Check if the window is nearly exhausted (>= 90% used)
    pub fn is_nearly_exhausted(&self) -> bool {
        self.used_percent >= 90.0
    }

    /// Format the reset time as a countdown string
    pub fn format_countdown(&self) -> Option<String> {
        let resets_at = self.resets_at?;
        let now = Utc::now();

        if resets_at <= now {
            return Some("now".to_string());
        }

        let duration = resets_at - now;
        let hours = duration.num_hours();
        let minutes = duration.num_minutes() % 60;

        if hours > 24 {
            let days = hours / 24;
            Some(format!("{}d {}h", days, hours % 24))
        } else if hours > 0 {
            Some(format!("{}h {}m", hours, minutes))
        } else {
            Some(format!("{}m", minutes))
        }
    }
}

impl Default for RateWindow {
    fn default() -> Self {
        Self::new(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remaining_percent() {
        let window = RateWindow::new(75.0);
        assert!((window.remaining_percent() - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_clamping() {
        let window = RateWindow::new(150.0);
        assert!((window.used_percent - 100.0).abs() < f64::EPSILON);

        let window = RateWindow::new(-10.0);
        assert!(window.used_percent.abs() < f64::EPSILON);
    }

    #[test]
    fn test_exhausted() {
        assert!(RateWindow::new(100.0).is_exhausted());
        assert!(!RateWindow::new(99.0).is_exhausted());
    }

    #[test]
    fn a_window_of_seven_days_or_more_is_weekly() {
        let mut window = RateWindow::new(48.0);
        window.window_minutes = Some(10_080);
        assert!(window.is_weekly());

        window.window_minutes = Some(300);
        assert!(!window.is_weekly());

        window.window_minutes = None;
        assert!(!window.is_weekly());
    }

    #[test]
    fn a_window_without_duration_reset_or_usage_is_empty() {
        // Codex sends a placeholder secondary window shaped like this.
        assert!(RateWindow::new(0.0).is_empty());
    }

    #[test]
    fn a_window_carrying_any_signal_is_not_empty() {
        assert!(!RateWindow::new(1.0).is_empty());

        let mut with_duration = RateWindow::new(0.0);
        with_duration.window_minutes = Some(10_080);
        assert!(!with_duration.is_empty());

        let mut with_reset = RateWindow::new(0.0);
        with_reset.resets_at = Some(Utc::now());
        assert!(!with_reset.is_empty());
    }

    #[test]
    fn weekly_windows_relabel_a_session_slot() {
        let mut weekly = RateWindow::new(48.0);
        weekly.window_minutes = Some(10_080);
        assert_eq!(window_label("Session", &weekly), "Weekly");

        let mut session = RateWindow::new(48.0);
        session.window_minutes = Some(300);
        assert_eq!(window_label("Session", &session), "Session");
    }

    #[test]
    fn a_label_that_already_matches_the_duration_is_left_alone() {
        let mut weekly = RateWindow::new(10.0);
        weekly.window_minutes = Some(10_080);
        // Grok reports a monthly window; do not rewrite it to "Weekly".
        assert_eq!(window_label("Monthly", &weekly), "Monthly");
        assert_eq!(window_label("Weekly", &weekly), "Weekly");
    }
}
