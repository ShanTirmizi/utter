//! Transcribe a WAV file from the command line, without the app. Handy for
//! checking a model or comparing models on the same recording.
//!
//!   cargo run --example transcribe -- path/to/ggml-base.en.bin recording.wav
//!
//! To score models on many recordings at once, see `replay.rs`.

use std::{path::Path, time::Instant};
use utter_lib::{audio, replay, transcribe::Transcriber};

fn main() -> Result<(), String> {
    whisper_rs::install_logging_hooks();
    let mut args = std::env::args().skip(1);
    let (Some(model), Some(wav)) = (args.next(), args.next()) else {
        return Err("usage: transcribe <model.bin> <audio.wav>".into());
    };

    let samples = replay::read_wav(Path::new(&wav))?;

    let language = if model.contains(".en") { "en" } else { "auto" };
    let transcriber = Transcriber::default();

    let started = Instant::now();
    transcriber.ensure_loaded("cli", Path::new(&model))?;
    let loaded = started.elapsed();
    let text = transcriber.transcribe(&samples, language)?;
    eprintln!("speech detected: {}", audio::has_speech(&samples));

    println!("{text}");
    eprintln!(
        "audio {:.1}s | model load {:.2}s | transcribe {:.2}s",
        samples.len() as f32 / audio::TARGET_RATE as f32,
        loaded.as_secs_f32(),
        (started.elapsed() - loaded).as_secs_f32()
    );
    Ok(())
}
