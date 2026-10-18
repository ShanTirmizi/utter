//! Transcribe WAV files from the command line, without the app. Handy for
//! checking a model or comparing models on the same recordings.
//!
//! One model, some recordings:
//!
//!   cargo run --release --example transcribe -- path/to/ggml-base.en.bin recording.wav
//!
//! A model is a whisper.cpp `.bin` file or a Parakeet model folder (the
//! Parakeet helper must be built: `swift build -c release --package-path
//! src-tauri/parakeet`).
//!
//! `--vad ggml-silero-v6.2.0.bin` transcribes only the speech, as the app
//! does. `--seconds 60` uses only the start of each recording. `--live` also
//! cuts it into the pieces a live dictation is transcribed in while the
//! speaker talks, and transcribes them twice: each on its own, and each shown
//! the end of what came before (as Utter does), to compare where they meet.
//!
//! Several models over a folder of recordings, each with a `.txt` beside it
//! holding what was really said, as a table of word error rate and time:
//!
//!   cargo run --release --example transcribe -- --compare fixtures/ \
//!       --terms "Anthropic,Claude,Groq" ggml-base.en.bin parakeet-tdt-0.6b-v3/
//!
//! With `--terms`, each model runs twice: without the words, then with them,
//! the way the app passes the user's dictionary. `--show` prints every
//! transcript. To score models on kept recordings, see `replay.rs`.

use std::{
    path::{Path, PathBuf},
    time::Instant,
};
use utter_lib::{audio, transcribe::{self, Transcriber}};

fn main() -> Result<(), String> {
    whisper_rs::install_logging_hooks();
    let mut compare = None;
    let mut terms: Vec<String> = Vec::new();
    let mut show = false;
    let mut live = false;
    let mut vad: Option<PathBuf> = None;
    let mut seconds: Option<f32> = None;
    let mut rest = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--compare" => compare = Some(PathBuf::from(args.next().ok_or("--compare needs a folder")?)),
            "--terms" => {
                let list = args.next().ok_or("--terms needs a list")?;
                terms = list.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
            }
            "--show" => show = true,
            "--live" => live = true,
            "--vad" => vad = Some(PathBuf::from(args.next().ok_or("--vad needs a model file")?)),
            "--seconds" => seconds = Some(args.next().and_then(|n| n.parse::<f32>().ok()).ok_or("--seconds needs a number")?),
            _ => rest.push(arg),
        }
    }
    match compare {
        Some(folder) => run_comparison(&folder, &rest, &terms, show, vad.as_deref()),
        None => {
            let (model, wavs) = rest.split_first().ok_or("usage: transcribe <model> <audio.wav>...")?;
            let transcriber = Transcriber::default();
            let started = Instant::now();
            transcriber.ensure_loaded(model, Path::new(model))?;
            if let Some(vad) = &vad {
                transcriber.use_vad(vad)?;
            }
            eprintln!("model load {:.2}s", started.elapsed().as_secs_f32());
            for wav in wavs {
                let mut samples = read_wav(Path::new(wav))?;
                if let Some(seconds) = seconds {
                    samples.truncate((seconds * audio::TARGET_RATE as f32) as usize);
                }
                let started = Instant::now();
                let text = transcriber.transcribe(&samples, language(model), &terms)?;
                println!("{text}");
                eprintln!(
                    "audio {:.1}s | transcribe {:.2}s | speech detected: {}",
                    samples.len() as f32 / audio::TARGET_RATE as f32,
                    started.elapsed().as_secs_f32(),
                    match transcriber.speech(&audio::high_pass(&samples)) {
                        Some(speech) => format!("{} ms", speech.ms),
                        None => audio::has_speech(&audio::high_pass(&samples)).to_string(),
                    }
                );
                if live {
                    compare_pieces(&transcriber, model, &samples, language(model))?;
                }
            }
            Ok(())
        }
    }
}

fn language(model: &str) -> &'static str {
    if model.contains(".en") { "en" } else { "auto" }
}

/// One recording and what was said in it.
struct Fixture {
    name: String,
    samples: Vec<f32>,
    reference: String,
}

fn run_comparison(folder: &Path, models: &[String], terms: &[String], show: bool, vad: Option<&Path>) -> Result<(), String> {
    let mut fixtures = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(folder)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "wav"))
        .collect();
    entries.sort();
    for wav in entries {
        let Ok(reference) = std::fs::read_to_string(wav.with_extension("txt")) else { continue };
        fixtures.push(Fixture {
            name: wav.file_stem().unwrap().to_string_lossy().into(),
            samples: read_wav(&wav)?,
            reference,
        });
    }
    if fixtures.is_empty() {
        return Err("No .wav files with a .txt beside them in that folder.".into());
    }
    let seconds: f32 = fixtures.iter().map(|f| f.samples.len() as f32 / audio::TARGET_RATE as f32).sum();
    eprintln!("{} recordings, {seconds:.0} s of audio", fixtures.len());

    let passes: Vec<&[String]> = if terms.is_empty() { vec![&[]] } else { vec![&[], terms] };
    let mut rows = Vec::new();
    for model in models {
        let path = Path::new(model);
        let label = if path.is_dir() { path.file_name() } else { path.file_stem() };
        let label = label.unwrap().to_string_lossy().trim_start_matches("ggml-").to_string();
        let transcriber = Transcriber::default();
        let started = Instant::now();
        transcriber.ensure_loaded(model, Path::new(model))?;
        if let Some(vad) = vad {
            transcriber.use_vad(vad)?;
        }
        let load = started.elapsed().as_secs_f32();
        // The app loads its model ahead of the first dictation, so the first
        // run, which also warms caches, is not timed.
        transcriber.transcribe(&fixtures[0].samples, language(model), &[])?;

        for pass in &passes {
            let (mut errors, mut words, mut times) = (0, 0, Vec::new());
            let (mut terms_heard, mut terms_said) = (0, 0);
            for fixture in &fixtures {
                let started = Instant::now();
                let text = transcriber.transcribe(&fixture.samples, language(model), pass)?;
                times.push(started.elapsed().as_secs_f32() * 1000.0);
                let (said, heard) = (normalise(&fixture.reference), normalise(&text));
                errors += edit_distance(&said, &heard);
                words += said.len();
                let (h, s) = term_hits(&said, &heard, terms);
                terms_heard += h;
                terms_said += s;
                if show {
                    let ms = times[times.len() - 1];
                    println!("[{label}{}] {} ({ms:.0} ms): {text}", if pass.is_empty() { "" } else { " +terms" }, fixture.name);
                }
            }
            times.sort_by(f32::total_cmp);
            rows.push(format!(
                "| {label} | {} | {:.1}% | {terms_heard}/{terms_said} | {:.0} | {:.0} | {:.1} |",
                if pass.is_empty() { "no" } else { "yes" },
                100.0 * errors as f32 / words as f32,
                times[times.len() / 2],
                times[times.len() - 1],
                load,
            ));
        }
    }
    println!("| model | terms | WER | jargon heard | median ms | max ms | load s |");
    println!("|---|---|---|---|---|---|---|");
    for row in rows {
        println!("{row}");
    }
    Ok(())
}

/// Lower case words, without punctuation, for counting errors. Hyphens are
/// dropped, so "stand-up" and "standup" count as the same word.
fn normalise(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace(['’', '‘'], "'")
        .replace('-', "")
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .map(|w| w.trim_matches('\'').to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

/// Word-level Levenshtein distance: substitutions, insertions and deletions.
fn edit_distance(said: &[String], heard: &[String]) -> usize {
    let mut row: Vec<usize> = (0..=heard.len()).collect();
    for (i, s) in said.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, h) in heard.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if s == h { diagonal } else { 1 + diagonal.min(above).min(row[j]) };
            diagonal = above;
        }
    }
    row[heard.len()]
}

/// How many times the listed terms were said, and how many of those the
/// transcript has, each term counted no more often than it was said.
fn term_hits(said: &[String], heard: &[String], terms: &[String]) -> (usize, usize) {
    let count = |words: &[String], term: &[String]| words.windows(term.len()).filter(|w| *w == term).count();
    let (mut hit, mut total) = (0, 0);
    for term in terms {
        let term = normalise(term);
        if term.is_empty() {
            continue;
        }
        let n = count(said, &term);
        total += n;
        hit += n.min(count(heard, &term));
    }
    (hit, total)
}

fn read_wav(path: &Path) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let spec = reader.spec();
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = (1_i64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().filter_map(Result::ok).map(|s| s as f32 / scale).collect()
        }
    };
    let channels = spec.channels as usize;
    let mono: Vec<f32> =
        interleaved.chunks(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32).collect();
    Ok(audio::resample(&mono, spec.sample_rate, audio::TARGET_RATE))
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
            let text = transcriber.transcribe_with(model, path, piece, language, &[], &before)?;
            println!("[{:.1} s] {text}", piece.len() as f32 / audio::TARGET_RATE as f32);
            said = format!("{said} {text}");
        }
        eprintln!("{} pieces {label}: {:.2}s", pieces.len(), started.elapsed().as_secs_f32());
    }
    Ok(())
}
