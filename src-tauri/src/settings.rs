//! User preferences, stored as a small JSON file in the app's config folder.
//! The API key is deliberately NOT here; see `secrets.rs`.

use crate::words::Word;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};
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
    /// Model for the rewrite hotkey. Empty means the provider's rewrite
    /// model if it has one, otherwise the cleanup model.
    pub rewrite_model: String,
    /// Names and terms the user wants spelled exactly this way, each with
    /// what the speech model tends to hear instead.
    pub dictionary: Vec<Word>,
    /// Words the user removed, lower case, so they are never learned or
    /// suggested again.
    pub forgotten: Vec<String>,
    /// How hard the speech model and cleanup lean on the user's words and
    /// the tech names Utter knows: off, light, normal (the default) or
    /// strong. See `words::Boost`.
    pub vocabulary_boost: crate::words::Boost,
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
    /// Mute the Mac's sound while recording, so music or a video playing
    /// out loud stays out of the dictation, and put it back afterwards.
    pub mute_while_dictating: bool,
    /// Fixed changes made to everything written into a particular app, with
    /// or without cleanup.
    pub app_rules: Vec<AppRule>,
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
            rewrite_model: String::new(),
            dictionary: Vec::new(),
            forgotten: Vec::new(),
            vocabulary_boost: crate::words::Boost::Normal,
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
            mute_while_dictating: false,
            app_rules: Vec::new(),
        }
    }
}

/// What Utter always does to text written into one app.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppRule {
    /// The app or site as Utter names it under a dictation ("Slack",
    /// "Gmail"), matched without regard to case.
    pub app: String,
    /// Start each sentence with a small letter, as people often type in chat.
    pub lower_case: bool,
    /// Leave off the full stop at the very end.
    pub no_full_stop: bool,
}

impl Settings {
    /// The rule for the app named `app`, if the user made one.
    pub fn rule_for(&self, app: &str) -> Option<&AppRule> {
        let app = app.trim().to_lowercase();
        self.app_rules.iter().find(|rule| rule.app.trim().to_lowercase() == app)
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

/// The settings as saved, held in memory once read. Every change goes
/// through `update`, under this lock, so two changes at once (a word learned
/// while the window saves, say) can never undo each other.
static CURRENT: Mutex<Option<Settings>> = Mutex::new(None);

fn current() -> MutexGuard<'static, Option<Settings>> {
    CURRENT.lock().unwrap_or_else(|e| e.into_inner())
}

/// The saved settings. A missing file is a first run: the defaults, and
/// nothing is written. A damaged one never stops the app starting, and is
/// never written over either: it is set aside, and the copy kept from the
/// save before is used instead, or the defaults if that is no good too.
pub fn load(app: &AppHandle) -> Settings {
    let mut held = current();
    held.get_or_insert_with(|| read_saved(app)).clone()
}

fn read_saved(app: &AppHandle) -> Settings {
    match path(app) {
        Ok(file) => load_from(&file),
        Err(e) => {
            eprintln!("[settings] could not find the settings folder: {e}");
            Settings::default()
        }
    }
}

/// Change the settings and save them, as one step no other change can come
/// between. `change` works on a copy: if it returns an error, or the save
/// fails, nothing is changed. `change` must not call `load` or `update`.
pub fn update<R>(app: &AppHandle, change: impl FnOnce(&mut Settings) -> Result<R, String>) -> Result<R, String> {
    let file = path(app)?;
    update_in(&CURRENT, &file, change)
}

fn update_in<R>(
    slot: &Mutex<Option<Settings>>,
    file: &Path,
    change: impl FnOnce(&mut Settings) -> Result<R, String>,
) -> Result<R, String> {
    let mut held = slot.lock().unwrap_or_else(|e| e.into_inner());
    let mut settings = held.get_or_insert_with(|| load_from(file)).clone();
    let outcome = change(&mut settings)?;
    save_to(file, &settings)?;
    *held = Some(settings);
    Ok(outcome)
}

/// The copy of the last good settings, kept by every save.
fn backup_path(file: &Path) -> PathBuf {
    sibling(file, ".bak")
}

fn sibling(file: &Path, suffix: &str) -> PathBuf {
    let mut name = file.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    file.with_file_name(name)
}

fn read(file: &Path) -> Result<Settings, String> {
    let text = fs::read_to_string(file).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

pub(crate) fn load_from(file: &Path) -> Settings {
    let problem = match fs::read_to_string(file) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(settings) => return settings,
            Err(e) => e.to_string(),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Settings::default(),
        Err(e) => e.to_string(),
    };
    eprintln!("[settings] {} is damaged ({problem})", file.display());
    set_aside(file);

    let backup = backup_path(file);
    match read(&backup) {
        Ok(settings) => {
            eprintln!("[settings] using the copy from the save before, {}", backup.display());
            // Put it back, so the next start finds good settings too.
            match serde_json::to_string_pretty(&settings) {
                Ok(text) => {
                    if let Err(e) = crate::atomic::write(file, text.as_bytes(), None) {
                        eprintln!("[settings] could not restore {}: {e}", file.display());
                    }
                }
                Err(e) => eprintln!("[settings] could not restore {}: {e}", file.display()),
            }
            settings
        }
        Err(e) => {
            eprintln!("[settings] no usable copy at {} ({e}); starting from the defaults", backup.display());
            Settings::default()
        }
    }
}

/// Move a damaged settings file out of the way, to
/// `settings.json.corrupt-<unix time>`, so nothing ever writes over it and
/// it can still be looked at or recovered by hand.
fn set_aside(file: &Path) {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut target = sibling(file, &format!(".corrupt-{seconds}"));
    let mut n = 1;
    while target.exists() {
        target = sibling(file, &format!(".corrupt-{seconds}-{n}"));
        n += 1;
    }
    let moved = fs::rename(file, &target).or_else(|_| {
        // Could not move it: keep a copy and then remove it, so a later
        // save cannot be mistaken for writing over the user's settings.
        fs::copy(file, &target).and_then(|_| fs::remove_file(file))
    });
    match moved {
        Ok(()) => {
            crate::atomic::sync_dir(file);
            eprintln!("[settings] kept the damaged file as {}", target.display());
        }
        Err(e) => eprintln!("[settings] could not set {} aside: {e}", file.display()),
    }
}

/// Write the settings, replacing what was there: the file is written whole
/// to a temporary file and renamed into place, so a crash mid-save leaves the
/// old settings or the new, never half of either. The settings being
/// replaced, if they are good, are kept first as `settings.json.bak`.
pub(crate) fn save_to(file: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    // Only a file that loads is worth keeping: a damaged one must not
    // replace a good copy.
    if let Ok(previous) = fs::read(file)
        && serde_json::from_slice::<Settings>(&previous).is_ok()
        && let Err(e) = crate::atomic::write(&backup_path(file), &previous, None)
    {
        eprintln!("[settings] could not keep a copy of the settings: {e}");
    }
    crate::atomic::write(file, text.as_bytes(), None)
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
    fn the_vocabulary_boost_is_saved_and_old_files_load_at_normal() {
        use crate::words::Boost;
        // Saved before the setting existed.
        let old: Settings = serde_json::from_str(r#"{"hotkey":"Fn","dictionary":["Priya"]}"#).unwrap();
        assert_eq!(old.vocabulary_boost, Boost::Normal);
        assert_eq!(old.dictionary.len(), 1);
        for (saved, boost) in [("off", Boost::Off), ("light", Boost::Light), ("normal", Boost::Normal), ("strong", Boost::Strong)] {
            let parsed: Settings = serde_json::from_str(&format!(r#"{{"vocabularyBoost":"{saved}"}}"#)).unwrap();
            assert_eq!(parsed.vocabulary_boost, boost);
            let text = serde_json::to_string(&parsed).unwrap();
            assert!(text.contains(&format!(r#""vocabularyBoost":"{saved}""#)), "{text}");
        }
        // A value this version does not know costs only this setting.
        let odd: Settings = serde_json::from_str(r#"{"vocabularyBoost":"max","hotkey":"Alt+KeyD"}"#).unwrap();
        assert_eq!((odd.vocabulary_boost, odd.hotkey.as_str()), (Boost::Normal, "Alt+KeyD"));
    }

    /// A fresh, empty folder for one test.
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("utter-settings-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> =
            fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    fn corrupt_copies(dir: &Path) -> Vec<PathBuf> {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("settings.json.corrupt-"))
            .collect()
    }

    fn with_words(words: &[&str]) -> Settings {
        let list = serde_json::to_string(words).unwrap();
        serde_json::from_str(&format!(r#"{{"dictionary":{list}}}"#)).unwrap()
    }

    fn words_of(settings: &Settings) -> Vec<String> {
        settings.dictionary.iter().map(|w| w.word.clone()).collect()
    }

    #[test]
    fn a_missing_file_is_a_first_run_and_nothing_is_written() {
        let dir = folder("missing");
        let file = dir.join("settings.json");
        assert_eq!(load_from(&file), Settings::default());
        assert!(names(&dir).is_empty(), "{:?}", names(&dir));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn saving_keeps_the_last_good_settings_as_a_backup() {
        let dir = folder("save");
        let file = dir.join("settings.json");
        save_to(&file, &with_words(&["Priya"])).unwrap();
        assert_eq!(names(&dir), ["settings.json"], "the first save has nothing to back up");
        save_to(&file, &with_words(&["Priya", "Kubernetes"])).unwrap();
        assert_eq!(names(&dir), ["settings.json", "settings.json.bak"]);
        assert_eq!(words_of(&load_from(&file)), ["Priya", "Kubernetes"]);
        assert_eq!(words_of(&read(&backup_path(&file)).unwrap()), ["Priya"]);

        // A damaged file is never made the backup.
        fs::write(&file, "{\"dictionary\": [").unwrap();
        save_to(&file, &with_words(&["Ada"])).unwrap();
        assert_eq!(words_of(&read(&backup_path(&file)).unwrap()), ["Priya"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_interrupted_save_is_recovered_from_the_backup() {
        let dir = folder("interrupted");
        let file = dir.join("settings.json");
        let mut rules = with_words(&["Priya", "Kubernetes"]);
        rules.app_rules.push(AppRule { app: "Slack".into(), lower_case: true, no_full_stop: true });
        save_to(&file, &with_words(&["Priya"])).unwrap();
        save_to(&file, &rules).unwrap();
        save_to(&file, &rules).unwrap();

        // A crash halfway through writing: the file cut short, and the
        // temporary file left behind.
        let whole = fs::read_to_string(&file).unwrap();
        let cut = &whole[..whole.len() / 2];
        fs::write(&file, cut).unwrap();
        fs::write(crate::atomic::partial_path(&file), &whole[..10]).unwrap();

        let loaded = load_from(&file);
        assert_eq!(loaded, rules, "the backup's word list and app rules come back");
        // The damaged file is kept as it was, beside the restored one.
        let corrupt = corrupt_copies(&dir);
        assert_eq!(corrupt.len(), 1);
        assert_eq!(fs::read_to_string(&corrupt[0]).unwrap(), cut);
        assert_eq!(read(&file).unwrap(), rules, "settings.json is restored from the backup");

        // Saving from here works as usual and clears the leftover.
        save_to(&file, &loaded).unwrap();
        assert!(!crate::atomic::partial_path(&file).exists());
        assert_eq!(load_from(&file), rules);
        assert_eq!(corrupt_copies(&dir).len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_damaged_file_with_no_backup_is_kept_and_not_written_over() {
        let dir = folder("corrupt");
        let file = dir.join("settings.json");
        fs::write(&file, "not json at all").unwrap();

        assert_eq!(load_from(&file), Settings::default());
        let corrupt = corrupt_copies(&dir);
        assert_eq!(corrupt.len(), 1);
        assert_eq!(fs::read_to_string(&corrupt[0]).unwrap(), "not json at all");
        assert!(!file.exists(), "nothing is written until a real save");
        // Loading again does not touch the copy set aside.
        assert_eq!(load_from(&file), Settings::default());
        assert_eq!(corrupt_copies(&dir), corrupt);

        // A damaged backup is no better than none.
        fs::write(&file, [0xff, 0xfe, 0x00]).unwrap();
        fs::write(backup_path(&file), "{").unwrap();
        assert_eq!(load_from(&file), Settings::default());
        assert_eq!(corrupt_copies(&dir).len(), 2);
        assert!(!file.exists());

        // A real save writes the file, and the damaged copies stay.
        save_to(&file, &with_words(&["Priya"])).unwrap();
        assert_eq!(words_of(&load_from(&file)), ["Priya"]);
        assert_eq!(corrupt_copies(&dir).len(), 2);
        assert_eq!(fs::read_to_string(&corrupt[0]).unwrap(), "not json at all");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn changes_made_at_once_are_all_kept() {
        let dir = folder("update");
        let file = dir.join("settings.json");
        save_to(&file, &with_words(&["Priya"])).unwrap();
        let slot = Mutex::new(None);
        std::thread::scope(|scope| {
            for n in 0..8 {
                let (slot, file) = (&slot, &file);
                scope.spawn(move || {
                    update_in(slot, file, |s| {
                        s.dictionary.extend(with_words(&[&format!("Word{n}")]).dictionary);
                        Ok(())
                    })
                    .unwrap()
                });
            }
        });
        let saved = words_of(&load_from(&file));
        assert_eq!(saved.len(), 9, "{saved:?}");
        assert_eq!(slot.lock().unwrap().as_ref().map(words_of), Some(saved));

        // A change that fails leaves both the file and memory as they were.
        let before = fs::read_to_string(&file).unwrap();
        let failed: Result<(), String> = update_in(&slot, &file, |s| {
            s.dictionary.clear();
            Err("no".into())
        });
        assert!(failed.is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), before);
        assert_eq!(slot.lock().unwrap().as_ref().unwrap().dictionary.len(), 9);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn settings_never_serialise_a_key_field() {
        let text = serde_json::to_string(&Settings::default()).unwrap().to_lowercase();
        assert!(!text.contains("key\":") || text.contains("hotkey\":"));
        assert!(!text.contains("apikey") && !text.contains("api_key"));
    }
}
