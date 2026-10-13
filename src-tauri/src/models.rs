//! Speech models. They are large, so they are downloaded once on request into
//! the app's data folder rather than shipped with the app or the repository.

use futures_util::StreamExt;
use serde::Serialize;
use std::{fs, io::Write, path::PathBuf};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub name: &'static str,
    pub label: &'static str,
    pub note: &'static str,
    pub size_mb: u32,
}

/// Models offered in settings, smallest first.
pub const CATALOG: [ModelInfo; 3] = [
    ModelInfo {
        name: "base.en",
        label: "Base (English)",
        note: "Fastest. Good for clear speech.",
        size_mb: 148,
    },
    ModelInfo {
        name: "small.en",
        label: "Small (English)",
        note: "More accurate, a little slower.",
        size_mb: 488,
    },
    ModelInfo {
        name: "large-v3-turbo-q5_0",
        label: "Large v3 Turbo (multilingual)",
        note: "Most accurate. Any language.",
        size_mb: 574,
    },
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    #[serde(flatten)]
    pub info: ModelInfo,
    pub downloaded: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress<'a> {
    name: &'a str,
    received: u64,
    total: u64,
}

pub fn find(name: &str) -> Option<ModelInfo> {
    CATALOG.iter().copied().find(|m| m.name == name)
}

/// English-only models skip language detection; others detect it per recording.
pub fn language(name: &str) -> &'static str {
    if name.ends_with(".en") { "en" } else { "auto" }
}

fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

pub fn path(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    find(name).ok_or_else(|| format!("Unknown model: {name}"))?;
    Ok(dir(app)?.join(format!("ggml-{name}.bin")))
}

pub fn is_downloaded(app: &AppHandle, name: &str) -> bool {
    path(app, name).map(|p| p.is_file()).unwrap_or(false)
}

pub fn statuses(app: &AppHandle) -> Vec<ModelStatus> {
    CATALOG
        .iter()
        .map(|info| ModelStatus {
            info: *info,
            downloaded: is_downloaded(app, info.name),
        })
        .collect()
}

fn url(name: &str) -> String {
    format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{name}.bin")
}

/// Download a model, reporting progress on the `model-download` event. The
/// file is written under a temporary name and renamed at the end, so an
/// interrupted download never looks like a usable model.
pub async fn download(app: &AppHandle, name: &str) -> Result<(), String> {
    let target = path(app, name)?;
    if target.is_file() {
        return Ok(());
    }
    let partial = target.with_extension("bin.part");

    let response = reqwest::Client::new()
        .get(url(name))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Download failed: {e}"))?;
    let total = response.content_length().unwrap_or(0);

    let mut file = fs::File::create(&partial).map_err(|e| e.to_string())?;
    let mut received: u64 = 0;
    let mut last_reported: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| {
            let _ = fs::remove_file(&partial);
            format!("Download interrupted: {e}")
        })?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        received += chunk.len() as u64;
        // Report roughly every megabyte; more often just floods the UI.
        if received - last_reported >= 1_000_000 {
            last_reported = received;
            let _ = app.emit("model-download", Progress { name, received, total });
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    if total > 0 && received != total {
        let _ = fs::remove_file(&partial);
        return Err("Download ended early. Please try again.".into());
    }
    fs::rename(&partial, &target).map_err(|e| e.to_string())?;
    let _ = app.emit("model-download", Progress { name, received, total: received });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_models_pin_the_language() {
        assert_eq!(language("base.en"), "en");
        assert_eq!(language("large-v3-turbo-q5_0"), "auto");
    }

    #[test]
    fn default_model_is_in_the_catalog() {
        assert!(find(&crate::settings::Settings::default().model).is_some());
        assert!(find("not-a-model").is_none());
    }
}
