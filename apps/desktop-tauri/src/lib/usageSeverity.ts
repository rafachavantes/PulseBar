/**
 * Canonical usage-severity classification, shared by every surface that
 * color-codes a usage percentage (tray panel cards, usage bars, float bar).
 *
 * The thresholds are expressed in percent **used** and mirror the Rust
 * `pulsebar::tray::severity` helper so all surfaces agree at a glance:
 *
 *   healthy  →  used <  high
 *   warn     →  high <= used <  critical
 *   critical →  used >= critical
 *
 * Defaults match the app's `high_usage_threshold` / `critical_usage_threshold`
 * settings (70 / 90). Callers that expose the configurable sliders (the float
 * bar) may pass the user's values; the panel cards use the canonical defaults.
 */

export type UsageSeverity = "normal" | "high" | "critical" | "exhausted";

/** Canonical high (warn) threshold, percent used. */
export const DEFAULT_HIGH_USED_PERCENT = 70;
/** Canonical critical threshold, percent used. */
export const DEFAULT_CRITICAL_USED_PERCENT = 90;

export function usageSeverity(
  usedPercent: number,
  exhausted = false,
  thresholds: { high?: number; critical?: number } = {},
): UsageSeverity {
  if (exhausted) return "exhausted";
  const high = thresholds.high ?? DEFAULT_HIGH_USED_PERCENT;
  const critical = thresholds.critical ?? DEFAULT_CRITICAL_USED_PERCENT;
  const used = Number.isFinite(usedPercent) ? usedPercent : 0;
  if (used >= critical) return "critical";
  if (used >= high) return "high";
  return "normal";
}
