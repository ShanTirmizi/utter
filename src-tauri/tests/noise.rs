//! Silence, steady noise, wind and quiet speech: what reaches the speech
//! model, and what comes out of it.
//!
//! The speech clips in `fixtures/` were rendered with macOS's own voices
//! (see `fixtures/render.sh`); the noise is made here. The tests that need
//! the real models are ignored by default; run them with
//!
//!   cargo test --test noise -- --ignored --nocapture
//!
//! They look for the models where the app keeps them, or at the paths in
//! UTTER_VAD_MODEL and UTTER_SPEECH_MODEL.

use std::path::PathBuf;
use utter_lib::{
    audio::{self, TARGET_RATE},
    speech::{self, Vad},
    transcribe::Transcriber,
};

const RATE: usize = TARGET_RATE as usize;

/// A spoken clip from `fixtures/`, as 16 kHz samples.
fn clip(name: &str) -> Vec<f32> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(format!("{name}.wav"));
    let mut reader = hound::WavReader::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(reader.spec().sample_rate, TARGET_RATE);
    reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32_768.0).collect()
}

/// A repeatable white noise source, -1..1 times `level`.
fn white(len: usize, level: f32, seed: u32) -> Vec<f32> {
    let mut state = seed.max(1);
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32 * 2.0 - 1.0) * level
        })
        .collect()
}

/// Wind on a microphone: deep rumble (white noise through two 40 Hz
/// low-pass stages) with `hiss` as much broadband noise, coming and going
/// in gusts of a third of a second to a second, over a quiet room. It
/// swings the way speech does, which is what fooled the loudness check.
fn gusts(seconds: f32, peak: f32, hiss: f32, seed: u32) -> Vec<f32> {
    let len = (seconds * RATE as f32) as usize;
    let raw = white(len, 1.0, seed);
    let k = 1.0 - (-2.0 * std::f32::consts::PI * 40.0 / RATE as f32).exp();
    let (mut a, mut b) = (0.0_f32, 0.0_f32);
    let mut rumble: Vec<f32> = raw
        .iter()
        .map(|x| {
            a += k * (x - a);
            b += k * (a - b);
            b
        })
        .collect();
    let loudest = audio::peak(&rumble).max(1e-9);
    let broadband = white(len, 1.0, seed ^ 0x9E37_79B9);
    for (r, h) in rumble.iter_mut().zip(&broadband) {
        *r = *r / loudest + hiss * h;
    }
    // Gust envelope: on for 0.3-1.0 s, off for 0.2-0.8 s, from the seed.
    let mut envelope = vec![0.0_f32; len];
    let mut at = 0;
    let mut pick = white(64, 1.0, seed.wrapping_mul(7)).into_iter().map(|v| (v + 1.0) / 2.0).cycle();
    while at < len {
        let on = ((0.3 + 0.7 * pick.next().unwrap()) * RATE as f32) as usize;
        let off = ((0.2 + 0.6 * pick.next().unwrap()) * RATE as f32) as usize;
        let strength = 0.4 + 0.6 * pick.next().unwrap();
        for i in 0..on.min(len - at) {
            // A smooth rise and fall.
            let phase = i as f32 / on as f32;
            envelope[at + i] = strength * (std::f32::consts::PI * phase).sin();
        }
        at += on + off;
    }
    let room = white(len, 0.001, seed ^ 0x51ED_270B);
    rumble.iter().zip(&envelope).zip(&room).map(|((r, e), q)| r * e * peak + q).collect()
}

/// `speech` scaled to `peak`, placed after `lead` seconds of `noise`, with
/// the noise running under it and after it.
fn mix(speech: &[f32], peak: f32, noise: &[f32], lead: f32) -> Vec<f32> {
    let gain = peak / audio::peak(speech);
    let start = (lead * RATE as f32) as usize;
    let mut out = noise.to_vec();
    out.resize(out.len().max(start + speech.len()), 0.0);
    for (o, s) in out[start..].iter_mut().zip(speech) {
        *o += s * gain;
    }
    out
}

fn sine(hz: f32, seconds: f32) -> Vec<f32> {
    (0..(seconds * RATE as f32) as usize)
        .map(|i| (2.0 * std::f32::consts::PI * hz * i as f32 / RATE as f32).sin() * 0.5)
        .collect()
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

/// Gain in dB of the high-pass filter at `hz`, measured after it settles.
fn gain_at(hz: f32) -> f32 {
    let input = sine(hz, 2.0);
    let output = audio::high_pass(&input);
    20.0 * (rms(&output[RATE..]) / rms(&input[RATE..])).log10()
}

#[test]
fn the_high_pass_cuts_rumble_and_leaves_the_voice_band() {
    assert!(gain_at(25.0) < -45.0, "25 Hz: {}", gain_at(25.0));
    assert!(gain_at(50.0) < -22.0, "50 Hz: {}", gain_at(50.0));
    assert!((gain_at(100.0) + 3.0).abs() < 0.3, "100 Hz: {}", gain_at(100.0));
    for hz in [150.0, 300.0, 1_000.0, 4_000.0] {
        assert!(gain_at(hz).abs() < 0.2, "{hz} Hz: {}", gain_at(hz));
    }
}

#[test]
fn the_high_pass_leaves_speech_alone() {
    // Ordinary voices keep 99% of their level.
    for name in ["send", "thanks", "yes"] {
        let speech = clip(name);
        let kept = rms(&audio::high_pass(&speech)) / rms(&speech);
        assert!(kept > 0.98, "{name}: kept {kept:.3} of its level");
    }
    // The deep voice's pitch is 84-107 Hz, so its fundamental is softened
    // (-1.3 dB overall), but the harmonics that carry the words are above
    // 150 Hz and untouched; the speech model hears the same words (see
    // `the_high_pass_does_not_change_what_is_heard`).
    let deep = clip("deep");
    let kept = rms(&audio::high_pass(&deep)) / rms(&deep);
    assert!(kept > 0.85, "deep: kept {kept:.3} of its level");
}

#[test]
fn the_high_pass_takes_most_of_the_wind() {
    let wind = gusts(4.0, 0.3, 0.0, 11);
    let left = rms(&audio::high_pass(&wind)) / rms(&wind);
    assert!(left < 0.2, "{left}");
}

/// The loudness check that stands in until the voice model is downloaded.
mod without_the_voice_model {
    use super::*;

    fn heard(samples: &[f32]) -> bool {
        audio::has_speech(&audio::high_pass(samples))
    }

    #[test]
    fn silence_and_steady_noise_are_not_speech() {
        assert!(!heard(&vec![0.0; 3 * RATE]));
        assert!(!heard(&white(3 * RATE, 0.002, 3)));
        assert!(!heard(&white(3 * RATE, 0.05, 4)));
    }

    // Gusts swing like speech, before filtering and after, so this check
    // lets them through to the speech model; there the no-speech estimate
    // stops them (`without_the_voice_model_noise_still_comes_out_as_nothing`).
    // Telling wind from speech before transcribing takes the voice model.

    #[test]
    fn quiet_speech_in_noise_is_still_speech() {
        // About -34 dBFS: someone speaking softly, or a microphone far off.
        let room = white(4 * RATE, 0.002, 5);
        assert!(heard(&mix(&clip("send"), 0.02, &room, 0.5)));
        assert!(heard(&mix(&clip("thanks"), 0.02, &room, 0.5)));
        let wind = gusts(4.0, 0.15, 0.04, 6);
        assert!(heard(&mix(&clip("send"), 0.05, &wind, 0.5)));
    }
}

fn models_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap()).join("Library/Application Support/dev.utter.app/models")
}

fn model(variable: &str, file: &str) -> PathBuf {
    let path = std::env::var_os(variable).map(PathBuf::from).unwrap_or_else(|| models_dir().join(file));
    assert!(path.is_file(), "no model at {} (set {variable})", path.display());
    path
}

fn vad() -> Vad {
    Vad::open(&model("UTTER_VAD_MODEL", "ggml-silero-v6.2.0.bin")).unwrap()
}

/// Noise that must never be taken for speech, and what it is.
fn noises() -> Vec<(&'static str, Vec<f32>)> {
    vec![
        ("silence", vec![0.0; 3 * RATE]),
        ("quiet room", white(3 * RATE, 0.002, 21)),
        ("steady fan", white(3 * RATE, 0.05, 22)),
        ("loud hiss", white(3 * RATE, 0.2, 23)),
        ("gusts", gusts(3.0, 0.3, 0.04, 24)),
        ("strong gusts", gusts(3.0, 0.8, 0.04, 25)),
        ("rumble only", gusts(3.0, 0.8, 0.0, 27)),
        ("short gust", gusts(0.8, 0.5, 0.04, 26)),
    ]
}

/// Speech, some of it quiet, some of it in noise, that must be heard.
fn speech_in_noise() -> Vec<(&'static str, Vec<f32>, &'static str)> {
    let room = white(4 * RATE, 0.002, 31);
    vec![
        ("clear", mix(&clip("send"), 0.5, &room, 0.5), "Friday"),
        ("quiet", mix(&clip("send"), 0.02, &room, 0.5), "Friday"),
        ("very quiet", mix(&clip("send"), 0.008, &white(4 * RATE, 0.0005, 32), 0.5), "Friday"),
        ("over a fan", mix(&clip("send"), 0.1, &white(4 * RATE, 0.02, 33), 0.5), "Friday"),
        ("in the wind", mix(&clip("send"), 0.1, &gusts(4.0, 0.3, 0.04, 34), 0.5), "Friday"),
        ("deep voice, quiet", mix(&clip("deep"), 0.02, &room, 0.5), "Thursday"),
        ("thank you, quiet", mix(&clip("thanks"), 0.02, &white(2 * RATE, 0.002, 35), 0.5), "Thank you"),
        ("yes, quiet", mix(&clip("yes"), 0.02, &white(2 * RATE, 0.002, 36), 0.5), "Yes"),
    ]
}

#[test]
#[ignore = "needs the voice activity model"]
fn the_voice_model_hears_speech_and_not_noise() {
    let mut vad = vad();
    for (name, samples) in noises() {
        let found = vad.detect(&audio::high_pass(&samples)).unwrap();
        eprintln!("{name:>18}: {} ms of speech", found.ms);
        assert!(found.ms < speech::MIN_SPEECH_MS, "{name}: {found:?}");
    }
    for (name, samples, _) in speech_in_noise() {
        let found = vad.detect(&audio::high_pass(&samples)).unwrap();
        eprintln!("{name:>18}: {} ms of speech in {} spans", found.ms, found.spans.len());
        assert!(found.ms >= speech::MIN_SPEECH_MS, "{name}: {found:?}");
    }
}

#[test]
#[ignore = "needs the voice activity model and a speech model"]
fn noise_comes_out_as_nothing_and_quiet_speech_as_words() {
    let transcriber = Transcriber::default();
    let speech_model = model("UTTER_SPEECH_MODEL", "ggml-base.en.bin");
    transcriber.ensure_loaded("test", &speech_model).unwrap();
    transcriber.use_vad(&model("UTTER_VAD_MODEL", "ggml-silero-v6.2.0.bin")).unwrap();
    for (name, samples) in noises() {
        let text = transcriber.transcribe(&samples, "en", &[]).unwrap();
        eprintln!("{name:>18}: {text:?}");
        assert_eq!(text, "", "{name}");
    }
    for (name, samples, expected) in speech_in_noise() {
        let text = transcriber.transcribe(&samples, "en", &[]).unwrap();
        eprintln!("{name:>18}: {text:?}");
        assert!(text.contains(expected), "{name}: {text:?}");
    }
}

#[test]
#[ignore = "needs a speech model"]
fn the_high_pass_does_not_change_what_is_heard() {
    // Without the voice model, so only the filter differs between the two.
    let transcriber = Transcriber::default();
    transcriber.ensure_loaded("test", &model("UTTER_SPEECH_MODEL", "ggml-base.en.bin")).unwrap();
    for name in ["send", "deep", "thanks", "yes"] {
        let speech = clip(name);
        let filtered = transcriber.transcribe(&speech, "en", &[]).unwrap();
        eprintln!("{name:>8}: {filtered:?}");
        assert!(!filtered.is_empty(), "{name}");
    }
}

/// Until the voice model is downloaded, the loudness check and the speech
/// model's own confidence are all there is. Noise must still come out as
/// nothing, and quiet speech as something. Debug builds print the speech
/// model's figures for each case: where the thresholds in `speech` come from.
#[test]
#[ignore = "needs a speech model"]
fn without_the_voice_model_noise_still_comes_out_as_nothing() {
    let transcriber = Transcriber::default();
    transcriber.ensure_loaded("test", &model("UTTER_SPEECH_MODEL", "ggml-base.en.bin")).unwrap();
    // A breath or a tap: 150 ms of noise.
    let mut breath = white(2 * RATE, 0.002, 41);
    for (i, s) in white(RATE * 15 / 100, 0.15, 42).iter().enumerate() {
        breath[RATE / 2 + i] += s;
    }
    // Typing: eight sharp clicks.
    let mut typing = white(3 * RATE, 0.002, 43);
    for k in 0..8 {
        for (i, s) in white(400, 0.4, 44 + k).iter().enumerate() {
            typing[RATE / 4 + k as usize * RATE / 3 + i] += s * (1.0 - i as f32 / 400.0);
        }
    }
    let mut cases = noises();
    cases.push(("breath", breath));
    cases.push(("typing", typing));
    for (name, samples) in cases {
        // Straight to the speech model, as if the loudness check had let
        // it through.
        let text = transcriber.transcribe(&samples, "en", &[]).unwrap();
        eprintln!("{name:>18}: {text:?}");
        assert_eq!(text, "", "{name}");
    }
    for (name, samples, _) in speech_in_noise() {
        assert!(audio::has_speech(&audio::high_pass(&samples)), "{name}");
        let text = transcriber.transcribe(&samples, "en", &[]).unwrap();
        eprintln!("{name:>18}: {text:?}");
        assert!(!text.is_empty(), "{name}");
    }
}
