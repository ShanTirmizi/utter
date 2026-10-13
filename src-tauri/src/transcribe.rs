//! Speech-to-text, entirely on this Mac, using whisper.cpp. No audio leaves
//! the machine and no API key is involved.

use std::{path::Path, sync::Mutex};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::audio::TARGET_RATE;

/// The model rejects very short clips, so shorter recordings are padded with
/// silence up to this length.
const MIN_SAMPLES: usize = (TARGET_RATE as usize * 11) / 10;

/// Below this peak level the recording is treated as silence. Transcribing
/// silence makes speech models invent text.
pub const SILENCE_PEAK: f32 = 0.003;

/// A loaded model, remembered so that only the first dictation pays the
/// load time.
struct Loaded {
    name: String,
    context: WhisperContext,
}

#[derive(Default)]
pub struct Transcriber {
    loaded: Mutex<Option<Loaded>>,
}

impl Transcriber {
    /// Load `name` from `path` unless it is already the model in memory.
    pub fn ensure_loaded(&self, name: &str, path: &Path) -> Result<(), String> {
        let mut loaded = self.loaded.lock().unwrap();
        if loaded.as_ref().is_some_and(|l| l.name == name) {
            return Ok(());
        }
        let path = path.to_str().ok_or("Model path is not valid UTF-8.")?;
        let context = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map_err(|e| format!("Could not load the speech model: {e}"))?;
        *loaded = Some(Loaded {
            name: name.to_string(),
            context,
        });
        Ok(())
    }

    /// Turn mono 16 kHz audio into text. `language` is a code such as "en",
    /// or "auto" to detect it.
    pub fn transcribe(&self, samples: &[f32], language: &str) -> Result<String, String> {
        let loaded = self.loaded.lock().unwrap();
        let loaded = loaded.as_ref().ok_or("No speech model is loaded.")?;

        let mut audio = samples.to_vec();
        if audio.len() < MIN_SAMPLES {
            audio.resize(MIN_SAMPLES, 0.0);
        }

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(language));
        params.set_n_threads(threads());
        // Each dictation stands alone; do not let one bleed into the next.
        params.set_no_context(true);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_no_timestamps(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        let mut state = loaded
            .context
            .create_state()
            .map_err(|e| format!("Could not start transcription: {e}"))?;
        state
            .full(params, &audio)
            .map_err(|e| format!("Transcription failed: {e}"))?;

        let text: String = state.as_iter().map(|segment| segment.to_string()).collect();
        Ok(tidy(&text))
    }
}

fn threads() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get().min(8) as i32)
        .unwrap_or(4)
}

/// Remove the model's non-speech annotations (such as "[BLANK_AUDIO]") and
/// collapse whitespace.
pub fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0usize;
    for ch in text.chars() {
        match ch {
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");

    // A transcript that is nothing but one parenthesised remark, for example
    // "(silence)", is an annotation rather than speech.
    let only_remark = collapsed.starts_with('(')
        && collapsed.ends_with(')')
        && collapsed.matches('(').count() == 1;
    if only_remark { String::new() } else { collapsed }
}

#[cfg(test)]
mod tests {
    use super::tidy;

    #[test]
    fn strips_bracketed_annotations() {
        assert_eq!(tidy(" [BLANK_AUDIO] "), "");
        assert_eq!(tidy("Hello [MUSIC] there."), "Hello there.");
    }

    #[test]
    fn drops_a_lone_parenthesised_remark_but_keeps_real_parentheses() {
        assert_eq!(tidy("(silence)"), "");
        assert_eq!(tidy("Call me (after lunch) please."), "Call me (after lunch) please.");
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(tidy("  one\n two   three "), "one two three");
    }
}
