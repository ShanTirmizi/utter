//! The global push-to-talk hotkeys: one to dictate, and an optional second
//! one to dictate and have the result rewritten.
//!
//! Key combinations are registered with the system as hotkeys, so macOS
//! delivers the press and the release to Utter and to no other app, and no
//! keystroke-monitoring permission is needed. The dictation hotkey can
//! instead be the fn key on its own, which is watched another way (see
//! `fnkey`).

use crate::{
    fnkey,
    pipeline::{self, Mode},
    settings::Settings,
};
use std::{str::FromStr, sync::Mutex};
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Check that a stored hotkey string is a usable key combination.
pub fn parse(hotkey: &str) -> Result<Shortcut, String> {
    Shortcut::from_str(hotkey)
        .map_err(|_| format!("\"{hotkey}\" is not a usable key combination."))
}

/// Shortcuts every Mac app relies on. Registering one as a global hotkey
/// would take it away from all of them: with Cmd+V taken, nothing could paste,
/// including Utter itself.
fn reserved(hotkey: &str) -> Option<String> {
    const KEYS: [&str; 14] = [
        "KeyA", "KeyC", "KeyF", "KeyN", "KeyP", "KeyQ", "KeyS", "KeyT", "KeyV", "KeyW", "KeyX", "KeyZ",
        "Tab", "Space",
    ];
    let parts: Vec<&str> = hotkey.split('+').map(str::trim).collect();
    let (key, modifiers) = parts.split_last()?;
    let command_only = modifiers.len() == 1
        && matches!(modifiers[0].to_lowercase().as_str(), "super" | "cmd" | "command" | "meta");
    (command_only && KEYS.contains(key)).then(|| {
        let name = key.strip_prefix("Key").unwrap_or(key);
        format!("⌘{name} is a shortcut every app uses. Pick a different combination.")
    })
}

/// The two shortcuts described by the settings, checked for clashes. The
/// dictation shortcut is `None` when it is the fn key, and the rewrite one
/// when that feature is switched off.
pub fn plan(settings: &Settings) -> Result<(Option<Shortcut>, Option<Shortcut>), String> {
    for hotkey in [&settings.hotkey, &settings.rewrite_hotkey] {
        if let Some(why) = reserved(hotkey) {
            return Err(why);
        }
    }
    if settings.rewrite_hotkey.trim() == fnkey::FN {
        return Err("The fn key can be the dictation hotkey, not the rewrite one.".into());
    }
    let dictate = match settings.hotkey.trim() {
        fnkey::FN => None,
        text => Some(parse(text)?),
    };
    let rewrite = match settings.rewrite_hotkey.trim() {
        "" => None,
        text => Some(parse(text)?),
    };
    if rewrite.is_some_and(|r| dictate.is_some_and(|d| r.id() == d.id())) {
        return Err("The rewrite hotkey must be different from the dictation hotkey.".into());
    }
    Ok((dictate, rewrite))
}

/// Why a hotkey is not working right now, if one is not. The window shows
/// this, because a dead hotkey otherwise looks exactly like a working one.
static PROBLEM: Mutex<Option<String>> = Mutex::new(None);

pub fn problem() -> Option<String> {
    PROBLEM.lock().unwrap().clone().or_else(fnkey::problem)
}

pub fn clear_problem() {
    *PROBLEM.lock().unwrap() = None;
}

/// Stop listening for both hotkeys, while the user records a new one. With
/// them live, pressing an existing hotkey during recording would start a
/// dictation instead of being captured.
pub fn pause(app: &AppHandle) -> Result<(), String> {
    fnkey::pause();
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())
}

/// Register the saved hotkeys, as at launch or after `pause`. If the rewrite
/// hotkey cannot be registered, dictation is still set up without it. Any
/// failure is remembered for `problem`.
pub fn restore(app: &AppHandle, settings: &Settings) {
    let outcome = match register(app, settings) {
        Ok(()) => None,
        Err(first) => {
            let dictation_only = Settings {
                rewrite_hotkey: String::new(),
                ..settings.clone()
            };
            match register(app, &dictation_only) {
                Ok(()) => Some(format!("The rewrite hotkey is not working. {first}")),
                Err(second) => Some(format!("The dictation hotkey is not working. {second}")),
            }
        }
    };
    if let Some(why) = &outcome {
        eprintln!("[utter] {why}");
    }
    *PROBLEM.lock().unwrap() = outcome;
}

/// Make the hotkeys in `settings` the only ones Utter listens for.
pub fn register(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let (dictate, rewrite) = plan(settings)?;
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())?;
    match dictate {
        Some(dictate) => {
            fnkey::disable();
            bind(app, dictate, Mode::Tidy)?;
        }
        None => fnkey::enable(app),
    }
    crate::globe::sync(app, dictate.is_none() && settings.quiet_globe);
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
    fn the_fn_key_can_be_the_dictation_hotkey_only() {
        let (dictate, rewrite) = plan(&settings("Fn", "Control+Alt+KeyR")).unwrap();
        assert!(dictate.is_none() && rewrite.is_some());
        assert!(plan(&settings("Control+Alt+Space", "Fn")).is_err());
        assert!(plan(&settings("Fn", "")).is_ok());
    }

    #[test]
    fn rejects_nonsense() {
        assert!(parse("").is_err());
        assert!(parse("Control+NotAKey").is_err());
    }

    #[test]
    fn the_default_hotkeys_are_valid_and_different() {
        // fn dictates, so no combination is registered for it.
        let (dictate, rewrite) = plan(&Settings::default()).unwrap();
        assert!(dictate.is_none() && rewrite.is_some());
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
    fn shortcuts_every_app_relies_on_are_refused() {
        assert!(plan(&settings("Super+KeyV", "")).is_err());
        assert!(plan(&settings("Control+Alt+Space", "Super+KeyC")).is_err());
        assert!(plan(&settings("Super+Space", "")).is_err());
        // The same keys with another modifier are fine.
        assert!(plan(&settings("Super+Shift+KeyV", "")).is_ok());
        assert!(plan(&settings("Control+KeyV", "")).is_ok());
        assert!(reserved("Super+KeyV").unwrap().contains("⌘V"));
        assert!(reserved("").is_none());
    }

    #[test]
    fn a_bad_rewrite_hotkey_is_reported() {
        assert!(plan(&settings("Control+Alt+Space", "Control+NotAKey")).is_err());
    }
}
