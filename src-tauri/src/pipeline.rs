//! The dictation pipeline, driven by the hotkey:
//!
//!   key down -> note the focused app -> record
//!   key up   -> transcribe -> tidy or rewrite (optional) -> paste into the focused app
//!
//! Which of the two hotkeys was pressed decides between tidying and rewriting.
//!
//! All the state for "what is Utter doing right now" lives here, so there is
//! exactly one place that decides what a key press means.

use crate::{
    audio::{self, Recorder, TARGET_RATE},
    cleanup::{self, Task},
    context::{self, AppContext},
    insert, models, permissions,
    providers::{self, NotReady},
    settings::{self, Settings},
    transcribe::Transcriber,
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

/// What to do with the words once they are transcribed. Each hotkey has one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Keep the speaker's wording; remove fillers and fix punctuation.
    Tidy,
    /// Say the same thing better, in the chosen style.
    Rewrite,
}

/// The result of one dictation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dictation {
    /// Exactly what the speech model heard.
    pub raw: String,
    /// What Utter produced from it.
    pub text: String,
    /// Which hotkey was used.
    pub mode: Mode,
    /// True when `text` came from the language model rather than being `raw`.
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
    pub mode: Mode,
    pub message: Option<String>,
    pub dictation: Option<Dictation>,
}

pub struct Pipeline {
    recorder: Recorder,
    transcriber: Arc<Transcriber>,
    phase: Mutex<Phase>,
    /// Which hotkey started the current dictation.
    mode: Mutex<Mode>,
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
            mode: Mutex::new(Mode::Tidy),
            target: Mutex::new(AppContext::unknown()),
            last: Mutex::new(None),
        }
    }

    pub fn last(&self) -> Option<Dictation> {
        self.last.lock().unwrap().clone()
    }
}

fn emit(app: &AppHandle, phase: &'static str, message: Option<String>, dictation: Option<Dictation>) {
    let mode = *app.state::<Pipeline>().mode.lock().unwrap();
    let _ = app.emit("pipeline", Update { phase, mode, message, dictation });
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

/// A hotkey went down: start recording. `mode` says which hotkey.
pub fn on_press(app: &AppHandle, mode: Mode) {
    let pipeline = app.state::<Pipeline>();
    let mut phase = pipeline.phase.lock().unwrap();
    // Holding a key can repeat the press; only the first one counts.
    if *phase != Phase::Idle {
        return;
    }
    *pipeline.mode.lock().unwrap() = mode;
    if !models::is_downloaded(app, &settings::load(app).model) {
        emit(app, "error", Some("Download a speech model in Utter's settings first.".into()), None);
        return;
    }
    if permissions::microphone() == permissions::MicStatus::Denied {
        emit(app, "error", Some("Microphone access is off. Allow Utter in System Settings.".into()), None);
        return;
    }
    *pipeline.target.lock().unwrap() = context::frontmost();
    match pipeline.recorder.start() {
        Ok(()) => {
            *phase = Phase::Recording;
            emit(app, "recording", None, None);
            warm_local_model(app, mode);
        }
        Err(e) => emit(app, "error", Some(e), None),
    }
}

/// If the language model runs on this Mac, start loading it now, while the
/// user is still speaking. Hosted providers need no head start, and are never
/// contacted until there is text to send.
fn warm_local_model(app: &AppHandle, mode: Mode) {
    let settings = settings::load(app);
    if mode == Mode::Tidy && !settings.cleanup_enabled {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let resolved = tauri::async_runtime::spawn_blocking(move || providers::resolve(&settings)).await;
        if let Ok(Ok(endpoint)) = resolved {
            if endpoint.local {
                cleanup::warm_up(&endpoint).await;
            }
        }
    });
}

/// A hotkey came up: stop recording and process the audio off the main thread.
pub fn on_release(app: &AppHandle, mode: Mode) {
    let pipeline = app.state::<Pipeline>();
    let samples = {
        let mut phase = pipeline.phase.lock().unwrap();
        // Only the hotkey that started the recording can end it.
        if *phase != Phase::Recording || *pipeline.mode.lock().unwrap() != mode {
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
    if !audio::has_speech(&samples) {
        return Err("Didn't catch any speech. Check which microphone macOS is using.".into());
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
    let mode = *app.state::<Pipeline>().mode.lock().unwrap();
    let tidied = improve(&raw, &target, &settings, mode).await;
    let (inserted, delivery_note) = deliver(app, &tidied.text).await?;

    Ok(Some(Dictation {
        text: tidied.text,
        mode,
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

/// Run the language-model step for `mode`. Whatever goes wrong, the raw
/// transcript comes back, so a dictation is never lost to a network or API
/// problem.
async fn improve(raw: &str, target: &AppContext, settings: &Settings, mode: Mode) -> Tidied {
    let untouched = |note: Option<String>, ms: Option<u64>| Tidied {
        text: raw.to_string(),
        cleaned: false,
        note,
        ms,
    };
    // The tidy hotkey respects the on/off switch. The rewrite hotkey is an
    // explicit request, so it always tries.
    let (task, what) = match mode {
        Mode::Tidy if !settings.cleanup_enabled => return untouched(None, None),
        Mode::Tidy => (Task::Tidy, "Cleanup"),
        Mode::Rewrite => (
            Task::Rewrite {
                instruction: cleanup::rewrite_instruction(
                    &settings.rewrite_style,
                    &settings.rewrite_prompt,
                ),
            },
            "Rewrite",
        ),
    };
    // Reading the Keychain can block on a system prompt; keep it off the
    // async threads.
    let for_lookup = settings.clone();
    let resolved = tauri::async_runtime::spawn_blocking(move || providers::resolve(&for_lookup)).await;
    let endpoint = match resolved {
        Ok(Ok(endpoint)) => endpoint,
        // For tidying, no key yet is the ordinary "raw text" mode, not a
        // fault. Someone who pressed the rewrite hotkey should be told.
        Ok(Err(NotReady::NoKey)) if mode == Mode::Tidy => return untouched(None, None),
        Ok(Err(NotReady::NoKey)) => {
            return untouched(
                Some("Rewrite needs a model. Set one up under Cleanup in settings.".into()),
                None,
            );
        }
        Ok(Err(NotReady::Problem(text))) => {
            return untouched(Some(format!("{what} skipped: {text}")), None);
        }
        Err(e) => return untouched(Some(format!("{what} skipped: {e}")), None),
    };

    let started = Instant::now();
    let result = cleanup::run(raw, target, &endpoint, &task).await;
    let ms = Some(started.elapsed().as_millis() as u64);
    match result {
        Ok(text) => Tidied {
            text,
            cleaned: true,
            note: None,
            ms,
        },
        Err(e) => untouched(Some(format!("{what} skipped: {e}")), ms),
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
