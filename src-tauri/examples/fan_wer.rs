//! Word error rate with a fan running. Renders jargon-heavy sentences with
//! two macOS voices, mixes in fan noise (broadband hiss, a low rumble, mains
//! hum and a blade tone) at several signal-to-noise ratios, and transcribes
//! each mix with the speech front-end in several configurations.
//!
//!   cargo run --example fan_wer -- <ggml-speech-model.bin> <ggml-silero-vad.bin>
//!
//! Set UTTER_WER_VERBOSE=1 to see every transcript that has an error, and
//! UTTER_WER_ROUNDS to repeat each mix with that many different stretches
//! of fan noise (1 by default). After the table, each front-end is compared
//! with the app's, mix by mix: on how many it made fewer errors, and on how
//! many more.

use std::{path::Path, process::Command};
use utter_lib::{audio::TARGET_RATE, transcribe::Transcriber};

const RATE: usize = TARGET_RATE as usize;

const SENTENCES: [&str; 10] = [
    "Deploy the Kubernetes cluster to staging before the Postgres migration.",
    "Ask Priya to review the OAuth refresh token flow in the auth service.",
    "The p95 latency on the GraphQL gateway doubled after the last release.",
    "Bump the React Native version and rerun the Jest snapshot tests.",
    "Our OKRs for this quarter include cutting the AWS bill.",
    "Rebase the feature branch onto main and squash the WIP commits.",
    "The Terraform plan wants to recreate the Redis node in the VPC.",
    "Schedule a sync with Anand and Mei about the Snowflake ingestion pipeline.",
    "Set the webhook timeout and retry with exponential backoff.",
    "The TypeScript compiler flags an implicit any in the Zod schema.",
];
const VOICES: [&str; 2] = ["Samantha", "Eddy (English (US))"];
/// Signal-to-noise ratios in dB; `None` is no fan at all.
const SNRS: [Option<f32>; 4] = [None, Some(20.0), Some(10.0), Some(5.0)];

#[derive(Clone, Copy, Debug)]
enum Front {
    /// The recording as it came.
    Raw,
    /// The 100 Hz high-pass only.
    HighPass,
    /// High-pass and voice activity: what the app does.
    App,
    /// RNNoise, a neural noise suppressor, in front of what the app does.
    Rnnoise,
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (Some(model), Some(vad)) = (args.next(), args.next()) else {
        return Err("usage: fan_wer <speech-model.bin> <vad.bin>".into());
    };
    let verbose = std::env::var_os("UTTER_WER_VERBOSE").is_some();
    let rounds: u32 = std::env::var("UTTER_WER_ROUNDS").ok().and_then(|r| r.parse().ok()).unwrap_or(1);
    // English for every model, so the large one is not also judged on
    // detecting the language.
    let language = "en";

    let clips: Vec<(String, Vec<f32>)> = VOICES
        .iter()
        .flat_map(|voice| SENTENCES.iter().map(move |text| (voice, text)))
        .map(|(voice, text)| render(voice, text).map(|samples| (text.to_string(), samples)))
        .collect::<Result<_, _>>()?;

    let transcriber = Transcriber::default();
    transcriber.ensure_loaded("eval", Path::new(&model))?;
    let fronts = [Front::Raw, Front::HighPass, Front::App, Front::Rnnoise];

    let header = SNRS.map(|s| s.map_or("clean".into(), |db| format!("{db} dB"))).map(|s| format!("{s:>9}")).join("");
    println!("model: {}, {} clips x {rounds} rounds", Path::new(&model).file_name().unwrap().to_string_lossy(), clips.len());
    println!("{:<10}{header}", "front-end");
    // Errors per mix, for each front-end and each noise level.
    let mut results: Vec<Vec<Vec<usize>>> = Vec::new();
    for front in fronts {
        match front {
            Front::App | Front::Rnnoise => transcriber.use_vad(Path::new(&vad))?,
            _ => transcriber.forget_vad(),
        }
        transcriber.set_high_pass(!matches!(front, Front::Raw));
        let mut row = Vec::new();
        let mut per_mix = Vec::new();
        for snr in SNRS {
            let (mut errors, mut words, mut each) = (0, 0, Vec::new());
            let mixes = clips.iter().enumerate().flat_map(|(i, clip)| (0..rounds).map(move |r| (r * 1_000 + i as u32 + 1, clip)));
            for (seed, (reference, speech)) in mixes {
                // A clean clip is the same every round.
                if snr.is_none() && seed > 1_000 {
                    continue;
                }
                let input = match snr {
                    Some(db) => mix(speech, &fan(speech.len() + RATE, seed), db),
                    None => pad(speech),
                };
                let input = if matches!(front, Front::Rnnoise) { denoise(&input) } else { input };
                let heard = transcriber.transcribe(&input, language)?;
                let (e, n) = word_errors(reference, &heard);
                if verbose && e > 0 {
                    eprintln!("{front:?} {snr:?} [{e}/{n}] {heard:?}");
                }
                errors += e;
                words += n;
                each.push(e);
            }
            row.push(100.0 * errors as f32 / words as f32);
            per_mix.push(each);
        }
        println!("{:<10}{}", format!("{front:?}"), row.iter().map(|w| format!("{w:>8.1}%")).collect::<String>());
        results.push(per_mix);
    }

    println!("\nmixes with fewer / more errors than App:");
    let app = fronts.iter().position(|f| matches!(f, Front::App)).unwrap();
    for (front, per_mix) in fronts.iter().zip(&results) {
        if matches!(front, Front::App) {
            continue;
        }
        let cells: String = per_mix
            .iter()
            .zip(&results[app])
            .map(|(mine, theirs)| {
                let better = mine.iter().zip(theirs).filter(|(m, t)| m < t).count();
                let worse = mine.iter().zip(theirs).filter(|(m, t)| m > t).count();
                format!("{:>9}", format!("{better}/{worse}"))
            })
            .collect();
        println!("{:<10}{cells}", format!("{front:?}"));
    }
    Ok(())
}

/// Speak `text` with a macOS voice, as 16 kHz samples. Cached between runs.
fn render(voice: &str, text: &str) -> Result<Vec<f32>, String> {
    let dir = std::env::temp_dir().join("utter-fan-wer");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let key: u64 = format!("{voice}|{text}").bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3));
    let wav = dir.join(format!("{key:016x}.wav"));
    if !wav.is_file() {
        let aiff = dir.join(format!("{key:016x}.aiff"));
        let ok = Command::new("say").args(["-v", voice, "-o"]).arg(&aiff).arg(text).status().map_err(|e| e.to_string())?;
        let ok = ok.success()
            && Command::new("afconvert")
                .args(["-f", "WAVE", "-d", "LEI16@16000", "-c", "1"])
                .arg(&aiff)
                .arg(&wav)
                .status()
                .map_err(|e| e.to_string())?
                .success();
        if !ok {
            return Err(format!("could not render {text:?} with {voice}"));
        }
    }
    let mut reader = hound::WavReader::open(&wav).map_err(|e| e.to_string())?;
    Ok(reader.samples::<i16>().map(|s| s.unwrap_or(0) as f32 / 32_768.0).collect())
}

/// Run RNNoise over 16 kHz audio. It works at 48 kHz, on 10 ms frames of
/// samples scaled like 16-bit audio.
fn denoise(samples: &[f32]) -> Vec<f32> {
    use nnnoiseless::DenoiseState;
    // Up to 48 kHz by linear interpolation; RNNoise ignores what little
    // that adds above 8 kHz.
    let mut high: Vec<f32> = Vec::with_capacity(samples.len() * 3);
    for (i, s) in samples.iter().enumerate() {
        let next = samples.get(i + 1).copied().unwrap_or(*s);
        for k in 0..3 {
            high.push((s + (next - s) * k as f32 / 3.0) * 32_768.0);
        }
    }
    high.resize(high.len().div_ceil(DenoiseState::FRAME_SIZE) * DenoiseState::FRAME_SIZE, 0.0);
    let mut state = DenoiseState::new();
    let mut out = vec![0.0; high.len()];
    for (input, output) in high.chunks(DenoiseState::FRAME_SIZE).zip(out.chunks_mut(DenoiseState::FRAME_SIZE)) {
        state.process_frame(output, input);
    }
    let out: Vec<f32> = out.iter().map(|s| s / 32_768.0).collect();
    utter_lib::audio::resample(&out, 48_000, TARGET_RATE)
}

/// Half a second of quiet on each side, as a real dictation has.
fn pad(speech: &[f32]) -> Vec<f32> {
    let mut out = vec![0.0; RATE / 2];
    out.extend_from_slice(speech);
    out.resize(out.len() + RATE / 2, 0.0);
    out
}

/// `speech` with the fan running from before it starts to after it ends,
/// at `snr` dB below the speech's active level.
fn mix(speech: &[f32], fan: &[f32], snr: f32) -> Vec<f32> {
    let speech_level = active_rms(speech);
    let fan_level = rms(fan);
    let gain = speech_level / fan_level / 10f32.powf(snr / 20.0);
    let mut out: Vec<f32> = fan.iter().map(|n| n * gain).collect();
    for (o, s) in out[RATE / 2..].iter_mut().zip(speech) {
        *o += s;
    }
    out
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

/// The level of the speech itself, leaving out its pauses: the RMS of the
/// 20 ms frames within 35 dB of the loudest.
fn active_rms(speech: &[f32]) -> f32 {
    let frames: Vec<f32> = speech.chunks(RATE / 50).map(rms).collect();
    let loudest = frames.iter().copied().fold(0.0, f32::max);
    let active: Vec<f32> = frames.into_iter().filter(|f| *f > loudest * 10f32.powf(-35.0 / 20.0)).collect();
    (active.iter().map(|f| f * f).sum::<f32>() / active.len().max(1) as f32).sqrt()
}

/// A desk or ceiling fan: pink hiss from the moving air, a brown rumble
/// under it, 60 Hz mains hum with two harmonics, and a blade tone at 140 Hz
/// (seven blades at 20 turns a second) that wobbles with each turn.
fn fan(len: usize, seed: u32) -> Vec<f32> {
    let white = {
        let mut state = seed.wrapping_mul(2_654_435_761).max(1);
        move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as f32 / u32::MAX as f32 * 2.0 - 1.0
        }
    };
    let mut white = white;
    // Paul Kellet's pink filter.
    let mut b = [0.0_f32; 7];
    let mut brown = 0.0_f32;
    let mut pink = Vec::with_capacity(len);
    let mut rumble = Vec::with_capacity(len);
    for _ in 0..len {
        let w = white();
        b[0] = 0.99886 * b[0] + w * 0.0555179;
        b[1] = 0.99332 * b[1] + w * 0.0750759;
        b[2] = 0.96900 * b[2] + w * 0.1538520;
        b[3] = 0.86650 * b[3] + w * 0.3104856;
        b[4] = 0.55000 * b[4] + w * 0.5329522;
        b[5] = -0.7616 * b[5] - w * 0.0168980;
        pink.push(b.iter().sum::<f32>() + w * 0.5362);
        b[6] = w * 0.115926;
        brown = 0.995 * brown + 0.05 * white();
        rumble.push(brown);
    }
    let (pink_level, rumble_level) = (rms(&pink), rms(&rumble));
    let tau = 2.0 * std::f32::consts::PI;
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let hum = 0.15 * (tau * 60.0 * t).sin() + 0.08 * (tau * 120.0 * t).sin() + 0.04 * (tau * 180.0 * t).sin();
            let blade = 0.1 * (tau * 140.0 * t).sin() * (1.0 + 0.3 * (tau * 20.0 * t).sin());
            pink[i] / pink_level + 0.6 * rumble[i] / rumble_level + hum + blade
        })
        .collect()
}

/// Word errors (substitutions, insertions, deletions) and reference words,
/// ignoring case and punctuation.
fn word_errors(reference: &str, heard: &str) -> (usize, usize) {
    let words = |text: &str| -> Vec<String> {
        text.to_lowercase()
            .replace(['-', '/'], " ")
            .split_whitespace()
            .map(|w| w.chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect::<String>())
            .filter(|w| !w.is_empty())
            .collect()
    };
    let r = words(reference);
    // "back off" for "backoff" and "on to" for "onto" are the same words.
    let mut h: Vec<String> = Vec::new();
    for word in words(heard) {
        match h.last_mut() {
            Some(last) if !r.contains(last) && r.contains(&format!("{last}{word}")) => last.push_str(&word),
            _ => h.push(word),
        }
    }
    let mut previous: Vec<usize> = (0..=h.len()).collect();
    for i in 1..=r.len() {
        let mut current = vec![i; h.len() + 1];
        for j in 1..=h.len() {
            let substitute = previous[j - 1] + usize::from(r[i - 1] != h[j - 1]);
            current[j] = substitute.min(previous[j] + 1).min(current[j - 1] + 1);
        }
        previous = current;
    }
    (previous[h.len()], r.len())
}
