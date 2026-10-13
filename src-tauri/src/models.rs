//! Speech models. They are large, so they are downloaded once on request into
//! the app's data folder rather than shipped with the app or the repository.

use futures_util::StreamExt;
use serde::Serialize;
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
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
    // Two downloads of one model would write over each other's partial file.
    let _claim = Claim::take(name).ok_or("That model is already downloading.")?;
    let partial = target.with_extension("bin.part");

    let result = fetch(app, name, &partial).await;
    match result {
        Ok(received) => {
            fs::rename(&partial, &target).map_err(|e| e.to_string())?;
            let _ = app.emit("model-download", Progress { name, received, total: received });
            Ok(())
        }
        Err(e) => {
            // Never leave a half-written file behind, whatever went wrong.
            let _ = fs::remove_file(&partial);
            Err(e)
        }
    }
}

/// Give up if the server sends nothing for this long, rather than waiting
/// forever on a stalled connection.
const STALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Stream the model into `partial`, returning the number of bytes written.
async fn fetch(app: &AppHandle, name: &str, partial: &Path) -> Result<u64, String> {
    let response = reqwest::Client::builder()
        .connect_timeout(STALL_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?
        .get(url(name))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Download failed: {e}"))?;
    let total = response.content_length().unwrap_or(0);

    let mut file = fs::File::create(partial).map_err(|e| e.to_string())?;
    let mut received: u64 = 0;
    let mut last_reported: u64 = 0;
    let mut stream = response.bytes_stream();

    loop {
        let next = tokio::time::timeout(STALL_TIMEOUT, stream.next())
            .await
            .map_err(|_| "The download stalled. Check your connection and try again.".to_string())?;
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(|e| format!("Download interrupted: {e}"))?;
        file.write_all(&chunk)
            .map_err(|e| format!("Could not save the model: {e}"))?;
        received += chunk.len() as u64;
        // Report roughly every megabyte; more often just floods the UI.
        if received - last_reported >= 1_000_000 {
            last_reported = received;
            let _ = app.emit("model-download", Progress { name, received, total });
        }
    }
    file.flush().map_err(|e| e.to_string())?;

    if total > 0 && received != total {
        return Err("Download ended early. Please try again.".into());
    }
    Ok(received)
}

/// Models being downloaded right now.
static IN_PROGRESS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Marks a model as downloading for as long as it is held.
struct Claim(String);

impl Claim {
    fn take(name: &str) -> Option<Self> {
        let mut active = IN_PROGRESS.lock().unwrap();
        active
            .get_or_insert_with(HashSet::new)
            .insert(name.to_string())
            .then(|| Self(name.to_string()))
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        if let Some(active) = IN_PROGRESS.lock().unwrap().as_mut() {
            active.remove(&self.0);
        }
    }
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
    fn a_model_can_only_be_downloading_once_at_a_time() {
        let first = Claim::take("test-model");
        assert!(first.is_some());
        assert!(Claim::take("test-model").is_none());
        assert!(Claim::take("another-model").is_some());
        drop(first);
        assert!(Claim::take("test-model").is_some());
    }

    #[test]
    fn default_model_is_in_the_catalog() {
        assert!(find(&crate::settings::Settings::default().model).is_some());
        assert!(find("not-a-model").is_none());
    }
}
