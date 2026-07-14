import type { LocaleKey } from "../i18n/keys";

/**
 * Format the pace "runs out in" ETA with its numeric value substituted into the
 * localized template. Sub-hour ETAs read as minutes
 * (`DetailPaceRunsOutInMinutes`, e.g. "~25m") instead of collapsing to a bare
 * "~0h"; everything else uses hours (`DetailPaceRunsOutIn`).
 *
 * The `{}` placeholder can appear mid-string (the ZH template is
 * "预计约 {} 小时后用完"), so `.replace("{}", …)` handles any position — do not
 * assume it is a suffix. Shared by `MenuCard` and the provider-detail
 * `PaceSection` so the two call sites cannot drift.
 */
export function formatPaceEta(
  t: (key: LocaleKey) => string,
  etaSeconds: number,
): string {
  if (etaSeconds < 3600) {
    const minutes = Math.max(1, Math.round(etaSeconds / 60));
    return t("DetailPaceRunsOutInMinutes").replace("{}", String(minutes));
  }
  const hours = Math.max(1, Math.round(etaSeconds / 3600));
  return t("DetailPaceRunsOutIn").replace("{}", String(hours));
}
