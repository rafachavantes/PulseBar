use super::*;

#[test]
fn test_settings_default() {
    let settings = Settings::default();
    assert!(settings.enabled_providers.contains("claude"));
    assert!(settings.enabled_providers.contains("codex"));
    assert_eq!(settings.refresh_interval_secs, 300);
    assert!(settings.show_notifications);
    assert_eq!(settings.high_usage_threshold, 70.0);
    assert_eq!(settings.critical_usage_threshold, 90.0);
}

#[test]
fn float_bar_defaults_are_safe() {
    let settings = Settings::default();
    assert!(!settings.float_bar_enabled);
    assert_eq!(settings.float_bar_opacity, 80);
    assert_eq!(settings.float_bar_scale, 100);
    assert_eq!(settings.float_bar_orientation, "horizontal");
    assert_eq!(settings.float_bar_style, "floating");
    assert!(!settings.float_bar_click_through);
    assert!(settings.float_bar_provider_ids.is_empty());
    assert!(!settings.float_bar_dark_text);
    assert!(!settings.float_bar_show_reset_inline);
}

#[test]
fn float_bar_opacity_clamp_pins_to_supported_range() {
    // Below 30 → 30 so the bar isn't accidentally invisible.
    assert_eq!(clamp_float_bar_opacity(0), 30);
    assert_eq!(clamp_float_bar_opacity(29), 30);
    // Within range → unchanged.
    assert_eq!(clamp_float_bar_opacity(45), 45);
    assert_eq!(clamp_float_bar_opacity(80), 80);
    // Above 100 → 100.
    assert_eq!(clamp_float_bar_opacity(150), 100);
    assert_eq!(clamp_float_bar_opacity(255), 100);
}

#[test]
fn float_bar_scale_clamp_pins_to_supported_range() {
    assert_eq!(clamp_float_bar_scale(0), 75);
    assert_eq!(clamp_float_bar_scale(74), 75);
    assert_eq!(clamp_float_bar_scale(100), 100);
    assert_eq!(clamp_float_bar_scale(150), 150);
    assert_eq!(clamp_float_bar_scale(250), 200);
}

#[test]
fn float_bar_orientation_normalization_rejects_unknown_values() {
    assert_eq!(normalize_float_bar_orientation("horizontal"), "horizontal");
    assert_eq!(normalize_float_bar_orientation("vertical"), "vertical");
    // Anything else collapses to horizontal so a corrupt settings file
    // can't poison the renderer with an unknown layout token.
    assert_eq!(normalize_float_bar_orientation(""), "horizontal");
    assert_eq!(normalize_float_bar_orientation("diagonal"), "horizontal");
    assert_eq!(normalize_float_bar_orientation("VERTICAL"), "horizontal");
}

#[test]
fn float_bar_style_normalization_rejects_unknown_values() {
    assert_eq!(normalize_float_bar_style("floating"), "floating");
    assert_eq!(normalize_float_bar_style("taskbar"), "taskbar");
    assert_eq!(normalize_float_bar_style(""), "floating");
    assert_eq!(normalize_float_bar_style("TASKBAR"), "floating");
    assert_eq!(normalize_float_bar_style("glass"), "floating");
}

#[test]
fn float_bar_settings_round_trip_through_raw() {
    // Serialize a Settings with custom float-bar values then deserialize
    // through the `from = "RawSettings"` path — values must survive intact
    // (after clamping/normalization).
    let s = Settings {
        float_bar_enabled: true,
        float_bar_opacity: 65,
        float_bar_scale: 140,
        float_bar_orientation: "vertical".to_string(),
        float_bar_style: "taskbar".to_string(),
        float_bar_click_through: true,
        float_bar_provider_ids: vec!["claude".into(), "codex".into()],
        float_bar_dark_text: true,
        float_bar_show_reset_inline: true,
        ..Settings::default()
    };

    let json = serde_json::to_string(&s).expect("serialize");
    let back: Settings = serde_json::from_str(&json).expect("deserialize");
    assert!(back.float_bar_enabled);
    assert_eq!(back.float_bar_opacity, 65);
    assert_eq!(back.float_bar_scale, 140);
    assert_eq!(back.float_bar_orientation, "vertical");
    assert_eq!(back.float_bar_style, "taskbar");
    assert!(back.float_bar_click_through);
    assert_eq!(back.float_bar_provider_ids, vec!["claude", "codex"]);
    assert!(back.float_bar_dark_text);
    assert!(back.float_bar_show_reset_inline);
}

#[test]
fn float_bar_raw_clamps_out_of_range_opacity_on_load() {
    // Simulate an externally-edited settings.json with a wild opacity.
    let json = r#"{
            "enabled_providers": [],
            "refresh_interval_secs": 300,
            "start_minimized": false,
            "start_at_login": false,
            "show_notifications": true,
            "sound_enabled": true,
            "sound_volume": 100,
            "high_usage_threshold": 70.0,
            "critical_usage_threshold": 90.0,
            "merge_tray_icons": false,
            "show_as_used": true,
            "surprise_animations": false,
            "enable_animations": true,
            "reset_time_relative": true,
            "menu_bar_display_mode": "detailed",
            "show_credits_extra_usage": true,
            "show_debug_settings": false,
            "disable_keychain_access": false,
            "hide_personal_info": false,
            "float_bar_opacity": 250,
            "float_bar_scale": 250,
            "float_bar_orientation": "diagonal",
            "float_bar_style": "glass"
        }"#;
    let loaded: Settings = serde_json::from_str(json).expect("parse");
    assert_eq!(loaded.float_bar_opacity, 100);
    assert_eq!(loaded.float_bar_scale, 200);
    assert_eq!(loaded.float_bar_orientation, "horizontal");
    assert_eq!(loaded.float_bar_style, "floating");
}

#[test]
fn menu_content_mode_defaults_to_lean_and_normalizes_on_load() {
    // Missing field (pre-existing settings.json) → lean default.
    let loaded: Settings = serde_json::from_str("{}").expect("parse");
    assert_eq!(loaded.menu_content_mode, "lean");

    // Unknown value → normalized back to lean.
    let loaded: Settings =
        serde_json::from_str(r#"{"menu_content_mode": "everything"}"#).expect("parse");
    assert_eq!(loaded.menu_content_mode, "lean");

    // Explicit full survives the load.
    let loaded: Settings = serde_json::from_str(r#"{"menu_content_mode": "full"}"#).expect("parse");
    assert_eq!(loaded.menu_content_mode, "full");
}

#[test]
fn test_settings_provider_enabled() {
    let settings = Settings::default();
    assert!(settings.is_provider_enabled(ProviderId::Claude));
    assert!(settings.is_provider_enabled(ProviderId::Codex));
    assert!(!settings.is_provider_enabled(ProviderId::Gemini));
}

#[test]
fn test_settings_toggle_provider() {
    let mut settings = Settings::default();

    // Claude starts enabled
    assert!(settings.is_provider_enabled(ProviderId::Claude));

    // Toggle off
    let enabled = settings.toggle_provider(ProviderId::Claude);
    assert!(!enabled);
    assert!(!settings.is_provider_enabled(ProviderId::Claude));

    // Toggle back on
    let enabled = settings.toggle_provider(ProviderId::Claude);
    assert!(enabled);
    assert!(settings.is_provider_enabled(ProviderId::Claude));
}

#[test]
fn test_settings_get_enabled_provider_ids() {
    let settings = Settings::default();
    let enabled = settings.get_enabled_provider_ids();
    assert!(enabled.contains(&ProviderId::Claude));
    assert!(enabled.contains(&ProviderId::Codex));
}

#[test]
fn provider_order_dedupes_unknowns_and_appends_canonical_ids() {
    let order = normalize_provider_order(&[
        "gemini".to_string(),
        "not-a-provider".to_string(),
        "claude".to_string(),
        "gemini".to_string(),
    ]);

    assert_eq!(order[0], "gemini");
    assert_eq!(order[1], "claude");
    assert!(!order.iter().any(|id| id == "not-a-provider"));
    assert_eq!(order.len(), ProviderId::all().len());
}

#[test]
fn enabled_provider_ids_follow_custom_provider_order() {
    let settings = Settings {
        enabled_providers: ["claude", "codex", "gemini"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        provider_order: normalize_provider_order(&[
            "gemini".to_string(),
            "claude".to_string(),
            "codex".to_string(),
        ]),
        ..Settings::default()
    };

    assert_eq!(
        settings.get_enabled_provider_ids(),
        vec![ProviderId::Gemini, ProviderId::Claude, ProviderId::Codex]
    );
}

#[test]
fn test_settings_get_all_providers_status() {
    let settings = Settings::default();
    let status = settings.get_all_providers_status();
    assert_eq!(status.len(), ProviderId::all().len());

    let claude_status = status.iter().find(|s| s.id == "claude").unwrap();
    assert_eq!(claude_status.name, "Claude");
    assert!(claude_status.enabled);

    let gemini_status = status.iter().find(|s| s.id == "gemini").unwrap();
    assert!(!gemini_status.enabled);
}

#[test]
fn test_api_key_provider_catalog_includes_token_providers() {
    let providers = get_api_key_providers();
    for id in [ProviderId::Zai, ProviderId::Grok, ProviderId::Synthetic] {
        assert!(
            providers.iter().any(|provider| provider.id == id),
            "{id} should be configurable from the API Keys UI"
        );
    }
}

#[test]
fn test_refresh_interval_options() {
    let options = get_refresh_interval_options();
    assert!(!options.is_empty());
    assert!(options.iter().any(|o| o.value == 60));
    assert!(options.iter().any(|o| o.value == 300));
}

#[test]
fn test_manual_cookies_default() {
    let cookies = ManualCookies::default();
    assert!(cookies.cookies.is_empty());
}

#[test]
fn test_manual_cookies_set_get_remove() {
    let mut cookies = ManualCookies::default();

    // Set a cookie
    cookies.set("claude", "session=abc123");
    assert_eq!(cookies.get("claude"), Some("session=abc123"));

    // Remove it
    cookies.remove("claude");
    assert_eq!(cookies.get("claude"), None);
}

#[test]
fn test_start_at_login_command_uses_only_the_executable_path() {
    let path = std::path::PathBuf::from(r"C:\Program Files\PulseBar\pulsebar-desktop-tauri.exe");
    let command = Settings::start_at_login_command(&path);
    assert_eq!(
        command,
        "\"C:\\Program Files\\PulseBar\\pulsebar-desktop-tauri.exe\""
    );
    assert!(!command.contains("menubar"));
}

#[test]
fn test_language_defaults_to_english() {
    let settings = Settings::default();
    assert_eq!(settings.ui_language, Language::English);
}

#[test]
fn test_language_all_variants_available() {
    let languages = Language::all();
    assert_eq!(languages.len(), 3);
    assert!(languages.contains(&Language::English));
    assert!(languages.contains(&Language::Chinese));
    assert!(languages.contains(&Language::Japanese));
}

#[test]
fn test_language_display_names() {
    assert_eq!(Language::English.display_name(), "English");
    assert_eq!(Language::Chinese.display_name(), "中文");
    assert_eq!(Language::Japanese.display_name(), "日本語");
}

#[test]
fn test_settings_load_missing_language_field_defaults_to_english() {
    // Simulate loading legacy settings JSON without ui_language field
    let legacy_json = r#"{
            "enabled_providers": ["claude", "codex"],
            "refresh_interval_secs": 300,
            "start_minimized": false,
            "ui_language": "english"
        }"#;

    let settings: Result<Settings, _> = serde_json::from_str(legacy_json);
    assert!(settings.is_ok());
    let settings = settings.unwrap();
    assert_eq!(settings.ui_language, Language::English);
}

#[test]
fn test_settings_roundtrip_with_language() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Create settings with Chinese language
    let settings = Settings {
        ui_language: Language::Chinese,
        ..Settings::default()
    };

    // Save to a temp file
    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let json = serde_json::to_string_pretty(&settings).expect("Failed to serialize settings");
    temp_file
        .write_all(json.as_bytes())
        .expect("Failed to write settings");
    let path = temp_file.path().to_path_buf();

    // Read back and verify
    let content = std::fs::read_to_string(&path).expect("Failed to read settings");
    let loaded: Settings = serde_json::from_str(&content).expect("Failed to deserialize settings");

    assert_eq!(loaded.ui_language, Language::Chinese);
}

#[test]
fn test_settings_with_utf8_bom_parses_perprovider_tray_mode() {
    let json = "\u{feff}{\n            \"enabled_providers\": [\"claude\", \"codex\"],\n            \"refresh_interval_secs\": 300,\n            \"tray_icon_mode\": \"perprovider\"\n        }";

    let settings: Settings = serde_json::from_str(json.trim_start_matches('\u{feff}')).unwrap();

    assert_eq!(settings.tray_icon_mode, TrayIconMode::PerProvider);
}

#[test]
fn test_language_serde_serialization() {
    // Test that Language serializes to lowercase string
    let english = Language::English;
    let chinese = Language::Chinese;

    let english_json = serde_json::to_string(&english).unwrap();
    let chinese_json = serde_json::to_string(&chinese).unwrap();

    assert_eq!(english_json, "\"english\"");
    assert_eq!(chinese_json, "\"chinese\"");
}

#[test]
fn test_language_serde_deserialization() {
    // Test that lowercase strings deserialize correctly
    let english: Language = serde_json::from_str("\"english\"").unwrap();
    let chinese: Language = serde_json::from_str("\"chinese\"").unwrap();

    assert_eq!(english, Language::English);
    assert_eq!(chinese, Language::Chinese);
}

#[test]
fn test_theme_defaults_to_auto() {
    let settings = Settings::default();
    assert_eq!(settings.theme, ThemePreference::Auto);
}

#[test]
fn test_theme_all_variants_available() {
    let themes = ThemePreference::all();
    assert_eq!(themes.len(), 3);
    assert!(themes.contains(&ThemePreference::Auto));
    assert!(themes.contains(&ThemePreference::Light));
    assert!(themes.contains(&ThemePreference::Dark));
}

#[test]
fn test_theme_serde_roundtrip() {
    for variant in [
        ThemePreference::Auto,
        ThemePreference::Light,
        ThemePreference::Dark,
    ] {
        let encoded = serde_json::to_string(&variant).unwrap();
        let decoded: ThemePreference = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, variant);
    }
    assert_eq!(
        serde_json::to_string(&ThemePreference::Light).unwrap(),
        "\"light\""
    );
    assert_eq!(
        serde_json::to_string(&ThemePreference::Dark).unwrap(),
        "\"dark\""
    );
    assert_eq!(
        serde_json::to_string(&ThemePreference::Auto).unwrap(),
        "\"auto\""
    );
}

#[test]
fn test_settings_missing_theme_defaults_to_auto() {
    // Legacy settings JSON without the theme field should still parse.
    let legacy_json = r#"{
            "enabled_providers": ["claude", "codex"],
            "refresh_interval_secs": 300,
            "ui_language": "english"
        }"#;

    let settings: Settings = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(settings.theme, ThemePreference::Auto);
}

#[test]
fn test_settings_roundtrip_with_theme() {
    let settings = Settings {
        theme: ThemePreference::Dark,
        ..Settings::default()
    };
    let json = serde_json::to_string(&settings).unwrap();
    let loaded: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.theme, ThemePreference::Dark);
}

// ── Phase 3: provider_configs migration tests ───────────────────────

/// Loading a legacy `settings.json` (with flat per-provider fields)
/// must populate `provider_configs` and surface every value through the
/// per-provider accessors.
#[test]
fn test_legacy_per_provider_fields_migrate_into_provider_configs() {
    // NOTE: placeholder values only — no real cookies/tokens.
    let legacy_json = r#"{
            "enabled_providers": ["claude", "codex"],
            "refresh_interval_secs": 300,
            "codex_cookie_source": "manual",
            "claude_cookie_source": "browser",
            "zai_api_region": "cn",
            "claude_usage_source": "ccusage",
            "codex_usage_source": "manual",
            "codex_openai_web_extras": false,
            "codex_historical_tracking": true,
            "claude_avoid_keychain_prompts": true
        }"#;

    let settings: Settings = serde_json::from_str(legacy_json).unwrap();

    // Cookie sources
    assert_eq!(settings.cookie_source(ProviderId::Codex), "manual");
    assert_eq!(settings.cookie_source(ProviderId::Claude), "browser");
    // Untouched providers fall through to the default "manual" to avoid
    // background browser-cookie reads unless the user opts into Automatic.
    assert_eq!(settings.cookie_source(ProviderId::Grok), "manual");

    // API regions
    assert_eq!(settings.api_region(ProviderId::Zai), "cn");

    // Usage sources
    assert_eq!(settings.usage_source(ProviderId::Claude), "ccusage");
    assert_eq!(settings.usage_source(ProviderId::Codex), "manual");

    // Codex booleans
    assert!(!settings.openai_web_extras(ProviderId::Codex));
    assert!(settings.historical_tracking(ProviderId::Codex));

    // Claude per-provider boolean
    assert!(settings.avoid_keychain_prompts(ProviderId::Claude));

    // Legacy field-name aliases agree with typed accessors.
    assert_eq!(settings.codex_cookie_source(), "manual");
    assert_eq!(settings.zai_api_region(), "cn");
    assert!(settings.codex_historical_tracking());
    assert!(!settings.codex_openai_web_extras());
    assert!(settings.claude_avoid_keychain_prompts());
}

/// Round-trip: build a `Settings` programmatically via the new map +
/// accessors, serialize, parse back, and assert equality of every
/// per-provider field.
#[test]
fn test_provider_configs_roundtrip() {
    let mut settings = Settings::default();
    settings.set_cookie_source(ProviderId::Codex, "manual");
    settings.set_cookie_source(ProviderId::Claude, "browser");
    settings.set_usage_source(ProviderId::Claude, "ccusage");
    settings.set_api_region(ProviderId::Zai, "cn");
    settings.set_manual_cookie_header(ProviderId::Grok, "grok=PLACEHOLDER");
    settings.set_workspace_id(ProviderId::Gemini, "ws_placeholder");
    settings.set_openai_web_extras(ProviderId::Codex, false);
    settings.set_historical_tracking(ProviderId::Codex, true);
    settings.set_avoid_keychain_prompts(ProviderId::Claude, true);

    let json = serde_json::to_string(&settings).unwrap();
    // The legacy flat fields must NOT appear in serialized output.
    assert!(!json.contains("\"codex_cookie_source\""), "json: {json}");
    assert!(!json.contains("\"zai_api_region\""), "json: {json}");
    assert!(
        !json.contains("\"claude_avoid_keychain_prompts\""),
        "json: {json}"
    );
    assert!(json.contains("\"provider_configs\""), "json: {json}");

    let loaded: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.cookie_source(ProviderId::Codex), "manual");
    assert_eq!(loaded.cookie_source(ProviderId::Claude), "browser");
    assert_eq!(loaded.usage_source(ProviderId::Claude), "ccusage");
    assert_eq!(loaded.api_region(ProviderId::Zai), "cn");
    assert_eq!(
        loaded.manual_cookie_header(ProviderId::Grok),
        "grok=PLACEHOLDER"
    );
    assert_eq!(loaded.workspace_id(ProviderId::Gemini), "ws_placeholder");
    assert!(!loaded.openai_web_extras(ProviderId::Codex));
    assert!(loaded.historical_tracking(ProviderId::Codex));
    assert!(loaded.avoid_keychain_prompts(ProviderId::Claude));
    assert_eq!(
        loaded.provider_configs.get(&ProviderId::Codex),
        settings.provider_configs.get(&ProviderId::Codex)
    );
}

/// New-format files (no legacy flat fields, only `provider_configs`)
/// must load identically.
#[test]
fn test_new_format_provider_configs_only() {
    let json = r#"{
            "enabled_providers": ["claude"],
            "refresh_interval_secs": 300,
            "provider_configs": {
                "codex": { "cookie_source": "manual", "openai_web_extras": false },
                "zai": { "api_region": "cn" }
            }
        }"#;

    let settings: Settings = serde_json::from_str(json).unwrap();
    assert_eq!(settings.cookie_source(ProviderId::Codex), "manual");
    assert!(!settings.openai_web_extras(ProviderId::Codex));
    assert_eq!(settings.api_region(ProviderId::Zai), "cn");
    // Untouched providers still get their defaults.
    assert_eq!(settings.cookie_source(ProviderId::Claude), "manual");
    assert_eq!(settings.api_region(ProviderId::Grok), "");
}

/// Default `Settings` should serialize WITHOUT a `provider_configs`
/// field (empty map skipped).
#[test]
fn test_default_settings_skip_empty_provider_configs() {
    let settings = Settings::default();
    let json = serde_json::to_string(&settings).unwrap();
    assert!(
        !json.contains("\"provider_configs\""),
        "empty map should be skipped, json: {json}"
    );
}

/// Per-provider defaults are applied even when the entry is absent.
#[test]
fn test_per_provider_defaults_applied() {
    let settings = Settings::default();
    assert_eq!(settings.cookie_source(ProviderId::Codex), "manual");
    assert_eq!(settings.usage_source(ProviderId::Codex), "auto");
    assert_eq!(settings.api_region(ProviderId::Zai), "global");
    // Providers without an explicit region default to empty.
    assert_eq!(settings.api_region(ProviderId::Grok), "");
    assert!(settings.openai_web_extras(ProviderId::Codex));
    assert!(!settings.historical_tracking(ProviderId::Codex));
    assert!(!settings.avoid_keychain_prompts(ProviderId::Claude));
}

// ── #27: field-tolerant deserialization ──────────────────────────────

/// A single wrong-typed scalar must not wipe every other preference: the bad
/// field falls back to its default while the rest survive.
#[test]
fn tolerant_parse_recovers_from_one_wrong_typed_field() {
    // `refresh_interval_secs` is a string here (should be a number). A strict
    // parse aborts and returns Default, losing everything.
    let corrupt = r#"{
            "enabled_providers": ["claude", "codex", "gemini"],
            "refresh_interval_secs": "not-a-number",
            "high_usage_threshold": 55.0,
            "menu_content_mode": "full"
        }"#;

    // Strict parse fails outright...
    assert!(serde_json::from_str::<Settings>(corrupt).is_err());

    // ...but tolerant recovery keeps the good fields.
    let recovered = parse_settings_tolerant(corrupt);
    assert!(recovered.enabled_providers.contains("gemini"));
    assert_eq!(recovered.high_usage_threshold, 55.0);
    assert_eq!(recovered.menu_content_mode, "full");
    // The bad field falls back to the default.
    assert_eq!(recovered.refresh_interval_secs, 300);
}

/// An invalid enum value (e.g. a bogus theme) drops only that field.
#[test]
fn tolerant_parse_drops_only_the_invalid_enum_field() {
    let corrupt = r#"{
            "enabled_providers": ["claude"],
            "theme": "banana",
            "ui_language": "japanese"
        }"#;

    let recovered = parse_settings_tolerant(corrupt);
    // Invalid theme → default (Auto); the valid language survives.
    assert_eq!(recovered.theme, ThemePreference::Auto);
    assert_eq!(recovered.ui_language, Language::Japanese);
    assert!(recovered.enabled_providers.contains("claude"));
    assert!(!recovered.enabled_providers.contains("codex"));
}

/// A corrupt top-level scalar must not take the stored provider tokens/cookies
/// (`provider_configs`) down with it.
#[test]
fn tolerant_parse_preserves_provider_configs_when_another_field_is_bad() {
    // NOTE: placeholder token only — no real secret.
    let corrupt = r#"{
            "enabled_providers": ["zai"],
            "sound_volume": "loud",
            "provider_configs": {
                "zai": { "api_token": "PLACEHOLDER_TOKEN", "api_region": "cn" }
            }
        }"#;

    let recovered = parse_settings_tolerant(corrupt);
    assert_eq!(recovered.api_token(ProviderId::Zai), "PLACEHOLDER_TOKEN");
    assert_eq!(recovered.api_region(ProviderId::Zai), "cn");
    // The malformed `sound_volume` falls back to its default (100).
    assert_eq!(recovered.sound_volume, 100);
}

/// Garbage that isn't even a JSON object yields defaults rather than panicking.
#[test]
fn tolerant_parse_non_object_returns_default() {
    let recovered = parse_settings_tolerant("not json at all");
    assert_eq!(recovered.refresh_interval_secs, 300);
    assert!(recovered.enabled_providers.contains("claude"));
}

// ── #28: short-lived load cache ──────────────────────────────────────

#[test]
fn cache_freshness_respects_ttl_boundary() {
    let ttl = Duration::from_secs(2);
    let t0 = Instant::now();
    assert!(is_cache_fresh(t0, t0, ttl));
    assert!(is_cache_fresh(t0, t0 + ttl - Duration::from_millis(1), ttl));
    // At/after the TTL the entry is stale.
    assert!(!is_cache_fresh(t0, t0 + ttl, ttl));
    assert!(!is_cache_fresh(
        t0,
        t0 + ttl + Duration::from_millis(1),
        ttl
    ));
}

#[test]
fn cache_store_serves_then_invalidates() {
    // No production test calls `Settings::load`, so this test owns the global
    // cache and is safe under parallel execution.
    Settings::invalidate_cache();
    assert!(cached_settings().is_none());

    let settings = Settings {
        refresh_interval_secs: 4242,
        ..Settings::default()
    };
    store_settings_cache(&settings);

    let served = cached_settings().expect("a freshly stored snapshot should be served");
    assert_eq!(served.refresh_interval_secs, 4242);

    Settings::invalidate_cache();
    assert!(cached_settings().is_none());
}
