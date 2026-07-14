import type { RateWindowSnapshot } from "../../types/bridge";
import { usageSeverity } from "../../lib/usageSeverity";

interface UsageBarProps {
  window: RateWindowSnapshot;
  label?: string;
  compact?: boolean;
}

export default function UsageBar({ window: w, label, compact }: UsageBarProps) {
  const rawPct = Number.isFinite(w.usedPercent) ? Math.max(0, w.usedPercent) : 0;
  const pct = Math.min(100, rawPct);
  const level = usageSeverity(rawPct, w.isExhausted);

  return (
    <div className={`usage-bar ${compact ? "usage-bar--compact" : ""}`}>
      {label && <span className="usage-bar__label">{label}</span>}
      <div className="usage-bar__track">
        <div
          className="usage-bar__fill"
          data-level={level}
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className="usage-bar__pct">{rawPct.toFixed(0)}%</span>
    </div>
  );
}
