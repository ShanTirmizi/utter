//! Utter: hold a hotkey, speak, release, and cleaned-up text is typed into the
//! app you are using. This file wires the pieces together; each piece lives in
//! its own module.

mod secrets;
mod settings;

use settings::Settings;
use tauri::AppHandle;

#[tauri::command]
fn get_settings(app: AppHandle) -> Settings {
    settings::load(&app)
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: Settings) -> Result<Settings, String> {
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
