//! The menu bar item: an icon that moves while Utter listens or works (so
//! there is a sign of it even when a full-screen app hides the pill), a dot
//! when something needs the user, and a menu headed by the same status the
//! window shows.

use crate::learn::Learned;
use serde::Deserialize;
use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tauri::{
    AppHandle, Emitter, Manager,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

const ID: &str = "utter";

/// The window's status, in its words: "Ready" / "Hold ⌃⌥Space in any app".
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub title: String,
    pub detail: String,
    /// "" when fine, "pe" when something needs the user, "bad" when dictation
    /// can't happen, "live" while something is under way.
    pub tone: String,
    /// The menu item that fixes it ("Allow Microphone…"), if any.
    pub action: Option<String>,
    /// The section of the window that fixes it.
    pub fix: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    Listening,
    Working,
}

pub struct Tray {
    status: Mutex<Status>,
    learned: Mutex<Option<Learned>>,
    phase: Mutex<Phase>,
    /// Bumped whenever the icon's animation should stop or change.
    generation: AtomicU64,
}

impl Tray {
    pub fn new() -> Self {
        Self {
            status: Mutex::new(Status { title: "Ready".into(), ..Status::default() }),
            learned: Mutex::new(None),
            phase: Mutex::new(Phase::Idle),
            generation: AtomicU64::new(0),
        }
    }
}

const IDLE: &[u8] = include_bytes!("../icons/tray/idle.png");
const IDLE_DOT: &[u8] = include_bytes!("../icons/tray/idle-dot.png");
const LIVE: [&[u8]; 8] = [
    include_bytes!("../icons/tray/live-0.png"),
    include_bytes!("../icons/tray/live-1.png"),
    include_bytes!("../icons/tray/live-2.png"),
    include_bytes!("../icons/tray/live-3.png"),
    include_bytes!("../icons/tray/live-4.png"),
    include_bytes!("../icons/tray/live-5.png"),
    include_bytes!("../icons/tray/live-6.png"),
    include_bytes!("../icons/tray/live-7.png"),
];
const WORK: [&[u8]; 8] = [
    include_bytes!("../icons/tray/work-0.png"),
    include_bytes!("../icons/tray/work-1.png"),
    include_bytes!("../icons/tray/work-2.png"),
    include_bytes!("../icons/tray/work-3.png"),
    include_bytes!("../icons/tray/work-4.png"),
    include_bytes!("../icons/tray/work-5.png"),
    include_bytes!("../icons/tray/work-6.png"),
    include_bytes!("../icons/tray/work-7.png"),
];
const FRAME: Duration = Duration::from_millis(110);

fn set_icon(app: &AppHandle, bytes: &'static [u8]) {
    let Some(tray) = app.tray_by_id(ID) else { return };
    if let Ok(image) = Image::from_bytes(bytes) {
        let _ = tray.set_icon(Some(image));
        let _ = tray.set_icon_as_template(true);
    }
}

/// The still icon: with a dot when something needs the user.
fn still_icon(app: &AppHandle) {
    let state = app.state::<Tray>();
    let needs_you = matches!(state.status.lock().unwrap().tone.as_str(), "pe" | "bad");
    set_icon(app, if needs_you { IDLE_DOT } else { IDLE });
}

/// Follow the dictation: called with each pipeline phase.
pub fn set_phase(app: &AppHandle, phase: &str) {
    let next = match phase {
        "recording" => Phase::Listening,
        "processing" => Phase::Working,
        _ => Phase::Idle,
    };
    let Some(state) = app.try_state::<Tray>() else { return };
    {
        let mut current = state.phase.lock().unwrap();
        if *current == next {
            return;
        }
        *current = next;
    }
    let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    if next == Phase::Idle {
        still_icon(app);
        refresh(app);
        return;
    }
    let frames = if next == Phase::Listening { LIVE } else { WORK };
    let app = app.clone();
    std::thread::spawn(move || {
        let mut at = 0;
        while app.state::<Tray>().generation.load(Ordering::SeqCst) == generation {
            set_icon(&app, frames[at % frames.len()]);
            at += 1;
            std::thread::sleep(FRAME);
        }
    });
}

/// The window's status changed.
pub fn set_status(app: &AppHandle, status: Status) {
    let state = app.state::<Tray>();
    {
        let mut current = state.status.lock().unwrap();
        if *current == status {
            return;
        }
        *current = status;
    }
    if *state.phase.lock().unwrap() == Phase::Idle {
        still_icon(app);
    }
    refresh(app);
}

/// A word was learned (Some) or the last one taken back (None).
pub fn set_learned(app: &AppHandle, learned: Option<Learned>) {
    *app.state::<Tray>().learned.lock().unwrap() = learned;
    refresh(app);
}

/// The word Undo Learning would take back.
pub fn learned(app: &AppHandle) -> Option<Learned> {
    app.state::<Tray>().learned.lock().unwrap().clone()
}

fn menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let state = app.state::<Tray>();
    let status = state.status.lock().unwrap().clone();
    let learned = state.learned.lock().unwrap().clone();
    let last = app.state::<crate::pipeline::Pipeline>().last();
    let fixable = last.as_ref().is_some_and(|d| d.selection.is_none());

    let menu = Menu::new(app)?;
    let title = if status.title.is_empty() { "Ready".to_string() } else { status.title.clone() };
    menu.append(&MenuItem::with_id(app, "status", title, false, None::<&str>)?)?;
    if !status.detail.is_empty() {
        menu.append(&MenuItem::with_id(app, "status-detail", &status.detail, false, None::<&str>)?)?;
    }
    if let Some(action) = status.action.as_ref().filter(|_| matches!(status.tone.as_str(), "pe" | "bad")) {
        menu.append(&MenuItem::with_id(app, "fix-status", action, true, None::<&str>)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "open", "Open Utter", true, Some("CmdOrCtrl+,"))?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "copy", "Copy Last Dictation", last.is_some(), None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "fix-last", "Fix Last Word…", fixable, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "add-word", "Add a Word…", true, None::<&str>)?)?;
    let undo = match &learned {
        Some(l) if l.new_word => format!("Undo Learning “{}”", l.word),
        Some(l) => format!("Undo “{}” Heard as “{}”", l.word, l.heard),
        None => "Undo Learned Word".to_string(),
    };
    menu.append(&MenuItem::with_id(app, "undo-learned", undo, learned.is_some(), None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "about", "About Utter", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "updates", "Check for Updates…", true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit Utter", true, Some("CmdOrCtrl+Q"))?)?;
    Ok(menu)
}

/// Rebuild the menu from the current state.
pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(ID) else { return };
    if let Ok(menu) = menu(app) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// Bring the window forward, optionally on one section.
fn open(app: &AppHandle, event: Option<(&str, &str)>) {
    crate::show_main_window(app);
    if let Some((name, section)) = event {
        let _ = app.emit(name, section.to_string());
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    app.manage(Tray::new());
    TrayIconBuilder::with_id(ID)
        .icon(Image::from_bytes(IDLE)?)
        .icon_as_template(true)
        .tooltip("Utter")
        .menu(&menu(app)?)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => open(app, None),
            "copy" => crate::pipeline::copy_last(app),
            "fix-last" => open(app, Some(("fix-last", ""))),
            // The window opens on the word list, ready to type into.
            "add-word" => open(app, Some(("add-word", ""))),
            "undo-learned" => {
                if let Some(learned) = learned(app) {
                    if let Err(e) = crate::unlearn(app, &learned) {
                        eprintln!("[utter] could not undo a learned word: {e}");
                    }
                }
            }
            "fix-status" => {
                let fix = app.state::<Tray>().status.lock().unwrap().fix.clone().unwrap_or_default();
                open(app, Some(("open-section", &fix)));
            }
            "about" => open(app, Some(("open-section", "about"))),
            "updates" => {
                open(app, Some(("open-section", "about")));
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    crate::updates::check(&app).await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
