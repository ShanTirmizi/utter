//! Keeping macOS's own globe-key action out of the way while fn dictates.
//!
//! macOS acts on a lone press of the fn (globe) key ("Press 🌐 key to" in
//! Keyboard settings: show emoji, change input source, start Apple's
//! dictation) inside the window server, before any event tap sees the key, so
//! the action cannot be stopped by holding the key back. What does stop it is
//! that setting being "Do Nothing". So, when the user asks for it, Utter
//! switches the setting to "Do Nothing" while it is using fn, and puts the
//! user's own choice back when it stops: when they quit, pick another hotkey,
//! or switch this off.
//!
//! The change is made through the same system call Keyboard settings uses,
//! which applies at once; writing the preference file directly would only
//! take effect at the next login. The user's original choice is written to a
//! small file first, so that after a crash the next launch can still put it
//! back. If the user changes the setting themselves meanwhile, theirs wins.

use std::{
    ffi::{CString, c_void},
    path::PathBuf,
    process::Command,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{AppHandle, Manager};

/// The value of "Press 🌐 key to" that means Do Nothing.
const DO_NOTHING: i32 = 0;

type GetUsage = unsafe extern "C" fn() -> i32;
type SetUsage = unsafe extern "C" fn(i32);

unsafe extern "C" {
    fn dlopen(path: *const std::ffi::c_char, mode: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const std::ffi::c_char) -> *mut c_void;
}
const RTLD_LAZY: i32 = 1;

/// The getter and setter that Keyboard settings uses, if this macOS has them.
fn calls() -> Option<(GetUsage, SetUsage)> {
    static FOUND: OnceLock<Option<(usize, usize)>> = OnceLock::new();
    let found = FOUND.get_or_init(|| unsafe {
        let path = CString::new("/System/Library/Frameworks/Carbon.framework/Carbon").ok()?;
        let library = dlopen(path.as_ptr(), RTLD_LAZY);
        if library.is_null() {
            return None;
        }
        let get = dlsym(library, CString::new("TISGetFnUsageType").ok()?.as_ptr());
        let set = dlsym(library, CString::new("TISUpdateFnUsageType").ok()?.as_ptr());
        (!get.is_null() && !set.is_null()).then_some((get as usize, set as usize))
    });
    found.map(|(get, set)| unsafe {
        (std::mem::transmute::<usize, GetUsage>(get), std::mem::transmute::<usize, SetUsage>(set))
    })
}

/// What "Press 🌐 key to" is set to now, as macOS's number for it.
pub fn current() -> Option<i32> {
    calls().map(|(get, _)| unsafe { get() })
}

/// What was there before Utter changed it.
#[derive(serde::Serialize, serde::Deserialize)]
struct Saved {
    original: i32,
    /// The preference had never been set: macOS was choosing a default for
    /// the keyboard. It is cleared again on the way back, so it still does.
    was_default: bool,
}

/// Utter has switched the action off and owes the user their own setting.
static HOLDING: AtomicBool = AtomicBool::new(false);
static LOCK: Mutex<()> = Mutex::new(());

fn marker(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("globe-key.json"))
}

fn preference_is_set() -> bool {
    Command::new("defaults")
        .args(["read", "com.apple.HIToolbox", "AppleFnUsageType"])
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Turn the globe action off while Utter uses fn, remembering the user's
/// choice first. Does nothing if it is already off.
fn hold(app: &AppHandle) {
    let _guard = LOCK.lock().unwrap();
    if HOLDING.load(Ordering::SeqCst) {
        return;
    }
    let (Some((get, set)), Some(path)) = (calls(), marker(app)) else {
        crate::fnkey::log("cannot switch off the globe key action on this macOS");
        return;
    };
    let original = unsafe { get() };
    if original == DO_NOTHING {
        return;
    }
    let saved = Saved { original, was_default: !preference_is_set() };
    // Write down how to get back before changing anything.
    let Ok(text) = serde_json::to_string(&saved) else { return };
    if std::fs::write(&path, text).is_err() {
        return;
    }
    unsafe { set(DO_NOTHING) };
    HOLDING.store(true, Ordering::SeqCst);
    crate::fnkey::log(&format!("globe key action switched off while fn dictates (was {original})"));
}

/// Put the user's own globe action back, if Utter took it.
pub fn release(app: &AppHandle) {
    let _guard = LOCK.lock().unwrap();
    let Some(path) = marker(app) else { return };
    let saved: Option<Saved> = std::fs::read(&path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok());
    HOLDING.store(false, Ordering::SeqCst);
    let Some(saved) = saved else { return };
    if let Some((get, set)) = calls() {
        // Only if it is still the value Utter set: a choice the user made
        // since then stays.
        if unsafe { get() } == DO_NOTHING {
            unsafe { set(saved.original) };
            if saved.was_default {
                let _ = Command::new("defaults")
                    .args(["delete", "com.apple.HIToolbox", "AppleFnUsageType"])
                    .status();
            }
            crate::fnkey::log(&format!("globe key action put back ({})", saved.original));
        }
    }
    let _ = std::fs::remove_file(&path);
}

/// At launch: if Utter stopped without putting the setting back, it is
/// still holding it (and will put it back later), unless the user has
/// picked something themselves since.
pub fn recover(app: &AppHandle) {
    let Some(path) = marker(app) else { return };
    if !path.exists() {
        return;
    }
    if current() == Some(DO_NOTHING) {
        HOLDING.store(true, Ordering::SeqCst);
    } else {
        let _ = std::fs::remove_file(&path);
    }
}

/// Hold the globe action off exactly while fn is the dictation key and the
/// user has asked for this; otherwise make sure it is back.
pub fn sync(app: &AppHandle, wanted: bool) {
    if wanted { hold(app) } else { release(app) }
}

/// Whether Utter is the reason the globe action is off right now.
pub fn holding() -> bool {
    HOLDING.load(Ordering::SeqCst)
}
