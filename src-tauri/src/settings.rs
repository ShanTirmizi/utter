//! User preferences, stored as a small JSON file in the app's config folder.
//! The API key is deliberately NOT here; see `secrets.rs`.

use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Key combination to hold while speaking, e.g. "Control+Alt+Space".
    pub hotkey: String,
    /// Run the transcript through the cleanup model when one is set up.
    pub cleanup_enabled: bool,
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
    /// Names and terms the user wants spelled exactly this way.
    pub dictionary: Vec<String>,
    /// Keep past dictations on this Mac so they can be found and copied again.
    pub keep_history: bool,
    /// A quick tap of a hotkey starts a recording that runs until the next
    /// tap, instead of having to hold the keys down.
    pub hands_free: bool,
    /// Which speech model to use; a name from `models::CATALOG`.
    pub model: String,
    /// Set once the first-run permissions screen has been completed.
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "Control+Alt+Space".into(),
            cleanup_enabled: true,
            provider: "anthropic".into(),
            cleanup_model: String::new(),
            base_url: String::new(),
            rewrite_hotkey: "Control+Alt+KeyR".into(),
            rewrite_style: "clear".into(),
            rewrite_prompt: String::new(),
            dictionary: Vec::new(),
            keep_history: true,
            hands_free: true,
            model: "base.en".into(),
            onboarded: false,
        }
    }
}

/// Most terms a dictionary may hold, and the longest a term may be. The list
/// is given to the speech model, which only reads a few hundred words of it.
pub const MAX_TERMS: usize = 200;
const MAX_TERM_CHARS: usize = 60;

/// Tidy a dictionary as typed: one term per entry, trimmed, without empty
/// lines or repeats, within the size limits.
pub fn clean_dictionary(terms: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    terms
        .iter()
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty())
        .map(|t| t.chars().take(MAX_TERM_CHARS).collect::<String>())
        .filter(|t| seen.insert(t.to_lowercase()))
        .take(MAX_TERMS)
        .collect()
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
        // Settings saved before providers existed keep working with Claude.
        assert_eq!(parsed.provider, "anthropic");
    }

    #[test]
    fn a_dictionary_is_trimmed_deduplicated_and_capped() {
        let typed: Vec<String> = ["  Priya  ", "", "priya", "Kubernetes", "two   words", "\n"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(clean_dictionary(&typed), ["Priya", "Kubernetes", "two words"]);
        let many: Vec<String> = (0..500).map(|i| format!("term{i}")).collect();
        assert_eq!(clean_dictionary(&many).len(), MAX_TERMS);
        assert_eq!(clean_dictionary(&["x".repeat(500)])[0].chars().count(), MAX_TERM_CHARS);
    }

    #[test]
    fn settings_never_serialise_a_key_field() {
        let text = serde_json::to_string(&Settings::default()).unwrap().to_lowercase();
        assert!(!text.contains("key\":") || text.contains("hotkey\":"));
        assert!(!text.contains("apikey") && !text.contains("api_key"));
    }
}
