/**
 * Demo provider snapshots for visual proof/comparison screenshots.
 *
 * These mimic realistic usage data across multiple providers so the tray
 * panel can be compared against a fully-populated macOS CodexBar panel.
 * Only used when `VITE_DEMO_PROVIDERS=1` is set at build time.
 */
import type { ProviderUsageSnapshot, RateWindowSnapshot } from "../types/bridge";

export const DEMO_ENABLED = import.meta.env.VITE_DEMO_PROVIDERS === "1";

const now = new Date().toISOString();

function demoProviderLimit(): number | null {
  const raw = import.meta.env.VITE_DEMO_PROVIDER_LIMIT;
  if (raw == null || raw === "") return null;
  const parsed = Number(raw);
  if (!Number.isFinite(parsed) || parsed <= 0) return null;
  return Math.floor(parsed);
}

function makeGridProvider(id: string, name: string, usedPct: number): ProviderUsageSnapshot {
  return {
    providerId: id,
    displayName: name,
    primaryLabel: "Monthly",
    primary: {
      usedPercent: usedPct,
      remainingPercent: 100 - usedPct,
      windowMinutes: null,
      resetsAt: null,
      resetDescription: `${Math.floor(Math.random() * 28 + 1)}d`,
      isExhausted: false,
      reservePercent: null,
      reserveDescription: null,
    },
    secondary: null,
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [],
    cost: null,
    planName: "Free",
    accountEmail: null,
    sourceLabel: "CLI",
    updatedAt: now,
    error: null,
    pace: null,
    accountOrganization: null,
    trayStatusLabel: null,
  };
}

const ALL_DEMO_PROVIDERS: ProviderUsageSnapshot[] = [
  // ── Featured providers (full card data) ──
  {
    providerId: "codex",
    displayName: "Codex",
    primaryLabel: "Session",
    primary: {
      usedPercent: 0,
      remainingPercent: 100,
      windowMinutes: 300,
      resetsAt: null,
      resetDescription: "5h",
      isExhausted: false,
      reservePercent: null,
      reserveDescription: null,
    },
    secondaryLabel: "Weekly",
    secondary: {
      usedPercent: 59,
      remainingPercent: 41,
      windowMinutes: null,
      resetsAt: null,
      resetDescription: "12h 41m",
      isExhausted: false,
      reservePercent: 33,
      reserveDescription: "Lasts until reset",
    },
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [],
    cost: null,
    planName: "Team",
    accountEmail: "demo.user@example.com",
    sourceLabel: "CLI",
    updatedAt: now,
    error: null,
    pace: null,
    accountOrganization: null,
    trayStatusLabel: null,
  },
  {
    providerId: "claude",
    displayName: "Claude",
    primaryLabel: "Session",
    primary: {
      usedPercent: 72,
      remainingPercent: 28,
      windowMinutes: 300,
      resetsAt: null,
      resetDescription: "1h 23m",
      isExhausted: false,
      reservePercent: null,
      reserveDescription: null,
    },
    secondaryLabel: "Weekly",
    secondary: {
      usedPercent: 45,
      remainingPercent: 55,
      windowMinutes: null,
      resetsAt: null,
      resetDescription: "4d 12h",
      isExhausted: false,
      reservePercent: 33,
      reserveDescription: "Lasts until reset",
    },
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [],
    cost: null,
    planName: "Pro",
    accountEmail: null,
    sourceLabel: "Cookie",
    updatedAt: now,
    error: null,
    pace: null,
    accountOrganization: null,
    trayStatusLabel: null,
  },
  // ── Grid-only providers (fill out icon grid) ──
  makeGridProvider("gemini", "Gemini", 35),
  makeGridProvider("zai", "z.ai", 50),
  makeGridProvider("grok", "Grok", 70),
  makeGridProvider("synthetic", "Synthetic", 64),
];

const limit = demoProviderLimit();
export const DEMO_PROVIDERS: ProviderUsageSnapshot[] =
  limit == null ? ALL_DEMO_PROVIDERS : ALL_DEMO_PROVIDERS.slice(0, limit);
