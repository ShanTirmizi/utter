//! Utter: hold a hotkey, speak, release, and cleaned-up text is typed into the
//! app you are using. This file wires the pieces together; each piece lives in
//! its own module.

pub mod audio;
mod cleanup;
mod context;
mod hotkey;
mod models;
mod permissions;
mod pipeline;
mod secrets;
mod settings;
pub mod transcribe;

use settings::Settings;
use tauri::{AppHandle, Manager};

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

/// Briefly open the microphone so macOS shows its permission prompt.
#[tauri::command]
fn request_microphone(app: AppHandle) {
    std::thread::spawn(move || app.state::<pipeline::Pipeline>().probe_microphone());
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
        .run(tauri::generate_context!())
        .expect("error while running Utter");
}
