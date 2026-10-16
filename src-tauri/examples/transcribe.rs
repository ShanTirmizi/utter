//! Transcribe a WAV file from the command line, without the app. Handy for
//! checking a model or comparing models on the same recording.
//!
//!   cargo run --example transcribe -- path/to/ggml-base.en.bin recording.wav [ggml-silero-v6.2.0.bin]
//!
//! With the voice activity model as a third argument, only the speech in
//! the recording is transcribed, as in the app.

use std::{path::Path, time::Instant};
use utter_lib::{audio, transcribe::Transcriber};

fn main() -> Result<(), String> {
    whisper_rs::install_logging_hooks();
    let mut args = std::env::args().skip(1);
    let (Some(model), Some(wav)) = (args.next(), args.next()) else {
        return Err("usage: transcribe <model.bin> <audio.wav> [vad.bin]".into());
    };
    let vad = args.next();

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
    let samples = audio::resample(&mono, spec.sample_rate, audio::TARGET_RATE);

    let language = if model.contains(".en") { "en" } else { "auto" };
    let transcriber = Transcriber::default();

    let started = Instant::now();
    transcriber.ensure_loaded("cli", Path::new(&model))?;
    if let Some(vad) = &vad {
        transcriber.use_vad(Path::new(vad))?;
    }
    let loaded = started.elapsed();
    let text = transcriber.transcribe(&samples, language)?;
    match transcriber.speech(&audio::high_pass(&samples)) {
        Some(speech) => eprintln!("speech detected: {} ms", speech.ms),
        None => eprintln!("speech detected: {}", audio::has_speech(&audio::high_pass(&samples))),
    }

    println!("{text}");
    eprintln!(
        "audio {:.1}s | model load {:.2}s | transcribe {:.2}s",
        samples.len() as f32 / audio::TARGET_RATE as f32,
        loaded.as_secs_f32(),
        (started.elapsed() - loaded).as_secs_f32()
    );
    Ok(())
}
