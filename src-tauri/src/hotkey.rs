//! The global push-to-talk hotkey.
//!
//! It is registered with the system as a hotkey, so macOS delivers the press
//! and the release to Utter and to no other app, and no keystroke-monitoring
//! permission is needed.

use crate::pipeline;
use std::str::FromStr;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Check that a stored hotkey string is a usable key combination.
pub fn parse(hotkey: &str) -> Result<Shortcut, String> {
    Shortcut::from_str(hotkey)
        .map_err(|_| format!("\"{hotkey}\" is not a usable key combination."))
}

/// Make `hotkey` the one and only dictation hotkey.
pub fn register(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let shortcut = parse(hotkey)?;
    let shortcuts = app.global_shortcut();
    shortcuts.unregister_all().map_err(|e| e.to_string())?;
    shortcuts
        .on_shortcut(shortcut, |app, _shortcut, event| match event.state {
            ShortcutState::Pressed => pipeline::on_press(app),
            ShortcutState::Released => pipeline::on_release(app),
        })
        .map_err(|e| format!("Could not register that hotkey. Another app may be using it. ({e})"))
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn accepts_what_the_settings_screen_produces() {
        for hotkey in ["Control+Alt+Space", "Alt+KeyD", "Control+Shift+Digit1", "F5", "Super+Shift+KeyU"] {
            assert!(parse(hotkey).is_ok(), "{hotkey} should parse");
        }
    }

    #[test]
    fn rejects_nonsense() {
        assert!(parse("").is_err());
        assert!(parse("Control+NotAKey").is_err());
    }
}
