//! The dictation pipeline, driven by the hotkey:
//!
//!   key down -> note the focused app -> record
//!   key up   -> transcribe -> clean up (optional) -> paste into the focused app
//!
//! All the state for "what is Utter doing right now" lives here, so there is
//! exactly one place that decides what a key press means.

use crate::{
    audio::{self, Recorder, TARGET_RATE},
    cleanup,
    context::{self, AppContext},
    insert, models, permissions, secrets, settings,
    transcribe::{SILENCE_PEAK, Transcriber},
};
use serde::Serialize;
use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
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
    /// True when `text` came from the cleanup model rather than being `raw`.
    pub cleaned: bool,
    /// True when the text was pasted for the user; false when it was only
    /// copied to the clipboard.
    pub inserted: bool,
    /// Anything the user should know, such as why cleanup was skipped.
    pub note: Option<String>,
    /// The app that had focus when the hotkey went down.
    pub app: AppContext,
    pub transcribe_ms: u64,
    pub cleanup_ms: Option<u64>,
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
    /// The app in focus at key-down. It decides the writing style; the paste
    /// itself goes to whatever has focus when the text is ready.
    target: Mutex<AppContext>,
    last: Mutex<Option<Dictation>>,
}

impl Pipeline {
    pub fn new() -> Self {
        Self {
            recorder: Recorder::spawn(),
            transcriber: Arc::new(Transcriber::default()),
            phase: Mutex::new(Phase::Idle),
            target: Mutex::new(AppContext::unknown()),
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
    *pipeline.target.lock().unwrap() = context::frontmost();
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

    let settings = settings::load(app);
    let name = settings.model.clone();
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

    let target = app.state::<Pipeline>().target.lock().unwrap().clone();
    let tidied = tidy_up(&raw, &target, settings.cleanup_enabled).await;
    let (inserted, delivery_note) = deliver(app, &tidied.text).await?;

    Ok(Some(Dictation {
        text: tidied.text,
        cleaned: tidied.cleaned,
        inserted,
        // A delivery problem matters more to the user than a cleanup one.
        note: delivery_note.or(tidied.note),
        cleanup_ms: tidied.ms,
        raw,
        app: target,
        transcribe_ms,
    }))
}

struct Tidied {
    text: String,
    cleaned: bool,
    note: Option<String>,
    ms: Option<u64>,
}

/// Run the optional cleanup step. Whatever goes wrong, the raw transcript
/// comes back, so a dictation is never lost to a network or API problem.
async fn tidy_up(raw: &str, target: &AppContext, enabled: bool) -> Tidied {
    let untouched = |note: Option<String>, ms: Option<u64>| Tidied {
        text: raw.to_string(),
        cleaned: false,
        note,
        ms,
    };
    if !enabled {
        return untouched(None, None);
    }
    // Reading the Keychain can block on a system prompt; keep it off the
    // async threads.
    let key = tauri::async_runtime::spawn_blocking(secrets::get)
        .await
        .ok()
        .flatten();
    let Some(key) = key else {
        return untouched(None, None);
    };

    let started = Instant::now();
    let result = cleanup::clean(raw, target, &key).await;
    let ms = Some(started.elapsed().as_millis() as u64);
    match result {
        Ok(text) => Tidied {
            text,
            cleaned: true,
            note: None,
            ms,
        },
        Err(e) => untouched(Some(format!("Cleanup skipped: {e}")), ms),
    }
}

/// Run `work` on the main thread and wait for its result. The pasteboard must
/// only be touched from there.
fn on_main<T: Send + 'static>(
    app: &AppHandle,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    let (reply, result) = mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = reply.send(work());
    })
    .map_err(|e| e.to_string())?;
    result.recv().map_err(|e| e.to_string())
}

/// Get the text into the focused app. Returns whether it was pasted, plus a
/// note when it could only be copied.
///
/// Fallback: if Utter may not press keys (Accessibility is off) or a password
/// field has locked keyboard input, the text is left on the clipboard for the
/// user to paste, so nothing dictated is ever lost.
async fn deliver(app: &AppHandle, text: &str) -> Result<(bool, Option<String>), String> {
    let blocked = if !permissions::accessibility() {
        Some("Accessibility is off")
    } else if permissions::secure_input_active() {
        Some("a password field is active")
    } else {
        None
    };
    if let Some(reason) = blocked {
        let plain = text.to_string();
        on_main(app, move || insert::set_clipboard_text(&plain, false))?;
        return Ok((
            false,
            Some(format!("Copied to the clipboard ({reason}). Press ⌘V to paste.")),
        ));
    }

    // A trailing space lets the next dictation follow on without running the
    // two together.
    let spaced = if text.ends_with(char::is_whitespace) {
        text.to_string()
    } else {
        format!("{text} ")
    };
    on_main(app, move || insert::stage(&spaced))?;
    // Give the system a moment to publish the new clipboard before pasting.
    tokio::time::sleep(Duration::from_millis(60)).await;
    on_main(app, insert::press_paste)??;

    // Put the user's clipboard back once the target app has had time to read
    // ours. This runs on its own so the pipeline is free for the next press.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(800)).await;
        let _ = on_main(&app, insert::restore_staged);
    });
    Ok((true, None))
}

/// Put the most recent dictation on the clipboard. Called from the tray menu,
/// whose handler already runs on the main thread.
pub fn copy_last(app: &AppHandle) {
    if let Some(last) = app.state::<Pipeline>().last() {
        insert::set_clipboard_text(&last.text, false);
    }
}
