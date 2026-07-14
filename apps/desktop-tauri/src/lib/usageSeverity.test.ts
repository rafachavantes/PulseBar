import { describe, expect, it } from "vitest";
import {
  DEFAULT_CRITICAL_USED_PERCENT,
  DEFAULT_HIGH_USED_PERCENT,
  usageSeverity,
} from "./usageSeverity";

// Shared fixture table — the Rust `severity` helper is exercised against the
// same boundary cases so the two languages cannot drift apart.
const CASES: Array<[number, string]> = [
  [0, "normal"],
  [55, "normal"],
  [69.9, "normal"],
  [70, "high"],
  [80, "high"],
  [89.9, "high"],
  [90, "critical"],
  [100, "critical"],
];

describe("usageSeverity", () => {
  it("classifies used percent against the canonical 70/90 thresholds", () => {
    for (const [used, expected] of CASES) {
      expect(usageSeverity(used)).toBe(expected);
    }
  });

  it("exposes canonical default thresholds", () => {
    expect(DEFAULT_HIGH_USED_PERCENT).toBe(70);
    expect(DEFAULT_CRITICAL_USED_PERCENT).toBe(90);
  });

  it("prioritizes the exhausted flag over the numeric level", () => {
    expect(usageSeverity(5, true)).toBe("exhausted");
    expect(usageSeverity(100, true)).toBe("exhausted");
  });

  it("honors caller-supplied thresholds (float bar sliders)", () => {
    expect(usageSeverity(60, false, { high: 50, critical: 80 })).toBe("high");
    expect(usageSeverity(85, false, { high: 50, critical: 80 })).toBe(
      "critical",
    );
    expect(usageSeverity(40, false, { high: 50, critical: 80 })).toBe("normal");
  });

  it("treats non-finite input as zero usage", () => {
    expect(usageSeverity(Number.NaN)).toBe("normal");
  });
});
