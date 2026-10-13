//! Utter: hold a hotkey, speak, release, and cleaned-up text is typed into the
//! app you are using. This file wires the pieces together; each piece lives in
//! its own module.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!())
        .expect("error while running Utter");
}
