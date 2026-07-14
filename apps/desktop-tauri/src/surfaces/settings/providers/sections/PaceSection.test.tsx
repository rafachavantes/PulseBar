import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { PaceSection } from "./PaceSection";
import type { LocaleKey } from "../../../../i18n/keys";
import type { PaceSnapshot } from "../../../../types/bridge";

// Mirror the real EN templates: the value is substituted for `{}`.
const TEMPLATES: Partial<Record<LocaleKey, string>> = {
  DetailPaceTitle: "Pace",
  DetailPaceBehind: "Behind",
  DetailPaceRunsOutIn: "Runs out in ~{}h",
  DetailPaceRunsOutInMinutes: "Runs out in ~{}m",
  DetailPaceWillLastToReset: "Lasts until reset",
};

const t = (key: LocaleKey): string => TEMPLATES[key] ?? key;

function pace(overrides: Partial<PaceSnapshot> = {}): PaceSnapshot {
  return {
    stage: "behind",
    deltaPercent: 12,
    willLastToReset: false,
    etaSeconds: null,
    expectedUsedPercent: 50,
    actualUsedPercent: 62,
    ...overrides,
  };
}

describe("PaceSection pace row", () => {
  it("substitutes the hour value for a multi-hour ETA and leaves no placeholder", () => {
    const { container } = render(
      <PaceSection pace={pace({ etaSeconds: 3 * 3600 })} t={t} />,
    );
    const aux = container.querySelector(".provider-detail-pace__aux");
    expect(aux?.textContent).toBe("Runs out in ~3h");
    expect(aux?.textContent).not.toContain("{}");
  });

  it("substitutes the minute value for a sub-hour ETA and leaves no placeholder", () => {
    const { container } = render(
      <PaceSection pace={pace({ etaSeconds: 25 * 60 })} t={t} />,
    );
    const aux = container.querySelector(".provider-detail-pace__aux");
    expect(aux?.textContent).toBe("Runs out in ~25m");
    expect(aux?.textContent).not.toContain("{}");
  });

  it("shows the lasts-until-reset copy when the window will survive", () => {
    const { container } = render(
      <PaceSection
        pace={pace({ willLastToReset: true, etaSeconds: null })}
        t={t}
      />,
    );
    const aux = container.querySelector(".provider-detail-pace__aux");
    expect(aux?.textContent).toBe("Lasts until reset");
  });
});
