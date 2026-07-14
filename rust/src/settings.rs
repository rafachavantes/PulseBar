//! Settings management for PulseBar
//!
//! Handles persistent configuration including:
//! - Enabled/disabled providers
//! - Refresh interval
//! - Manual cookies
//! - Other user preferences

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

use crate::core::ProviderId;

mod api_keys;
mod manual_cookies;
mod raw;
mod status;
mod types;

pub use api_keys::*;
pub use manual_cookies::*;
use raw::RawSettings;
pub use status::*;
pub use types::*;

#[cfg(test)]
mod tests;

/// Application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "RawSettings", default)]
pub struct Settings {
    /// Enabled provider IDs (by CLI name)
    pub enabled_providers: HashSet<String>,

    /// Refresh interval in seconds (0 = manual only)
    pub refresh_interval_secs: u64,

    /// Whether to start minimized
    pub start_minimized: bool,

    /// Whether to start at login
    pub start_at_login: bool,

    /// Whether to show notifications
    pub show_notifications: bool,

    /// Whether to play sound effects for threshold alerts
    pub sound_enabled: bool,

    /// Sound volume for alerts (0-100)
    pub sound_volume: u8,

    /// High usage threshold for warnings (percentage)
    pub high_usage_threshold: f64,

    /// Critical usage threshold for alerts (percentage)
    pub critical_usage_threshold: f64,

    /// Merge mode: show all enabled providers in a single tray icon
    pub merge_tray_icons: bool,

    /// Tray icon display mode: single icon or per-provider icons
    #[serde(default)]
    pub tray_icon_mode: TrayIconMode,

    /// Show provider icons in the merged switcher UI
    #[serde(default = "default_true")]
    pub switcher_shows_icons: bool,

    /// Prefer the provider closest to its limit in merged menu bar display
    #[serde(default)]
    pub menu_bar_shows_highest_usage: bool,

    /// Replace bar-only tray display with provider branding plus percent text where supported
    #[serde(default)]
    pub menu_bar_shows_percent: bool,

    /// Show usage bars as "used" (true) or "remaining" (false)
    pub show_as_used: bool,

    /// Enable random "surprise" animations (blinks, wiggles)
    pub surprise_animations: bool,

    /// Enable UI animations (chart entrances, transitions)
    pub enable_animations: bool,

    /// Show reset times as relative (e.g., "2h 30m" instead of "3:00 PM")
    pub reset_time_relative: bool,

    /// Menu bar display mode: "minimal", "compact", or "detailed"
    pub menu_bar_display_mode: String,

    /// Menu card content mode: "lean" (session, weekly, pace and tokens
    /// only) or "full" (every metric, cost and chart section)
    pub menu_content_mode: String,

    /// Show credits and extra usage information in the UI
    pub show_credits_extra_usage: bool,

    /// Show all token accounts in provider menus instead of collapsing behind switchers
    #[serde(default)]
    pub show_all_token_accounts_in_menu: bool,

    /// Per-provider configuration map (cookie/usage source, region, manual
    /// headers, API tokens, etc). Replaces the legacy flat per-provider
    /// fields; legacy `settings.json` files are migrated via [`RawSettings`].
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub provider_configs: HashMap<ProviderId, ProviderConfig>,

    /// Show debug-oriented settings and troubleshooting surfaces
    #[serde(default)]
    pub show_debug_settings: bool,

    /// Disable credential/keychain-style reads where supported
    #[serde(default)]
    pub disable_keychain_access: bool,

    /// Hide personal info (emails, account names) for streaming/sharing
    pub hide_personal_info: bool,

    /// Update channel for receiving updates (Stable or Beta)
    pub update_channel: UpdateChannel,

    /// Per-provider metric preference for tray display
    #[serde(default)]
    pub provider_metrics: HashMap<String, MetricPreference>,

    /// Preferred display order of provider IDs (CLI names).
    ///
    /// An empty list means "fall back to the canonical `ProviderId::all()`
    /// order". Unknown or duplicated ids are filtered out on load; new
    /// providers are appended in their canonical order.
    #[serde(default)]
    pub provider_order: Vec<String>,

    /// Global keyboard shortcut to open the menu (e.g., "Ctrl+Shift+U")
    #[serde(default = "default_global_shortcut")]
    pub global_shortcut: String,

    /// Automatically download updates in the background
    #[serde(default)]
    pub auto_download_updates: bool,

    /// Install pending updates when quitting the application
    #[serde(default)]
    pub install_updates_on_quit: bool,

    /// UI language for the application (English default for backward compatibility)
    #[serde(default)]
    pub ui_language: Language,

    /// UI theme preference (Phase 12). Defaults to Auto (prefers-color-scheme).
    #[serde(default)]
    pub theme: ThemePreference,

    /// Show the always-on-top floating capacity bar.
    #[serde(default)]
    pub float_bar_enabled: bool,

    /// Opacity of the floating bar window, in the inclusive range 30..=100.
    /// Stored as `u8` so the on-disk format remains stable.
    #[serde(default = "default_float_bar_opacity")]
    pub float_bar_opacity: u8,

    /// Floating-bar visual scale, in the inclusive range 75..=200.
    #[serde(default = "default_float_bar_scale")]
    pub float_bar_scale: u8,

    /// Floating-bar orientation: "horizontal" (default) or "vertical".
    #[serde(default = "default_float_bar_orientation")]
    pub float_bar_orientation: String,

    /// Floating-bar visual style: "floating" (default) or "taskbar".
    #[serde(default = "default_float_bar_style")]
    pub float_bar_style: String,

    /// When true the floating bar is fully click-through (overlay mode).
    #[serde(default)]
    pub float_bar_click_through: bool,

    /// Provider CLI names to display in the floating bar. Empty = all enabled.
    #[serde(default)]
    pub float_bar_provider_ids: Vec<String>,

    /// When true, the floating bar uses a dark-on-light palette so it
    /// stays legible on light desktop backgrounds. Defaults to false
    /// (light-on-dark, the original look).
    #[serde(default)]
    pub float_bar_dark_text: bool,

    /// When true, show the primary window's next reset inline in each pill.
    #[serde(default)]
    pub float_bar_show_reset_inline: bool,
}

fn default_float_bar_opacity() -> u8 {
    80
}

fn default_float_bar_scale() -> u8 {
    100
}

fn default_float_bar_orientation() -> String {
    "horizontal".to_string()
}

fn default_float_bar_style() -> String {
    "floating".to_string()
}

/// Clamp the floating-bar opacity to the supported range.
///
/// Opacity values below 30% would make the bar effectively invisible, so we
/// pin the lower bound; the upper bound is the natural 100%.
pub fn clamp_float_bar_opacity(value: u8) -> u8 {
    value.clamp(30, 100)
}

/// Clamp the floating-bar visual scale to the supported range.
pub fn clamp_float_bar_scale(value: u8) -> u8 {
    value.clamp(75, 200)
}

/// Normalize a floating-bar orientation string. Unknown values fall back to
/// the default ("horizontal") so a corrupt settings file can't put the
/// renderer into an undefined state.
pub fn normalize_float_bar_orientation(value: &str) -> String {
    match value {
        "vertical" => "vertical".to_string(),
        _ => "horizontal".to_string(),
    }
}

fn default_menu_content_mode() -> String {
    "lean".to_string()
}

/// Normalize a menu content mode string. Unknown values fall back to "lean"
/// so the menu cards always render a defined layout.
pub fn normalize_menu_content_mode(value: &str) -> String {
    match value {
        "full" => "full".to_string(),
        _ => "lean".to_string(),
    }
}

/// Normalize a floating-bar style string. Unknown values fall back to the
/// original floating style so existing settings keep their previous look.
pub fn normalize_float_bar_style(value: &str) -> String {
    match value {
        "taskbar" => "taskbar".to_string(),
        _ => "floating".to_string(),
    }
}

/// Canonicalize a requested provider display order.
///
/// Keeps requested provider IDs that map to a real [`ProviderId`], drops
/// duplicates, and appends omitted providers in canonical order. An empty
/// request intentionally returns the full canonical order so display callers
/// can use one path for default and customized ordering.
pub fn normalize_provider_order(requested: &[String]) -> Vec<String> {
    let canonical = ProviderId::all()
        .iter()
        .map(|provider| provider.cli_name().to_string())
        .collect::<Vec<_>>();
    let valid = canonical.iter().map(String::as_str).collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(canonical.len());

    for provider_id in requested {
        if valid.contains(provider_id.as_str()) && seen.insert(provider_id.clone()) {
            out.push(provider_id.clone());
        }
    }
    for provider_id in canonical {
        if seen.insert(provider_id.clone()) {
            out.push(provider_id);
        }
    }

    out
}

fn default_global_shortcut() -> String {
    "Ctrl+Shift+U".to_string()
}

fn default_true() -> bool {
    true
}

/// Default cookie source value for browser-authenticated providers.
///
/// Browser cookie extraction reads browser profile databases and decrypts
/// Chromium cookies via Windows DPAPI, which can trigger behavior-based AV
/// engines. Keep that path explicit opt-in by default.
const DEFAULT_COOKIE_SOURCE: &str = "manual";

/// Default usage source value for any provider.
const DEFAULT_PROVIDER_SOURCE: &str = "auto";

/// Default API region for providers that expose one.
fn default_api_region(id: ProviderId) -> &'static str {
    match id {
        ProviderId::Zai => "global",
        _ => "",
    }
}

/// Default for the codex `openai_web_extras` boolean (true = show extras).
const DEFAULT_CODEX_OPENAI_WEB_EXTRAS: bool = true;

// ── Short-lived process cache for `Settings::load` ───────────────────────
//
// A refresh cycle reloads settings ~5x across the tray/catalog helpers. The
// cache collapses those into a single disk+registry read per TTL window while
// still picking up external edits shortly after they land.

/// How long a cached [`Settings`] snapshot is served before `load()` re-reads
/// disk. Short enough that an external edit is reflected quickly, long enough
/// to collapse the burst of reloads within one refresh cycle.
const SETTINGS_CACHE_TTL: Duration = Duration::from_secs(2);

struct CachedSettings {
    settings: Settings,
    stored_at: Instant,
}

fn settings_cache() -> &'static RwLock<Option<CachedSettings>> {
    static CACHE: OnceLock<RwLock<Option<CachedSettings>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(None))
}

/// Whether a cache entry stored at `stored_at` is still fresh at `now`.
/// Pulled out as a pure function so the TTL boundary is unit-testable.
fn is_cache_fresh(stored_at: Instant, now: Instant, ttl: Duration) -> bool {
    now.saturating_duration_since(stored_at) < ttl
}

/// Return the cached snapshot if it is still within [`SETTINGS_CACHE_TTL`].
fn cached_settings() -> Option<Settings> {
    let guard = settings_cache().read().ok()?;
    let entry = guard.as_ref()?;
    is_cache_fresh(entry.stored_at, Instant::now(), SETTINGS_CACHE_TTL)
        .then(|| entry.settings.clone())
}

fn store_settings_cache(settings: &Settings) {
    if let Ok(mut guard) = settings_cache().write() {
        *guard = Some(CachedSettings {
            settings: settings.clone(),
            stored_at: Instant::now(),
        });
    }
}

/// Parse a `settings.json` body, tolerating a single malformed field.
///
/// `#[serde(default)]` only rescues *missing* fields; one wrong-typed field
/// (hand edit, interrupted write, future schema change) aborts the whole parse
/// and would otherwise discard every preference. Here we first try a strict
/// parse, then fall back to per-field recovery: each top-level key is probed
/// against [`RawSettings`] individually and dropped only if it fails to
/// deserialize, so the remaining good fields survive.
fn parse_settings_tolerant(content: &str) -> Settings {
    if let Ok(settings) = serde_json::from_str::<Settings>(content) {
        return settings;
    }

    let Ok(serde_json::Value::Object(object)) = serde_json::from_str::<serde_json::Value>(content)
    else {
        // Not even a JSON object — nothing to recover.
        return Settings::default();
    };

    let mut recovered = serde_json::Map::new();
    for (key, value) in object {
        let mut probe = serde_json::Map::new();
        probe.insert(key.clone(), value.clone());
        // A field survives only if it deserializes on its own (all other
        // fields fall back to `RawSettings`' defaults during the probe).
        if serde_json::from_value::<RawSettings>(serde_json::Value::Object(probe)).is_ok() {
            recovered.insert(key, value);
        } else {
            tracing::warn!("Dropping malformed settings field during recovery: {key}");
        }
    }

    serde_json::from_value::<Settings>(serde_json::Value::Object(recovered)).unwrap_or_default()
}

impl Default for Settings {
    fn default() -> Self {
        let mut enabled = HashSet::new();
        // Default enabled providers
        enabled.insert("claude".to_string());
        enabled.insert("codex".to_string());

        Self {
            enabled_providers: enabled,
            refresh_interval_secs: 300, // 5 minutes
            start_minimized: false,
            start_at_login: false,
            show_notifications: true,
            sound_enabled: true,
            sound_volume: 100,
            high_usage_threshold: 70.0,
            critical_usage_threshold: 90.0,
            merge_tray_icons: false, // Show single provider by default
            tray_icon_mode: TrayIconMode::default(), // Single icon by default
            switcher_shows_icons: true,
            menu_bar_shows_highest_usage: false,
            menu_bar_shows_percent: false,
            show_as_used: true,         // Show as "used" by default
            surprise_animations: false, // Off by default
            enable_animations: true,    // Animations enabled by default
            reset_time_relative: true,  // Show relative times by default
            menu_bar_display_mode: "detailed".to_string(), // Detailed mode by default
            menu_content_mode: default_menu_content_mode(), // Lean cards by default
            show_credits_extra_usage: true, // Show credits + extra usage by default
            show_all_token_accounts_in_menu: false,
            provider_configs: HashMap::new(),
            show_debug_settings: false,
            disable_keychain_access: false,
            hide_personal_info: false, // Show personal info by default
            update_channel: UpdateChannel::default(), // Stable by default
            provider_metrics: HashMap::new(), // Empty = use Automatic for all
            provider_order: Vec::new(), // Empty = canonical ProviderId::all() order
            global_shortcut: default_global_shortcut(), // Ctrl+Shift+U by default
            auto_download_updates: false, // Require explicit opt-in for background downloads
            install_updates_on_quit: false, // Don't auto-install on quit by default
            ui_language: Language::default(), // English by default
            theme: ThemePreference::default(), // Auto (follows prefers-color-scheme)
            float_bar_enabled: false,
            float_bar_opacity: default_float_bar_opacity(),
            float_bar_scale: default_float_bar_scale(),
            float_bar_orientation: default_float_bar_orientation(),
            float_bar_style: default_float_bar_style(),
            float_bar_click_through: false,
            float_bar_provider_ids: Vec::new(),
            float_bar_dark_text: false,
            float_bar_show_reset_inline: false,
        }
    }
}

impl Settings {
    /// Get the settings file path
    pub fn settings_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("PulseBar").join("settings.json"))
    }

    /// Load settings from disk.
    ///
    /// A single refresh cycle calls this several times across the tray/catalog
    /// helpers, so the parsed snapshot is memoized in a short-lived process
    /// cache ([`SETTINGS_CACHE_TTL`]) to avoid re-reading disk (and the Windows
    /// registry) ~5x per cycle. [`Settings::save`] refreshes the cache so
    /// in-process writes are reflected immediately.
    pub fn load() -> Self {
        if let Some(cached) = cached_settings() {
            return cached;
        }

        #[allow(unused_mut)]
        let mut settings = Self::load_from_disk();

        // Sync autostart toggle with actual registry state. This only runs on a
        // cache miss (roughly once per TTL window) instead of on every call.
        #[cfg(target_os = "windows")]
        {
            settings.start_at_login = Self::is_start_at_login_enabled();
        }

        store_settings_cache(&settings);
        settings
    }

    /// Read + parse the settings file, tolerating a single malformed field
    /// instead of discarding every preference. On an outright parse failure the
    /// offending file is backed up to `settings.json.bak` (preserving whatever
    /// on-disk form it has, including DPAPI wrapping) before per-field recovery.
    fn load_from_disk() -> Self {
        let Some(path) = Self::settings_path() else {
            return Self::default();
        };
        if !path.exists() {
            return Self::default();
        }

        let content = match crate::secure_file::read_string(&path) {
            Ok(content) => content,
            Err(err) => {
                tracing::warn!("Could not read settings file: {err}");
                return Self::default();
            }
        };
        let trimmed = content.trim_start_matches('\u{feff}');

        match serde_json::from_str::<Settings>(trimmed) {
            Ok(settings) => settings,
            Err(err) => {
                tracing::warn!(
                    "settings.json failed to parse ({err}); backing up to settings.json.bak and \
                     recovering field-by-field so one bad value doesn't reset every preference"
                );
                Self::backup_corrupt_settings(&path);
                parse_settings_tolerant(trimmed)
            }
        }
    }

    /// Copy a corrupt settings file aside so the user can inspect/recover it.
    /// Best-effort: failures are logged, never propagated.
    fn backup_corrupt_settings(path: &Path) {
        let mut backup = path.as_os_str().to_owned();
        backup.push(".bak");
        let backup = PathBuf::from(backup);
        match std::fs::copy(path, &backup) {
            Ok(_) => tracing::warn!("Backed up corrupt settings to {}", backup.display()),
            Err(err) => tracing::warn!("Could not back up corrupt settings file: {err}"),
        }
    }

    /// Save settings to disk.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::settings_path()
            .ok_or_else(|| anyhow::anyhow!("Could not determine settings path"))?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        crate::secure_file::write_string(&path, &json)?;

        // Keep the process cache coherent with what we just persisted so the
        // next `load()` in this refresh cycle sees the new values.
        store_settings_cache(self);

        Ok(())
    }

    /// Drop the in-process settings cache, forcing the next [`Settings::load`]
    /// to re-read disk. Useful when the file may have changed out from under us.
    pub fn invalidate_cache() {
        if let Ok(mut guard) = settings_cache().write() {
            *guard = None;
        }
    }

    fn start_at_login_command(exe_path: &std::path::Path) -> String {
        format!("\"{}\"", exe_path.display())
    }

    #[cfg(target_os = "windows")]
    pub fn apply_start_at_login_registry(enabled: bool) -> anyhow::Result<()> {
        use winreg::RegKey;
        use winreg::enums::*;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = hkcu.open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            KEY_READ | KEY_WRITE,
        )?;

        // One-time cleanup: remove orphaned "CodexBar" run-key from pre-rename installs
        let _ = run_key.delete_value("CodexBar");

        if enabled {
            let exe_path = std::env::current_exe()?;
            let command = Self::start_at_login_command(&exe_path);
            run_key.set_value("PulseBar", &command)?;
        } else {
            let _ = run_key.delete_value("PulseBar");
        }

        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    pub fn apply_start_at_login_registry(_enabled: bool) -> anyhow::Result<()> {
        Ok(())
    }

    /// Set start at login (updates Windows registry)
    pub fn set_start_at_login(&mut self, enabled: bool) -> anyhow::Result<()> {
        self.start_at_login = enabled;
        Self::apply_start_at_login_registry(enabled)?;
        Ok(())
    }

    /// Check if start at login is actually enabled in registry
    #[cfg(target_os = "windows")]
    pub fn is_start_at_login_enabled() -> bool {
        use winreg::RegKey;
        use winreg::enums::*;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(run_key) = hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run") {
            run_key.get_value::<String, _>("PulseBar").is_ok()
        } else {
            false
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn is_start_at_login_enabled() -> bool {
        false
    }

    /// Check if a provider is enabled
    pub fn is_provider_enabled(&self, id: ProviderId) -> bool {
        self.enabled_providers.contains(id.cli_name())
    }

    /// Enable a provider
    pub fn enable_provider(&mut self, id: ProviderId) {
        self.enabled_providers.insert(id.cli_name().to_string());
    }

    /// Disable a provider
    pub fn disable_provider(&mut self, id: ProviderId) {
        self.enabled_providers.remove(id.cli_name());
    }

    /// Toggle a provider's enabled state
    pub fn toggle_provider(&mut self, id: ProviderId) -> bool {
        let name = id.cli_name().to_string();
        if self.enabled_providers.contains(&name) {
            self.enabled_providers.remove(&name);
            false
        } else {
            self.enabled_providers.insert(name);
            true
        }
    }

    /// Get list of enabled provider IDs
    pub fn get_enabled_provider_ids(&self) -> Vec<ProviderId> {
        self.provider_display_order()
            .into_iter()
            .filter(|id| self.is_provider_enabled(*id))
            .collect()
    }

    /// Get all available providers with their enabled status
    pub fn get_all_providers_status(&self) -> Vec<ProviderStatus> {
        self.provider_display_order()
            .into_iter()
            .map(|id| ProviderStatus {
                id: id.cli_name().to_string(),
                name: id.display_name().to_string(),
                enabled: self.is_provider_enabled(id),
            })
            .collect()
    }

    /// Provider display order as typed IDs, falling back to canonical order
    /// when no custom order has been persisted.
    pub fn provider_display_order(&self) -> Vec<ProviderId> {
        normalize_provider_order(&self.provider_order)
            .into_iter()
            .filter_map(|provider_id| ProviderId::from_cli_name(&provider_id))
            .collect()
    }

    /// Provider display order as CLI-name strings.
    pub fn provider_display_order_names(&self) -> Vec<String> {
        normalize_provider_order(&self.provider_order)
    }

    /// Get the metric preference for a provider
    pub fn get_provider_metric(&self, id: ProviderId) -> MetricPreference {
        self.provider_metrics
            .get(id.cli_name())
            .copied()
            .unwrap_or_default()
    }

    /// Set the metric preference for a provider
    pub fn set_provider_metric(&mut self, id: ProviderId, metric: MetricPreference) {
        self.provider_metrics
            .insert(id.cli_name().to_string(), metric);
    }

    // ── Per-provider configuration accessors ─────────────────────────
    //
    // These thin wrappers around `provider_configs` apply provider-specific
    // defaults (e.g. cookie/usage source defaults to `"auto"`) so callers
    // never have to reach into the raw `Option<String>` fields. The
    // `*_str` / boolean / setter pairs intentionally mirror the names of
    // the legacy flat fields so call-site migration is mechanical.

    /// Read-only access to a provider's stored config, if any.
    pub fn provider_config(&self, id: ProviderId) -> Option<&ProviderConfig> {
        self.provider_configs.get(&id)
    }

    /// Mutable access to a provider's config, lazily creating an empty
    /// entry if none exists.
    pub fn provider_config_mut(&mut self, id: ProviderId) -> &mut ProviderConfig {
        self.provider_configs.entry(id).or_default()
    }

    /// Cookie source for `id`, or the default `"manual"` if unset.
    pub fn cookie_source(&self, id: ProviderId) -> &str {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.cookie_source.as_deref())
            .unwrap_or(DEFAULT_COOKIE_SOURCE)
    }

    pub fn set_cookie_source(&mut self, id: ProviderId, source: impl Into<String>) {
        self.provider_config_mut(id).cookie_source = Some(source.into());
    }

    /// Usage source for `id`, or the default `"auto"` if unset.
    pub fn usage_source(&self, id: ProviderId) -> &str {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.usage_source.as_deref())
            .unwrap_or(DEFAULT_PROVIDER_SOURCE)
    }

    pub fn set_usage_source(&mut self, id: ProviderId, source: impl Into<String>) {
        self.provider_config_mut(id).usage_source = Some(source.into());
    }

    /// API region for `id`, or the provider-specific default if unset.
    pub fn api_region(&self, id: ProviderId) -> &str {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.api_region.as_deref())
            .unwrap_or_else(|| default_api_region(id))
    }

    pub fn set_api_region(&mut self, id: ProviderId, region: impl Into<String>) {
        self.provider_config_mut(id).api_region = Some(region.into());
    }

    /// Manual cookie header for `id`, or `""` if unset.
    pub fn manual_cookie_header(&self, id: ProviderId) -> &str {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.manual_cookie_header.as_deref())
            .unwrap_or("")
    }

    pub fn set_manual_cookie_header(&mut self, id: ProviderId, header: impl Into<String>) {
        self.provider_config_mut(id).manual_cookie_header = Some(header.into());
    }

    /// API token for `id`, or `""` if unset.
    pub fn api_token(&self, id: ProviderId) -> &str {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.api_token.as_deref())
            .unwrap_or("")
    }

    pub fn set_api_token(&mut self, id: ProviderId, token: impl Into<String>) {
        self.provider_config_mut(id).api_token = Some(token.into());
    }

    /// Workspace ID override for `id`, or `""` if unset.
    pub fn workspace_id(&self, id: ProviderId) -> &str {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.workspace_id.as_deref())
            .unwrap_or("")
    }

    pub fn set_workspace_id(&mut self, id: ProviderId, value: impl Into<String>) {
        self.provider_config_mut(id).workspace_id = Some(value.into());
    }

    /// IDE base path override for `id`, or `""` if unset.
    pub fn ide_base_path(&self, id: ProviderId) -> &str {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.ide_base_path.as_deref())
            .unwrap_or("")
    }

    pub fn set_ide_base_path(&mut self, id: ProviderId, value: impl Into<String>) {
        self.provider_config_mut(id).ide_base_path = Some(value.into());
    }

    /// Codex `openai_web_extras` toggle, default `true`.
    pub fn openai_web_extras(&self, id: ProviderId) -> bool {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.openai_web_extras)
            .unwrap_or(DEFAULT_CODEX_OPENAI_WEB_EXTRAS)
    }

    pub fn set_openai_web_extras(&mut self, id: ProviderId, value: bool) {
        self.provider_config_mut(id).openai_web_extras = Some(value);
    }

    /// Per-provider historical-tracking toggle (currently codex-only).
    pub fn historical_tracking(&self, id: ProviderId) -> bool {
        self.provider_configs
            .get(&id)
            .map(|c| c.historical_tracking)
            .unwrap_or(false)
    }

    pub fn set_historical_tracking(&mut self, id: ProviderId, value: bool) {
        self.provider_config_mut(id).historical_tracking = value;
    }

    /// Per-provider "avoid keychain prompts" toggle (currently claude-only).
    pub fn avoid_keychain_prompts(&self, id: ProviderId) -> bool {
        self.provider_configs
            .get(&id)
            .map(|c| c.avoid_keychain_prompts)
            .unwrap_or(false)
    }

    pub fn set_avoid_keychain_prompts(&mut self, id: ProviderId, value: bool) {
        self.provider_config_mut(id).avoid_keychain_prompts = value;
    }

    // ── Legacy field-name aliases ────────────────────────────────────
    //
    // Keep the names of the old flat per-provider fields available as
    // accessor methods so existing call sites only need a `()` (read) or
    // `set_` prefix (write). New code should prefer the typed accessors
    // above.

    pub fn codex_cookie_source(&self) -> &str {
        self.cookie_source(ProviderId::Codex)
    }
    pub fn set_codex_cookie_source(&mut self, v: impl Into<String>) {
        self.set_cookie_source(ProviderId::Codex, v)
    }
    pub fn claude_cookie_source(&self) -> &str {
        self.cookie_source(ProviderId::Claude)
    }
    pub fn set_claude_cookie_source(&mut self, v: impl Into<String>) {
        self.set_cookie_source(ProviderId::Claude, v)
    }

    pub fn claude_usage_source(&self) -> &str {
        self.usage_source(ProviderId::Claude)
    }
    pub fn set_claude_usage_source(&mut self, v: impl Into<String>) {
        self.set_usage_source(ProviderId::Claude, v)
    }
    pub fn codex_usage_source(&self) -> &str {
        self.usage_source(ProviderId::Codex)
    }
    pub fn set_codex_usage_source(&mut self, v: impl Into<String>) {
        self.set_usage_source(ProviderId::Codex, v)
    }

    pub fn zai_api_region(&self) -> &str {
        self.api_region(ProviderId::Zai)
    }
    pub fn set_zai_api_region(&mut self, v: impl Into<String>) {
        self.set_api_region(ProviderId::Zai, v)
    }

    pub fn codex_openai_web_extras(&self) -> bool {
        self.openai_web_extras(ProviderId::Codex)
    }
    pub fn set_codex_openai_web_extras(&mut self, v: bool) {
        self.set_openai_web_extras(ProviderId::Codex, v)
    }
    pub fn codex_historical_tracking(&self) -> bool {
        self.historical_tracking(ProviderId::Codex)
    }
    pub fn set_codex_historical_tracking(&mut self, v: bool) {
        self.set_historical_tracking(ProviderId::Codex, v)
    }
    pub fn claude_avoid_keychain_prompts(&self) -> bool {
        self.avoid_keychain_prompts(ProviderId::Claude)
    }
    pub fn set_claude_avoid_keychain_prompts(&mut self, v: bool) {
        self.set_avoid_keychain_prompts(ProviderId::Claude, v)
    }
}
