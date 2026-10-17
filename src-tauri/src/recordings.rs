//! Recordings of recent dictations, kept only when the user switches
//! **Keep recordings** on (it is off by default), so that other speech models
//! and prompts can be tried on what they really said (`examples/replay.rs`).
//!
//! Each is a 16 kHz WAV file in a `recordings` folder next to the history
//! file, named after its dictation, and belongs to that history entry: when
//! the entry goes, so does the recording. Only the newest `LIMIT` are kept.
//! They are never sent anywhere.

use crate::{audio::TARGET_RATE, pipeline::Dictation};
use std::{
    collections::HashSet,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};

/// How many recordings are kept; older ones are deleted.
pub const LIMIT: usize = 50;

/// The folder, which may not exist yet.
fn folder(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("recordings"))
}

/// The folder, made if need be.
fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = folder(app)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Recordings of the user's voice: theirs alone to open.
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// The file name for the dictation that finished at `finished_at`.
fn name(finished_at: u64) -> String {
    format!("{finished_at}.wav")
}

/// Is this one of ours? Only files named like `name` are ever deleted, so
/// anything else in the folder (a labels file, say) is left alone.
fn ours(file: &str) -> bool {
    file.strip_suffix(".wav").is_some_and(|stem| !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit()))
}

/// Save 16 kHz mono audio for one dictation. Returns the file name.
pub fn save(app: &AppHandle, finished_at: u64, samples: &[f32]) -> Result<String, String> {
    let file = name(finished_at);
    write_wav(&dir(app)?.join(&file), samples)?;
    Ok(file)
}

/// Written as 16-bit PCM, half the size of the floats it came from and well
/// beyond what speech needs.
fn write_wav(path: &Path, samples: &[f32]) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let partial = path.with_extension("wav.part");
    let mut writer = hound::WavWriter::create(&partial, spec).map_err(|e| e.to_string())?;
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer.write_sample(value).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())?;
    fs::set_permissions(&partial, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    fs::rename(&partial, path).map_err(|e| e.to_string())
}

/// Match the folder to the history: only the newest `LIMIT` entries keep
/// their recording, and every file no entry names is deleted, along with
/// anything half written. Returns true if an entry changed.
pub fn tidy(app: &AppHandle, entries: &mut [Dictation]) -> bool {
    match folder(app) {
        Ok(dir) => tidy_in(&dir, entries),
        Err(_) => false,
    }
}

fn tidy_in(dir: &Path, entries: &mut [Dictation]) -> bool {
    let mut changed = false;
    let mut kept = HashSet::new();
    // Newest first, which is the end of the history.
    for entry in entries.iter_mut().rev() {
        if let Some(file) = entry.recording.clone() {
            if kept.len() < LIMIT && ours(&file) && dir.join(&file).is_file() {
                kept.insert(file);
                continue;
            }
            entry.recording = None;
            changed = true;
        }
        // The corrected text is kept only to score a recording by.
        if let Some(edit) = entry.edit.as_mut().filter(|e| e.corrected.is_some()) {
            edit.corrected = None;
            changed = true;
        }
    }
    if let Ok(found) = fs::read_dir(dir) {
        for file in found.flatten() {
            let name = file.file_name().to_string_lossy().to_string();
            let stale = (ours(&name) && !kept.contains(&name)) || name.ends_with(".wav.part");
            if stale {
                let _ = fs::remove_file(file.path());
            }
        }
    }
    changed
}

/// Delete every recording and forget them in `entries`, because the user
/// switched Keep recordings off. Returns true if an entry changed.
pub fn remove_all(app: &AppHandle, entries: &mut [Dictation]) -> bool {
    let mut changed = false;
    for entry in entries.iter_mut() {
        changed |= entry.recording.take().is_some();
    }
    tidy(app, entries) || changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accuracy::Edit;

    fn folder(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("utter-recordings-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(dir: &Path, n: u64) -> Dictation {
        write_wav(&dir.join(name(n)), &[0.0, 0.5, -0.5]).unwrap();
        Dictation {
            finished_at: n,
            recording: Some(name(n)),
            edit: Some(Edit { words: 3, changed: 1, corrected: Some("fixed text".into()), unwatched: false }),
            ..Dictation::default()
        }
    }

    #[test]
    fn a_recording_is_a_private_16_khz_wav() {
        let dir = folder("wav");
        let file = dir.join(name(7));
        write_wav(&file, &[0.0, 0.25, -1.5]).unwrap();
        let reader = hound::WavReader::open(&file).unwrap();
        assert_eq!(reader.spec().sample_rate, 16_000);
        assert_eq!(reader.spec().channels, 1);
        let back: Vec<i16> = reader.into_samples::<i16>().map(Result::unwrap).collect();
        // Too loud is clipped, not wrapped around.
        assert_eq!(back, [0, i16::MAX / 4, -i16::MAX]);
        assert_eq!(fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(!file.with_extension("wav.part").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_the_newest_are_kept_and_strays_are_deleted() {
        let dir = folder("limit");
        let mut entries: Vec<Dictation> = (0..LIMIT as u64 + 3).map(|n| entry(&dir, n)).collect();
        // A file whose history entry is gone, half a file, and the user's own.
        write_wav(&dir.join(name(999_999)), &[0.0]).unwrap();
        fs::write(dir.join("123.wav.part"), b"").unwrap();
        fs::write(dir.join("labels.tsv"), b"1\thello").unwrap();

        assert!(tidy_in(&dir, &mut entries));
        let with: Vec<u64> = entries.iter().filter(|d| d.recording.is_some()).map(|d| d.finished_at).collect();
        assert_eq!(with.len(), LIMIT);
        assert_eq!(with[0], 3, "the oldest three lost theirs");
        assert_eq!(entries[0].edit.as_ref().unwrap().corrected, None);
        assert_eq!(entries[0].edit.as_ref().unwrap().changed, 1, "what was counted stays");
        assert!(!dir.join(name(0)).exists());
        assert!(dir.join(name(3)).exists());
        assert!(!dir.join(name(999_999)).exists());
        assert!(!dir.join("123.wav.part").exists());
        assert!(dir.join("labels.tsv").exists());
        // Nothing more to do the second time.
        assert!(!tidy_in(&dir, &mut entries));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_entry_whose_file_is_missing_or_odd_loses_it() {
        let dir = folder("missing");
        let mut entries = vec![entry(&dir, 1), entry(&dir, 2)];
        fs::remove_file(dir.join(name(1))).unwrap();
        entries[1].recording = Some("../elsewhere.wav".into());
        assert!(tidy_in(&dir, &mut entries));
        assert!(entries.iter().all(|d| d.recording.is_none()));
        assert!(!dir.join(name(2)).exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_our_own_names_count() {
        assert!(ours("1739999999999.wav"));
        assert!(!ours(".wav"));
        assert!(!ours("labels.wav"));
        assert!(!ours("../1.wav"));
        assert!(!ours("1.wav.part"));
    }
}
