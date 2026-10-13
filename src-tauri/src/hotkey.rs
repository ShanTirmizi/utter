//! The global push-to-talk hotkeys: one to dictate, and an optional second
//! one to dictate and have the result rewritten.
//!
//! They are registered with the system as hotkeys, so macOS delivers the
//! press and the release to Utter and to no other app, and no
//! keystroke-monitoring permission is needed.

use crate::{
    pipeline::{self, Mode},
    settings::Settings,
};
use std::str::FromStr;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Check that a stored hotkey string is a usable key combination.
pub fn parse(hotkey: &str) -> Result<Shortcut, String> {
    Shortcut::from_str(hotkey)
        .map_err(|_| format!("\"{hotkey}\" is not a usable key combination."))
}

/// The two shortcuts described by the settings, checked for clashes. The
/// rewrite shortcut is `None` when that feature is switched off.
pub fn plan(settings: &Settings) -> Result<(Shortcut, Option<Shortcut>), String> {
    let dictate = parse(&settings.hotkey)?;
    let rewrite = match settings.rewrite_hotkey.trim() {
        "" => None,
        text => Some(parse(text)?),
    };
    if rewrite.is_some_and(|r| r.id() == dictate.id()) {
        return Err("The rewrite hotkey must be different from the dictation hotkey.".into());
    }
    Ok((dictate, rewrite))
}

/// Make the hotkeys in `settings` the only ones Utter listens for.
pub fn register(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let (dictate, rewrite) = plan(settings)?;
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())?;
    bind(app, dictate, Mode::Tidy)?;
    if let Some(rewrite) = rewrite {
        bind(app, rewrite, Mode::Rewrite)?;
    }
    Ok(())
}

fn bind(app: &AppHandle, shortcut: Shortcut, mode: Mode) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| match event.state {
            ShortcutState::Pressed => pipeline::on_press(app, mode),
            ShortcutState::Released => pipeline::on_release(app, mode),
        })
        .map_err(|e| format!("Could not register that hotkey. Another app may be using it. ({e})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(hotkey: &str, rewrite: &str) -> Settings {
        Settings {
            hotkey: hotkey.into(),
            rewrite_hotkey: rewrite.into(),
            ..Settings::default()
        }
    }

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

    #[test]
    fn the_default_hotkeys_are_valid_and_different() {
        let (_, rewrite) = plan(&Settings::default()).unwrap();
        assert!(rewrite.is_some());
    }

    #[test]
    fn an_empty_rewrite_hotkey_switches_the_feature_off() {
        let (_, rewrite) = plan(&settings("Control+Alt+Space", "  ")).unwrap();
        assert!(rewrite.is_none());
    }

    #[test]
    fn the_two_hotkeys_cannot_be_the_same() {
        assert!(plan(&settings("Control+Alt+Space", "Control+Alt+Space")).is_err());
        // Same keys written in a different order are still the same hotkey.
        assert!(plan(&settings("Control+Alt+Space", "Alt+Control+Space")).is_err());
        assert!(plan(&settings("Control+Alt+Space", "Control+Alt+KeyR")).is_ok());
    }

    #[test]
    fn a_bad_rewrite_hotkey_is_reported() {
        assert!(plan(&settings("Control+Alt+Space", "Control+NotAKey")).is_err());
    }
}
