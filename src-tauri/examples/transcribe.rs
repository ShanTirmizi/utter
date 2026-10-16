//! Transcribe a WAV file from the command line, without the app. Handy for
//! checking a model or comparing models on the same recording.
//!
//!   cargo run --example transcribe -- path/to/ggml-base.en.bin recording.wav
//!
//! `--seconds 60` uses only the start of the recording. `--live` also cuts it
//! into the pieces a live dictation is transcribed in while the speaker
//! talks, and transcribes them twice: each on its own, and each shown the
//! end of what came before (as Utter does), to compare where they meet.

use std::{path::Path, time::Instant};
use utter_lib::{audio, transcribe::{self, Transcriber}};

fn main() -> Result<(), String> {
    whisper_rs::install_logging_hooks();
    let mut args = std::env::args().skip(1);
    let (Some(model), Some(wav)) = (args.next(), args.next()) else {
        return Err("usage: transcribe <model.bin> <audio.wav> [--seconds N] [--live]".into());
    };
    let rest: Vec<String> = args.collect();
    let live = rest.iter().any(|a| a == "--live");
    let seconds: Option<f32> = rest
        .iter()
        .position(|a| a == "--seconds")
        .and_then(|i| rest.get(i + 1))
        .and_then(|n| n.parse().ok());

    let mut reader = hound::WavReader::open(&wav).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = (1_i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .filter_map(Result::ok)
                .map(|s| s as f32 / scale)
                .collect()
        }
    };
    let channels = spec.channels as usize;
    let mono: Vec<f32> = interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    let mut samples = audio::resample(&mono, spec.sample_rate, audio::TARGET_RATE);
    if let Some(seconds) = seconds {
        samples.truncate((seconds * audio::TARGET_RATE as f32) as usize);
    }

    let language = if model.contains(".en") { "en" } else { "auto" };
    let transcriber = Transcriber::default();

    let started = Instant::now();
    transcriber.ensure_loaded("cli", Path::new(&model))?;
    let loaded = started.elapsed();
    let text = transcriber.transcribe(&samples, language)?;
    eprintln!("speech detected: {}", audio::has_speech(&samples));

    println!("{text}");
    if live {
        compare_pieces(&transcriber, &model, &samples, language)?;
    }
    eprintln!(
        "audio {:.1}s | model load {:.2}s | transcribe {:.2}s",
        samples.len() as f32 / audio::TARGET_RATE as f32,
        loaded.as_secs_f32(),
        (started.elapsed() - loaded).as_secs_f32()
    );
    Ok(())
}

/// Cut `samples` as a live dictation would be (see `settled_cut`), then
/// transcribe the pieces alone and with what came before, and print both.
fn compare_pieces(transcriber: &Transcriber, model: &str, samples: &[f32], language: &str) -> Result<(), String> {
    let step = audio::TARGET_RATE as usize * 3 / 2;
    let (mut done, mut end) = (0, 0);
    let mut pieces = Vec::new();
    while end < samples.len() {
        end = (end + step).min(samples.len());
        if let Some(cut) = transcribe::settled_cut(&samples[done..end]) {
            pieces.push(&samples[done..done + cut]);
            done += cut;
        }
    }
    pieces.push(&samples[done..]);
    let path = Path::new(model);
    for (label, carry_on) in [("alone", false), ("shown what came before", true)] {
        let mut said = String::new();
        let started = Instant::now();
        println!("\n--- pieces {label}");
        for piece in &pieces {
            let before = if carry_on { said.clone() } else { String::new() };
            let text = transcriber.transcribe_with("cli", path, piece, language, &[], &before)?;
            println!("[{:.1} s] {text}", piece.len() as f32 / audio::TARGET_RATE as f32);
            said = format!("{said} {text}");
        }
        eprintln!("{} pieces {label}: {:.2}s", pieces.len(), started.elapsed().as_secs_f32());
    }
    Ok(())
}
