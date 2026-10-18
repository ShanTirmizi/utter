//! User preferences, stored as a small JSON file in the app's config folder.
//! The API key is deliberately NOT here; see `secrets.rs`.

use crate::words::Word;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// What to hold while speaking: "Fn" for the fn (globe) key on its own,
    /// the default, or a key combination such as "Control+Alt+Space".
    pub hotkey: String,
    /// Run the transcript through the cleanup model when one is set up.
    pub cleanup_enabled: bool,
    /// How a dictation is tidied: all at once, or piece by piece while the
    /// user talks.
    pub way: Way,
    /// Which service runs cleanup; an id from `providers::CATALOG`.
    pub provider: String,
    /// Model to ask for. Empty means the provider's default, if it has one.
    pub cleanup_model: String,
    /// Base URL, used only by the "custom" provider.
    pub base_url: String,
    /// Second key combination: hold it to have the dictation rewritten rather
    /// than just tidied. Empty switches the feature off.
    pub rewrite_hotkey: String,
    /// Which rewrite style to use: a preset id from `cleanup::REWRITE_STYLES`,
    /// or "custom" for the user's own instruction below.
    pub rewrite_style: String,
    /// The user's own rewrite instruction.
    pub rewrite_prompt: String,
    /// Names and terms the user wants spelled exactly this way, each with
    /// what the speech model tends to hear instead.
    pub dictionary: Vec<Word>,
    /// Words the user removed, lower case, so they are never learned or
    /// suggested again.
    pub forgotten: Vec<String>,
    /// Learn a word when the user corrects it in what Utter just pasted.
    pub learn_words: bool,
    /// Use names and terms in the window in front as hints while dictating.
    pub screen_words: bool,
    /// Also give the cleanup model the text around the cursor, so it can put
    /// misheard words right from what the user is writing about. Off unless
    /// the user turns it on: with a hosted provider, that text leaves the Mac.
    pub screen_context: bool,
    /// Names seen on screen that the user then said, offered for the list.
    pub suggested: Vec<crate::screen::Suggestion>,
    /// Keep past dictations on this Mac so they can be found and copied again.
    pub keep_history: bool,
    /// Also keep the audio of the most recent dictations, next to the
    /// history, to replay through other models. Off unless the user turns it
    /// on: it is a recording of their voice.
    pub keep_audio: bool,
    /// A quick tap of a hotkey starts a recording that runs until the next
    /// tap, instead of having to hold the keys down.
    pub hands_free: bool,
    /// While fn is the dictation hotkey, switch macOS's own globe-key action
    /// (emoji picker and so on) off, and put it back afterwards. On by
    /// default: with fn as the hotkey, the picker opening on every dictation
    /// is never wanted. A saved "off" is kept.
    pub quiet_globe: bool,
    /// Which speech model to use; a name from `models::CATALOG`.
    pub model: String,
    /// The language the multilingual model should expect, as a Whisper code
    /// ("fr"). Empty means detect it for each dictation.
    pub language: String,
    /// Words the user removed, kept for 30 days so they can be restored.
    pub removed: Vec<crate::words::Removed>,
    /// Ask GitHub once a day whether a newer release is out. Off unless the
    /// user turns it on: it is a network request.
    pub check_updates: bool,
    /// "system" follows the Mac's appearance; "light" or "dark" fixes it.
    pub appearance: String,
    /// The microphone to record from, by its Core Audio UID. Empty means the
    /// system default; a picked one that is unplugged falls back to it.
    pub microphone: String,
    /// Set once the first-run permissions screen has been completed.
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            // One key, which every Mac keyboard has and nothing else needs
            // held: hold it to talk, double-tap it for hands-free.
            hotkey: crate::fnkey::FN.into(),
            cleanup_enabled: true,
            way: Way::Accurate,
            provider: "anthropic".into(),
            cleanup_model: String::new(),
            base_url: String::new(),
            rewrite_hotkey: "Control+Alt+KeyR".into(),
            rewrite_style: "clear".into(),
            rewrite_prompt: String::new(),
            dictionary: Vec::new(),
            forgotten: Vec::new(),
            learn_words: true,
            screen_words: true,
            screen_context: false,
            suggested: Vec::new(),
            keep_history: true,
            keep_audio: false,
            hands_free: true,
            quiet_globe: true,
            // Base mishears names and jargon far more often, and cleanup
            // cannot always put that right. Small costs about a second per
            // stretch of speech on an M1 Pro; Large Turbo, the most accurate,
            // five or more, which is too slow to start everyone on.
            model: "small.en".into(),
            language: String::new(),
            removed: Vec::new(),
            check_updates: false,
            appearance: "system".into(),
            microphone: String::new(),
            onboarded: false,
        }
    }
}

/// How a dictation is tidied.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Way {
    /// The whole dictation in one go once the key comes up, so the model
    /// sees every sentence whole, wherever the speaker paused.
    #[default]
    Accurate,
    /// Each stretch while the user is still talking, so only the last one
    /// waits for the model; a pause mid-sentence can end a sentence early.
    Quick,
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

/// Missing or unreadable settings fall back to defaults rather than failing
/// startup, so a corrupt file can never lock someone out of the app.
pub fn load(app: &AppHandle) -> Settings {
    path(app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let file = path(app)?;
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(file, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_or_missing_fields_fall_back_to_defaults() {
        let parsed: Settings = serde_json::from_str(r#"{"hotkey":"Alt+KeyD"}"#).unwrap();
        assert_eq!(parsed.hotkey, "Alt+KeyD");
        assert_eq!(parsed.model, Settings::default().model);
        assert!(parsed.cleanup_enabled);
        // New users start on Small, more accurate than Base; Base stays on offer.
        assert_eq!(Settings::default().model, "small.en");
        // Settings saved before providers existed keep working with Claude.
        assert_eq!(parsed.provider, "anthropic");
        // Nobody's voice is kept unless they asked for it.
        assert!(!parsed.keep_audio);
        // New users dictate with fn, and macOS's globe action is held off
        // while they do; a combination someone saved is kept.
        assert_eq!(Settings::default().hotkey, "Fn");
        assert!(Settings::default().quiet_globe);
        let saved: Settings = serde_json::from_str(r#"{"hotkey":"Control+Alt+Space"}"#).unwrap();
        assert_eq!(saved.hotkey, "Control+Alt+Space");
    }

    #[test]
    fn settings_never_serialise_a_key_field() {
        let text = serde_json::to_string(&Settings::default()).unwrap().to_lowercase();
        assert!(!text.contains("key\":") || text.contains("hotkey\":"));
        assert!(!text.contains("apikey") && !text.contains("api_key"));
    }
}
