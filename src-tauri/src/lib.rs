//! Utter: hold a hotkey, speak, release, and cleaned-up text is typed into the
//! app you are using. This file wires the pieces together; each piece lives in
//! its own module.

pub mod audio;
mod cleanup;
mod context;
mod fnkey;
mod globe;
mod history;
mod hotkey;
mod insert;
mod models;
mod permissions;
mod pipeline;
mod providers;
mod secrets;
mod selection;
mod settings;
pub mod transcribe;
mod words;

use settings::Settings;
use tauri::{
    AppHandle, Emitter, LogicalPosition, Manager, RunEvent, WindowEvent,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

/// Size of the status pill window, matching `tauri.conf.json`.
const PILL_SIZE: (f64, f64) = (400.0, 112.0);
/// Gap between the pill's window and the bottom of the screen. The pill sits
/// in the middle of its window, which puts it clear of the Dock.
const PILL_BOTTOM_MARGIN: f64 = 76.0;

#[tauri::command]
fn get_settings(app: AppHandle) -> Settings {
    settings::load(&app)
}

#[tauri::command]
fn save_settings(app: AppHandle, mut settings: Settings) -> Result<Settings, String> {
    let previous = settings::load(&app);
    settings.dictionary = words::clean(&settings.dictionary);
    settings.forgotten = words::clean_forgotten(&settings.forgotten, &settings.dictionary);

    // Check everything that can be checked before changing anything.
    hotkey::plan(&settings)?;
    if settings.provider == "custom" && !settings.base_url.trim().is_empty() {
        providers::check_base_url(&settings.base_url)?;
    }

    let hotkeys_changed =
        settings.hotkey != previous.hotkey || settings.rewrite_hotkey != previous.rewrite_hotkey;
    if hotkeys_changed {
        // If the system refuses a hotkey, keep the old ones working and
        // leave the saved settings untouched.
        if let Err(e) = hotkey::register(&app, &settings) {
            hotkey::restore(&app, &previous);
            return Err(e);
        }
        // Both registered: whatever was wrong before no longer is.
        hotkey::clear_problem();
    }
    if let Err(e) = settings::save(&app, &settings) {
        // What is live must match what is on disk.
        if hotkeys_changed {
            hotkey::restore(&app, &previous);
        }
        return Err(e);
    }

    // A key saved for one custom service must never be sent to another. If
    // the custom base URL now points at a different host, forget the key.
    let old_host = providers::host_of(&previous.base_url);
    if !old_host.is_empty() && old_host != providers::host_of(&settings.base_url) {
        let _ = secrets::delete("custom");
    }
    if settings.model != previous.model {
        pipeline::preload_model(&app);
    }
    // Switching the globe-key option takes effect now, not at the next
    // launch. (A hotkey change already did this while registering.)
    if settings.quiet_globe != previous.quiet_globe && !hotkeys_changed {
        globe::sync(&app, settings.hotkey.trim() == fnkey::FN && settings.quiet_globe);
    }
    Ok(settings)
}

/// The cleanup providers offered in settings.
#[tauri::command]
fn providers() -> Vec<providers::Provider> {
    providers::CATALOG.to_vec()
}

fn known_provider(id: &str) -> Result<(), String> {
    providers::find(id)
        .map(|_| ())
        .ok_or_else(|| format!("Unknown provider: {id}"))
}

/// Run Keychain work off the main thread: macOS may show a prompt, and the
/// app must not freeze behind it.
async fn off_main<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())
}

/// The UI only ever learns whether a key exists, never the key itself.
#[tauri::command]
async fn has_api_key(provider: String) -> bool {
    off_main(move || secrets::exists(&provider)).await.unwrap_or(false)
}

#[tauri::command]
async fn set_api_key(provider: String, key: String) -> Result<(), String> {
    known_provider(&provider)?;
    off_main(move || secrets::set(&provider, &key)).await?
}

#[tauri::command]
async fn delete_api_key(provider: String) -> Result<(), String> {
    known_provider(&provider)?;
    off_main(move || secrets::delete(&provider)).await?
}

/// The endpoint for the current settings, or a message saying what is missing.
async fn current_endpoint(app: &AppHandle) -> Result<providers::Endpoint, String> {
    let settings = settings::load(app);
    tauri::async_runtime::spawn_blocking(move || providers::resolve(&settings))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|not_ready| match not_ready {
            providers::NotReady::NoKey => "Add an API key first.".to_string(),
            providers::NotReady::Problem(text) => text,
        })
}

/// Send one fixed sentence through cleanup to confirm the setup works.
#[tauri::command]
async fn test_cleanup(app: AppHandle) -> Result<String, String> {
    let endpoint = current_endpoint(&app).await?;
    let sample = "um so this is like a quick test of the the cleanup step";
    cleanup::run(sample, &context::AppContext::unknown(), &endpoint, &cleanup::Task::Tidy, &[]).await
}

/// The ready-made rewrite styles offered in settings.
#[tauri::command]
fn rewrite_styles() -> Vec<cleanup::RewriteStyle> {
    cleanup::REWRITE_STYLES.to_vec()
}

/// Rewrite one fixed, rambling sentence with the current style, so the user
/// can see what a style does before relying on it.
#[tauri::command]
async fn test_rewrite(app: AppHandle) -> Result<String, String> {
    let endpoint = current_endpoint(&app).await?;
    let settings = settings::load(&app);
    let task = cleanup::rewrite_task(&settings.rewrite_style, &settings.rewrite_prompt);
    let sample = "um so basically I think we should like push the meeting to friday because uh \
                  half the team is out on thursday and yeah I just think it makes more sense that way";
    cleanup::run(sample, &context::AppContext::unknown(), &endpoint, &task, &[]).await
}

/// Models the chosen provider offers for the saved key.
#[tauri::command]
async fn list_cleanup_models(app: AppHandle) -> Result<Vec<String>, String> {
    // Listing needs a provider and key but not a chosen model yet.
    let mut settings = settings::load(&app);
    if settings.cleanup_model.trim().is_empty() {
        settings.cleanup_model = "-".into();
    }
    let endpoint = tauri::async_runtime::spawn_blocking(move || providers::resolve(&settings))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|not_ready| match not_ready {
            providers::NotReady::NoKey => "Add an API key first.".to_string(),
            providers::NotReady::Problem(text) => text,
        })?;
    cleanup::list_models(&endpoint).await
}

#[tauri::command]
fn permission_status(app: AppHandle) -> permissions::PermissionStatus {
    let status = permissions::status();
    if status.microphone == permissions::MicStatus::Granted {
        prepare_microphone_once(&app);
    }
    status
}

/// Open the microphone stream ahead of the first dictation, once. Only ever
/// called after the user has allowed the microphone, or macOS would ask for
/// it out of the blue.
fn prepare_microphone_once(app: &AppHandle) {
    static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
        app.state::<pipeline::Pipeline>().prepare_microphone();
    }
}

/// Ask macOS to show its microphone permission prompt.
#[tauri::command]
fn request_microphone() {
    permissions::request_microphone();
}

/// Show the system dialog that leads to the Accessibility settings pane.
#[tauri::command]
fn request_accessibility() -> bool {
    permissions::prompt_accessibility()
}

/// Forget any Accessibility decision macOS holds for Utter, then ask again.
/// This is the way out when the switch is on in System Settings but macOS is
/// matching it against an older build of the app.
#[tauri::command]
fn reset_accessibility(app: AppHandle) -> Result<(), String> {
    permissions::reset_accessibility(&app.config().identifier)?;
    permissions::prompt_accessibility();
    Ok(())
}

/// Stop listening for the hotkeys while the user records a new one.
#[tauri::command]
fn pause_hotkeys(app: AppHandle) -> Result<(), String> {
    hotkey::pause(&app)
}

/// Listen for the saved hotkeys again once recording is over.
#[tauri::command]
fn resume_hotkeys(app: AppHandle) -> Option<String> {
    hotkey::restore(&app, &settings::load(&app));
    hotkey::problem()
}

/// Why a hotkey is not working, if one is not.
#[tauri::command]
fn hotkey_problem() -> Option<String> {
    hotkey::problem()
}

#[tauri::command]
fn open_privacy_settings(pane: String) -> Result<(), String> {
    permissions::open_settings(&pane)
}

/// What macOS also does when fn is pressed, if anything: worth knowing when
/// fn is the dictation hotkey.
#[tauri::command]
async fn fn_key_conflict() -> Option<String> {
    // Not a conflict while Utter itself is keeping it off.
    if globe::holding() {
        return None;
    }
    tauri::async_runtime::spawn_blocking(|| fnkey::system_action().map(str::to_string))
        .await
        .ok()
        .flatten()
}

#[tauri::command]
fn model_statuses(app: AppHandle) -> Vec<models::ModelStatus> {
    models::statuses(&app)
}

/// Download a speech model; progress arrives on the `model-download` event.
#[tauri::command]
async fn download_model(app: AppHandle, name: String) -> Result<(), String> {
    models::download(&app, &name).await?;
    pipeline::preload_model(&app);
    Ok(())
}

#[tauri::command]
fn last_dictation(app: AppHandle) -> Option<pipeline::Dictation> {
    app.state::<pipeline::Pipeline>().last()
}

/// Past dictations kept on this Mac, newest first.
#[tauri::command]
fn history(app: AppHandle) -> Vec<pipeline::Dictation> {
    app.state::<pipeline::Pipeline>().history()
}

#[tauri::command]
fn clear_history(app: AppHandle) -> Result<(), String> {
    pipeline::clear_history(&app)
}

/// Forget one past dictation, named by when it finished.
#[tauri::command]
fn forget_dictation(app: AppHandle, finished_at: u64) -> Result<(), String> {
    pipeline::forget(&app, finished_at)
}

/// Add a word to the user's list, or teach a word already there what it may
/// be misheard as, and save. Returns the settings as saved.
fn remember_word(
    app: &AppHandle,
    word: &str,
    heard: Option<&str>,
    source: words::Source,
) -> Result<(Settings, words::Remembered), String> {
    let mut settings = settings::load(app);
    let outcome = words::remember(&mut settings.dictionary, word, heard, source);
    if outcome == words::Remembered::Full {
        return Err(format!("Your words are full ({} of them). Remove one to add another.", words::MAX_WORDS));
    }
    settings.dictionary = words::clean(&settings.dictionary);
    settings.forgotten = words::clean_forgotten(&settings.forgotten, &settings.dictionary);
    settings::save(app, &settings)?;
    Ok((settings, outcome))
}

/// Fix a misheard word in a past dictation: `wrong` becomes `right` in what
/// was written and, if asked, Utter remembers the spelling and the mishearing.
#[tauri::command]
fn fix_word(app: AppHandle, finished_at: u64, wrong: String, right: String, remember: bool) -> Result<Settings, String> {
    let right = right.split_whitespace().collect::<Vec<_>>().join(" ");
    if right.is_empty() {
        return Err("Type the right spelling first.".into());
    }
    pipeline::correct(&app, finished_at, &wrong, &right)?;
    if !remember {
        return Ok(settings::load(&app));
    }
    // A change of case alone is a spelling, not a mishearing.
    let heard = (wrong.trim().to_lowercase() != right.to_lowercase()).then_some(wrong.as_str());
    remember_word(&app, &right, heard, words::Source::Added).map(|(settings, _)| settings)
}

/// Put text on the clipboard, for the Copy buttons in the window.
#[tauri::command]
fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    // The pasteboard is only touched from the main thread.
    app.run_on_main_thread(move || {
        insert::set_clipboard_text(&text, false);
    })
    .map_err(|e| e.to_string())
}

/// Development aid, absent from release builds. Set `UTTER_SIMULATE` to one
/// or more comma-separated recordings (raw 32-bit float, 16 kHz, mono) and
/// each is run through the pipeline shortly after launch, as if dictated.
/// A path ending in `@rewrite` uses the rewrite hotkey's mode, and `@edit`
/// uses it with `UTTER_SIMULATE_SELECTION` as the selected text. Each outcome
/// is printed as a `[pipeline]` line and left on the clipboard; nothing is
/// pasted. `[probe]` lines show what each window displayed along the way.
///
///   ffmpeg -i clip.wav -ar 16000 -ac 1 -f f32le clip.f32
///   UTTER_SIMULATE=clip.f32,clip.f32@rewrite pnpm tauri dev
/// Development aid, absent from release builds: ask each window to describe
/// what it is showing right now (its visible text, and the box that text
/// occupies) and print the answers as `[probe]` lines. This lets a test run
/// check the real windows without anyone looking at the screen.
#[cfg(debug_assertions)]
fn probe(app: &AppHandle, moment: &str) {
    const SCRIPT: &str = r#"(() => {
  const opacity = (el) => {
    let total = 1;
    for (let n = el; n && n.nodeType === 1; n = n.parentElement) {
      const s = getComputedStyle(n);
      if (s.display === 'none' || s.visibility === 'hidden') return 0;
      total *= parseFloat(s.opacity);
    }
    return total;
  };
  let left = 1e9, top = 1e9, right = 0, bottom = 0, shown = 0;
  const texts = [];
  for (const el of document.querySelectorAll('#root *')) {
    const box = el.getBoundingClientRect();
    if (!box.width || !box.height || opacity(el) < 0.05) continue;
    shown += 1;
    left = Math.min(left, box.left); top = Math.min(top, box.top);
    right = Math.max(right, box.right); bottom = Math.max(bottom, box.bottom);
    if (el.children.length === 0 && el.textContent.trim()) texts.push(el.textContent.trim());
  }
  const report = {
    moment: '__MOMENT__',
    window: window.__TAURI_INTERNALS__.metadata.currentWindow.label,
    size: [innerWidth, innerHeight],
    shown,
    box: shown ? [left, top, right, bottom].map(Math.round) : null,
    text: texts.join(' | ').slice(0, 400),
    // State that does not depend on a CSS transition having been painted,
    // which never happens while the screen is locked or the window hidden.
    labelled: [...document.querySelectorAll('#root [aria-label]')]
      .map((el) => `${el.className} => ${el.getAttribute('aria-label')}`)
      .slice(0, 6),
    page: document.visibilityState,
    fonts: [...new Set([...document.fonts].filter((f) => f.status === 'loaded').map((f) => f.family))],
  };
  window.__TAURI_INTERNALS__.invoke('plugin:event|emit', { event: 'debug-probe', payload: report });
})();"#;
    for label in ["main", "pill"] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.eval(SCRIPT.replace("__MOMENT__", moment));
        }
    }
}

#[cfg(debug_assertions)]
fn simulate_from_env(app: &AppHandle) {
    let Ok(list) = std::env::var("UTTER_SIMULATE") else {
        return;
    };
    {
        use tauri::Listener;
        app.listen("debug-probe", |event| eprintln!("[probe] {}", event.payload()));
    }
    let app = app.clone();
    std::thread::spawn(move || {
        use std::time::Duration;
        // Give the speech model time to load first.
        std::thread::sleep(Duration::from_secs(4));
        probe(&app, "before any dictation");
        {
            let (reply, code) = std::sync::mpsc::channel();
            let _ = app.run_on_main_thread(move || {
                let _ = reply.send(insert::paste_key_code());
            });
            if let Ok(code) = code.recv() {
                eprintln!("[simulate] paste key code for this keyboard layout: {code}");
            }
        }
        for entry in list.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            // "@edit" is the rewrite hotkey with text selected; the pretend
            // selection comes from UTTER_SIMULATE_SELECTION.
            let (path, mode, selected) = if let Some(path) = entry.strip_suffix("@rewrite") {
                (path, pipeline::Mode::Rewrite, None)
            } else if let Some(path) = entry.strip_suffix("@edit") {
                (path, pipeline::Mode::Rewrite, std::env::var("UTTER_SIMULATE_SELECTION").ok())
            } else {
                (entry, pipeline::Mode::Tidy, None)
            };
            let samples: Vec<f32> = match std::fs::read(path) {
                Ok(bytes) => bytes
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect(),
                Err(e) => {
                    eprintln!("[simulate] {path}: {e}");
                    continue;
                }
            };
            let started = std::time::Instant::now();
            if !pipeline::simulate(&app, samples, mode, selected) {
                eprintln!("[simulate] {entry}: busy, skipped");
                continue;
            }
            let mut polls = 0;
            while pipeline::busy(&app) {
                std::thread::sleep(Duration::from_millis(50));
                polls += 1;
                // Look at the windows shortly after the start, and again
                // once processing has been going for a while.
                if polls == 12 || polls == 80 {
                    probe(&app, &format!("{:.1}s into processing", polls as f32 * 0.05));
                }
            }
            probe(&app, "just finished");
            std::thread::sleep(Duration::from_millis(1500));
            probe(&app, "1.5s after finishing");
            std::thread::sleep(Duration::from_millis(6000));
            probe(&app, "7.5s after finishing");
            // The outcome itself is printed by the pipeline as it finishes.
            eprintln!("[simulate] {entry} took {:.1}s", started.elapsed().as_secs_f32());
        }
        eprintln!("[simulate] finished");
    });
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// The status pill is a small always-on-top window at the bottom centre of
/// the screen. It never takes clicks or focus, so it cannot steal the cursor
/// from the app being dictated into.
fn place_pill(app: &AppHandle) {
    let Some(pill) = app.get_webview_window("pill") else {
        return;
    };
    let _ = pill.set_ignore_cursor_events(true);
    if let Ok(Some(monitor)) = pill.primary_monitor() {
        let scale = monitor.scale_factor();
        let size = monitor.size().to_logical::<f64>(scale);
        let origin = monitor.position().to_logical::<f64>(scale);
        let x = origin.x + (size.width - PILL_SIZE.0) / 2.0;
        let y = origin.y + size.height - PILL_SIZE.1 - PILL_BOTTOM_MARGIN;
        let _ = pill.set_position(LogicalPosition::new(x, y));
    }
}

/// Menu-bar item: reopen settings, recover the last dictation, add a word, quit.
fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Utter", true, None::<&str>)?;
    let copy = MenuItem::with_id(app, "copy", "Copy Last Dictation", true, None::<&str>)?;
    let add_word = MenuItem::with_id(app, "add-word", "Add a Word…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Utter", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open, &copy, &add_word, &PredefinedMenuItem::separator(app)?, &quit],
    )?;
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    TrayIconBuilder::with_id("utter")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Utter")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "copy" => pipeline::copy_last(app),
            // The window opens on the word list, ready to type into.
            "add-word" => {
                show_main_window(app);
                let _ = app.emit("add-word", ());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            // Keep whisper.cpp's own logging out of the terminal.
            whisper_rs::install_logging_hooks();
            app.manage(pipeline::Pipeline::new());
            pipeline::load_history(app.handle());
            pipeline::preload_model(app.handle());
            // If Utter last stopped while holding the globe-key action off,
            // it still owes the user their own setting.
            globe::recover(app.handle());
            // Open the microphone stream now, stopped, so even the first
            // dictation starts quickly. Only once access has been granted:
            // doing this earlier would make macOS ask for it out of the blue.
            if permissions::microphone() == permissions::MicStatus::Granted {
                prepare_microphone_once(app.handle());
            }
            #[cfg(debug_assertions)]
            simulate_from_env(app.handle());
            place_pill(app.handle());
            build_tray(app.handle())?;
            let saved = settings::load(app.handle());
            hotkey::restore(app.handle(), &saved);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            providers,
            has_api_key,
            set_api_key,
            delete_api_key,
            test_cleanup,
            list_cleanup_models,
            rewrite_styles,
            test_rewrite,
            permission_status,
            request_microphone,
            request_accessibility,
            reset_accessibility,
            fn_key_conflict,
            pause_hotkeys,
            resume_hotkeys,
            hotkey_problem,
            open_privacy_settings,
            model_statuses,
            download_model,
            last_dictation,
            history,
            clear_history,
            forget_dictation,
            copy_text,
            fix_word,
        ])
        // Closing the settings window hides it; Utter keeps running in the
        // menu bar so the hotkey stays available.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Utter")
        .run(|app, event| match event {
            // Clicking the Dock icon brings the settings window back.
            RunEvent::Reopen { .. } => show_main_window(app),
            // Give the user's own globe-key setting back on the way out.
            RunEvent::Exit => globe::release(app),
            _ => {}
        });
}
