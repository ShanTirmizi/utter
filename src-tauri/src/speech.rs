//! Telling speech from silence, wind and background noise.
//!
//! Whisper learned from subtitled video, so given audio with nobody talking
//! it writes what subtitles say at such moments: "you", "Thank you.",
//! "Thanks for watching!", a lone ".". Three checks keep that out:
//!
//! 1. A voice activity model (Silero, run by whisper.cpp) marks which parts
//!    of a recording are speech. A recording with less than
//!    `MIN_SPEECH_MS` of it is not transcribed at all, and in one that is,
//!    only the speech reaches the speech model.
//! 2. For each segment it writes, the speech model reports how likely it
//!    thinks it is that nobody spoke, and how sure it is of its words.
//!    Segments it doubts on both counts, or is sure are not speech, go.
//! 3. When there was little speech, the phrases Whisper is known to invent
//!    from noise are dropped too. Never when the speech was clear: people
//!    do dictate "Thank you."

use std::{ops::Range, path::Path};
use whisper_rs::{WhisperVadContext, WhisperVadContextParams, WhisperVadParams};

use crate::audio::TARGET_RATE;

/// A recording needs at least this much speech in it to be transcribed.
/// One short word ("yes", "no") is about 250-350 ms; a cough, a click or a
/// gust that fools the voice model for a frame or two is well under it.
pub const MIN_SPEECH_MS: u32 = 250;

/// How sure the voice model must be that a 32 ms frame is speech. Silero's
/// usual 0.5 marks quiet speech too (it judges the shape of the sound, not
/// its loudness), and lower lets breathing and rustle in.
const VAD_THRESHOLD: f32 = 0.5;
/// Speech kept on each side of a stretch the voice model found, so the
/// soft start and end of a word (an "h", a trailing "s") are not clipped.
const VAD_PAD_MS: i32 = 200;
/// Pauses shorter than this stay inside one stretch of speech; only longer
/// ones are cut out. Dictation has natural pauses for thought, and keeping
/// the shorter ones leaves the speech model the rhythm it expects.
const VAD_MIN_SILENCE_MS: i32 = 300;
/// A stretch of speech shorter than this is ignored by the voice model.
/// Kept low so a one-word dictation counts; `MIN_SPEECH_MS` is the gate.
const VAD_MIN_SPEECH_MS: i32 = 100;
/// The quiet put between the stretches of speech that are kept, as
/// whisper.cpp does when it runs the voice model itself.
const GAP: usize = TARGET_RATE as usize / 10;

/// The parts of a recording the voice model heard speech in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Speech {
    /// How many milliseconds of frames the model took for speech.
    pub ms: u32,
    /// Stretches of speech, padded, as sample ranges into the recording.
    pub spans: Vec<Range<usize>>,
}

impl Speech {
    /// Only the speech, with a short silence between stretches.
    pub fn keep(&self, samples: &[f32]) -> Vec<f32> {
        let mut out = Vec::with_capacity(samples.len());
        for span in &self.spans {
            let span = span.start.min(samples.len())..span.end.min(samples.len());
            if span.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.resize(out.len() + GAP, 0.0);
            }
            out.extend_from_slice(&samples[span]);
        }
        out
    }
}

/// The voice activity model, loaded.
pub struct Vad {
    context: WhisperVadContext,
}

impl Vad {
    pub fn open(path: &Path) -> Result<Self, String> {
        let path = path.to_str().ok_or("Voice model path is not valid UTF-8.")?;
        let mut params = WhisperVadContextParams::default();
        // It is tiny; the GPU would only add start-up time.
        params.set_use_gpu(false);
        params.set_n_threads(2);
        let context =
            WhisperVadContext::new(path, params).map_err(|e| format!("Could not load the voice model: {e}"))?;
        Ok(Self { context })
    }

    /// Find the speech in mono 16 kHz audio.
    pub fn detect(&mut self, samples: &[f32]) -> Result<Speech, String> {
        if samples.is_empty() {
            return Ok(Speech::default());
        }
        let failed = |e| format!("Voice detection failed: {e}");
        self.context.detect_speech(samples).map_err(failed)?;
        let probabilities = self.context.probabilities();
        // One probability per window of audio (512 samples, 32 ms).
        let window_ms = samples.len() as f64 * 1000.0 / TARGET_RATE as f64 / probabilities.len().max(1) as f64;
        let frames = probabilities.iter().filter(|p| **p >= VAD_THRESHOLD).count();
        let ms = (frames as f64 * window_ms).round() as u32;

        let mut params = WhisperVadParams::new();
        params.set_threshold(VAD_THRESHOLD);
        params.set_min_speech_duration(VAD_MIN_SPEECH_MS);
        params.set_min_silence_duration(VAD_MIN_SILENCE_MS);
        params.set_speech_pad(VAD_PAD_MS);
        params.set_samples_overlap(0.0);
        let segments = self.context.segments_from_probabilities(params).map_err(failed)?;
        // Segment times are in centiseconds.
        let at = |cs: f32| ((cs as f64 * TARGET_RATE as f64 / 100.0).max(0.0) as usize).min(samples.len());
        let spans = segments.map(|s| at(s.start)..at(s.end)).filter(|r| !r.is_empty()).collect();
        Ok(Speech { ms, spans })
    }
}

/// One segment of text from the speech model, with how sure it was.
#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    pub text: String,
    /// The model's estimate that the audio held no speech, 0 to 1.
    pub no_speech: f32,
    /// Mean log-probability of the segment's text tokens; 0 is certain,
    /// and -1 means the average word had about a 1-in-3 chance.
    pub avg_logprob: f32,
}

// The figures below were measured on base.en with the clips and noise in
// tests/noise.rs. Given noise and no voice model to stop it, Whisper wrote
// "you", "Shh." or "." with a no-speech estimate of 0.77 to 0.92. Speech,
// even at -42 dBFS or under wind, never went above 0.46, and its mean
// log-probability never below -0.69.

/// Above this no-speech estimate, a segment is dropped whatever its words.
/// Between the 0.46 of the quietest speech and the 0.77 of noise, leaning
/// towards keeping words: dropping a real dictation costs more than
/// letting a rare invented word through.
const SURE_NO_SPEECH: f32 = 0.7;
/// A segment that is fairly likely to be no speech and whose words the
/// model doubts goes too: the rule Whisper itself was published with.
const LIKELY_NO_SPEECH: f32 = 0.6;
const DOUBTFUL_LOGPROB: f32 = -1.0;
/// Words this unsure are a guess even when the model thinks someone spoke.
/// Noise came out at -1.5 to -1.9; accented speech can reach -1.2.
const HOPELESS_LOGPROB: f32 = -1.5;

/// Should this segment be kept?
pub fn keep_segment(heard: &Heard) -> bool {
    if heard.no_speech >= SURE_NO_SPEECH || heard.avg_logprob < HOPELESS_LOGPROB {
        return false;
    }
    !(heard.no_speech >= LIKELY_NO_SPEECH && heard.avg_logprob < DOUBTFUL_LOGPROB)
}

/// With at least this much speech, nothing that was heard is second-guessed
/// for being a known invention: a deliberate "Thank you." lasts longer
/// than the blip of noise that produces an invented one.
const CLEAR_SPEECH_MS: u32 = 700;
/// ... and with less, the text is still kept when the model was this sure
/// of it and this sure that someone spoke. A quiet "Thank you." measured
/// -0.14 and 0.19 (571 ms of speech); one invented from a gust, -0.11 and
/// 0.84. So the no-speech estimate is what tells them apart.
const CONFIDENT_LOGPROB: f32 = -0.4;
const CONFIDENT_NO_SPEECH: f32 = 0.4;
/// A lone word other than those is dropped, when there was little speech,
/// if the model leaned this far towards no speech or doubted it this much.
/// A quiet "Yes." measured 0.40 and -0.61.
const LONE_WORD_NO_SPEECH: f32 = 0.5;
const LONE_WORD_LOGPROB: f32 = -1.0;

/// What Whisper writes for noise, after `plain`.
const INVENTED: &[&str] = &[
    "",
    "you",
    "thank you",
    "thank you very much",
    "thanks",
    "thanks for watching",
    "thank you for watching",
    "thank you so much for watching",
    "thanks for listening",
    "thank you for listening",
    "please subscribe",
    "subscribe",
    "like and subscribe",
    "see you next time",
    "bye",
    "bye bye",
    "the end",
    "so",
    "oh",
    "uh",
    "um",
    "hmm",
    "mm",
    "huh",
    "ah",
];

/// Is the whole of `text` most likely invented from noise? `speech` is what
/// the voice model found (None when it is not available), and `heard` the
/// segments the text came from.
pub fn is_invented(text: &str, speech: Option<&Speech>, heard: &[Heard]) -> bool {
    let plain = plain(text);
    let known = INVENTED.contains(&plain.as_str());
    let lone_word = !plain.is_empty() && !plain.contains(' ');
    if !known && !lone_word {
        return false;
    }
    if speech.is_some_and(|s| s.ms >= CLEAR_SPEECH_MS) {
        return false;
    }
    let confident = !heard.is_empty()
        && heard.iter().all(|h| h.avg_logprob >= CONFIDENT_LOGPROB && h.no_speech <= CONFIDENT_NO_SPEECH);
    if known {
        // A lone "." or "..." from a short clip goes however sure the model was.
        return plain.is_empty() || !confident;
    }
    // Any other lone word: dropped only when the model was guessing.
    heard.iter().any(|h| h.avg_logprob < LONE_WORD_LOGPROB || h.no_speech >= LONE_WORD_NO_SPEECH)
}

/// Lowercase words separated by single spaces, punctuation gone: "Thank
/// you!" and "thank you." are both "thank you".
fn plain(text: &str) -> String {
    text.split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heard(text: &str, no_speech: f32, avg_logprob: f32) -> Heard {
        Heard { text: text.into(), no_speech, avg_logprob }
    }

    fn speech_ms(ms: u32) -> Speech {
        Speech { ms, spans: vec![0..ms as usize * 16] }
    }

    #[test]
    fn segments_the_model_doubts_are_dropped() {
        assert!(keep_segment(&heard("Send it today.", 0.02, -0.2)));
        // Quiet or accented speech: unsure words, but clearly speech.
        // Quiet speech in noise, as measured: the worst of each.
        assert!(keep_segment(&heard("Can we move the meeting?", 0.46, -0.41)));
        assert!(keep_segment(&heard("Send a report to Preah by Friday.", 0.21, -0.69)));
        assert!(keep_segment(&heard("Ask Priya about the Q3 roadmap.", 0.1, -1.2)));
        // Noise, as measured.
        assert!(!keep_segment(&heard(" you", 0.84, -0.11)));
        assert!(!keep_segment(&heard(" Shh.", 0.77, -0.56)));
        assert!(!keep_segment(&heard(" .", 0.80, -1.63)));
        assert!(!keep_segment(&heard("Lord", 0.65, -1.1)));
        assert!(!keep_segment(&heard("Civil", 0.1, -1.7)));
    }

    #[test]
    fn known_inventions_go_when_there_was_little_speech() {
        let blip = speech_ms(320);
        assert!(is_invented("you", Some(&blip), &[heard(" you", 0.45, -0.6)]));
        assert!(is_invented("Thank you.", Some(&blip), &[heard(" Thank you.", 0.5, -0.2)]));
        assert!(is_invented("Thanks for watching!", None, &[heard(" Thanks for watching!", 0.1, -0.5)]));
        assert!(is_invented(".", Some(&blip), &[heard(" .", 0.05, -0.1)]));
        assert!(is_invented("...", None, &[]));
    }

    #[test]
    fn known_phrases_said_clearly_are_kept() {
        let clear = speech_ms(900);
        assert!(!is_invented("Thank you.", Some(&clear), &[heard(" Thank you.", 0.3, -0.6)]));
        // Short, but the model is sure someone said it.
        let short = speech_ms(571);
        assert!(!is_invented("Thank you.", Some(&short), &[heard(" Thank you.", 0.19, -0.14)]));
        assert!(!is_invented("Bye!", None, &[heard(" Bye!", 0.1, -0.2)]));
    }

    #[test]
    fn a_lone_word_goes_only_when_the_model_was_guessing() {
        let blip = speech_ms(300);
        assert!(is_invented("Civil", Some(&blip), &[heard(" Civil", 0.3, -1.2)]));
        assert!(is_invented("Lord", None, &[heard(" Lord", 0.6, -0.4)]));
        assert!(!is_invented("Yes.", Some(&speech_ms(381)), &[heard(" Yes.", 0.40, -0.61)]));
        assert!(!is_invented("Kubernetes", None, &[heard(" Kubernetes", 0.15, -0.6)]));
    }

    #[test]
    fn sentences_are_never_taken_for_inventions() {
        assert!(!is_invented("Thank you for the notes on the draft.", Some(&speech_ms(260)), &[heard("", 0.5, -0.9)]));
        assert!(!is_invented("Send it", None, &[heard("", 0.5, -0.9)]));
    }

    #[test]
    fn only_speech_is_kept_with_short_gaps_between() {
        let samples: Vec<f32> = (0..16_000).map(|i| i as f32).collect();
        let speech = Speech { ms: 300, spans: vec![100..200, 1_000..1_050, 20_000..30_000] };
        let kept = speech.keep(&samples);
        assert_eq!(kept.len(), 100 + GAP + 50);
        assert_eq!(kept[0], 100.0);
        assert_eq!(kept[100 + GAP], 1_000.0);
        assert!(kept[100..100 + GAP].iter().all(|s| *s == 0.0));
        assert!(Speech::default().keep(&samples).is_empty());
    }

    #[test]
    fn plain_text_ignores_case_and_punctuation() {
        assert_eq!(plain(" Thank you! "), "thank you");
        assert_eq!(plain("..."), "");
        assert_eq!(plain("It's done."), "it's done");
    }
}
