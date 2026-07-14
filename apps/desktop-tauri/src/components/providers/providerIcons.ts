// Ported from rust/src/native_ui/provider_icons.rs and
// rust/src/native_ui/theme.rs::{provider_color, provider_icon}.
// Keep in sync with the Rust registries when new providers are added.

import claude from "./icons/ProviderIcon-claude.svg?raw";
import codex from "./icons/ProviderIcon-codex.svg?raw";
import gemini from "./icons/ProviderIcon-gemini.svg?raw";
import grok from "./icons/ProviderIcon-grok.svg?raw";
import opencodego from "./icons/ProviderIcon-opencodego.svg?raw";
import synthetic from "./icons/ProviderIcon-synthetic.svg?raw";
import zai from "./icons/ProviderIcon-zai.svg?raw";

/**
 * Replace hard-coded fills/strokes in the bundled brand SVGs with
 * `currentColor` so the icon picks up the brand color via CSS, making each
 * provider visually distinct in compact tray rows.
 */
function tint(raw: string): string {
  return raw
    .replace(/fill="white"/gi, 'fill="currentColor"')
    .replace(/fill="#fff"/gi, 'fill="currentColor"')
    .replace(/fill="#ffffff"/gi, 'fill="currentColor"')
    .replace(/stroke="white"/gi, 'stroke="currentColor"');
}

export interface ProviderIcon {
  /** CLI-style provider id (lowercase, normalized). */
  id: string;
  /** Brand hex color. */
  brandColor: string;
  /** Single-character fallback used when no SVG is available. */
  fallbackLetter: string;
  /** Raw SVG markup when the provider ships a brand asset. */
  svgPath?: string;
}

const RAW: Record<string, string> = {
  claude: tint(claude),
  codex: tint(codex),
  gemini: tint(gemini),
  grok: tint(grok),
  opencodego: tint(opencodego),
  synthetic: tint(synthetic),
  zai: tint(zai),
};

/**
 * Registry of provider icons. Matches the entries in
 * `rust/src/native_ui/provider_icons.rs` and pulls brand colors / fallback
 * letters from `rust/src/native_ui/theme.rs::{provider_color, provider_icon}`.
 */
export const PROVIDER_ICON_REGISTRY: Record<string, ProviderIcon> = {
  claude: { id: "claude", brandColor: "#cc7c5e", fallbackLetter: "◈", svgPath: RAW.claude },
  codex: { id: "codex", brandColor: "#49a3b0", fallbackLetter: "◆", svgPath: RAW.codex },
  gemini: { id: "gemini", brandColor: "#ab87ea", fallbackLetter: "✦", svgPath: RAW.gemini },
  grok: { id: "grok", brandColor: "#111827", fallbackLetter: "G", svgPath: RAW.grok },
  opencodego: { id: "opencodego", brandColor: "#3b82f6", fallbackLetter: "○", svgPath: RAW.opencodego },
  synthetic: { id: "synthetic", brandColor: "#141414", fallbackLetter: "◇", svgPath: RAW.synthetic },
  zai: { id: "zai", brandColor: "#e85a6a", fallbackLetter: "Z", svgPath: RAW.zai },
};

const ALIASES: Record<string, string> = {
  "z.ai": "zai",
  xai: "grok",
  "x.ai": "grok",
  supergrok: "grok",
  "super-grok": "grok",
};

function normalize(id: string): string {
  const lower = id.toLowerCase();
  const aliased = ALIASES[lower];
  if (aliased) return aliased;
  return lower.replace(/[ \-]/g, "");
}

/** Return the registry entry for a provider id, falling back to a generic one. */
export function getProviderIcon(id: string): ProviderIcon {
  const key = normalize(id);
  return (
    PROVIDER_ICON_REGISTRY[key] ?? {
      id: key,
      brandColor: "#5d87ff",
      fallbackLetter: id.charAt(0).toUpperCase() || "●",
    }
  );
}
