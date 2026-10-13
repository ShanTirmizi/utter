//! The dictation pipeline, driven by the hotkey:
//!
//!   key down -> record -> key up -> transcribe -> (more stages in later steps)
//!
//! All the state for "what is Utter doing right now" lives here, so there is
//! exactly one place that decides what a key press means.

use crate::{
    audio::{self, Recorder, TARGET_RATE},
    models, settings,
    transcribe::{SILENCE_PEAK, Transcriber},
};
use serde::Serialize;
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};
use tauri::{AppHandle, Emitter, Manager};

/// Recordings shorter than this are treated as an accidental tap.
const MIN_SAMPLES: usize = TARGET_RATE as usize * 3 / 10;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    Recording,
    Processing,
}

/// The result of one dictation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dictation {
    /// Exactly what the speech model heard.
    pub raw: String,
    /// What Utter produced from it.
    pub text: String,
    pub transcribe_ms: u64,
}

/// Sent to the UI on the `pipeline` event whenever something changes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub phase: &'static str,
    pub message: Option<String>,
    pub dictation: Option<Dictation>,
}

pub struct Pipeline {
    recorder: Recorder,
    transcriber: Arc<Transcriber>,
    phase: Mutex<Phase>,
    last: Mutex<Option<Dictation>>,
}

impl Pipeline {
    pub fn new() -> Self {
        Self {
            recorder: Recorder::spawn(),
            transcriber: Arc::new(Transcriber::default()),
            phase: Mutex::new(Phase::Idle),
            last: Mutex::new(None),
        }
    }

    /// Open the microphone for a moment. The first time, this is what makes
    /// macOS ask the user for microphone access.
    pub fn probe_microphone(&self) {
        let phase = self.phase.lock().unwrap();
        if *phase != Phase::Idle {
            return;
        }
        if self.recorder.start().is_ok() {
            std::thread::sleep(std::time::Duration::from_millis(400));
            self.recorder.stop();
        }
    }

    pub fn last(&self) -> Option<Dictation> {
        self.last.lock().unwrap().clone()
    }
}

fn emit(app: &AppHandle, phase: &'static str, message: Option<String>, dictation: Option<Dictation>) {
    let _ = app.emit("pipeline", Update { phase, message, dictation });
}

/// Load the selected speech model in the background so the first dictation
/// does not have to wait for it.
pub fn preload_model(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let name = settings::load(&app).model;
        if !models::is_downloaded(&app, &name) {
            return;
        }
        if let Ok(path) = models::path(&app, &name) {
            let transcriber = app.state::<Pipeline>().transcriber.clone();
            if let Err(e) = transcriber.ensure_loaded(&name, &path) {
                eprintln!("[utter] {e}");
            }
        }
    });
}

/// Hotkey went down: start recording.
pub fn on_press(app: &AppHandle) {
    let pipeline = app.state::<Pipeline>();
    let mut phase = pipeline.phase.lock().unwrap();
    // Holding a key can repeat the press; only the first one counts.
    if *phase != Phase::Idle {
        return;
    }
    if !models::is_downloaded(app, &settings::load(app).model) {
        emit(app, "error", Some("Download a speech model in Utter's settings first.".into()), None);
        return;
    }
    match pipeline.recorder.start() {
        Ok(()) => {
            *phase = Phase::Recording;
            emit(app, "recording", None, None);
        }
        Err(e) => emit(app, "error", Some(e), None),
    }
}

/// Hotkey came up: stop recording and process the audio off the main thread.
pub fn on_release(app: &AppHandle) {
    let pipeline = app.state::<Pipeline>();
    let samples = {
        let mut phase = pipeline.phase.lock().unwrap();
        if *phase != Phase::Recording {
            return;
        }
        *phase = Phase::Processing;
        pipeline.recorder.stop()
    };
    emit(app, "processing", None, None);

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = process(&app, samples).await;
        let pipeline = app.state::<Pipeline>();
        *pipeline.phase.lock().unwrap() = Phase::Idle;
        match outcome {
            Ok(Some(dictation)) => {
                *pipeline.last.lock().unwrap() = Some(dictation.clone());
                emit(&app, "done", None, Some(dictation));
            }
            Ok(None) => emit(&app, "idle", None, None),
            Err(e) => emit(&app, "error", Some(e), None),
        }
    });
}

/// Everything that happens after the key is released. `Ok(None)` means there
/// was nothing worth doing (an accidental tap).
async fn process(app: &AppHandle, samples: Vec<f32>) -> Result<Option<Dictation>, String> {
    if samples.len() < MIN_SAMPLES {
        return Ok(None);
    }
    if audio::peak(&samples) < SILENCE_PEAK {
        return Err("Heard only silence. Check which microphone macOS is using.".into());
    }

    let name = settings::load(app).model;
    let path = models::path(app, &name)?;
    let transcriber = app.state::<Pipeline>().transcriber.clone();

    let started = Instant::now();
    let raw = tauri::async_runtime::spawn_blocking(move || {
        transcriber.ensure_loaded(&name, &path)?;
        transcriber.transcribe(&samples, models::language(&name))
    })
    .await
    .map_err(|e| e.to_string())??;
    let transcribe_ms = started.elapsed().as_millis() as u64;

    if raw.is_empty() {
        return Err("No speech detected.".into());
    }

    Ok(Some(Dictation {
        text: raw.clone(),
        raw,
        transcribe_ms,
    }))
}
