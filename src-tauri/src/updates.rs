//! Is there a newer Utter? Asks GitHub for the latest release, and nothing
//! else: no identifier, no usage data. Only when the user presses Check for
//! updates, or once a day if they switched on Check automatically (off by
//! default, since it is a network request).

use serde::{Deserialize, Serialize};
use std::{
    sync::Mutex,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};

const LATEST: &str = "https://api.github.com/repos/ShanTirmizi/utter/releases/latest";
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// What the last check found.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    /// This copy's version.
    pub current: String,
    /// The newest release's version, once known.
    pub latest: Option<String>,
    /// True when `latest` is newer than `current`.
    pub newer: bool,
    /// The first line of the release notes.
    pub summary: Option<String>,
    /// Where the release can be downloaded.
    pub url: Option<String>,
    /// When the last check finished, in milliseconds since 1970; 0 if never.
    pub checked_at: u64,
    /// Why the last check failed, if it did.
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Updates(Mutex<UpdateStatus>);

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// "0.4.1" or "v0.4.1" as numbers; anything after a dash is ignored.
fn parts(version: &str) -> Vec<u64> {
    version
        .trim()
        .trim_start_matches('v')
        .split('-')
        .next()
        .unwrap_or("")
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    let (a, b) = (parts(latest), parts(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

/// The first line of release notes worth reading: no headings, no "Beta."
fn summary(notes: &str) -> Option<String> {
    notes
        .lines()
        .map(|l| l.trim().trim_start_matches(['#', '-', '*', ' ']).replace("**", ""))
        .find(|l| l.len() > 12)
        .map(|l| l.chars().take(160).collect())
}

pub fn status(app: &AppHandle) -> UpdateStatus {
    let mut status = app.state::<Updates>().0.lock().unwrap().clone();
    status.current = current_version().to_string();
    status
}

/// Ask GitHub now, remember the answer, and tell the window.
pub async fn check(app: &AppHandle) -> UpdateStatus {
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
        html_url: String,
        #[serde(default)]
        body: String,
    }
    let found = async {
        let response = reqwest::Client::new()
            .get(LATEST)
            .header("User-Agent", format!("Utter/{}", current_version()))
            .header("Accept", "application/vnd.github+json")
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(|_| "Couldn't reach GitHub.".to_string())?;
        if !response.status().is_success() {
            return Err(format!("GitHub answered {}.", response.status().as_u16()));
        }
        response.json::<Release>().await.map_err(|_| "GitHub's answer couldn't be read.".to_string())
    }
    .await;

    let mut status = UpdateStatus { current: current_version().to_string(), checked_at: crate::words::now(), ..Default::default() };
    match found {
        Ok(release) => {
            let latest = release.tag_name.trim_start_matches('v').to_string();
            status.newer = is_newer(&latest, current_version());
            status.latest = Some(latest);
            status.summary = summary(&release.body);
            status.url = Some(release.html_url);
        }
        Err(e) => {
            // Keep what an earlier check found; only say this one failed.
            let before = app.state::<Updates>().0.lock().unwrap().clone();
            status = UpdateStatus { error: Some(e), checked_at: status.checked_at, ..before };
            status.current = current_version().to_string();
        }
    }
    *app.state::<Updates>().0.lock().unwrap() = status.clone();
    let _ = app.emit("update-status", status.clone());
    status
}

/// While Check automatically is on, check at launch and then once a day.
pub fn start_daily(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Let the app finish starting first.
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            if crate::settings::load(&app).check_updates {
                check(&app).await;
            }
            tokio::time::sleep(EVERY).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert!(is_newer("0.4.1", "0.4.0"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("1.0", "0.9.12"));
        assert!(!is_newer("0.4.0", "0.4.0"));
        assert!(!is_newer("0.3.9", "0.4.0"));
        assert!(!is_newer("0.4.0-beta", "0.4.0"));
    }

    #[test]
    fn the_summary_skips_headings_and_short_lines() {
        let notes = "**Beta.** Utter now learns your words.\n\n### Words\n- Words has its own tab.";
        assert_eq!(summary(notes).as_deref(), Some("Beta. Utter now learns your words."));
        assert_eq!(summary(""), None);
    }
}
