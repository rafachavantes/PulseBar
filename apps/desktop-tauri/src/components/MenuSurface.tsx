import type { ReactNode } from "react";
import { useLocale } from "../hooks/useLocale";
import { ProviderIcon } from "./providers/ProviderIcon";
import pulsebarLogo from "../assets/pulsebar-logo.svg?raw";

/** Providers surfaced as brand marks in the first-run empty state, in the
 *  same order as the caption ("Codex, Claude, Gemini, z.ai, Grok"). */
const EMPTY_STATE_PROVIDER_MARKS = ["codex", "claude", "gemini", "zai", "grok", "opencodego"];

export interface MenuSurfaceAction {
  icon: string;
  title: string;
  onClick: () => void;
}

export interface MenuFooterRow {
  icon: string;
  label: string;
  shortcut?: string;
  onClick: () => void;
}

interface MenuSurfaceProps {
  variant: "tray" | "popout";
  onRefresh: () => void;
  isRefreshing: boolean;
  actions: MenuSurfaceAction[];
  summary?: ReactNode;
  banner?: ReactNode;
  footerRows?: MenuFooterRow[];
  /** Optional subtle hint rendered beneath the footer rows (e.g. the global
   *  summon shortcut). */
  footerHint?: ReactNode;
  children: ReactNode;
}

/**
 * Flush, compact container that both `TrayPanel` and `PopOutPanel` consume.
 *
 * Mirrors the upstream macOS `MenuContent`: a narrow VStack(spacing: 8)
 * inside an NSMenu-like popover (310pt wide, vertical 6 / horizontal 10
 * padding, no hero framing). The body holds a stack of full provider
 * cards (`MenuCard`) — one per enabled provider — exactly like upstream.
 */
export default function MenuSurface({
  variant,
  onRefresh,
  isRefreshing,
  actions,
  summary,
  banner,
  footerRows,
  footerHint,
  children,
}: MenuSurfaceProps) {
  return (
    <div className={`menu-surface menu-surface--${variant}`}>
      {banner}
      {summary}
      <div className="menu-surface__body">{children}</div>
      {footerRows && footerRows.length > 0 && (
        <nav className="menu-surface__footer" aria-label="Menu">
          {footerRows.map((row) => (
            <button
              key={row.label}
              type="button"
              className={`menu-surface__footer-row${row.icon ? "" : " menu-surface__footer-row--no-icon"}`}
              onClick={row.onClick}
            >
              {row.icon && (
                <span className="menu-surface__footer-icon" aria-hidden>
                  {row.icon}
                </span>
              )}
              <span>{row.label}</span>
              {row.shortcut && (
                <span className="menu-surface__footer-shortcut">{row.shortcut}</span>
              )}
            </button>
          ))}
        </nav>
      )}
      {footerHint}
    </div>
  );
}

interface MenuSummaryProps {
  total: number;
  errorCount: number;
  isRefreshing: boolean;
  lastRefresh: { providerCount: number; errorCount: number } | null;
}

export function MenuSummary({
  total,
  errorCount,
  isRefreshing,
  lastRefresh,
}: MenuSummaryProps) {
  const { t } = useLocale();
  const providersLabel = t("SummaryProvidersLabel");
  const providerLabel =
    total === 1 && providersLabel.toLocaleLowerCase("en-US") === "providers"
      ? "provider"
      : providersLabel;
  const parts: string[] = [`${total} ${providerLabel}`];
  if (isRefreshing) {
    parts.push(t("SummaryRefreshing"));
  } else if (lastRefresh && lastRefresh.errorCount > 0) {
    parts.push(`${lastRefresh.errorCount} ${t("SummaryFailed")}`);
  }
  if (!isRefreshing && errorCount > 0) {
    parts.push(`${errorCount} ${t("SummaryWithErrors")}`);
  }
  return <div className="menu-surface__summary">{parts.join(" · ")}</div>;
}

interface MenuEmptyProps {
  isLoading: boolean;
  onSettings: () => void;
  /** First-run CTA — opens Settings on the Providers tab (see item 2). Falls
   *  back to `onSettings` when not supplied. */
  onConfigureProviders?: () => void;
  onRefresh?: () => void;
  mode?: "noProviders" | "noData";
}

export function MenuEmpty({
  isLoading,
  onSettings,
  onConfigureProviders,
  onRefresh,
  mode = "noProviders",
}: MenuEmptyProps) {
  const { t } = useLocale();

  if (isLoading) {
    return (
      <div className="menu-surface__empty">
        <div className="menu-surface__spinner" />
        <p>{t("FetchingProviderData")}</p>
      </div>
    );
  }

  if (mode === "noData") {
    return (
      <div className="menu-surface__empty">
        <p>{t("StateNoProviderData")}</p>
        {onRefresh && (
          <button
            className="menu-surface__primary-btn"
            onClick={onRefresh}
            type="button"
          >
            {t("TooltipRefresh")}
          </button>
        )}
      </div>
    );
  }

  return (
    <div className="menu-surface__empty menu-surface__empty--onboarding">
      <span
        className="menu-surface__empty-logo"
        aria-hidden
        // eslint-disable-next-line react/no-danger -- bundled local SVG, no user input.
        dangerouslySetInnerHTML={{ __html: pulsebarLogo }}
      />
      <h3 className="menu-surface__empty-headline">{t("EmptyStateHeadline")}</h3>
      <div className="menu-surface__empty-marks" aria-hidden>
        {EMPTY_STATE_PROVIDER_MARKS.map((id) => (
          <ProviderIcon key={id} providerId={id} size={22} />
        ))}
      </div>
      <p className="menu-surface__empty-caption">
        {t("EmptyStateProvidersCaption")}
      </p>
      <button
        className="menu-surface__primary-btn"
        onClick={onConfigureProviders ?? onSettings}
        type="button"
      >
        {t("OpenSettingsButton")}
      </button>
    </div>
  );
}
