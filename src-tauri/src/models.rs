//! Speech models. They are large, so they are downloaded once on request into
//! the app's data folder rather than shipped with the app or the repository.

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};

/// What runs a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Engine {
    /// whisper.cpp, on the GPU. The model is one file.
    Whisper,
    /// FluidAudio, on the Neural Engine, in a helper program. The model is a
    /// folder of Core ML bundles. See `parakeet.rs`.
    Parakeet,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub name: &'static str,
    pub label: &'static str,
    pub note: &'static str,
    pub size_mb: u32,
    pub engine: Engine,
}

/// Models offered in settings, smallest first.
pub const CATALOG: [ModelInfo; 4] = [
    ModelInfo {
        name: "base.en",
        label: "Base (English)",
        note: "Fastest, but mishears more. Good for clear speech.",
        size_mb: 148,
        engine: Engine::Whisper,
    },
    ModelInfo {
        name: "small.en",
        label: "Small (English)",
        note: "Recommended. Hears names and jargon far better than Base.",
        size_mb: 488,
        engine: Engine::Whisper,
    },
    ModelInfo {
        name: "large-v3-turbo-q5_0",
        label: "Large v3 Turbo (multilingual)",
        note: "Most accurate, any language. Several times slower.",
        size_mb: 574,
        engine: Engine::Whisper,
    },
    ModelInfo {
        name: "parakeet-tdt-0.6b-v3",
        label: "Parakeet v3 (25 languages)",
        note: "Fast and accurate, on the Neural Engine. European languages. macOS 14 or later.",
        size_mb: 586,
        engine: Engine::Parakeet,
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

/// English-only models skip language detection. The others use the language
/// the user chose (a Whisper code such as "fr"), or detect it per recording
/// when they chose none.
pub fn language(name: &str, chosen: &str) -> String {
    let chosen = chosen.trim();
    if name.ends_with(".en") {
        "en".into()
    } else if chosen.is_empty() || chosen == "auto" {
        "auto".into()
    } else {
        chosen.to_string()
    }
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

/// Where a model is kept: a file for Whisper, a folder for Parakeet.
pub fn path(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let info = find(name).ok_or_else(|| format!("Unknown model: {name}"))?;
    Ok(match info.engine {
        Engine::Whisper => dir(app)?.join(format!("ggml-{name}.bin")),
        Engine::Parakeet => dir(app)?.join(name),
    })
}

/// A model is only ever at its path once completely downloaded: it is
/// assembled under another name and moved there at the end. It does not
/// count as downloaded while it is still being prepared (see `ready`).
pub fn is_downloaded(app: &AppHandle, name: &str) -> bool {
    path(app, name).is_ok_and(|p| p.exists()) && ready(name)
}

/// Whether a model that is on disk can be used. Not while its download is
/// still running: a Parakeet model is in place a minute or more before its
/// download ends, while Core ML prepares it, and loading it then would start
/// a second helper doing the same. And not a Parakeet model on a Mac that
/// cannot run it (older than macOS 14, or without the helper), where it is
/// hidden in settings but may still be the one chosen: dictation then asks
/// for a model to be downloaded instead of failing after the recording.
fn ready(name: &str) -> bool {
    !downloading(name) && (find(name).map(|m| m.engine) != Some(Engine::Parakeet) || crate::parakeet::available())
}

/// The models offered here. Parakeet is left out on a Mac that cannot run it.
pub fn statuses(app: &AppHandle) -> Vec<ModelStatus> {
    CATALOG
        .iter()
        .filter(|info| info.engine != Engine::Parakeet || crate::parakeet::available())
        .map(|info| ModelStatus {
            info: *info,
            downloaded: is_downloaded(app, info.name),
        })
        .collect()
}

fn url(name: &str) -> String {
    format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{name}.bin")
}

/// The voice activity model: Silero VAD, converted for whisper.cpp. It
/// tells speech from silence, wind and other noise (see `speech`). Under
/// 1 MB, so it is fetched with whichever speech model is downloaded first,
/// and by itself for anyone who already has a speech model.
pub const VAD: &str = "silero-v6.2.0";
/// At a fixed revision, and checked against the hash Hugging Face lists for it.
const VAD_URL: &str =
    "https://huggingface.co/ggml-org/whisper-vad/resolve/9ffd54a1e1ee413ddf265af9913beaf518d1639b/ggml-silero-v6.2.0.bin";
const VAD_SHA256: &str = "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987";

pub fn vad_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(dir(app)?.join(format!("ggml-{VAD}.bin")))
}

/// The voice activity model, if it has been downloaded.
pub fn vad(app: &AppHandle) -> Option<PathBuf> {
    vad_path(app).ok().filter(|p| p.is_file())
}

/// Download the voice activity model unless it is already here. Quietly:
/// it is small, and until it arrives a simpler loudness check stands in.
pub async fn ensure_vad(app: &AppHandle) -> Result<(), String> {
    let target = vad_path(app)?;
    if target.is_file() {
        return Ok(());
    }
    let Some(_claim) = Claim::take(VAD) else { return Ok(()) };
    let partial = target.with_extension("bin.part");
    match fetch(None, VAD, VAD_URL, &partial, Some(VAD_SHA256), &mut Tally::default()).await {
        Ok(_) => fs::rename(&partial, &target).map_err(|e| e.to_string()),
        Err(e) => {
            let _ = fs::remove_file(&partial);
            Err(e)
        }
    }
}

/// Download a model, reporting progress on the `model-download` event. The
/// file is written under a temporary name and renamed at the end, so an
/// interrupted download never looks like a usable model.
pub async fn download(app: &AppHandle, name: &str) -> Result<(), String> {
    let target = path(app, name)?;
    if target.exists() {
        return Ok(());
    }
    // Two downloads of one model would write over each other's partial file.
    let claim = Claim::take(name).ok_or("That model is already downloading.")?;
    // A stop asked for before this download began is not for it.
    stop_asked(name);
    let mut tally = Tally::default();

    let result = match find(name).map(|m| m.engine) {
        Some(Engine::Parakeet) => {
            let partial = dir(app)?.join(format!("{name}.part"));
            let _ = fs::remove_dir_all(&partial);
            tally.total = crate::parakeet::TOTAL_BYTES;
            let mut result = Ok(());
            for (url, file, sha256) in crate::parakeet::files() {
                result = fetch(Some(app), name, &url, &partial.join(file), sha256, &mut tally).await;
                if result.is_err() {
                    break;
                }
            }
            let result = finish(result, &partial, &target, || fs::remove_dir_all(&partial));
            if result.is_ok() {
                // Core ML prepares a model for this Mac's Neural Engine the
                // first time it is loaded from a new place, which takes a
                // minute or more. Do it now, while the download still shows
                // as running, rather than in the first dictation. A model that
                // cannot be loaded is removed again, so it does not look
                // downloaded for good and can simply be downloaded again.
                let folder = target.clone();
                let prepared = tauri::async_runtime::spawn_blocking(move || {
                    crate::parakeet::Parakeet::start(&folder).map(drop)
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
                if let Err(e) = prepared {
                    eprintln!("[models] could not prepare {name}: {e}");
                    let _ = fs::remove_dir_all(&target);
                    Err(format!("The model downloaded but could not be loaded: {e}"))
                } else {
                    Ok(())
                }
            } else {
                result
            }
        }
        _ => {
            let partial = target.with_extension("bin.part");
            let result = fetch(Some(app), name, &url(name), &partial, None, &mut tally).await;
            finish(result, &partial, &target, || fs::remove_file(&partial))
        }
    };
    if result.is_ok() {
        // Downloaded from here on, for whatever the end of the download sets off.
        drop(claim);
        let received = tally.received;
        let _ = app.emit("model-download", Progress { name, received, total: received });
        // After saying so: the speech model is ready without it. Not worth
        // failing the speech model over either: until it is here a simpler
        // check stands in, and the next start tries again.
        if let Err(e) = ensure_vad(app).await {
            eprintln!("[utter] could not download the voice activity model: {e}");
        }
    }
    result
}

/// Move a finished download into place, or, whatever went wrong, never leave
/// a half-written model behind.
fn finish(
    result: Result<(), String>,
    partial: &Path,
    target: &Path,
    remove: impl FnOnce() -> std::io::Result<()>,
) -> Result<(), String> {
    let result = result.and_then(|()| fs::rename(partial, target).map_err(|e| e.to_string()));
    if result.is_err() {
        let _ = remove();
    }
    result
}

/// Bytes received so far across every file of one model, for progress.
#[derive(Default)]
struct Tally {
    received: u64,
    /// Zero until known.
    total: u64,
    reported: u64,
}

/// Give up if the server sends nothing for this long, rather than waiting
/// forever on a stalled connection.
const STALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Stream one file of model `name` from `url` into `partial`, adding what
/// arrives to `tally`. Progress is reported to `app`, when given. With
/// `sha256`, the file must have that hash.
async fn fetch(
    app: Option<&AppHandle>,
    name: &str,
    url: &str,
    partial: &Path,
    sha256: Option<&str>,
    tally: &mut Tally,
) -> Result<(), String> {
    let response = reqwest::Client::builder()
        .connect_timeout(STALL_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Download failed: {e}"))?;
    let expected = response.content_length().unwrap_or(0);
    if tally.total == 0 {
        tally.total = expected;
    }

    if let Some(folder) = partial.parent() {
        fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    }
    let mut file = fs::File::create(partial).map_err(|e| e.to_string())?;
    let mut received: u64 = 0;
    let mut hash = sha256.map(|_| Sha256::new());
    let mut stream = response.bytes_stream();

    loop {
        let next = tokio::time::timeout(STALL_TIMEOUT, stream.next())
            .await
            .map_err(|_| "The download stalled. Check your connection and try again.".to_string())?;
        let Some(chunk) = next else { break };
        if stop_asked(name) {
            return Err(STOPPED.into());
        }
        let chunk = chunk.map_err(|e| format!("Download interrupted: {e}"))?;
        file.write_all(&chunk)
            .map_err(|e| format!("Could not save the model: {e}"))?;
        if let Some(hash) = &mut hash {
            hash.update(&chunk);
        }
        received += chunk.len() as u64;
        tally.received += chunk.len() as u64;
        // Report roughly every megabyte; more often just floods the UI.
        if tally.received - tally.reported >= 1_000_000 {
            tally.reported = tally.received;
            if let Some(app) = app {
                let _ = app.emit("model-download", Progress { name, received: tally.received, total: tally.total });
            }
        }
    }
    file.flush().map_err(|e| e.to_string())?;

    if expected > 0 && received != expected {
        return Err("Download ended early. Please try again.".into());
    }
    if let (Some(wanted), Some(hash)) = (sha256, hash)
        && !matches_hash(&hash.finalize(), wanted)
    {
        return Err("The download was damaged on the way. Please try again.".into());
    }
    Ok(())
}

/// Does a SHA-256 digest match one written in hex?
fn matches_hash(digest: &[u8], hex: &str) -> bool {
    let written: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    written.eq_ignore_ascii_case(hex)
}

/// The error a download stopped by the user ends with.
pub const STOPPED: &str = "Stopped.";

/// Models whose download the user asked to stop.
static STOP: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Ask the download of `name`, if one is running, to stop. Its partial file
/// is removed, as for any download that does not finish.
pub fn stop(name: &str) {
    STOP.lock().unwrap().get_or_insert_with(HashSet::new).insert(name.to_string());
}

/// True once, after `stop` was called for `name`.
fn stop_asked(name: &str) -> bool {
    STOP.lock().unwrap().as_mut().is_some_and(|asked| asked.remove(name))
}

/// Models being downloaded right now.
static IN_PROGRESS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Is `name` being downloaded right now?
fn downloading(name: &str) -> bool {
    IN_PROGRESS.lock().unwrap().as_ref().is_some_and(|active| active.contains(name))
}

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
        assert_eq!(language("base.en", "fr"), "en");
        assert_eq!(language("large-v3-turbo-q5_0", ""), "auto");
        assert_eq!(language("large-v3-turbo-q5_0", "ur"), "ur");
    }

    #[test]
    fn each_model_has_one_engine() {
        assert_eq!(find("base.en").unwrap().engine, Engine::Whisper);
        assert_eq!(find("parakeet-tdt-0.6b-v3").unwrap().engine, Engine::Parakeet);
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
    fn a_model_still_downloading_is_not_ready() {
        assert!(ready("small.en"));
        let claim = Claim::take("small.en").unwrap();
        assert!(!ready("small.en"));
        drop(claim);
        assert!(ready("small.en"));
    }

    #[test]
    fn parakeet_is_only_ready_where_it_can_run() {
        assert_eq!(ready("parakeet-tdt-0.6b-v3"), crate::parakeet::available());
    }

    #[test]
    fn downloads_are_checked_against_their_hash() {
        let digest = Sha256::digest(b"abc");
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(matches_hash(&digest, abc));
        assert!(matches_hash(&digest, &abc.to_uppercase()));
        assert!(!matches_hash(&digest, &abc.replace('b', "c")));
    }

    #[test]
    fn default_model_is_in_the_catalog() {
        assert!(find(&crate::settings::Settings::default().model).is_some());
        assert!(find("not-a-model").is_none());
    }
}
