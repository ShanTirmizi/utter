//! What the replay harness (`examples/replay.rs`) needs from the app without
//! running it: the recordings kept with Keep recordings on, what to score
//! each one against, the user's settings, and the tidy step exactly as a
//! dictation runs it.
//!
//! A recording is scored against, in order of preference:
//!
//! 1. a hand-written reference from a labels file,
//! 2. the text as the user corrected it in the app it was pasted into,
//! 3. the text as pasted, when the user left it as it was.
//!
//! The last is right by the user's own account, but it also flatters the
//! settings that wrote it; a label beats both.

use crate::{
    audio,
    cleanup::{self, Task},
    context::{AppContext, Category},
    history,
    pipeline::{Dictation, Mode},
    providers::{self, Endpoint, NotReady},
    settings::Settings,
    words,
};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

/// Utter's folder in Application Support, where it keeps its settings,
/// history, models and recordings.
pub fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join("Library/Application Support/dev.utter.app")
}

/// Where a reference came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Source {
    /// Written by hand in a labels file.
    Labelled,
    /// What the user changed the pasted text to.
    Corrected,
    /// The pasted text, which the user left as it was.
    Accepted,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Labelled => "labelled",
            Source::Corrected => "corrected",
            Source::Accepted => "accepted",
        }
    }
}

/// One recording to replay.
#[derive(Debug, Clone)]
pub struct Recording {
    pub file: PathBuf,
    /// The kind of app it was dictated into, which decides the writing style.
    category: Category,
    /// What Utter wrote at the time; empty for a recording with no history.
    pub written: String,
    /// What it should have said, and where that came from. `None` when
    /// nothing says: a rewrite or an edit of selected text, which a tidy
    /// cannot be compared with, or a dictation nobody could follow.
    pub reference: Option<(String, Source)>,
}

/// Read a labels file: one recording per line, its file name, a tab, and
/// what was said. Blank lines and lines starting with # are skipped.
pub fn read_labels(file: &Path) -> Result<HashMap<String, String>, String> {
    let text = fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(parse_labels(&text))
}

fn parse_labels(text: &str) -> HashMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('\t'))
        .map(|(file, said)| (file.trim().to_string(), said.trim().to_string()))
        .filter(|(file, said)| !file.is_empty() && !said.is_empty())
        .collect()
}

/// What a history entry can be scored against, short of a label.
fn reference_from(entry: &Dictation) -> Option<(String, Source)> {
    if entry.mode != Mode::Tidy || entry.selection.is_some() {
        return None;
    }
    let edit = entry.edit.as_ref()?;
    // Left as it was only counts when Utter saw the user finish with it.
    if edit.changed == 0 && !edit.unwatched {
        return Some((entry.text.clone(), Source::Accepted));
    }
    edit.corrected.clone().map(|text| (text, Source::Corrected))
}

/// The recordings in `folder`. That is either Utter's own folder (see
/// `data_dir`), with the history saying what each recording was, or any
/// folder of WAV files, scored against `labels` alone.
pub fn gather(folder: &Path, labels: &HashMap<String, String>) -> Vec<Recording> {
    let history = folder.join("history.json");
    let (wavs, entries) = if history.is_file() {
        (folder.join("recordings"), history::load_from(&history))
    } else {
        (folder.to_path_buf(), Vec::new())
    };
    let mut found = Vec::new();
    let mut named = std::collections::HashSet::new();
    for entry in &entries {
        let Some(name) = &entry.recording else { continue };
        let file = wavs.join(name);
        if !file.is_file() {
            continue;
        }
        named.insert(name.clone());
        let reference = match labels.get(name) {
            Some(said) => Some((said.clone(), Source::Labelled)),
            None => reference_from(entry),
        };
        found.push(Recording { file, category: entry.app.category, written: entry.text.clone(), reference });
    }
    // Any other WAV in the folder, such as a test set copied in by hand.
    let mut others: Vec<PathBuf> = fs::read_dir(&wavs)
        .into_iter()
        .flatten()
        .flatten()
        .map(|file| file.path())
        .filter(|path| path.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")))
        .filter(|path| !named.contains(&*path.file_name().unwrap_or_default().to_string_lossy()))
        .collect();
    others.sort();
    for file in others {
        let name = file.file_name().unwrap_or_default().to_string_lossy().to_string();
        let reference = labels.get(&name).map(|said| (said.clone(), Source::Labelled));
        found.push(Recording { file, category: Category::Other, written: String::new(), reference });
    }
    found
}

/// Read a WAV file as 16 kHz mono, whatever it was recorded at.
pub fn read_wav(file: &Path) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::open(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let spec = reader.spec();
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = (1_i64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().filter_map(Result::ok).map(|s| s as f32 / scale).collect()
        }
    };
    let channels = spec.channels.max(1) as usize;
    let mono: Vec<f32> = interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    Ok(audio::resample(&mono, spec.sample_rate, audio::TARGET_RATE))
}

/// The user's saved settings, as far as replaying needs them.
pub struct Setup {
    settings: Settings,
}

impl Setup {
    /// Read `settings.json` from `folder`; defaults if there is none.
    pub fn load(folder: &Path) -> Self {
        let settings = fs::read_to_string(folder.join("settings.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { settings }
    }

    /// The speech model chosen in the app.
    pub fn model(&self) -> &str {
        &self.settings.model
    }

    /// The model file for `name`: a path as given, or a model from the
    /// catalog that the app downloaded into `folder`.
    pub fn model_file(folder: &Path, name: &str) -> PathBuf {
        let given = PathBuf::from(name);
        if given.is_file() { given } else { folder.join("models").join(format!("ggml-{name}.bin")) }
    }

    /// The language to tell the speech model, as the app would.
    pub fn language(&self, model: &str) -> String {
        // A file's name ends in ".en.bin" for an English-only model.
        let name = model.trim_end_matches(".bin");
        crate::models::language(name, &self.settings.language)
    }

    /// The user's words, as hints for the speech model. Names read from the
    /// screen helped at the time too, but were never saved.
    pub fn hints(&self) -> Vec<String> {
        // As the app gives them, less the names on screen: the user's words,
        // then the tech names Utter knows.
        let mut hints = words::spellings(&self.settings.dictionary);
        if without_tech() {
            return hints;
        }
        let known: std::collections::HashSet<String> = hints.iter().map(|h| h.to_lowercase()).collect();
        hints.extend(crate::tech::hints().into_iter().filter(|h| !known.contains(&h.to_lowercase())));
        hints
    }

    /// Choose the accurate or the quick way, whatever the saved settings say.
    pub fn set_way(&mut self, way: crate::settings::Way) {
        self.settings.way = way;
    }
}

/// With `UTTER_REPLAY_NO_TECH` set, the tech names Utter knows are left out
/// everywhere, to measure what they add.
fn without_tech() -> bool {
    std::env::var_os("UTTER_REPLAY_NO_TECH").is_some()
}

/// The tidy step, as one of the configurations being compared.
pub struct Cleaner {
    /// "none", or the provider and model: "anthropic/claude-haiku-5-5".
    pub label: String,
    endpoint: Option<Endpoint>,
}

/// The tidy step's result for one recording.
pub struct Tidied {
    pub text: String,
    /// How long the model took; `None` when it was not asked.
    pub model_ms: Option<u64>,
    /// The model failed, so the words went in as heard, as they would in the app.
    pub failed: bool,
}

impl Cleaner {
    /// No tidying: the speech model's text as it is.
    pub fn none() -> Self {
        Self { label: "none".into(), endpoint: None }
    }

    /// The provider and model chosen in the app.
    pub fn saved(setup: &Setup) -> Result<Self, String> {
        let provider = setup.settings.provider.clone();
        Self::with(setup, &provider, &setup.settings.cleanup_model)
    }

    /// `spec` is "provider/model", a provider id from the app's list and one
    /// of its models: "groq/llama-3.1-8b-instant", "ollama/gemma3:4b". The
    /// custom provider uses the base URL saved in the app.
    pub fn named(setup: &Setup, spec: &str) -> Result<Self, String> {
        let (provider, model) = spec.split_once('/').unwrap_or((spec, ""));
        Self::with(setup, provider, model)
    }

    /// The key comes from `UTTER_API_KEY`, or else the Keychain entry the app
    /// saved, which macOS may ask about.
    fn with(setup: &Setup, provider: &str, model: &str) -> Result<Self, String> {
        let settings = Settings {
            provider: provider.to_string(),
            cleanup_model: model.to_string(),
            ..setup.settings.clone()
        };
        let needs_key = providers::find(provider).is_some_and(|p| p.needs_key);
        let key = std::env::var("UTTER_API_KEY").ok().filter(|k| !k.trim().is_empty());
        let key = key.or_else(|| needs_key.then(|| crate::secrets::get(provider)).flatten());
        let endpoint = providers::endpoint(&settings, key).map_err(|not_ready| match not_ready {
            NotReady::NoKey => format!("{provider}: no key. Set UTTER_API_KEY, or add one in Utter."),
            NotReady::Problem(text) => format!("{provider}: {text}"),
        })?;
        Ok(Self { label: format!("{provider}/{}", endpoint.model), endpoint: Some(endpoint) })
    }

    /// Tidy `raw` as a dictation into the recording's app would be: the same
    /// shortcut for text that needs nothing, the same prompt, the same
    /// paragraphs. One difference: the app tidies a long dictation a piece at
    /// a time while the user is still talking, and this tidies it whole.
    pub fn tidy(&self, raw: &str, recording: &Recording, setup: &Setup) -> Tidied {
        let category = recording.category;
        let mut terms = setup.settings.dictionary.clone();
        if !without_tech() {
            crate::tech::add_to(&mut terms);
        }
        let terms = &terms;
        let fixed = if category == Category::Code || without_tech() { raw.to_string() } else { crate::tech::fix_spoken(raw) };
        let raw = fixed.as_str();
        let as_heard = |model_ms, failed| Tidied { text: raw.to_string(), model_ms, failed };
        let mut tidied = match &self.endpoint {
            None => as_heard(None, false),
            Some(_) if cleanup::goes_as_is(raw, category, terms, setup.settings.way) => as_heard(None, false),
            Some(endpoint) => {
            let target = AppContext { category, ..AppContext::unknown() };
            // A replay asks far faster than anyone dictates, so a provider's
            // rate limit is waited out rather than counted as a failure. Only
            // the attempt that answers is timed.
            let mut waits = [5u64, 10, 20, 30, 30, 45, 60, 60].into_iter();
            let (result, ms) = loop {
                let started = Instant::now();
                let result =
                    tauri::async_runtime::block_on(cleanup::run(raw, &target, endpoint, &Task::Tidy, terms));
                let ms = Some(started.elapsed().as_millis() as u64);
                match (&result, waits.next()) {
                    (Err(e), Some(wait)) if e.to_string().contains("Rate limited") => {
                        eprintln!("[rate limited] {}: waiting {wait}s", self.label);
                        std::thread::sleep(std::time::Duration::from_secs(wait));
                    }
                    _ => break (result, ms),
                }
            };
            match result {
                Ok(text) => Tidied { text, model_ms: ms, failed: false },
                Err(e) => {
                    eprintln!("[tidy failed] {}: {e}", self.label);
                    as_heard(ms, true)
                }
            }
            }
        };
        // However it was produced, as in the app.
        if category != Category::Code {
            tidied.text = cleanup::paragraphs(&tidied.text);
        }
        tidied
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accuracy::Edit;

    #[test]
    fn labels_are_a_file_name_a_tab_and_the_words() {
        let labels = parse_labels("# file\twords\n\n1.wav\tAsk Claude.\nno tab here\n2.wav\t \n 3.wav \t Hi. \n");
        assert_eq!(labels.len(), 2);
        assert_eq!(labels["1.wav"], "Ask Claude.");
        assert_eq!(labels["3.wav"], "Hi.");
    }

    #[test]
    fn a_dictation_is_scored_against_what_the_user_kept() {
        let entry = |edit: Option<Edit>| Dictation { text: "Ask cloud.".into(), edit, ..Dictation::default() };
        assert_eq!(reference_from(&entry(None)), None);
        let left = Edit { words: 2, changed: 0, corrected: None, unwatched: false };
        assert_eq!(reference_from(&entry(Some(left.clone()))), Some(("Ask cloud.".into(), Source::Accepted)));
        // Never seen in the text box: not accepted.
        assert_eq!(reference_from(&entry(Some(Edit { unwatched: true, ..left }))), None);
        let fixed = Edit { words: 2, changed: 1, corrected: Some("Ask Claude.".into()), unwatched: false };
        assert_eq!(reference_from(&entry(Some(fixed.clone()))), Some(("Ask Claude.".into(), Source::Corrected)));
        // Edited, but the text was not kept.
        assert_eq!(reference_from(&entry(Some(Edit { corrected: None, ..fixed.clone() }))), None);
        // A rewrite is not a transcript.
        let rewrite = Dictation { mode: Mode::Rewrite, ..entry(Some(fixed)) };
        assert_eq!(reference_from(&rewrite), None);
    }

    #[test]
    fn recordings_are_found_with_their_history_or_on_their_own() {
        let folder = std::env::temp_dir().join(format!("utter-replay-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("recordings")).unwrap();
        let wav = |path: &Path| {
            let spec = hound::WavSpec { channels: 2, sample_rate: 48_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
            let mut writer = hound::WavWriter::create(path, spec).unwrap();
            for _ in 0..4_800 {
                writer.write_sample(i16::MAX / 2).unwrap();
                writer.write_sample(0_i16).unwrap();
            }
            writer.finalize().unwrap();
        };
        wav(&folder.join("recordings/100.wav"));
        wav(&folder.join("recordings/extra.wav"));
        let entries = vec![
            Dictation {
                text: "Ask cloud.".into(),
                finished_at: 100,
                recording: Some("100.wav".into()),
                edit: Some(Edit { words: 2, changed: 1, corrected: Some("Ask Claude.".into()), unwatched: false }),
                ..Dictation::default()
            },
            // Its recording is gone.
            Dictation { finished_at: 200, recording: Some("200.wav".into()), ..Dictation::default() },
        ];
        fs::write(folder.join("history.json"), serde_json::to_string(&entries).unwrap()).unwrap();

        let labels = parse_labels("extra.wav\tHello there.");
        let found = gather(&folder, &labels);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].reference, Some(("Ask Claude.".into(), Source::Corrected)));
        assert_eq!(found[1].reference, Some(("Hello there.".into(), Source::Labelled)));
        // A label wins over the user's correction.
        let found = gather(&folder, &parse_labels("100.wav\tAsk Claude, please."));
        assert_eq!(found[0].reference.as_ref().unwrap().1, Source::Labelled);
        // Any folder of WAV files works too.
        let loose = gather(&folder.join("recordings"), &labels);
        assert_eq!(loose.len(), 2);
        assert_eq!(loose[0].reference, None);

        // Down to 16 kHz mono: a tenth of a second, the two channels averaged.
        let samples = read_wav(&folder.join("recordings/100.wav")).unwrap();
        assert_eq!(samples.len(), 1_600);
        assert!((samples[0] - 0.25).abs() < 0.01);
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_model_is_a_file_or_a_name_from_the_catalog() {
        let folder = Path::new("/somewhere");
        assert_eq!(Setup::model_file(folder, "base.en"), Path::new("/somewhere/models/ggml-base.en.bin"));
        let setup = Setup { settings: Settings { language: "fr".into(), ..Settings::default() } };
        assert_eq!(setup.language("base.en"), "en");
        assert_eq!(setup.language("/models/ggml-small.en.bin"), "en");
        assert_eq!(setup.language("large-v3-turbo"), "fr");
    }

    #[test]
    fn no_cleanup_leaves_the_words_as_heard() {
        let setup = Setup { settings: Settings::default() };
        let recording = Recording { file: PathBuf::new(), category: Category::Other, written: String::new(), reference: None };
        let tidied = Cleaner::none().tidy("um so hello", &recording, &setup);
        assert_eq!(tidied.text, "um so hello");
        assert_eq!(tidied.model_ms, None);
        assert!(!tidied.failed);
    }
}
