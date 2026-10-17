//! Replay kept recordings through speech models and cleanup settings, and
//! score each combination against what the user meant to say. Recordings are
//! kept only with **Keep recordings** switched on in the app (under Accuracy
//! in About, with Keep history on), and this reads them where the app keeps
//! them.
//!
//!   cargo run --release --example replay
//!   cargo run --release --example replay -- --model base.en --model large-v3-turbo-q5_0 --cleanup none --cleanup saved
//!   cargo run --release --example replay -- --dir ~/clips --labels ~/clips/labels.tsv --model ~/ggml-small.en.bin
//!
//!   --dir DIR      Utter's folder in Application Support (the default), or
//!                  any folder of WAV files
//!   --labels FILE  references written by hand: a file name, a tab, and what
//!                  was said, one per line. Default: labels.tsv next to the
//!                  recordings, if there is one
//!   --model M      a speech model file, or the name of one the app has
//!                  downloaded. Repeat to compare. Default: the app's
//!   --cleanup C    none, saved (the app's provider and model), or
//!                  provider/model, such as groq/llama-3.1-8b-instant. Repeat
//!                  to compare. Default: none, and saved if it is set up.
//!                  The key comes from UTTER_API_KEY or the app's Keychain entry
//!   --each         print every recording's result, not just the totals
//!
//! For each combination it prints the word error rate (case and punctuation
//! ignored), the words a user would still have to change (exactly as typed),
//! the share that would need no change at all, and the time taken.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Instant,
};
use utter_lib::{
    accuracy::Score,
    replay::{self, Cleaner, Recording, Setup, Source},
    transcribe::Transcriber,
};

struct Options {
    dir: PathBuf,
    labels: Option<PathBuf>,
    models: Vec<String>,
    cleanups: Vec<String>,
    each: bool,
}

fn options() -> Result<Options, String> {
    let mut options = Options { dir: replay::data_dir(), labels: None, models: Vec::new(), cleanups: Vec::new(), each: false };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--dir" => options.dir = PathBuf::from(value()?),
            "--labels" => options.labels = Some(PathBuf::from(value()?)),
            "--model" => options.models.push(value()?),
            "--cleanup" => options.cleanups.push(value()?),
            "--each" => options.each = true,
            "-h" | "--help" => return Err("usage: replay [--dir DIR] [--labels FILE] [--model M]... [--cleanup C]... [--each]".into()),
            other => return Err(format!("unknown option {other}; try --help")),
        }
    }
    Ok(options)
}

/// One recording through one speech model.
struct Heard {
    raw: String,
    ms: u64,
}

/// Totals for one combination of speech model and cleanup.
#[derive(Default)]
struct Totals {
    score: Score,
    count: usize,
    /// Recordings that came out exactly as the reference.
    perfect: usize,
    failed: usize,
    transcribe_ms: Vec<u64>,
    model_ms: Vec<u64>,
    total_ms: Vec<u64>,
}

fn mean(values: &[u64]) -> f64 {
    if values.is_empty() { 0.0 } else { values.iter().sum::<u64>() as f64 / values.len() as f64 / 1000.0 }
}

/// The value nine in ten are at or under.
fn p90(values: &[u64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted.get((sorted.len() * 9).div_ceil(10).saturating_sub(1)).map_or(0.0, |&ms| ms as f64 / 1000.0)
}

fn main() -> Result<(), String> {
    whisper_rs::install_logging_hooks();
    let options = options()?;
    let setup = Setup::load(&options.dir);

    let labels_file = options.labels.clone().or_else(|| {
        [options.dir.join("recordings/labels.tsv"), options.dir.join("labels.tsv")].into_iter().find(|f| f.is_file())
    });
    let labels = match &labels_file {
        Some(file) => replay::read_labels(file)?,
        None => Default::default(),
    };
    let all = replay::gather(&options.dir, &labels);
    let recordings: Vec<&Recording> = all.iter().filter(|r| r.reference.is_some()).collect();

    // What there is to go on.
    let mut sources: BTreeMap<Source, usize> = BTreeMap::new();
    for recording in &recordings {
        *sources.entry(recording.reference.as_ref().unwrap().1).or_default() += 1;
    }
    let by_source: Vec<String> = sources.iter().map(|(source, n)| format!("{n} {}", source.name())).collect();
    println!(
        "{} recordings in {}: {} to score ({}), {} with nothing to score against",
        all.len(),
        options.dir.display(),
        recordings.len(),
        if by_source.is_empty() { "none".to_string() } else { by_source.join(", ") },
        all.len() - recordings.len()
    );
    if let Some(file) = &labels_file {
        println!("labels from {} ({} lines)", file.display(), labels.len());
    }
    if recordings.is_empty() {
        return Err("Nothing to replay. Switch on Keep recordings in Utter and dictate, or pass --dir and --labels.".into());
    }
    if sources.contains_key(&Source::Accepted) {
        println!("note: \"accepted\" references are what the app wrote at the time, so they favour the settings in use then");
    }

    let models = if options.models.is_empty() { vec![setup.model().to_string()] } else { options.models.clone() };
    let mut cleaners = Vec::new();
    let wanted = if options.cleanups.is_empty() { vec!["none".to_string(), "saved".to_string()] } else { options.cleanups.clone() };
    for spec in &wanted {
        let cleaner = match spec.as_str() {
            "none" => Ok(Cleaner::none()),
            "saved" => Cleaner::saved(&setup),
            spec => Cleaner::named(&setup, spec),
        };
        match cleaner {
            Ok(cleaner) => cleaners.push(cleaner),
            // Without a key the saved cleanup is simply left out of the default run.
            Err(e) if options.cleanups.is_empty() => eprintln!("skipping saved cleanup: {e}"),
            Err(e) => return Err(e),
        }
    }

    let audio: Vec<Vec<f32>> = recordings.iter().map(|r| replay::read_wav(&r.file)).collect::<Result<_, _>>()?;
    let hints = setup.hints();
    let transcriber = Transcriber::default();
    // Listen as the app does: only the speech, once the voice model is here.
    let vad = options.dir.join("models").join(format!("ggml-{}.bin", utter_lib::models::VAD));
    if vad.is_file() {
        transcriber.use_vad(&vad)?;
    } else {
        eprintln!("no voice activity model in {}; every stretch is transcribed", vad.display());
    }
    let mut rows = Vec::new();

    for model in &models {
        let file = Setup::model_file(&options.dir, model);
        let name = file.file_stem().unwrap_or_default().to_string_lossy().trim_start_matches("ggml-").to_string();
        let language = setup.language(model);
        let loading = Instant::now();
        transcriber.ensure_loaded(&name, &file).map_err(|e| format!("{model}: {e}"))?;
        eprintln!("{name}: loaded in {:.1}s, transcribing {} recordings", loading.elapsed().as_secs_f32(), audio.len());
        let mut heard = Vec::new();
        for samples in &audio {
            let started = Instant::now();
            let raw = transcriber.transcribe_with(&name, &file, samples, &language, &hints, "").unwrap_or_default();
            heard.push(Heard { raw, ms: started.elapsed().as_millis() as u64 });
        }

        for cleaner in &cleaners {
            let label = format!("{name} + {}", cleaner.label);
            let mut totals = Totals::default();
            if options.each {
                println!("\n== {label}");
            }
            for (recording, heard) in recordings.iter().zip(&heard) {
                let (reference, source) = recording.reference.as_ref().unwrap();
                let tidied = cleaner.tidy(&heard.raw, recording, &setup);
                let score = Score::of(&tidied.text, reference);
                totals.score += score;
                totals.count += 1;
                totals.perfect += usize::from(score.edits == 0);
                totals.failed += usize::from(tidied.failed);
                totals.transcribe_ms.push(heard.ms);
                if let Some(ms) = tidied.model_ms {
                    totals.model_ms.push(ms);
                }
                totals.total_ms.push(heard.ms + tidied.model_ms.unwrap_or(0));
                if options.each {
                    print_one(recording, *source, reference, &tidied.text, score);
                }
            }
            rows.push((label, totals));
        }
    }

    println!();
    println!(
        "{:<44} {:>4} {:>6} {:>7} {:>8} {:>10} {:>7} {:>11}",
        "speech model + cleanup", "n", "WER", "edits", "no edit", "transcribe", "tidy", "total p90"
    );
    for (label, t) in &rows {
        let failed = if t.failed > 0 { format!("  ({} tidy failed)", t.failed) } else { String::new() };
        println!(
            "{:<44} {:>4} {:>5.1}% {:>7.2} {:>7.0}% {:>9.2}s {:>6} {:>10.2}s{failed}",
            label,
            t.count,
            t.score.error_rate() * 100.0,
            t.score.edits as f64 / t.count as f64,
            t.perfect as f64 * 100.0 / t.count as f64,
            mean(&t.transcribe_ms),
            if t.model_ms.is_empty() { "-".to_string() } else { format!("{:.2}s", mean(&t.model_ms)) },
            p90(&t.total_ms),
        );
    }
    println!();
    println!("WER: word errors per reference word, ignoring case and punctuation. edits: words to change per");
    println!("recording, exactly as typed. no edit: recordings that came out exactly right. Times are means,");
    println!("the whole recording at once; the app transcribes long ones while you speak, so it waits less.");
    Ok(())
}

fn print_one(recording: &Recording, source: Source, reference: &str, written: &str, score: Score) {
    let name = recording.file.file_name().map(Path::new).unwrap_or(Path::new("?")).display();
    let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    println!(
        "{name} [{}] {} of {} wrong, {}",
        source.name(),
        score.errors,
        plural(score.words, "word", "words"),
        plural(score.edits, "edit", "edits")
    );
    if score.edits > 0 {
        println!("  meant: {reference}");
        println!("  wrote: {written}");
    }
}
