//! Global keyboard shortcut registration for toggling the tray panel.
//!
//! Reads the persisted `global_shortcut` setting (e.g. `"Ctrl+Shift+U"`)
//! and registers it through the Tauri global-shortcut plugin. The shortcut
//! toggles the tray panel via the surface state machine.

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::shell;

const KEY_ALIASES: &[(&str, Code)] = &[
    ("a", Code::KeyA),
    ("b", Code::KeyB),
    ("c", Code::KeyC),
    ("d", Code::KeyD),
    ("e", Code::KeyE),
    ("f", Code::KeyF),
    ("g", Code::KeyG),
    ("h", Code::KeyH),
    ("i", Code::KeyI),
    ("j", Code::KeyJ),
    ("k", Code::KeyK),
    ("l", Code::KeyL),
    ("m", Code::KeyM),
    ("n", Code::KeyN),
    ("o", Code::KeyO),
    ("p", Code::KeyP),
    ("q", Code::KeyQ),
    ("r", Code::KeyR),
    ("s", Code::KeyS),
    ("t", Code::KeyT),
    ("u", Code::KeyU),
    ("v", Code::KeyV),
    ("w", Code::KeyW),
    ("x", Code::KeyX),
    ("y", Code::KeyY),
    ("z", Code::KeyZ),
    ("0", Code::Digit0),
    ("1", Code::Digit1),
    ("2", Code::Digit2),
    ("3", Code::Digit3),
    ("4", Code::Digit4),
    ("5", Code::Digit5),
    ("6", Code::Digit6),
    ("7", Code::Digit7),
    ("8", Code::Digit8),
    ("9", Code::Digit9),
    ("f1", Code::F1),
    ("f2", Code::F2),
    ("f3", Code::F3),
    ("f4", Code::F4),
    ("f5", Code::F5),
    ("f6", Code::F6),
    ("f7", Code::F7),
    ("f8", Code::F8),
    ("f9", Code::F9),
    ("f10", Code::F10),
    ("f11", Code::F11),
    ("f12", Code::F12),
    ("space", Code::Space),
    ("enter", Code::Enter),
    ("return", Code::Enter),
    ("escape", Code::Escape),
    ("esc", Code::Escape),
    ("tab", Code::Tab),
];

/// Parse a settings shortcut string (e.g. `"Ctrl+Shift+U"`) into a Tauri `Shortcut`.
///
/// Public so that callers (e.g. the settings command) can validate a shortcut
/// string before persisting it.
pub fn parse_shortcut(s: &str) -> Option<Shortcut> {
    let mut mods = Modifiers::empty();
    let mut key_code: Option<Code> = None;

    for part in s.split('+').map(str::trim).filter(|part| !part.is_empty()) {
        if let Some(parsed_modifier) = parse_modifier(part) {
            mods |= parsed_modifier;
        } else if let Some(parsed_key) = parse_key(part) {
            key_code = Some(parsed_key);
        }
    }

    let key = key_code?;
    let m = if mods.is_empty() { None } else { Some(mods) };
    Some(Shortcut::new(m, key))
}

fn parse_modifier(token: &str) -> Option<Modifiers> {
    match token.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(Modifiers::CONTROL),
        "shift" => Some(Modifiers::SHIFT),
        "alt" => Some(Modifiers::ALT),
        "super" | "win" | "meta" => Some(Modifiers::SUPER),
        _ => None,
    }
}

fn parse_key(token: &str) -> Option<Code> {
    let normalized = token.to_ascii_lowercase();
    KEY_ALIASES
        .iter()
        .find_map(|(alias, code)| (*alias == normalized).then_some(*code))
}

/// Build the Tauri global-shortcut plugin with the tray-panel toggle handler.
pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                shell::toggle_tray_panel(app, None);
            }
        })
        .build()
}

/// Register the persisted global shortcut from settings.
///
/// Call this in the Tauri `setup` closure after the plugin is initialised.
pub fn register(app: &AppHandle) {
    let settings = pulsebar::settings::Settings::load();
    let shortcut_str = &settings.global_shortcut;

    let Some(shortcut) = parse_shortcut(shortcut_str) else {
        tracing::warn!("Could not parse global shortcut: {shortcut_str}");
        return;
    };

    match app.global_shortcut().register(shortcut) {
        Ok(()) => {
            tracing::info!("Registered global shortcut: {shortcut_str}");
        }
        Err(e) => {
            tracing::warn!("Failed to register global shortcut '{shortcut_str}': {e}");
        }
    }
}

/// Whether a requested shortcut string means "clear the shortcut" (no binding).
///
/// An empty or whitespace-only value is a valid request to unbind the hotkey,
/// not an error — so `clearShortcut` in the UI can actually clear it.
pub fn is_clear_request(new: &str) -> bool {
    new.trim().is_empty()
}

/// Live-swap the global shortcut: unregister `old` and register `new`.
///
/// Called from `update_settings` when the user changes the shortcut in the
/// Settings UI. The old accelerator is always unregistered first. An empty
/// `new` is treated as "clear": unregister only and succeed, so an empty
/// value can persist. Returns `Err` with a user-facing message only when a
/// non-empty `new` cannot be parsed or registration fails; on that error the
/// old shortcut is restored (best-effort).
pub fn reregister_shortcut(app: &AppHandle, old: &str, new: &str) -> Result<(), String> {
    // Unregister the previous shortcut first (ignore errors — it may not be
    // registered). Doing this up front means the "clear" path drops the old
    // binding too.
    if let Some(old_shortcut) = parse_shortcut(old) {
        let _ = app.global_shortcut().unregister(old_shortcut);
    }

    // Clearing the shortcut: nothing left to register, and an empty value
    // must persist rather than error out on `parse_shortcut("") == None`.
    if is_clear_request(new) {
        tracing::info!("Cleared global shortcut (was: \"{old}\")");
        return Ok(());
    }

    let new_shortcut = parse_shortcut(new).ok_or_else(|| {
        format!("Invalid shortcut \"{new}\". Use a combination like Ctrl+Shift+U.")
    })?;

    app.global_shortcut().register(new_shortcut).map_err(|e| {
        // Best-effort: try to restore the old shortcut.
        if let Some(old_shortcut) = parse_shortcut(old) {
            let _ = app.global_shortcut().register(old_shortcut);
        }
        format!("Failed to register shortcut \"{new}\": {e}")
    })?;

    tracing::info!("Re-registered global shortcut: {old} → {new}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ctrl_shift_u() {
        let s = parse_shortcut("Ctrl+Shift+U").unwrap();
        assert_eq!(s.key, Code::KeyU);
        assert!(s.mods.contains(Modifiers::CONTROL));
        assert!(s.mods.contains(Modifiers::SHIFT));
    }

    #[test]
    fn parse_alt_f1() {
        let s = parse_shortcut("Alt+F1").unwrap();
        assert_eq!(s.key, Code::F1);
        assert!(s.mods.contains(Modifiers::ALT));
    }

    #[test]
    fn parse_single_letter_no_mods() {
        let s = parse_shortcut("A").unwrap();
        assert_eq!(s.key, Code::KeyA);
        assert!(s.mods.is_empty());
    }

    #[test]
    fn parse_empty_returns_none() {
        assert!(parse_shortcut("").is_none());
    }

    #[test]
    fn parse_invalid_returns_none() {
        assert!(parse_shortcut("Ctrl+???").is_none());
    }

    #[test]
    fn parse_digit() {
        let s = parse_shortcut("Ctrl+5").unwrap();
        assert_eq!(s.key, Code::Digit5);
        assert!(s.mods.contains(Modifiers::CONTROL));
    }

    #[test]
    fn parse_super_key() {
        let s = parse_shortcut("Super+A").unwrap();
        assert_eq!(s.key, Code::KeyA);
        assert!(s.mods.contains(Modifiers::SUPER));
    }

    #[test]
    fn parse_win_alias() {
        let s = parse_shortcut("Win+Z").unwrap();
        assert_eq!(s.key, Code::KeyZ);
        assert!(s.mods.contains(Modifiers::SUPER));
    }

    #[test]
    fn parse_meta_alias() {
        let s = parse_shortcut("Meta+F12").unwrap();
        assert_eq!(s.key, Code::F12);
        assert!(s.mods.contains(Modifiers::SUPER));
    }

    #[test]
    fn parse_special_keys() {
        assert_eq!(parse_shortcut("Ctrl+Space").unwrap().key, Code::Space);
        assert_eq!(parse_shortcut("Alt+Enter").unwrap().key, Code::Enter);
        assert_eq!(parse_shortcut("Ctrl+Tab").unwrap().key, Code::Tab);
        assert_eq!(parse_shortcut("Ctrl+Escape").unwrap().key, Code::Escape);
    }

    #[test]
    fn parse_return_alias() {
        assert_eq!(parse_shortcut("Ctrl+Return").unwrap().key, Code::Enter);
    }

    #[test]
    fn parse_esc_alias() {
        assert_eq!(parse_shortcut("Ctrl+Esc").unwrap().key, Code::Escape);
    }

    #[test]
    fn parse_control_alias() {
        let s = parse_shortcut("Control+Shift+B").unwrap();
        assert!(s.mods.contains(Modifiers::CONTROL));
        assert!(s.mods.contains(Modifiers::SHIFT));
        assert_eq!(s.key, Code::KeyB);
    }

    #[test]
    fn parse_all_function_keys() {
        for i in 1..=12u8 {
            let input = format!("F{i}");
            let s = parse_shortcut(&input);
            assert!(s.is_some(), "F{i} should parse");
        }
    }

    #[test]
    fn parse_no_key_returns_none() {
        // Only modifiers, no actual key
        assert!(parse_shortcut("Ctrl+Shift").is_none());
    }

    #[test]
    fn parse_whitespace_tolerant() {
        let s = parse_shortcut("Ctrl + Shift + U").unwrap();
        assert_eq!(s.key, Code::KeyU);
        assert!(s.mods.contains(Modifiers::CONTROL));
        assert!(s.mods.contains(Modifiers::SHIFT));
    }

    #[test]
    fn empty_and_whitespace_are_clear_requests() {
        // These must be treated as "unbind", not as a parse error, so the
        // Settings "Clear" action can actually clear the shortcut.
        assert!(is_clear_request(""));
        assert!(is_clear_request("   "));
        assert!(is_clear_request("\t"));
    }

    #[test]
    fn real_accelerators_are_not_clear_requests() {
        assert!(!is_clear_request("Ctrl+Shift+U"));
        assert!(!is_clear_request("A"));
    }
}
