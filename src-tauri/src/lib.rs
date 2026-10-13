//! Utter: hold a hotkey, speak, release, and cleaned-up text is typed into the
//! app you are using. This file wires the pieces together; each piece lives in
//! its own module.

mod audio;
mod hotkey;
mod pipeline;
mod secrets;
mod settings;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            app.manage(pipeline::Pipeline::new());
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Utter");
}
