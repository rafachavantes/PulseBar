//! Canonical usage-severity classification shared by every surface that
//! colour-codes a usage percentage.
//!
//! Thresholds are expressed in percent **used** and mirror the TypeScript
//! `usageSeverity` helper (`apps/desktop-tauri/src/lib/usageSeverity.ts`) so
//! the tray icon, panel cards, and float bar all agree at a glance:
//!
//! ```text
//! healthy  ->  used <  high
//! warn     ->  high <= used <  critical
//! critical ->  used >= critical
//! ```
//!
//! The colours are drawn from the app's terminal dev-core tokens so the tray
//! icon matches the in-app usage bars (phosphor green / amber / orange-red).

/// Canonical high (warn) threshold, percent used. Matches the default
/// `Settings::high_usage_threshold`.
pub const DEFAULT_HIGH_USED_PERCENT: f64 = 70.0;
/// Canonical critical threshold, percent used. Matches the default
/// `Settings::critical_usage_threshold`.
pub const DEFAULT_CRITICAL_USED_PERCENT: f64 = 90.0;

/// A usage state, ordered from healthiest to most severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Below the high threshold — phosphor green.
    Healthy,
    /// At or above the high threshold — amber.
    Warn,
    /// At or above the critical threshold — orange-red.
    Critical,
}

impl Severity {
    /// Classify a **used** percentage against explicit high/critical cut-offs.
    ///
    /// Colour must always be derived from the used percentage — never a
    /// "remaining" display value — so a healthy account never renders as
    /// critical when the tray is set to show remaining capacity.
    pub fn from_used_percent(used_percent: f64, high: f64, critical: f64) -> Self {
        let used = if used_percent.is_finite() {
            used_percent
        } else {
            0.0
        };
        if used >= critical {
            Severity::Critical
        } else if used >= high {
            Severity::Warn
        } else {
            Severity::Healthy
        }
    }

    /// Classify against the canonical 70/90 defaults.
    pub fn from_used_percent_default(used_percent: f64) -> Self {
        Self::from_used_percent(
            used_percent,
            DEFAULT_HIGH_USED_PERCENT,
            DEFAULT_CRITICAL_USED_PERCENT,
        )
    }

    /// RGB fill colour, aligned with the in-app `--usage-bar-*` design tokens.
    pub fn color(&self) -> (u8, u8, u8) {
        match self {
            Severity::Healthy => (53, 224, 138),  // #35E08A phosphor green
            Severity::Warn => (245, 177, 76),     // #F5B14C amber
            Severity::Critical => (255, 138, 61), // #FF8A3D orange-red
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Boundary table shared conceptually with usageSeverity.test.ts.
    const CASES: &[(f64, Severity)] = &[
        (0.0, Severity::Healthy),
        (55.0, Severity::Healthy),
        (69.9, Severity::Healthy),
        (70.0, Severity::Warn),
        (80.0, Severity::Warn),
        (89.9, Severity::Warn),
        (90.0, Severity::Critical),
        (100.0, Severity::Critical),
    ];

    #[test]
    fn classifies_used_percent_against_canonical_thresholds() {
        for (used, expected) in CASES {
            assert_eq!(
                Severity::from_used_percent_default(*used),
                *expected,
                "used={used}"
            );
        }
    }

    #[test]
    fn honors_explicit_thresholds() {
        assert_eq!(
            Severity::from_used_percent(60.0, 50.0, 80.0),
            Severity::Warn
        );
        assert_eq!(
            Severity::from_used_percent(85.0, 50.0, 80.0),
            Severity::Critical
        );
        assert_eq!(
            Severity::from_used_percent(40.0, 50.0, 80.0),
            Severity::Healthy
        );
    }

    #[test]
    fn non_finite_is_treated_as_zero() {
        assert_eq!(
            Severity::from_used_percent_default(f64::NAN),
            Severity::Healthy
        );
    }

    #[test]
    fn colors_are_distinct_per_level() {
        assert_ne!(Severity::Healthy.color(), Severity::Warn.color());
        assert_ne!(Severity::Warn.color(), Severity::Critical.color());
        // Healthy should be green-dominant; critical should be red-dominant.
        let (hr, hg, hb) = Severity::Healthy.color();
        assert!(hg > hr && hg > hb);
        let (cr, cg, cb) = Severity::Critical.color();
        assert!(cr > cg && cr > cb);
    }
}
