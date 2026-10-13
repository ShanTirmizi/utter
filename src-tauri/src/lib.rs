//! Utter: hold a hotkey, speak, release, and cleaned-up text is typed into the
//! app you are using. This file wires the pieces together; each piece lives in
//! its own module.

pub mod audio;
mod cleanup;
mod context;
mod hotkey;
mod insert;
mod models;
mod permissions;
mod pipeline;
mod secrets;
mod settings;
pub mod transcribe;

use settings::Settings;
use tauri::{
    AppHandle, LogicalPosition, Manager, RunEvent, WindowEvent,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

/// Size of the status pill window, matching `tauri.conf.json`.
const PILL_SIZE: (f64, f64) = (360.0, 72.0);
/// Gap between the pill and the bottom of the screen, clear of the Dock.
const PILL_BOTTOM_MARGIN: f64 = 96.0;

#[tauri::command]
fn get_settings(app: AppHandle) -> Settings {
    settings::load(&app)
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: Settings) -> Result<Settings, String> {
    let previous = settings::load(&app);
    if settings.hotkey != previous.hotkey {
        // Try the new hotkey first; if the system refuses it, keep the old one
        // working and leave the saved settings untouched.
        if let Err(e) = hotkey::register(&app, &settings.hotkey) {
            let _ = hotkey::register(&app, &previous.hotkey);
            return Err(e);
        }
    }
    settings::save(&app, &settings)?;
    if settings.model != previous.model {
        pipeline::preload_model(&app);
    }
    Ok(settings)
}

/// The UI only ever learns whether a key exists, never the key itself.
#[tauri::command]
fn has_api_key() -> bool {
    secrets::exists()
}

#[tauri::command]
fn set_api_key(key: String) -> Result<(), String> {
    secrets::set(&key)
}

#[tauri::command]
fn delete_api_key() -> Result<(), String> {
    secrets::delete()
}

/// Send one fixed sentence through cleanup to confirm the stored key works.
#[tauri::command]
async fn test_cleanup() -> Result<String, String> {
    let key = tauri::async_runtime::spawn_blocking(secrets::get)
        .await
        .ok()
        .flatten()
        .ok_or("Add an API key first.")?;
    let sample = "um so this is like a quick test of the the cleanup step";
    cleanup::clean(sample, &context::AppContext::unknown(), &key).await
}

#[tauri::command]
fn permission_status() -> permissions::PermissionStatus {
    permissions::status()
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

#[tauri::command]
fn open_privacy_settings(pane: String) -> Result<(), String> {
    permissions::open_settings(&pane)
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

/// Menu-bar item: reopen settings, recover the last dictation, quit.
fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Utter", true, None::<&str>)?;
    let copy = MenuItem::with_id(app, "copy", "Copy Last Dictation", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Utter", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open, &copy, &PredefinedMenuItem::separator(app)?, &quit],
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
            pipeline::preload_model(app.handle());
            place_pill(app.handle());
            build_tray(app.handle())?;
            let saved = settings::load(app.handle());
            if let Err(e) = hotkey::register(app.handle(), &saved.hotkey) {
                eprintln!("[utter] {e}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            has_api_key,
            set_api_key,
            delete_api_key,
            test_cleanup,
            permission_status,
            request_microphone,
            request_accessibility,
            open_privacy_settings,
            model_statuses,
            download_model,
            last_dictation,
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
        .run(|app, event| {
            // Clicking the Dock icon brings the settings window back.
            if let RunEvent::Reopen { .. } = event {
                show_main_window(app);
            }
        });
}
