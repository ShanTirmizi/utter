//! The dictation pipeline, driven by the hotkey:
//!
//!   key down -> record -> key up -> (more stages arrive in later steps)
//!
//! All the state for "what is Utter doing right now" lives here, so there is
//! exactly one place that decides what a key press means.

use crate::audio::{Recorder, TARGET_RATE};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    Recording,
}

/// Sent to the UI on the `pipeline` event whenever something changes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub phase: &'static str,
    pub message: Option<String>,
}

pub struct Pipeline {
    recorder: Recorder,
    phase: Mutex<Phase>,
}

impl Pipeline {
    pub fn new() -> Self {
        Self {
            recorder: Recorder::spawn(),
            phase: Mutex::new(Phase::Idle),
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
}

fn emit(app: &AppHandle, phase: &'static str, message: Option<String>) {
    let _ = app.emit("pipeline", Update { phase, message });
}

/// Hotkey went down: start recording.
pub fn on_press(app: &AppHandle) {
    let pipeline = app.state::<Pipeline>();
    let mut phase = pipeline.phase.lock().unwrap();
    // Holding a key can repeat the press; only the first one counts.
    if *phase != Phase::Idle {
        return;
    }
    match pipeline.recorder.start() {
        Ok(()) => {
            *phase = Phase::Recording;
            emit(app, "recording", None);
        }
        Err(e) => emit(app, "error", Some(e)),
    }
}

/// Hotkey came up: stop recording and hand the audio on.
pub fn on_release(app: &AppHandle) {
    let pipeline = app.state::<Pipeline>();
    let mut phase = pipeline.phase.lock().unwrap();
    if *phase != Phase::Recording {
        return;
    }
    let samples = pipeline.recorder.stop();
    *phase = Phase::Idle;
    let seconds = samples.len() as f32 / TARGET_RATE as f32;
    emit(app, "idle", Some(format!("Recorded {seconds:.1} s of audio.")));
}
