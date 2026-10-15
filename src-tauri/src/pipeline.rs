//! The dictation pipeline, driven by the hotkey:
//!
//!   key down -> note the focused app -> record
//!   key up   -> transcribe -> tidy or rewrite (optional) -> paste into the focused app
//!
//! Which of the two hotkeys was pressed decides between tidying and rewriting.
//! With text selected, the rewrite hotkey edits that text as instructed
//! instead. A quick tap, rather than a hold, starts a hands-free recording
//! that runs until the next tap.
//!
//! All the state for "what is Utter doing right now" lives here, so there is
//! exactly one place that decides what a key press means.

use crate::{
    audio::{self, Recorder, TARGET_RATE},
    cleanup::{self, Task},
    context::{self, AppContext},
    history, insert, models, permissions,
    selection::{self, Selection},
    providers::{self, NotReady},
    settings::{self, Settings},
    transcribe::Transcriber,
    words::Word,
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Keep the speaker's wording; remove fillers and fix punctuation.
    #[default]
    Tidy,
    /// Say the same thing better, in the chosen style.
    Rewrite,
}

/// The result of one dictation. Also what the history file holds, so every
/// field has a default and older files keep loading as fields are added.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Dictation {
    /// Exactly what the speech model heard.
    pub raw: String,
    /// What Utter produced from it.
    pub text: String,
    /// Which hotkey was used.
    pub mode: Mode,
    /// The text that was selected and has been replaced, when the dictation
    /// was an instruction for editing it. Then `raw` is that instruction.
    pub selection: Option<String>,
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
    /// How long the hotkey was held.
    pub spoken_ms: u64,
    /// From the hotkey coming up to the text being delivered.
    pub total_ms: u64,
    /// When it finished, in milliseconds since 1970.
    pub finished_at: u64,
}

/// Sent to the UI on the `pipeline` event whenever something changes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub phase: &'static str,
    pub mode: Mode,
    /// True while a recording was started by a tap and runs until the next.
    pub hands_free: bool,
    /// While processing: "transcribing", then "tidying" or "rewriting".
    pub stage: Option<&'static str>,
    /// The app being dictated into, while processing.
    pub app: Option<AppContext>,
    pub message: Option<String>,
    pub dictation: Option<Dictation>,
}

pub struct Pipeline {
    recorder: Recorder,
    transcriber: Arc<Transcriber>,
    /// Speech from the recording under way that was already transcribed
    /// while the user kept talking.
    early: Mutex<Early>,
    /// Held while an early piece is being transcribed, so the end of the
    /// dictation waits for it instead of starting over.
    early_busy: tokio::sync::Mutex<()>,
    phase: Mutex<Phase>,
    /// Which hotkey started the current dictation.
    mode: Mutex<Mode>,
    /// Counts recordings, so a timer set for one can tell it is still the
    /// one in progress.
    recording_id: AtomicU64,
    /// The app in focus at key-down. It decides the writing style; the paste
    /// itself goes to whatever has focus when the text is ready.
    target: Mutex<AppContext>,
    /// When the current recording's key went down, to tell a tap from a hold.
    pressed_at: Mutex<Option<Instant>>,
    /// When the press had been fully dealt with and recording was under way.
    ready_at: Mutex<Option<Instant>>,
    /// Set while a tap-started, hands-free recording is running.
    latched: AtomicBool,
    /// A quick tap just ended; a second press soon after makes it a double
    /// tap, which turns the recording hands-free.
    awaiting_second_tap: AtomicBool,
    /// Set when a key press ended a hands-free recording; its own release
    /// must then do nothing.
    swallow_release: AtomicBool,
    /// The most recent dictation, whether or not history is being kept.
    last: Mutex<Option<Dictation>>,
    /// Past dictations, oldest first; mirrors the history file.
    history: Mutex<Vec<Dictation>>,
    /// Debug builds: a pretend selection for a simulated dictation.
    simulated_selection: Mutex<Option<String>>,
    /// Names and terms in the window in front when recording number `.0`
    /// began; hints for that dictation only.
    screen: Mutex<(u64, Vec<String>)>,
}

/// A hold shorter than this is a tap.
const TAP: Duration = Duration::from_millis(300);
/// How long after a quick tap a second press still counts as a double tap.
const DOUBLE_TAP: Duration = Duration::from_millis(400);
/// A release handled this soon after its press was dealt with had already
/// happened while the press was being handled.
const QUEUED: Duration = Duration::from_millis(25);
/// Selected text longer than this is too much for a model running on this
/// Mac, whose working memory is small; it would quietly drop the end.
const MAX_SELECTION_CHARS_LOCAL: usize = 3_000;
/// The most selected text that can be edited by voice in one go.
const MAX_SELECTION_CHARS: usize = 8_000;

impl Pipeline {
    pub fn new() -> Self {
        Self {
            recorder: Recorder::spawn(),
            transcriber: Arc::new(Transcriber::default()),
            early: Mutex::new(Early::default()),
            early_busy: tokio::sync::Mutex::new(()),
            phase: Mutex::new(Phase::Idle),
            mode: Mutex::new(Mode::Tidy),
            recording_id: AtomicU64::new(0),
            target: Mutex::new(AppContext::unknown()),
            pressed_at: Mutex::new(None),
            ready_at: Mutex::new(None),
            latched: AtomicBool::new(false),
            awaiting_second_tap: AtomicBool::new(false),
            swallow_release: AtomicBool::new(false),
            last: Mutex::new(None),
            history: Mutex::new(Vec::new()),
            simulated_selection: Mutex::new(None),
            screen: Mutex::new((0, Vec::new())),
        }
    }

    pub fn last(&self) -> Option<Dictation> {
        self.last.lock().unwrap().clone()
    }

    /// Past dictations, newest first.
    pub fn history(&self) -> Vec<Dictation> {
        self.history.lock().unwrap().iter().rev().cloned().collect()
    }

    /// Get the microphone ready ahead of the first dictation.
    pub fn prepare_microphone(&self) {
        self.recorder.prepare();
    }
}

/// Read the history file into memory at launch. The newest entry also
/// becomes "the last dictation", so it can still be copied after a restart.
pub fn load_history(app: &AppHandle) {
    let entries = history::load(app);
    let pipeline = app.state::<Pipeline>();
    *pipeline.last.lock().unwrap() = entries.last().cloned();
    *pipeline.history.lock().unwrap() = entries;
}

/// Forget every past dictation, in memory and on disk.
pub fn clear_history(app: &AppHandle) -> Result<(), String> {
    let pipeline = app.state::<Pipeline>();
    // Held across the write, so a dictation finishing now cannot write too.
    let mut entries = pipeline.history.lock().unwrap();
    entries.clear();
    *pipeline.last.lock().unwrap() = None;
    history::save(app, &entries)
}

/// Forget one past dictation, named by when it finished.
pub fn forget(app: &AppHandle, finished_at: u64) -> Result<(), String> {
    let pipeline = app.state::<Pipeline>();
    let mut entries = pipeline.history.lock().unwrap();
    entries.retain(|d| d.finished_at != finished_at);
    let mut last = pipeline.last.lock().unwrap();
    if last.as_ref().is_some_and(|d| d.finished_at == finished_at) {
        *last = entries.last().cloned();
    }
    history::save(app, &entries)
}

/// Correct a word in one past dictation, named by when it finished. Every
/// whole-word occurrence of `wrong` in what was written becomes `right`.
pub fn correct(app: &AppHandle, finished_at: u64, wrong: &str, right: &str) -> Result<(), String> {
    let pipeline = app.state::<Pipeline>();
    let mut entries = pipeline.history.lock().unwrap();
    let mut changed = false;
    for entry in entries.iter_mut().filter(|d| d.finished_at == finished_at) {
        entry.text = crate::words::replace_word(&entry.text, wrong, right);
        changed = true;
    }
    let mut last = pipeline.last.lock().unwrap();
    if let Some(entry) = last.as_mut().filter(|d| d.finished_at == finished_at) {
        entry.text = crate::words::replace_word(&entry.text, wrong, right);
    }
    if changed { history::save(app, &entries) } else { Ok(()) }
}

fn emit(app: &AppHandle, phase: &'static str, message: Option<String>, dictation: Option<Dictation>) {
    crate::tray::set_phase(app, phase);
    let pipeline = app.state::<Pipeline>();
    let mode = *pipeline.mode.lock().unwrap();
    let hands_free = pipeline.latched.load(Ordering::SeqCst);
    let _ = app.emit(
        "pipeline",
        Update { phase, mode, hands_free, stage: None, app: None, message, dictation },
    );
}

/// Say which step of processing has begun, so the pill can name it.
fn emit_stage(app: &AppHandle, stage: &'static str) {
    crate::tray::set_phase(app, "processing");
    let pipeline = app.state::<Pipeline>();
    let mode = *pipeline.mode.lock().unwrap();
    let target = pipeline.target.lock().unwrap().clone();
    let _ = app.emit(
        "pipeline",
        Update {
            phase: "processing",
            mode,
            hands_free: false,
            stage: Some(stage),
            app: Some(target),
            message: None,
            dictation: None,
        },
    );
}

/// How often the microphone level is sent to the UI while recording.
const LEVEL_INTERVAL: Duration = Duration::from_millis(50);
/// A microphone level (see `audio::meter`) that only a voice or another real
/// sound reaches; the hiss of a quiet room stays under it.
const VOICE_LEVEL: f32 = 0.33;
/// How many level readings in a row must reach it to count as a voice.
const VOICE_TICKS: u32 = 3;
/// A hands-free recording ends by itself after this long without a sound.
const HANDS_FREE_SILENCE: Duration = Duration::from_secs(30);

/// Send the microphone level on the `audio-level` event for as long as
/// recording number `id` lasts. The pill draws its waveform from it.
fn report_level(app: &AppHandle, id: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut heard_at = Instant::now();
        let mut loud_run = 0;
        loop {
            tokio::time::sleep(LEVEL_INTERVAL).await;
            let pipeline = app.state::<Pipeline>();
            let recording = *pipeline.phase.lock().unwrap() == Phase::Recording
                && pipeline.recording_id.load(Ordering::SeqCst) == id;
            if !recording {
                break;
            }
            let level = pipeline.recorder.level();
            let _ = app.emit("audio-level", level);
            // A voice stays up for a while; a key click or a door does not.
            loud_run = if level >= VOICE_LEVEL { loud_run + 1 } else { 0 };
            if loud_run >= VOICE_TICKS {
                heard_at = Instant::now();
            }
            // Nobody is holding a key during a hands-free recording, so it
            // has to end by itself once the speaker has plainly stopped.
            if pipeline.latched.load(Ordering::SeqCst) && heard_at.elapsed() >= HANDS_FREE_SILENCE {
                // The quiet that ended it is not part of the dictation.
                stop_hands_free(&app, id, HANDS_FREE_SILENCE - Duration::from_secs(1));
                break;
            }
        }
    });
}

/// End a hands-free recording that nobody tapped to finish. What was said is
/// kept, but only copied: typing it into whatever happens to be focused by
/// now, with nobody asking, could put it somewhere it does not belong.
fn stop_hands_free(app: &AppHandle, id: u64, quiet_tail: Duration) {
    let pipeline = app.state::<Pipeline>();
    let mut samples = {
        let mut phase = pipeline.phase.lock().unwrap();
        let same_recording = pipeline.recording_id.load(Ordering::SeqCst) == id;
        if *phase != Phase::Recording || !same_recording || !pipeline.latched.swap(false, Ordering::SeqCst) {
            return;
        }
        *phase = Phase::Processing;
        pipeline.recorder.stop()
    };
    let tail = (quiet_tail.as_secs_f32() * TARGET_RATE as f32) as usize;
    samples.truncate(samples.len().saturating_sub(tail));
    emit_stage(app, "transcribing");
    finish(app.clone(), samples, Delivery::StoppedItself);
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
    crate::fnkey::log(&format!("hotkey pressed ({mode:?})"));
    let pipeline = app.state::<Pipeline>();
    let mut phase = pipeline.phase.lock().unwrap();
    match *phase {
        Phase::Idle => {}
        Phase::Recording => {
            // The second tap of a double tap: keep recording, hands-free.
            if pipeline.awaiting_second_tap.swap(false, Ordering::SeqCst) {
                pipeline.latched.store(true, Ordering::SeqCst);
                drop(phase);
                pipeline.swallow_release.store(true, Ordering::SeqCst);
                emit(app, "recording", Some("Hands-free. Tap the hotkey again to finish.".into()), None);
                return;
            }
            // A tap of either hotkey ends a hands-free recording. Its own
            // release, which follows, must then do nothing. Otherwise this
            // is the key repeating while held, and only the first press counts.
            if pipeline.latched.swap(false, Ordering::SeqCst) {
                *phase = Phase::Processing;
                let samples = pipeline.recorder.stop();
                drop(phase);
                pipeline.swallow_release.store(true, Ordering::SeqCst);
                emit_stage(app, "transcribing");
                finish(app.clone(), samples, Delivery::Paste);
            }
            return;
        }
        // The last dictation is still being transcribed or tidied. A press
        // now is ignored, so it can neither interrupt that nor start a
        // second one; the pill keeps showing the work under way.
        Phase::Processing => {
            drop(phase);
            crate::fnkey::log("hotkey pressed while still working: ignored");
            return;
        }
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
    pipeline.latched.store(false, Ordering::SeqCst);
    pipeline.awaiting_second_tap.store(false, Ordering::SeqCst);
    pipeline.swallow_release.store(false, Ordering::SeqCst);
    *pipeline.pressed_at.lock().unwrap() = Some(Instant::now());
    *pipeline.ready_at.lock().unwrap() = None;
    match pipeline.recorder.start() {
        Ok(()) => {
            *phase = Phase::Recording;
            emit(app, "recording", None, None);
            if mode == Mode::Rewrite && permissions::accessibility() {
                // So that an app built on web technology can tell us what is
                // selected by the time the key comes up.
                selection::wake_frontmost_app();
            }
            warm_local_model(app, mode);
            let id = pipeline.recording_id.fetch_add(1, Ordering::SeqCst) + 1;
            read_screen(app, id);
            abandon_if_never_released(app, id);
            report_level(app, id);
            transcribe_while_speaking(app, id);
            *pipeline.ready_at.lock().unwrap() = Some(Instant::now());
        }
        Err(e) => emit(app, "error", Some(e), None),
    }
}

/// Read the names and terms in the window in front, for recording number
/// `id`, on a thread of its own: some apps are slow to describe themselves.
fn read_screen(app: &AppHandle, id: u64) {
    let settings = settings::load(app);
    if !settings.screen_words || !permissions::accessibility() {
        return;
    }
    // Apps built on web technology describe their contents only once asked.
    selection::wake_frontmost_app();
    let app = app.clone();
    std::thread::spawn(move || {
        // Give a woken app a moment to build its description.
        std::thread::sleep(Duration::from_millis(150));
        let text = selection::window_text();
        let found = crate::screen::terms(&text, &settings.dictionary, &settings.forgotten);
        let pipeline = app.state::<Pipeline>();
        if pipeline.recording_id.load(Ordering::SeqCst) == id {
            *pipeline.screen.lock().unwrap() = (id, found);
        }
    });
}

/// The names on screen for recording number `id`, if they were read in time.
fn screen_terms(app: &AppHandle, id: u64) -> Vec<String> {
    let pipeline = app.state::<Pipeline>();
    let screen = pipeline.screen.lock().unwrap();
    if screen.0 == id { screen.1.clone() } else { Vec::new() }
}

/// The user's words plus the names on screen, for the speech model: the
/// screen's few most frequent, after the user's own.
fn speech_terms(settings: &Settings, screen: &[String]) -> Vec<String> {
    let mut terms = crate::words::spellings(&settings.dictionary);
    terms.extend(screen.iter().take(12).cloned());
    terms
}

/// macOS very occasionally fails to report a key coming back up. Without a
/// limit the microphone would then stay open indefinitely, and the next tap
/// would type minutes of whatever was said in the room. So a recording that
/// reaches the maximum length is closed and thrown away. A hands-free
/// recording was started on purpose, so that one is kept and copied instead.
fn abandon_if_never_released(app: &AppHandle, id: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(audio::MAX_SECONDS as u64)).await;
        let pipeline = app.state::<Pipeline>();
        if pipeline.latched.load(Ordering::SeqCst) {
            return stop_hands_free(&app, id, Duration::ZERO);
        }
        {
            let mut phase = pipeline.phase.lock().unwrap();
            let same_recording = pipeline.recording_id.load(Ordering::SeqCst) == id;
            if *phase != Phase::Recording || !same_recording {
                return;
            }
            pipeline.recorder.stop();
            pipeline.latched.store(false, Ordering::SeqCst);
            *phase = Phase::Idle;
        }
        let minutes = audio::MAX_SECONDS / 60;
        emit(
            &app,
            "error",
            Some(format!("Stopped listening after {minutes} minutes. Nothing was typed.")),
            None,
        );
    });
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
    crate::fnkey::log(&format!("hotkey released ({mode:?})"));
    let pipeline = app.state::<Pipeline>();
    // The release of the tap that ended a hands-free recording.
    if pipeline.swallow_release.swap(false, Ordering::SeqCst) {
        return;
    }
    let samples = {
        let mut phase = pipeline.phase.lock().unwrap();
        // Only the hotkey that started the recording can end it.
        if *phase != Phase::Recording || *pipeline.mode.lock().unwrap() != mode {
            return;
        }
        if pipeline.latched.load(Ordering::SeqCst) || pipeline.awaiting_second_tap.load(Ordering::SeqCst) {
            return;
        }
        // A quick tap, not a hold. Wait a moment for a second tap, which
        // makes it hands-free; on its own, a tap is let go.
        let held = pipeline.pressed_at.lock().unwrap().map(|at| at.elapsed());
        // Presses and releases are handled one after the other. If opening
        // the microphone was slow (Bluetooth, say), the release has been
        // waiting its turn and is only now handled: the key was let go
        // before any audio could be captured, so that was a tap too.
        let waiting = pipeline.ready_at.lock().unwrap().is_some_and(|at| at.elapsed() < QUEUED);
        if (held.is_some_and(|held| held < TAP) || waiting) && settings::load(app).hands_free {
            pipeline.awaiting_second_tap.store(true, Ordering::SeqCst);
            let id = pipeline.recording_id.load(Ordering::SeqCst);
            drop(phase);
            let_go_unless_tapped_again(app, id);
            return;
        }
        *phase = Phase::Processing;
        pipeline.recorder.stop()
    };
    emit_stage(app, "transcribing");
    finish(app.clone(), samples, Delivery::Paste);
}

/// After a single quick tap, give the second tap of a double tap a moment to
/// arrive. If it does not, the tap was just a tap: stop listening, keep
/// nothing, and say how hands-free is started.
fn let_go_unless_tapped_again(app: &AppHandle, id: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(DOUBLE_TAP).await;
        let pipeline = app.state::<Pipeline>();
        {
            let mut phase = pipeline.phase.lock().unwrap();
            let same_recording = pipeline.recording_id.load(Ordering::SeqCst) == id;
            if *phase != Phase::Recording || !same_recording || !pipeline.awaiting_second_tap.swap(false, Ordering::SeqCst) {
                return;
            }
            pipeline.recorder.stop();
            *phase = Phase::Idle;
        }
        emit(&app, "idle", Some("Hold to talk, or double-tap for hands-free.".into()), None);
    });
}

/// Drop the recording under way without a word, because its key turned out
/// to be part of another shortcut (fn held with an arrow key, say). A
/// hands-free recording is left alone: nobody is holding a key for it.
pub fn cancel(app: &AppHandle) {
    let pipeline = app.state::<Pipeline>();
    {
        let mut phase = pipeline.phase.lock().unwrap();
        if *phase != Phase::Recording || pipeline.latched.load(Ordering::SeqCst) {
            return;
        }
        pipeline.awaiting_second_tap.store(false, Ordering::SeqCst);
        pipeline.recorder.stop();
        *phase = Phase::Idle;
    }
    // An empty message tells the pill to go without a note.
    emit(app, "idle", Some(String::new()), None);
}

/// The part of a recording transcribed before it ended.
#[derive(Default)]
struct Early {
    /// Which recording this belongs to; 0 for none.
    id: u64,
    /// The audio so far, at 16 kHz.
    audio: Vec<f32>,
    /// Where to ask the recorder for more from.
    offset: usize,
    /// How much of `audio` has been transcribed.
    done: usize,
    /// What it said, piece by piece.
    texts: Vec<String>,
    /// Each piece tidied too, when cleanup is on: `None` if that failed.
    cleaned: Vec<Option<String>>,
}

/// Tidy one finished stretch of a dictation. Short, clean speech goes
/// through as it is, the same as for a whole dictation.
async fn tidy_piece(text: &str, target: &AppContext, endpoint: &crate::providers::Endpoint, terms: &[Word]) -> Option<String> {
    if text.trim().is_empty() {
        return Some(String::new());
    }
    if target.category != context::Category::Code
        && cleanup::already_clean(text)
        && crate::words::suspects(text, terms).is_empty()
    {
        return Some(text.to_string());
    }
    cleanup::run(text, target, endpoint, &Task::Tidy, terms).await.ok()
}

/// How often the recording under way is checked for speech that can be
/// transcribed already.
const EARLY_EVERY: Duration = Duration::from_millis(1500);

/// While the user is still talking, transcribe what they have finished
/// saying, a sentence or two at a time, cut at clear pauses. When they let go
/// only the last stretch is left, so a long dictation is ready about as fast
/// as a short one.
fn transcribe_while_speaking(app: &AppHandle, id: u64) {
    let app = app.clone();
    *app.state::<Pipeline>().early.lock().unwrap() = Early { id, ..Early::default() };
    tauri::async_runtime::spawn(async move {
        let settings = settings::load(&app);
        let name = settings.model.clone();
        let Ok(path) = models::path(&app, &name) else { return };
        if !models::is_downloaded(&app, &name) {
            return;
        }
        // Each finished stretch is tidied as well, so that only the last one
        // waits for the model when the key comes up. Rewriting needs the
        // whole thing at once, so this is for the dictation hotkey only.
        let mode = *app.state::<Pipeline>().mode.lock().unwrap();
        let endpoint = if mode == Mode::Tidy && settings.cleanup_enabled {
            let lookup = settings.clone();
            tauri::async_runtime::spawn_blocking(move || providers::resolve(&lookup)).await.ok().and_then(Result::ok)
        } else {
            None
        };
        // In a browser, the site decides the style; find it once, now.
        if endpoint.is_some() && permissions::accessibility() {
            let pressed_in = app.state::<Pipeline>().target.lock().unwrap().clone();
            if context::is_browser(&pressed_in.bundle_id) {
                let page = tauri::async_runtime::spawn_blocking(selection::focused_page).await.unwrap_or_default();
                if let Some((site, category)) = context::site(page.url.as_deref(), page.title.as_deref()) {
                    let pipeline = app.state::<Pipeline>();
                    let mut target = pipeline.target.lock().unwrap();
                    target.name = site.to_string();
                    target.category = category;
                }
            }
        }
        loop {
            tokio::time::sleep(EARLY_EVERY).await;
            let pipeline = app.state::<Pipeline>();
            let recording = *pipeline.phase.lock().unwrap() == Phase::Recording
                && pipeline.recording_id.load(Ordering::SeqCst) == id;
            if !recording {
                break;
            }
            let offset = pipeline.early.lock().unwrap().offset;
            let recorder = pipeline.recorder.clone();
            let Ok((chunk, next)) = tauri::async_runtime::spawn_blocking(move || recorder.peek(offset)).await else {
                break;
            };
            let piece = {
                let mut early = pipeline.early.lock().unwrap();
                if early.id != id {
                    break;
                }
                early.audio.extend(chunk);
                early.offset = next;
                let done = early.done;
                crate::transcribe::settled_cut(&early.audio[done..]).map(|cut| (done, early.audio[done..done + cut].to_vec()))
            };
            let Some((done, piece)) = piece else { continue };
            let length = piece.len();
            let _busy = pipeline.early_busy.lock().await;
            if pipeline.early.lock().unwrap().id != id {
                break;
            }
            let screen = screen_terms(&app, id);
            let piece_terms = speech_terms(&settings, &screen);
            let (transcriber, name, path, language) =
                (pipeline.transcriber.clone(), name.clone(), path.clone(), settings.language.clone());
            let text = tauri::async_runtime::spawn_blocking(move || {
                transcriber.transcribe_with(&name, &path, &piece, &models::language(&name, &language), &piece_terms)
            })
            .await;
            // A piece that fails is simply left for the end.
            let Ok(Ok(text)) = text else { continue };
            let cleaned = match &endpoint {
                Some(endpoint) => {
                    let target = pipeline.target.lock().unwrap().clone();
                    let mut terms = settings.dictionary.clone();
                    terms.extend(crate::screen::as_words(&screen));
                    Some(tidy_piece(&text, &target, endpoint, &terms).await)
                }
                None => None,
            };
            let mut early = pipeline.early.lock().unwrap();
            if early.id == id && early.done == done {
                early.texts.push(text);
                if let Some(cleaned) = cleaned {
                    early.cleaned.push(cleaned);
                }
                early.done = done + length;
                #[cfg(debug_assertions)]
                eprintln!("[pipeline] transcribed {:.1} s while the user kept talking", length as f32 / TARGET_RATE as f32);
            }
        }
    });
}

/// How the finished text reaches the user.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Delivery {
    /// Paste into the focused app (falling back to the clipboard if blocked).
    Paste,
    /// Leave it on the clipboard and press nothing.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    ClipboardOnly,
    /// A hands-free recording that ended by itself: copy, and press nothing.
    StoppedItself,
    /// The selection an edit was for is no longer there: copy, do not paste.
    SelectionGone,
}

/// Process a finished recording in the background and report the outcome.
fn finish(app: AppHandle, samples: Vec<f32>, delivery: Delivery) {
    tauri::async_runtime::spawn(async move {
        let outcome = process(&app, samples, delivery).await;
        let pipeline = app.state::<Pipeline>();
        #[cfg(debug_assertions)]
        eprintln!(
            "[pipeline] {}",
            match &outcome {
                Ok(Some(d)) => format!("done {}", serde_json::to_string(d).unwrap_or_default()),
                Ok(None) => "idle (nothing to do)".to_string(),
                Err(e) => format!("error {e}"),
            }
        );
        // Report the outcome before accepting a new press, so that press's
        // "recording" event can never be overtaken by this "done".
        match outcome {
            Ok(Some(dictation)) => {
                *pipeline.last.lock().unwrap() = Some(dictation.clone());
                // A simulated dictation is a test, not something the user said.
                if delivery != Delivery::ClipboardOnly && settings::load(&app).keep_history {
                    let mut entries = pipeline.history.lock().unwrap();
                    history::push(&mut entries, dictation.clone());
                    if let Err(e) = history::save(&app, &entries) {
                        eprintln!("[utter] could not save history: {e}");
                    }
                }
                emit(&app, "done", None, Some(dictation));
            }
            // "Hold the keys while you speak" is the wrong advice for
            // someone who tapped, so hands-free gets the short version.
            Ok(None) => {
                let note = settings::load(&app).hands_free.then(|| "Nothing heard.".to_string());
                emit(&app, "idle", note, None)
            }
            Err(e) => emit(&app, "error", Some(e), None),
        }
        *pipeline.phase.lock().unwrap() = Phase::Idle;
    });
}

/// Everything that happens after the key is released. `Ok(None)` means there
/// was nothing worth doing (an accidental tap).
async fn process(
    app: &AppHandle,
    samples: Vec<f32>,
    delivery: Delivery,
) -> Result<Option<Dictation>, String> {
    // This runs as soon as the key comes up, so it times the user's wait.
    let released = Instant::now();
    let spoken_ms = samples.len() as u64 * 1000 / TARGET_RATE as u64;
    if samples.len() < MIN_SAMPLES {
        return Ok(None);
    }
    if !audio::has_speech(&samples) {
        return Err("Didn't catch any speech. Check which microphone macOS is using.".into());
    }

    let mut settings = settings::load(app);
    let name = settings.model.clone();
    let language = settings.language.clone();
    let path = models::path(app, &name)?;
    let transcriber = app.state::<Pipeline>().transcriber.clone();
    // Names on screen help with this dictation only: they join the user's
    // words here, in memory, and are never saved with them.
    let recording = app.state::<Pipeline>().recording_id.load(Ordering::SeqCst);
    let screen = screen_terms(app, recording);
    let terms = speech_terms(&settings, &screen);
    settings.dictionary.extend(crate::screen::as_words(&screen));

    // In a browser, which site is open decides the writing style: Gmail is
    // email, claude.ai is a prompt. Ask now, while the speech is transcribed.
    let pressed_in = app.state::<Pipeline>().target.lock().unwrap().clone();
    let page_lookup = (delivery != Delivery::ClipboardOnly
        && context::is_browser(&pressed_in.bundle_id)
        && permissions::accessibility())
    .then(|| tauri::async_runtime::spawn_blocking(selection::focused_page));

    let started = Instant::now();
    // Whatever was transcribed while the user was still talking is used as
    // it is; only the rest is transcribed now. Wait for a piece in progress.
    let early = {
        let pipeline = app.state::<Pipeline>();
        let _busy = pipeline.early_busy.lock().await;
        let id = pipeline.recording_id.load(Ordering::SeqCst);
        let mut early = pipeline.early.lock().unwrap();
        let replayed = cfg!(debug_assertions) && std::env::var_os("UTTER_SIMULATE_LIVE").is_some();
        let mine = early.id == id && (delivery != Delivery::ClipboardOnly || replayed);
        let taken = std::mem::take(&mut *early);
        if mine { taken } else { Early::default() }
    };
    let rest_from = early.done.min(samples.len());
    let early_texts = early.texts.clone();
    let early_cleaned = early.cleaned;
    let rest = tauri::async_runtime::spawn_blocking(move || {
        transcriber.transcribe_with(&name, &path, &samples[rest_from..], &models::language(&name, &language), &terms)
    })
    .await
    .map_err(|e| e.to_string())??;
    let rest_raw = rest.clone();
    let raw = if early.texts.is_empty() {
        rest
    } else {
        let joined = format!("{} {}", early.texts.join(" "), rest);
        crate::transcribe::collapse_repeats(joined.trim())
    };
    let transcribe_ms = started.elapsed().as_millis() as u64;

    if raw.is_empty() {
        return Err("No speech detected.".into());
    }

    if let Some(lookup) = page_lookup {
        let page = lookup.await.unwrap_or_default();
        let found = context::site(page.url.as_deref(), page.title.as_deref());
        // Only if the browser is still in front: the page read must be the
        // one the dictation is for, not a window the user has moved to.
        let in_front = on_main(app, context::frontmost).map(|now| now.bundle_id).unwrap_or_default();
        if let Some((site, category)) = found.filter(|_| in_front == pressed_in.bundle_id) {
            let pipeline = app.state::<Pipeline>();
            let mut target = pipeline.target.lock().unwrap();
            target.name = site.to_string();
            target.category = category;
        }
    }

    let target = app.state::<Pipeline>().target.lock().unwrap().clone();
    let mode = *app.state::<Pipeline>().mode.lock().unwrap();
    let finished_at = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_millis() as u64)
            .unwrap_or(0)
    };

    // With text selected, the rewrite hotkey edits that text as instructed.
    if mode == Mode::Rewrite {
        if let Some((selected, found)) = selected_text(app, &target, delivery).await {
            let began = Instant::now();
            let edited = edit_selection(app, &raw, &selected, &target, &settings).await?;
            let edited = keep_edges(&selected, &edited);
            let cleanup_ms = Some(began.elapsed().as_millis() as u64);
            // The model took a while. Paste only if the selection is still
            // where it was; otherwise the result would land somewhere else.
            let delivery = if delivery == Delivery::Paste && !still_selected(app, &target, &selected, found).await {
                Delivery::SelectionGone
            } else {
                delivery
            };
            // The result replaces the selection exactly; no space is added.
            let (inserted, note) = deliver(app, &edited, delivery, false).await?;
            return Ok(Some(Dictation {
                text: edited,
                mode,
                selection: Some(selected),
                cleaned: true,
                inserted,
                note,
                cleanup_ms,
                raw,
                app: target,
                transcribe_ms,
                spoken_ms,
                total_ms: released.elapsed().as_millis() as u64,
                finished_at: finished_at(),
            }));
        }
    }

    // Pieces tidied while the user talked are used as they are, one
    // paragraph each, and only the rest is tidied now. If any piece could not
    // be tidied, the whole dictation is tidied in one go instead.
    let all_tidied = mode == Mode::Tidy
        && !early_texts.is_empty()
        && early_cleaned.len() == early_texts.len()
        && early_cleaned.iter().all(Option::is_some);
    let mut tidied = if all_tidied {
        let mut parts: Vec<String> = early_cleaned.into_iter().flatten().collect();
        let mut rest = Tidied { text: String::new(), cleaned: false, note: None, ms: None };
        if !rest_raw.trim().is_empty() {
            rest = improve(app, &rest_raw, &target, &settings, mode).await;
            parts.push(rest.text);
        }
        Tidied {
            text: cleanup::join_pieces(&parts),
            cleaned: true,
            note: rest.note,
            ms: Some(rest.ms.unwrap_or(0)),
        }
    } else {
        improve(app, &raw, &target, &settings, mode).await
    };
    // However it was produced, a long dictation never goes in as one block,
    // except where line breaks have meaning of their own: code and terminals.
    if target.category != context::Category::Code {
        tidied.text = cleanup::paragraphs(&tidied.text);
    }
    let (inserted, delivery_note) = deliver(app, &tidied.text, delivery, true).await?;
    if delivery != Delivery::ClipboardOnly && !screen.is_empty() {
        suggest_from_screen(app, &screen, &raw, &tidied.text, &target.name);
    }
    // Watch for the user correcting a word of it, to learn from.
    if inserted && delivery == Delivery::Paste {
        let id = app.state::<Pipeline>().recording_id.load(Ordering::SeqCst);
        let watcher = app.clone();
        crate::learn::watch(app.clone(), tidied.text.clone(), move || {
            watcher.state::<Pipeline>().recording_id.load(Ordering::SeqCst) == id
        });
    }

    Ok(Some(Dictation {
        text: tidied.text,
        mode,
        selection: None,
        cleaned: tidied.cleaned,
        inserted,
        // Say both when both happened; the delivery problem first, since
        // it decides what the user has to do next.
        note: match (delivery_note, tidied.note) {
            (Some(delivery), Some(improve)) => Some(format!("{delivery} {improve}")),
            (delivery, improve) => delivery.or(improve),
        },
        cleanup_ms: tidied.ms,
        raw,
        app: target,
        transcribe_ms,
        spoken_ms,
        total_ms: released.elapsed().as_millis() as u64,
        finished_at: finished_at(),
    }))
}

/// Note which names from the screen the user said, and tell the window about
/// any that are now worth suggesting for Your words.
fn suggest_from_screen(app: &AppHandle, screen: &[String], raw: &str, written: &str, app_name: &str) {
    let mut saved = settings::load(app);
    let fresh = crate::screen::note_used(&mut saved.suggested, screen, raw, written, app_name);
    saved.suggested = crate::screen::clean_suggestions(&saved.suggested, &saved.dictionary, &saved.forgotten);
    if let Err(e) = settings::save(app, &saved) {
        eprintln!("[utter] could not save a suggestion: {e}");
        return;
    }
    if !fresh.is_empty() {
        let _ = app.emit("suggested", fresh);
    }
}

/// The text selected in the focused app, if any.
///
/// The app is asked through the Accessibility API first. An app that cannot
/// answer that way is asked by copying, except code editors and terminals:
/// there, copying with nothing selected copies the whole line, which would
/// then be mistaken for a selection and overwritten.
async fn selected_text(app: &AppHandle, target: &AppContext, delivery: Delivery) -> Option<(String, Found)> {
    if delivery == Delivery::ClipboardOnly {
        // A simulated dictation never reads or presses anything real.
        let given = app.state::<Pipeline>().simulated_selection.lock().unwrap().take();
        return given.map(|text| (text, Found::ByCopying));
    }
    if delivery == Delivery::StoppedItself {
        // Nobody is at the keys, so nothing is replaced and nothing is pressed.
        return None;
    }
    if !permissions::accessibility() || permissions::secure_input_active() {
        return None;
    }
    let found = tauri::async_runtime::spawn_blocking(selection::read)
        .await
        .unwrap_or(Selection::Unknown);
    match found {
        Selection::Text(text) => Some((text, Found::ByAsking)),
        Selection::Nothing => None,
        Selection::Unknown if target.category == context::Category::Code => None,
        // In Finder a copy is the name of a file, not text being written.
        Selection::Unknown if target.bundle_id == "com.apple.finder" => None,
        Selection::Unknown => {
            let emptied = on_main(app, insert::begin_copy_probe).ok()?.ok()?;
            // Most apps answer within a few hundredths of a second; a busy
            // one gets half a second before it is taken to have nothing selected.
            for _ in 0..12 {
                tokio::time::sleep(Duration::from_millis(40)).await;
                if on_main(app, move || insert::copy_probe_landed(emptied)).unwrap_or(false) {
                    // Let the app finish writing every form of what it copied.
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    break;
                }
            }
            let copied = on_main(app, move || insert::end_copy_probe(emptied)).ok()?;
            copied.map(|text| (text, Found::ByCopying))
        }
    }
}

/// How the selection was read, which decides how it can be checked again.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Found {
    /// The app told us through the Accessibility API.
    ByAsking,
    /// The app was made to copy it.
    ByCopying,
}

/// Is `selected` still the selection in the app the dictation began in? An
/// app that cannot be asked gets the benefit of the doubt, so long as it is
/// still the one in front.
async fn still_selected(app: &AppHandle, target: &AppContext, selected: &str, found: Found) -> bool {
    let in_front = on_main(app, context::frontmost).map(|now| now.bundle_id).unwrap_or_default();
    if in_front != target.bundle_id {
        return false;
    }
    if found == Found::ByCopying {
        return true;
    }
    match tauri::async_runtime::spawn_blocking(selection::read).await {
        Ok(Selection::Text(now)) => now == selected,
        Ok(Selection::Nothing) => false,
        Ok(Selection::Unknown) | Err(_) => true,
    }
}

/// An edit's reply comes back trimmed. Give it the white space the selection
/// began and ended with, so a selected paragraph keeps its line break and an
/// indented line its indent.
fn keep_edges(selected: &str, edited: &str) -> String {
    let body = selected.trim();
    let lead = &selected[..selected.len() - selected.trim_start().len()];
    let trail = &selected[selected.trim_end().len()..];
    if body.is_empty() {
        return edited.to_string();
    }
    format!("{lead}{}{trail}", edited.trim())
}

/// Have the language model change `selected` as the spoken `instruction`
/// asks. Any failure is an error rather than a fallback: the selection is
/// the user's own text, and must be left exactly as it was unless the edit
/// really worked.
async fn edit_selection(
    app: &AppHandle,
    instruction: &str,
    selected: &str,
    target: &AppContext,
    settings: &Settings,
) -> Result<String, String> {
    const UNCHANGED: &str = "Your selected text was not changed.";
    let length = selected.chars().count();
    if length > MAX_SELECTION_CHARS {
        return Err(format!("That is too much text to edit by voice. Select less. {UNCHANGED}"));
    }
    let for_lookup = settings.clone();
    let endpoint = tauri::async_runtime::spawn_blocking(move || providers::resolve(&for_lookup))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|not_ready| match not_ready {
            NotReady::NoKey => {
                format!("Editing selected text needs a model. Set one up under Cleanup. {UNCHANGED}")
            }
            NotReady::Problem(text) => format!("{text} {UNCHANGED}"),
        })?;

    if endpoint.local && length > MAX_SELECTION_CHARS_LOCAL {
        return Err(format!(
            "That is too much text for a model running on this Mac. Select less. {UNCHANGED}"
        ));
    }

    emit_stage(app, "editing");
    let task = Task::Edit { selection: selected.to_string() };
    cleanup::run(instruction, target, &endpoint, &task, &settings.dictionary)
        .await
        .map_err(|e| format!("{e} {UNCHANGED}"))
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
async fn improve(
    app: &AppHandle,
    raw: &str,
    target: &AppContext,
    settings: &Settings,
    mode: Mode,
) -> Tidied {
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
        // Nothing for a model to do: the speech model's text goes straight
        // in, a second or more sooner. Code editors still go to the model,
        // which spells identifiers and commands exactly, and so does anything
        // that may have one of the user's words misheard in it.
        Mode::Tidy
            if target.category != context::Category::Code
                && cleanup::already_clean(raw)
                && crate::words::suspects(raw, &settings.dictionary).is_empty() =>
        {
            return untouched(None, None);
        }
        Mode::Tidy => (Task::Tidy, "Cleanup"),
        Mode::Rewrite => (
            cleanup::rewrite_task(&settings.rewrite_style, &settings.rewrite_prompt),
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

    // Only now is a model really about to be asked, so only now say so. With
    // no key set up, the pill must not claim to be tidying.
    emit_stage(app, if mode == Mode::Rewrite { "rewriting" } else { "tidying" });
    let started = Instant::now();
    let result = cleanup::run(raw, target, &endpoint, &task, &settings.dictionary).await;
    let ms = Some(started.elapsed().as_millis() as u64);
    if endpoint.local {
        // Keep the model loaded for the next dictation (see `warm_up`).
        let endpoint = endpoint.clone();
        tauri::async_runtime::spawn(async move { cleanup::warm_up(&endpoint).await });
    }
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
async fn deliver(
    app: &AppHandle,
    text: &str,
    delivery: Delivery,
    space_after: bool,
) -> Result<(bool, Option<String>), String> {
    let blocked = if delivery == Delivery::ClipboardOnly {
        Some("simulated dictation")
    } else if delivery == Delivery::StoppedItself {
        Some("hands-free stopped by itself")
    } else if delivery == Delivery::SelectionGone {
        Some("the selection changed while Utter worked")
    } else if !permissions::accessibility() {
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
    let spaced = if !space_after || text.ends_with(char::is_whitespace) {
        text.to_string()
    } else {
        format!("{text} ")
    };
    let token = on_main(app, move || insert::stage(&spaced))?;
    // Give the system a moment to publish the new clipboard before pasting.
    tokio::time::sleep(Duration::from_millis(60)).await;
    if let Err(e) = on_main(app, insert::press_paste)? {
        // The keys could not be pressed. Leave the text on the clipboard as
        // ordinary text so the user can paste it themselves.
        let plain = text.to_string();
        on_main(app, move || {
            insert::abandon_staged();
            insert::set_clipboard_text(&plain, false)
        })?;
        return Ok((false, Some(format!("Copied to the clipboard. Press ⌘V to paste. ({e})"))));
    }

    // Put the user's clipboard back once the target app has had time to read
    // ours. This runs on its own so the pipeline is free for the next press,
    // and it only undoes this paste, never a later one.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(800)).await;
        let _ = on_main(&app, move || insert::restore_staged_if(token));
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

/// Development aid, absent from release builds: push a recording from disk
/// through the whole pipeline as if it had just been dictated with the given
/// hotkey. The result is left on the clipboard and no key is pressed, so a
/// test run can never type into whatever app happens to be in front.
///
/// Returns false if a dictation is already in progress.
#[cfg(debug_assertions)]
pub fn simulate(app: &AppHandle, samples: Vec<f32>, mode: Mode, selected: Option<String>) -> bool {
    let pipeline = app.state::<Pipeline>();
    *pipeline.simulated_selection.lock().unwrap() = selected;
    {
        let mut phase = pipeline.phase.lock().unwrap();
        if *phase != Phase::Idle {
            return false;
        }
        *phase = Phase::Processing;
    }
    *pipeline.mode.lock().unwrap() = mode;
    let mut target = context::frontmost();
    // `UTTER_SIMULATE_SITE=https://mail.google.com/` stands in for a browser
    // showing that page.
    if let Some((site, category)) = std::env::var("UTTER_SIMULATE_SITE")
        .ok()
        .and_then(|url| context::site(Some(&url), None))
    {
        target.name = site.to_string();
        target.category = category;
    }
    *pipeline.target.lock().unwrap() = target;
    emit(app, "recording", None, None);
    warm_local_model(app, mode);
    // `UTTER_SIMULATE_LIVE=1`: replay the recording as if it were arriving
    // live, transcribing early pieces the way a real dictation would, so the
    // stitched result can be compared with transcribing it whole.
    if std::env::var_os("UTTER_SIMULATE_LIVE").is_some() {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let id = app.state::<Pipeline>().recording_id.fetch_add(1, Ordering::SeqCst) + 1;
            let for_replay = samples.clone();
            let replayer = app.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || replay_early(&replayer, id, &for_replay)).await;
            emit_stage(&app, "transcribing");
            finish(app.clone(), samples, Delivery::ClipboardOnly);
        });
        return true;
    }
    emit_stage(app, "transcribing");
    finish(app.clone(), samples, Delivery::ClipboardOnly);
    true
}

/// The early transcription of `transcribe_while_speaking`, run over a whole
/// recording as though it arrived 1.5 seconds at a time.
#[cfg(debug_assertions)]
fn replay_early(app: &AppHandle, id: u64, samples: &[f32]) {
    let pipeline = app.state::<Pipeline>();
    let settings = settings::load(app);
    let Ok(path) = models::path(app, &settings.model) else { return };
    let mut early = Early { id, ..Early::default() };
    let step = EARLY_EVERY.as_millis() as usize * TARGET_RATE as usize / 1000;
    let mut end = 0;
    while end < samples.len() {
        end = (end + step).min(samples.len());
        early.audio = samples[..end].to_vec();
        if let Some(cut) = crate::transcribe::settled_cut(&early.audio[early.done..]) {
            let piece = &early.audio[early.done..early.done + cut];
            if let Ok(text) = pipeline.transcriber.transcribe_with(
                &settings.model,
                &path,
                piece,
                &models::language(&settings.model, &settings.language),
                &crate::words::spellings(&settings.dictionary),
            ) {
                eprintln!("[pipeline] early piece {:.1} s: {text}", cut as f32 / TARGET_RATE as f32);
                // Tidied as a live dictation would be, on a runtime of its own.
                if settings.cleanup_enabled && *pipeline.mode.lock().unwrap() == Mode::Tidy {
                    if let Ok(endpoint) = providers::resolve(&settings) {
                        let target = pipeline.target.lock().unwrap().clone();
                        let cleaned = tauri::async_runtime::block_on(tidy_piece(&text, &target, &endpoint, &settings.dictionary));
                        early.cleaned.push(cleaned);
                    }
                }
                early.texts.push(text);
                early.done += cut;
            }
        }
    }
    *pipeline.early.lock().unwrap() = early;
}

/// True while a dictation is being recorded or processed.
#[cfg(debug_assertions)]
pub fn busy(app: &AppHandle) -> bool {
    *app.state::<Pipeline>().phase.lock().unwrap() != Phase::Idle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_edit_keeps_the_white_space_around_the_selection() {
        // A paragraph selected with its line break keeps it.
        assert_eq!(keep_edges("A long paragraph.\n", "Shorter."), "Shorter.\n");
        // An indented line keeps its indent.
        assert_eq!(keep_edges("    let total = a + b;", "let sum = a + b;"), "    let sum = a + b;");
        assert_eq!(keep_edges("plain", " changed \n"), "changed");
    }
}
