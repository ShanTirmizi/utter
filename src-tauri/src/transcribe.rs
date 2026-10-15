//! Speech-to-text, entirely on this Mac, using whisper.cpp. No audio leaves
//! the machine and no API key is involved.

use std::{path::Path, sync::Mutex};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::audio::{self, TARGET_RATE};

/// The model rejects very short clips, so shorter recordings are padded with
/// silence up to this length.
const MIN_SAMPLES: usize = (TARGET_RATE as usize * 11) / 10;

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
        Self::load(&mut self.loaded.lock().unwrap(), name, path)
    }

    fn load(loaded: &mut Option<Loaded>, name: &str, path: &Path) -> Result<(), String> {
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
        Self::run(loaded.as_ref().ok_or("No speech model is loaded.")?, samples, language, &[])
    }

    /// Make sure `name` is the loaded model, then transcribe with it. Both
    /// happen under one lock, so a model change in settings part-way through
    /// cannot swap the model out from under a dictation.
    pub fn transcribe_with(
        &self,
        name: &str,
        path: &Path,
        samples: &[f32],
        language: &str,
        terms: &[String],
    ) -> Result<String, String> {
        let mut loaded = self.loaded.lock().unwrap();
        Self::load(&mut loaded, name, path)?;
        Self::run(loaded.as_ref().ok_or("No speech model is loaded.")?, samples, language, terms)
    }

    fn run(loaded: &Loaded, samples: &[f32], language: &str, terms: &[String]) -> Result<String, String> {
        let hint = glossary(terms);

        let mut state = loaded
            .context
            .create_state()
            .map_err(|e| format!("Could not start transcription: {e}"))?;

        // The model reads audio in 30-second windows. Left to walk through a
        // long recording by itself it drops words where windows meet and can
        // get stuck repeating a phrase. So a long recording is cut at natural
        // pauses into pieces that each fit one window, and every piece is
        // transcribed as its own short dictation.
        let mut pieces = Vec::new();
        for piece in split_at_pauses(samples) {
            if audio::peak(piece) < PIECE_SILENCE_PEAK {
                continue;
            }
            let mut audio = piece.to_vec();
            if audio.len() < MIN_SAMPLES {
                audio.resize(MIN_SAMPLES, 0.0);
            }

            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            params.set_language(Some(language));
            params.set_n_threads(threads());
            // Each piece stands alone; do not let one bleed into the next.
            params.set_no_context(true);
            if !hint.is_empty() {
                // Text the model is told came just before. Having "seen" the
                // user's names and terms, it is more likely to hear them.
                params.set_initial_prompt(&hint);
            }
            params.set_no_timestamps(true);
            params.set_suppress_blank(true);
            params.set_suppress_nst(true);
            params.set_print_special(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);

            state
                .full(params, &audio)
                .map_err(|e| format!("Transcription failed: {e}"))?;
            let text: String = state.as_iter().map(|segment| segment.to_string()).collect();
            let text = tidy(&text);
            if !text.is_empty() && !is_prompt_echo(&text, terms) {
                pieces.push(text);
            }
        }
        Ok(collapse_repeats(&pieces.join(" ")))
    }
}

/// Did the model repeat the glossary it was shown instead of hearing speech?
/// It can, on a stretch of noise. A real dictation neither opens with the
/// word "Glossary" nor is exactly a run of three or more listed words.
fn is_prompt_echo(text: &str, terms: &[String]) -> bool {
    if terms.is_empty() {
        return false;
    }
    let plain = |text: &str| -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .collect()
    };
    let heard = plain(text);
    if heard.first().is_some_and(|w| w == "glossary") {
        return true;
    }
    let listed: Vec<String> = terms.iter().flat_map(|t| plain(t)).collect();
    heard.len() >= 3 && listed.windows(heard.len()).any(|run| run == heard.as_slice())
}

/// The user's dictionary as a line of text for the speech model. The model
/// reads only a limited amount of such text, so the list is cut to fit.
pub fn glossary(terms: &[String]) -> String {
    const BUDGET: usize = 600;
    let mut line = String::new();
    for term in terms {
        if line.len() + term.len() + 2 > BUDGET {
            break;
        }
        if !line.is_empty() {
            line.push_str(", ");
        }
        line.push_str(term);
    }
    if line.is_empty() { line } else { format!("Glossary: {line}.") }
}

/// Longest piece of audio given to the model in one go, safely inside its
/// 30-second window.
const MAX_PIECE: usize = TARGET_RATE as usize * 27;
/// A cut is never made earlier than this, so pieces stay long enough for the
/// model to have context.
const MIN_PIECE: usize = TARGET_RATE as usize * 12;
/// A piece quieter than this holds no speech and is skipped.
const PIECE_SILENCE_PEAK: f32 = 0.003;

/// Cut a recording into pieces of at most `MAX_PIECE`, each cut placed at the
/// quietest moment available, which in speech is a pause between phrases.
/// Recordings that already fit are returned whole.
pub fn split_at_pauses(samples: &[f32]) -> Vec<&[f32]> {
    let mut pieces = Vec::new();
    let mut rest = samples;
    while rest.len() > MAX_PIECE {
        let cut = MIN_PIECE + quietest_point(&rest[MIN_PIECE..MAX_PIECE]);
        pieces.push(&rest[..cut]);
        rest = &rest[cut..];
    }
    if !rest.is_empty() {
        pieces.push(rest);
    }
    // A sliver left over at the end (a breath, a key click) is too short to
    // transcribe by itself and tends to come back as an invented word. Give
    // it to the piece before, which still fits the model's window.
    if pieces.len() >= 2 && pieces[pieces.len() - 1].len() < LAST_PIECE_MIN {
        let tail = pieces.pop().unwrap().len();
        let previous = pieces.pop().unwrap().len();
        let start = samples.len() - tail - previous;
        pieces.push(&samples[start..]);
    }
    pieces
}

/// Where, in audio still being recorded, a finished stretch of speech can be
/// cut off and transcribed early: the middle of a clear pause that is well
/// behind the live edge, with enough speech before it to be worth it. `None`
/// when there is no such pause yet.
pub fn settled_cut(samples: &[f32]) -> Option<usize> {
    const RATE: usize = TARGET_RATE as usize;
    const FRAME: usize = RATE * 45 / 100; // a 450 ms pause: usually between sentences
    const STEP: usize = RATE / 20; // looked for every 50 ms
    const MIN_SPEECH: usize = RATE * 6; // at least 6 s before the cut
    const BEHIND_EDGE: usize = RATE; // at least 1 s behind what is being said now
    if samples.len() < MIN_SPEECH + FRAME + BEHIND_EDGE {
        return None;
    }
    let rms = |part: &[f32]| (part.iter().map(|s| s * s).sum::<f32>() / part.len().max(1) as f32).sqrt();
    let overall = rms(samples);
    if overall < PIECE_SILENCE_PEAK {
        return None;
    }
    // The latest quiet frame, searching back from the live edge.
    let mut start = samples.len() - BEHIND_EDGE - FRAME;
    while start >= MIN_SPEECH {
        if rms(&samples[start..start + FRAME]) < overall * 0.2 {
            return Some(start + FRAME / 2);
        }
        start = start.checked_sub(STEP)?;
    }
    None
}

/// A final piece shorter than this is merged into the one before it.
const LAST_PIECE_MIN: usize = TARGET_RATE as usize * 2;

/// The middle of the quietest 200 ms stretch in `window`.
fn quietest_point(window: &[f32]) -> usize {
    const FRAME: usize = TARGET_RATE as usize / 5; // 200 ms
    const STEP: usize = TARGET_RATE as usize / 20; // 50 ms
    let mut best = (f32::MAX, window.len() / 2);
    let mut start = 0;
    while start + FRAME <= window.len() {
        let energy: f32 = window[start..start + FRAME].iter().map(|s| s * s).sum();
        if energy < best.0 {
            best = (energy, start + FRAME / 2);
        }
        start += STEP;
    }
    best.1
}

/// Speech models sometimes get stuck and emit the same sentence over and
/// over. Nobody dictates one sentence three times running, so a run of three
/// or more identical sentences is reduced to one.
pub fn collapse_repeats(text: &str) -> String {
    let normalise = |s: &str| -> String {
        s.to_lowercase().chars().filter(|c| c.is_alphanumeric() || *c == ' ').collect::<String>().trim().to_string()
    };
    // Split after sentence-ending punctuation, keeping it with its sentence.
    let mut sentences: Vec<&str> = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    for (i, c) in text.char_indices() {
        if matches!(c, '.' | '!' | '?') && bytes.get(i + 1).is_none_or(|b| *b == b' ') {
            sentences.push(text[start..=i].trim());
            start = i + 1;
        }
    }
    if start < text.len() && !text[start..].trim().is_empty() {
        sentences.push(text[start..].trim());
    }

    let mut out: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < sentences.len() {
        let key = normalise(sentences[i]);
        let mut run = 1;
        while i + run < sentences.len() && normalise(sentences[i + run]) == key {
            run += 1;
        }
        // Only whole sentences count; "No. No. No." is a person, not a loop.
        let is_loop = run >= 3 && key.split(' ').count() >= 3;
        let keep = if is_loop { 1 } else { run };
        out.extend(&sentences[i..i + keep]);
        i += run;
    }
    out.join(" ")
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
    use super::*;

    #[test]
    fn early_cuts_fall_in_a_settled_pause() {
        let rate = TARGET_RATE as usize;
        let tone = |seconds: f32| -> Vec<f32> {
            (0..(seconds * rate as f32) as usize).map(|i| 0.2 * ((i as f32) * 0.05).sin()).collect()
        };
        let quiet = |seconds: f32| vec![0.0_f32; (seconds * rate as f32) as usize];
        // Speech, a pause, speech still going: cut in the pause.
        let mut audio = tone(7.0);
        audio.extend(quiet(0.6));
        audio.extend(tone(2.0));
        let cut = settled_cut(&audio).expect("a pause after 7 s of speech");
        assert!(cut > 7 * rate && cut < 7 * rate + rate * 6 / 10, "{cut}");
        // Too little speech yet, or the only pause is at the live edge.
        let mut short = tone(3.0);
        short.extend(quiet(0.6));
        short.extend(tone(2.0));
        assert_eq!(settled_cut(&short), None);
        let mut edge = tone(8.0);
        edge.extend(quiet(0.5));
        assert_eq!(settled_cut(&edge), None);
        assert_eq!(settled_cut(&quiet(10.0)), None);
    }

    #[test]
    fn the_glossary_coming_back_is_not_a_dictation() {
        let terms: Vec<String> = ["Priya", "Kubernetes", "Q3 roadmap"].iter().map(|t| t.to_string()).collect();
        assert!(is_prompt_echo("Glossary: Priya, Kubernetes.", &terms));
        assert!(is_prompt_echo("Priya, Kubernetes, Q3 roadmap.", &terms));
        // Saying one of the words, or a sentence with them in it, is fine.
        assert!(!is_prompt_echo("Priya", &terms));
        assert!(!is_prompt_echo("Ask Priya about Kubernetes.", &terms));
        assert!(!is_prompt_echo("Glossary: anything", &[]));
    }

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
    fn short_recordings_are_not_split() {
        let clip = vec![0.1_f32; TARGET_RATE as usize * 20];
        let pieces = split_at_pauses(&clip);
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].len(), clip.len());
    }

    #[test]
    fn long_recordings_are_cut_at_the_pause_and_nothing_is_lost() {
        // 60 seconds of "speech" with a silent gap from 20.0 s to 20.5 s.
        let rate = TARGET_RATE as usize;
        let mut clip = vec![0.2_f32; rate * 60];
        for s in &mut clip[rate * 20..rate * 20 + rate / 2] {
            *s = 0.0;
        }
        let pieces = split_at_pauses(&clip);
        assert!(pieces.len() >= 3);
        // The first cut lands inside the gap.
        let first = pieces[0].len();
        assert!(first > rate * 20 && first < rate * 20 + rate / 2, "cut at {first}");
        // Every piece fits the model's window, and together they are the whole clip.
        assert!(pieces.iter().all(|p| p.len() <= MAX_PIECE));
        assert_eq!(pieces.iter().map(|p| p.len()).sum::<usize>(), clip.len());
    }

    #[test]
    fn a_short_leftover_is_joined_to_the_piece_before_it() {
        let rate = TARGET_RATE as usize;
        // 27 s is the most one piece may hold, so 28 s leaves a 1 s tail
        // at worst; it must not become a piece of its own.
        let clip = vec![0.2_f32; rate * 28];
        let pieces = split_at_pauses(&clip);
        assert!(pieces.iter().all(|p| p.len() >= LAST_PIECE_MIN), "{:?}", pieces.iter().map(|p| p.len()).collect::<Vec<_>>());
        assert_eq!(pieces.iter().map(|p| p.len()).sum::<usize>(), clip.len());
        // The model's window is 30 s; a merged piece must still fit.
        assert!(pieces.iter().all(|p| p.len() <= rate * 30));
    }

    #[test]
    fn a_stuck_model_loop_is_reduced_to_one_sentence() {
        let looped = "Send it today. I think it's a good idea. I think it's a good idea. \
                      I think it's a good idea. I think it's a good idea. Thanks!";
        assert_eq!(collapse_repeats(looped), "Send it today. I think it's a good idea. Thanks!");
    }

    #[test]
    fn ordinary_repetition_is_left_alone() {
        assert_eq!(collapse_repeats("No. No. No. Stop."), "No. No. No. Stop.");
        assert_eq!(collapse_repeats("I said it twice. I said it twice."), "I said it twice. I said it twice.");
        assert_eq!(collapse_repeats("Version 3.5 is out. It works"), "Version 3.5 is out. It works");
        assert_eq!(collapse_repeats(""), "");
    }

    #[test]
    fn the_glossary_lists_terms_and_stays_within_budget() {
        assert_eq!(glossary(&[]), "");
        assert_eq!(glossary(&["Priya".into(), "Kubernetes".into()]), "Glossary: Priya, Kubernetes.");
        let many: Vec<String> = (0..300).map(|i| format!("longterm{i}")).collect();
        assert!(glossary(&many).len() <= 620);
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(tidy("  one\n two   three "), "one two three");
    }
}
