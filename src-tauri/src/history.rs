//! Past dictations, kept on this Mac so they can be found and copied again.
//!
//! They live in one JSON file in the app's data folder and go nowhere else.
//! Only the most recent ones are kept, and the user can clear them or switch
//! the whole thing off.

use crate::pipeline::Dictation;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};

/// How many dictations are remembered; older ones are dropped.
pub const LIMIT: usize = 200;

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("history.json"))
}

/// The saved dictations, oldest first. A missing or unreadable file is an
/// empty history, never an error.
pub fn load(app: &AppHandle) -> Vec<Dictation> {
    path(app).map(|file| load_from(&file)).unwrap_or_default()
}

fn load_from(file: &Path) -> Vec<Dictation> {
    fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Write the history, replacing what was there. Written to a temporary file
/// first, so a crash mid-write cannot leave a half-written history behind.
pub fn save(app: &AppHandle, entries: &[Dictation]) -> Result<(), String> {
    save_to(&path(app)?, entries)
}

fn save_to(file: &Path, entries: &[Dictation]) -> Result<(), String> {
    let partial = file.with_extension("json.part");
    let text = serde_json::to_string(entries).map_err(|e| e.to_string())?;
    fs::write(&partial, text).map_err(|e| e.to_string())?;
    // Everything the user has dictated is in here: readable by them alone.
    fs::set_permissions(&partial, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    fs::rename(&partial, file).map_err(|e| e.to_string())
}

/// Add one dictation, dropping the oldest beyond `LIMIT`.
pub fn push(entries: &mut Vec<Dictation>, dictation: Dictation) {
    entries.push(dictation);
    if entries.len() > LIMIT {
        let extra = entries.len() - LIMIT;
        entries.drain(..extra);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(n: u64) -> Dictation {
        Dictation {
            text: format!("dictation {n}"),
            finished_at: n,
            ..Dictation::default()
        }
    }

    #[test]
    fn only_the_most_recent_are_kept() {
        let mut entries = Vec::new();
        for n in 0..(LIMIT as u64 + 25) {
            push(&mut entries, entry(n));
        }
        assert_eq!(entries.len(), LIMIT);
        assert_eq!(entries.first().unwrap().finished_at, 25);
        assert_eq!(entries.last().unwrap().finished_at, LIMIT as u64 + 24);
    }

    #[test]
    fn saving_and_loading_a_file() {
        let dir = std::env::temp_dir().join(format!("utter-history-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("history.json");
        assert!(load_from(&file).is_empty(), "a missing file is an empty history");

        save_to(&file, &[entry(1), entry(2)]).unwrap();
        let back = load_from(&file);
        assert_eq!(back.len(), 2);
        assert_eq!(back[1].text, "dictation 2");
        assert_eq!(fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(!file.with_extension("json.part").exists());

        save_to(&file, &[]).unwrap();
        assert!(load_from(&file).is_empty());
        fs::write(&file, "not json").unwrap();
        assert!(load_from(&file).is_empty(), "a damaged file is an empty history");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_entry_survives_a_round_trip_and_older_files_still_load() {
        let text = serde_json::to_string(&[entry(7)]).unwrap();
        let back: Vec<Dictation> = serde_json::from_str(&text).unwrap();
        assert_eq!(back[0].text, "dictation 7");
        // A file written before a field existed loads with that field empty.
        let old: Vec<Dictation> = serde_json::from_str(r#"[{"raw":"hi","text":"Hi."}]"#).unwrap();
        assert_eq!(old[0].text, "Hi.");
        assert!(old[0].selection.is_none());
    }
}
