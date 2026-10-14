//! The fn (globe) key as the dictation hotkey: hold it to talk, double-tap
//! it for hands-free.
//!
//! macOS will not register a lone modifier key as a hotkey, so the key is
//! watched with an event tap instead. A tap that may act on events needs the
//! Accessibility permission, which Utter already asks for to paste; no
//! keystroke-monitoring permission is involved. The tap looks at only two
//! things: the fn key going down and up, and whether any other key is pressed
//! while fn is held (then fn is part of another shortcut, such as fn+arrow,
//! and the dictation is dropped). Which key that was is never looked at.
//!
//! The fn key's own down and up events are held back from other apps while fn
//! is the hotkey, from as early in the event path as macOS allows, to stop it
//! also doing whatever "Press 🌐 key to" is set to (the emoji picker, say).
//! The Hotkey tab also points at that setting, in case macOS acts first.
//! What the tap sees is logged to ~/Library/Logs/Utter/fn-key.log.
//! Every other event passes through unchanged; fn still works in shortcuts,
//! because the keys pressed with it carry the fn flag themselves.

use crate::pipeline::{self, Mode};
use core_foundation::{
    base::TCFType,
    mach_port::CFMachPortRef,
    runloop::{CFRunLoop, kCFRunLoopCommonModes},
};
use core_graphics::event::{
    CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CallbackResult, EventField,
};
use std::{
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
    thread,
    time::Duration,
};
use tauri::AppHandle;

/// The value a setting holds when the fn key is the dictation hotkey.
pub const FN: &str = "Fn";

/// Key code of the fn key (and the globe key on newer keyboards).
const FN_KEY_CODE: i64 = 63;

/// Utter wants the fn key: it is the saved dictation hotkey.
static WANTED: AtomicBool = AtomicBool::new(false);
/// Hotkeys are paused while a new one is being recorded in settings.
static PAUSED: AtomicBool = AtomicBool::new(false);
/// The event tap is installed and receiving events.
static RUNNING: AtomicBool = AtomicBool::new(false);
/// A thread is installing (or trying to install) the tap.
static STARTED: AtomicBool = AtomicBool::new(false);
/// fn is down right now.
static DOWN: AtomicBool = AtomicBool::new(false);
/// This press of fn started a dictation and has not been used with another key.
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// The tap's port, to switch it back on if macOS switches it off.
static PORT: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
}

/// Start (or resume) treating fn as the dictation hotkey. The tap is put in
/// once and then kept; if macOS refuses it, because Accessibility is not
/// allowed yet, it is tried again every few seconds until it works.
pub fn enable(app: &AppHandle) {
    if !WANTED.load(Ordering::SeqCst) {
        log(&format!("fn key wanted as the dictation hotkey (Utter {})", env!("CARGO_PKG_VERSION")));
    }
    WANTED.store(true, Ordering::SeqCst);
    PAUSED.store(false, Ordering::SeqCst);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    let spawned = thread::Builder::new().name("utter-fn-key".into()).spawn(move || {
        loop {
            match install(app.clone()) {
                // `install` only returns when the tap could not be made.
                Ok(()) => {}
                Err(()) => RUNNING.store(false, Ordering::SeqCst),
            }
            thread::sleep(Duration::from_secs(2));
        }
    });
    if spawned.is_err() {
        STARTED.store(false, Ordering::SeqCst);
    }
}

/// Stop treating fn as a hotkey. The tap stays in but ignores everything.
pub fn disable() {
    WANTED.store(false, Ordering::SeqCst);
    ACTIVE.store(false, Ordering::SeqCst);
}

/// Ignore fn while a new hotkey is being recorded.
pub fn pause() {
    PAUSED.store(true, Ordering::SeqCst);
    ACTIVE.store(false, Ordering::SeqCst);
}

/// Why the fn key is not working right now, if it is wanted and is not.
pub fn problem() -> Option<String> {
    (WANTED.load(Ordering::SeqCst) && !RUNNING.load(Ordering::SeqCst)).then(|| {
        "The fn key needs the Accessibility permission. Allow Utter under Permissions; it starts \
         working a moment later."
            .to_string()
    })
}

/// Put the tap in and run it on this thread. Returns only if it could not
/// be made.
fn install(app: AppHandle) -> Result<(), ()> {
    // A session-level tap at the head of the queue: it sees key events
    // before any app does, which is where the fn key's own events can be held
    // back from macOS's globe-key handling.
    static LAST_FAILURE_LOGGED: AtomicBool = AtomicBool::new(false);
    let tap = match make(CGEventTapLocation::Session, app) {
        Ok(tap) => tap,
        Err(()) => {
            if !LAST_FAILURE_LOGGED.swap(true, Ordering::SeqCst) {
                log(&format!(
                    "could not install the fn key tap (Accessibility allowed: {}); retrying every 2 s",
                    crate::permissions::accessibility()
                ));
            }
            return Err(());
        }
    };
    LAST_FAILURE_LOGGED.store(false, Ordering::SeqCst);
    let source = tap.mach_port().create_runloop_source(0).map_err(|_| ())?;
    CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
    tap.enable();
    PORT.store(tap.mach_port().as_concrete_TypeRef() as *mut _, Ordering::SeqCst);
    RUNNING.store(true, Ordering::SeqCst);
    log("fn key tap installed and listening");
    CFRunLoop::run_current();
    // The run loop only stops if the tap is torn down.
    RUNNING.store(false, Ordering::SeqCst);
    drop(tap);
    Err(())
}

fn make(location: CGEventTapLocation, app: AppHandle) -> Result<CGEventTap<'static>, ()> {
    CGEventTap::new(
        location,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        vec![CGEventType::FlagsChanged, CGEventType::KeyDown],
        move |_proxy, kind, event| {
            let hold_back = on_event(&app, kind, event.get_flags(), || {
                event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE)
            });
            if hold_back { CallbackResult::Drop } else { CallbackResult::Keep }
        },
    )
}

/// What the tap does with one event. Kept small: it runs for every key the
/// user presses, and must hand any real work to the main thread at once.
/// Returns true for an event to hold back from other apps: the fn key's own.
fn on_event(app: &AppHandle, kind: CGEventType, flags: CGEventFlags, key_code: impl Fn() -> i64) -> bool {
    match kind {
        // macOS switches off a tap it thinks is too slow; switch it back on.
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
            log("macOS switched the fn key tap off; switching it back on");
            let port = PORT.load(Ordering::SeqCst);
            if !port.is_null() {
                unsafe { CGEventTapEnable(port as CFMachPortRef, true) };
            }
            return false;
        }
        _ => {}
    }
    if !WANTED.load(Ordering::SeqCst) || PAUSED.load(Ordering::SeqCst) {
        DOWN.store(false, Ordering::SeqCst);
        ACTIVE.store(false, Ordering::SeqCst);
        return false;
    }
    // Only modifier changes are looked at closely. For a key press, all that
    // matters is that one happened: which key it was is never read.
    let modifier_change = matches!(kind, CGEventType::FlagsChanged);
    let code = if modifier_change { key_code() } else { -1 };
    let own_event = modifier_change && code == FN_KEY_CODE;
    if own_event {
        let down = flags.contains(CGEventFlags::CGEventFlagSecondaryFn);
        log(&format!("fn {} (held back from other apps)", if down { "down" } else { "up" }));
    }
    let others = CGEventFlags::CGEventFlagCommand
        | CGEventFlags::CGEventFlagControl
        | CGEventFlags::CGEventFlagAlternate
        | CGEventFlags::CGEventFlagShift;
    match step(kind, code, flags.contains(CGEventFlags::CGEventFlagSecondaryFn), flags.intersects(others)) {
        Some(Action::Press) => dispatch(app, |app| pipeline::on_press(app, Mode::Tidy)),
        Some(Action::Release) => dispatch(app, |app| pipeline::on_release(app, Mode::Tidy)),
        Some(Action::Cancel) => {
            log("another key was pressed with fn: dictation dropped");
            dispatch(app, pipeline::cancel)
        }
        None => {}
    }
    own_event
}

#[derive(Debug, PartialEq)]
enum Action {
    Press,
    Release,
    Cancel,
}

/// The fn key's state machine, apart from the system calls so it can be
/// tested. `fn_held` is the fn flag on the event; `other_modifiers` whether
/// Command, Control, Option or Shift is down too.
fn step(kind: CGEventType, key_code: i64, fn_held: bool, other_modifiers: bool) -> Option<Action> {
    match kind {
        CGEventType::FlagsChanged if key_code == FN_KEY_CODE => {
            if fn_held && !DOWN.swap(true, Ordering::SeqCst) {
                // fn on its own starts a dictation; fn added to a held
                // modifier is somebody else's shortcut.
                let alone = !other_modifiers;
                ACTIVE.store(alone, Ordering::SeqCst);
                alone.then_some(Action::Press)
            } else if !fn_held && DOWN.swap(false, Ordering::SeqCst) {
                ACTIVE.swap(false, Ordering::SeqCst).then_some(Action::Release)
            } else {
                None
            }
        }
        // Another modifier joined fn, or another key was pressed with it:
        // fn is being used for a shortcut, not for dictation.
        CGEventType::FlagsChanged | CGEventType::KeyDown => {
            (DOWN.load(Ordering::SeqCst) && ACTIVE.swap(false, Ordering::SeqCst)).then_some(Action::Cancel)
        }
        _ => None,
    }
}

fn dispatch(app: &AppHandle, work: impl FnOnce(&AppHandle) + Send + 'static) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || work(&handle));
}

/// A line in `~/Library/Logs/Utter/fn-key.log`, for finding out why fn does
/// or does not work on a given Mac. Only the fn key's own state is ever
/// written: never another key, never anything typed or said.
pub fn log(line: &str) {
    use std::io::Write;
    let Some(home) = std::env::var_os("HOME") else { return };
    let dir = std::path::Path::new(&home).join("Library/Logs/Utter");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("fn-key.log");
    // Kept small: start again once it passes 200 KB.
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 200_000) {
        let _ = std::fs::remove_file(&path);
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{stamp} {line}");
    }
}

/// What macOS itself does when the fn (globe) key is pressed, if that would
/// fire alongside dictation: "Press 🌐 key to" in Keyboard settings.
pub fn system_action() -> Option<&'static str> {
    let output = std::process::Command::new("defaults")
        .args(["read", "com.apple.HIToolbox", "AppleFnUsageType"])
        .output()
        .ok()?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    match value.as_str() {
        "0" => None,
        "1" => Some("change the input source"),
        "3" => Some("start Apple's dictation"),
        // "2", or never changed from the factory setting.
        _ => Some("show emoji and symbols"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset() {
        DOWN.store(false, Ordering::SeqCst);
        ACTIVE.store(false, Ordering::SeqCst);
    }

    #[test]
    fn fn_alone_presses_and_releases_and_fn_in_a_shortcut_cancels() {
        reset();
        // Hold fn and let go: a dictation.
        assert_eq!(step(CGEventType::FlagsChanged, FN_KEY_CODE, true, false), Some(Action::Press));
        assert_eq!(step(CGEventType::FlagsChanged, FN_KEY_CODE, true, false), None, "a repeat is ignored");
        assert_eq!(step(CGEventType::FlagsChanged, FN_KEY_CODE, false, false), Some(Action::Release));
        // fn and an arrow key: the dictation is dropped, and letting go of
        // fn afterwards does nothing.
        assert_eq!(step(CGEventType::FlagsChanged, FN_KEY_CODE, true, false), Some(Action::Press));
        assert_eq!(step(CGEventType::KeyDown, 123, true, false), Some(Action::Cancel));
        assert_eq!(step(CGEventType::KeyDown, 124, true, false), None);
        assert_eq!(step(CGEventType::FlagsChanged, FN_KEY_CODE, false, false), None);
        // fn added while Shift is held is a shortcut from the start.
        assert_eq!(step(CGEventType::FlagsChanged, FN_KEY_CODE, true, true), None);
        assert_eq!(step(CGEventType::FlagsChanged, FN_KEY_CODE, false, true), None);
        // Typing with fn up is none of its business.
        assert_eq!(step(CGEventType::KeyDown, 0, false, false), None);
        // Shift pressed while fn is held for dictation drops the dictation.
        assert_eq!(step(CGEventType::FlagsChanged, FN_KEY_CODE, true, false), Some(Action::Press));
        assert_eq!(step(CGEventType::FlagsChanged, 56, true, true), Some(Action::Cancel));
        assert_eq!(step(CGEventType::FlagsChanged, FN_KEY_CODE, false, false), None);
        reset();
    }
}
