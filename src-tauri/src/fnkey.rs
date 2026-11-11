//! The fn (globe) key as the dictation hotkey, alone or held together with
//! Control, Option or Command: hold to talk, double-tap it for hands-free
//! (fn alone only).
//!
//! macOS will not register a lone modifier key as a hotkey, so the key is
//! watched with an event tap instead. A tap that may act on events needs the
//! Accessibility permission, which Utter already asks for to paste; no
//! keystroke-monitoring permission is involved. The tap looks at only the
//! current modifier flags on every event: whether fn (and, when one is
//! required, its companion modifier) are held with nothing else. Which other
//! key, if any, was pressed is never looked at, only that one was.
//!
//! Every event passes through unchanged. macOS's own globe-key action (the
//! emoji picker, say) cannot be stopped from here, because the window server
//! runs it before any tap; see `globe` for how that is handled. What the tap
//! sees is logged to ~/Library/Logs/Utter/fn-key.log.

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

/// The stored value for the fn (globe) key on its own.
pub const FN: &str = "Fn";
/// The stored value for the fn key held together with Control.
pub const FN_CONTROL: &str = "Fn+Control";
/// The stored value for the fn key held together with Option.
pub const FN_ALT: &str = "Fn+Alt";
/// The stored value for the fn key held together with Command.
pub const FN_COMMAND: &str = "Fn+Super";

/// Key code of the fn key (and the globe key on newer keyboards).
const FN_KEY_CODE: i64 = 63;

/// The modifier a fn-key variant must be held together with, if any.
fn companion(hotkey: &str) -> Option<CGEventFlags> {
    match hotkey {
        FN_CONTROL => Some(CGEventFlags::CGEventFlagControl),
        FN_ALT => Some(CGEventFlags::CGEventFlagAlternate),
        FN_COMMAND => Some(CGEventFlags::CGEventFlagCommand),
        _ => None,
    }
}

/// Whether a stored hotkey is one of the fn-key variants: fn on its own, or
/// fn held together with Control, Option or Command.
pub fn is_variant(hotkey: &str) -> bool {
    hotkey == FN || companion(hotkey).is_some()
}

/// Utter wants the fn key: it is the saved dictation hotkey.
static WANTED: AtomicBool = AtomicBool::new(false);
/// The companion modifier that must be held along with fn, as raw
/// `CGEventFlags` bits (0 for fn alone).
static COMPANION_BITS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Hotkeys are paused while a new one is being recorded in settings.
static PAUSED: AtomicBool = AtomicBool::new(false);
/// The event tap is installed and receiving events.
static RUNNING: AtomicBool = AtomicBool::new(false);
/// A thread is installing (or trying to install) the tap.
static STARTED: AtomicBool = AtomicBool::new(false);
/// A dictation is in progress: the wanted combination is currently held.
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// fn's own flag, last seen, only to log its edges.
static PREV_FN: AtomicBool = AtomicBool::new(false);
/// The tap's port, to switch it back on if macOS switches it off.
static PORT: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
}

/// Start (or resume) treating a fn-key variant as the dictation hotkey. The
/// tap is put in once and then kept; if macOS refuses it, because
/// Accessibility is not allowed yet, it is tried again every few seconds
/// until it works.
pub fn enable(app: &AppHandle, hotkey: &str) {
    let required = companion(hotkey);
    if !WANTED.load(Ordering::SeqCst) {
        let with = match hotkey {
            FN_CONTROL => ", held together with control",
            FN_ALT => ", held together with option",
            FN_COMMAND => ", held together with command",
            _ => "",
        };
        log(&format!("fn key wanted as the dictation hotkey{with} (Utter {})", env!("CARGO_PKG_VERSION")));
    }
    WANTED.store(true, Ordering::SeqCst);
    COMPANION_BITS.store(required.map_or(0, |f| f.bits()), Ordering::SeqCst);
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
            on_event(&app, kind, event.get_flags(), || {
                event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE)
            });
            CallbackResult::Keep
        },
    )
}

/// What the tap does with one event. Kept small: it runs for every key the
/// user presses, and must hand any real work to the main thread at once.
fn on_event(app: &AppHandle, kind: CGEventType, flags: CGEventFlags, key_code: impl Fn() -> i64) {
    match kind {
        // macOS switches off a tap it thinks is too slow; switch it back on.
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
            log("macOS switched the fn key tap off; switching it back on");
            let port = PORT.load(Ordering::SeqCst);
            if !port.is_null() {
                unsafe { CGEventTapEnable(port as CFMachPortRef, true) };
            }
            return;
        }
        _ => {}
    }
    if !WANTED.load(Ordering::SeqCst) || PAUSED.load(Ordering::SeqCst) {
        ACTIVE.store(false, Ordering::SeqCst);
        PREV_FN.store(false, Ordering::SeqCst);
        return;
    }
    if matches!(kind, CGEventType::FlagsChanged) && key_code() == FN_KEY_CODE {
        let fn_held = flags.contains(CGEventFlags::CGEventFlagSecondaryFn);
        if PREV_FN.swap(fn_held, Ordering::SeqCst) != fn_held {
            log(&format!("fn {}", if fn_held { "down" } else { "up" }));
        }
    }
    let bits = COMPANION_BITS.load(Ordering::SeqCst);
    let required = (bits != 0).then(|| CGEventFlags::from_bits_retain(bits));
    match step(kind, flags, required) {
        Some(Action::Press) => dispatch(app, |app| pipeline::on_press(app, Mode::Tidy)),
        Some(Action::Release) => dispatch(app, |app| pipeline::on_release(app, Mode::Tidy)),
        Some(Action::Cancel) => {
            log("another key was pressed with fn: dictation dropped");
            dispatch(app, pipeline::cancel)
        }
        None => {}
    }
}

#[derive(Debug, PartialEq)]
enum Action {
    Press,
    Release,
    Cancel,
}

/// Modifiers that must not be held alongside the wanted combination, because
/// their presence means these keys are somebody else's shortcut, not a
/// dictation. `required`, if any, is the one companion modifier allowed to
/// be held (and that must be, for the combination to be satisfied).
fn disallowed(flags: CGEventFlags, required: Option<CGEventFlags>) -> bool {
    let mut extra = CGEventFlags::CGEventFlagControl
        | CGEventFlags::CGEventFlagAlternate
        | CGEventFlags::CGEventFlagShift
        | CGEventFlags::CGEventFlagCommand;
    if let Some(required) = required {
        extra.remove(required);
    }
    flags.intersects(extra)
}

/// Whether the wanted combination (fn alone, or fn plus `required`) is held
/// right now, with nothing extra.
fn satisfied(flags: CGEventFlags, required: Option<CGEventFlags>) -> bool {
    flags.contains(CGEventFlags::CGEventFlagSecondaryFn)
        && required.is_none_or(|required| flags.contains(required))
        && !disallowed(flags, required)
}

/// The fn key's state machine, apart from the system calls so it can be
/// tested. It looks only at the modifier flags current on each event: a
/// transition into the wanted combination presses, letting go of fn or its
/// companion releases cleanly, and anything else appearing while it is held
/// (another modifier, or any other key going down) cancels instead, because
/// that means the keys are being used for somebody else's shortcut.
fn step(kind: CGEventType, flags: CGEventFlags, required: Option<CGEventFlags>) -> Option<Action> {
    match kind {
        CGEventType::FlagsChanged => {
            let now = satisfied(flags, required);
            let was = ACTIVE.swap(now, Ordering::SeqCst);
            if now == was {
                None
            } else if now {
                Some(Action::Press)
            } else if disallowed(flags, required) {
                Some(Action::Cancel)
            } else {
                Some(Action::Release)
            }
        }
        CGEventType::KeyDown => ACTIVE.swap(false, Ordering::SeqCst).then_some(Action::Cancel),
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
        ACTIVE.store(false, Ordering::SeqCst);
    }

    const FN_FLAG: CGEventFlags = CGEventFlags::CGEventFlagSecondaryFn;
    const CMD_FLAG: CGEventFlags = CGEventFlags::CGEventFlagCommand;
    const CTRL_FLAG: CGEventFlags = CGEventFlags::CGEventFlagControl;
    const SHIFT_FLAG: CGEventFlags = CGEventFlags::CGEventFlagShift;
    const NONE: CGEventFlags = CGEventFlags::CGEventFlagNull;

    // One test, not two: `step` reads and writes the same global ACTIVE
    // static that every call in this module shares, so separate `#[test]`
    // functions would run on separate threads and race each other.
    #[test]
    fn fn_alone_and_fn_with_a_companion_required() {
        reset();
        // Hold fn and let go: a dictation.
        assert_eq!(step(CGEventType::FlagsChanged, FN_FLAG, None), Some(Action::Press));
        assert_eq!(step(CGEventType::FlagsChanged, FN_FLAG, None), None, "a repeat is ignored");
        assert_eq!(step(CGEventType::FlagsChanged, NONE, None), Some(Action::Release));
        // fn and an arrow key: the dictation is dropped, and letting go of
        // fn afterwards does nothing.
        assert_eq!(step(CGEventType::FlagsChanged, FN_FLAG, None), Some(Action::Press));
        assert_eq!(step(CGEventType::KeyDown, FN_FLAG, None), Some(Action::Cancel));
        assert_eq!(step(CGEventType::KeyDown, FN_FLAG, None), None);
        assert_eq!(step(CGEventType::FlagsChanged, NONE, None), None);
        // fn added while Shift is held is a shortcut from the start.
        assert_eq!(step(CGEventType::FlagsChanged, FN_FLAG | SHIFT_FLAG, None), None);
        assert_eq!(step(CGEventType::FlagsChanged, SHIFT_FLAG, None), None);
        // Typing with fn up is none of its business.
        assert_eq!(step(CGEventType::KeyDown, NONE, None), None);
        // Shift pressed while fn is held for dictation drops the dictation.
        assert_eq!(step(CGEventType::FlagsChanged, FN_FLAG, None), Some(Action::Press));
        assert_eq!(step(CGEventType::FlagsChanged, FN_FLAG | SHIFT_FLAG, None), Some(Action::Cancel));
        assert_eq!(step(CGEventType::FlagsChanged, NONE, None), None);
        reset();

        // fn alone does nothing until its companion joins it.
        assert_eq!(step(CGEventType::FlagsChanged, FN_FLAG, Some(CMD_FLAG)), None);
        assert_eq!(step(CGEventType::FlagsChanged, FN_FLAG | CMD_FLAG, Some(CMD_FLAG)), Some(Action::Press));
        assert_eq!(
            step(CGEventType::FlagsChanged, FN_FLAG | CMD_FLAG, Some(CMD_FLAG)),
            None,
            "a repeat is ignored"
        );
        // Letting go of either key ends it cleanly.
        assert_eq!(step(CGEventType::FlagsChanged, FN_FLAG, Some(CMD_FLAG)), Some(Action::Release));
        // The companion can come first instead.
        assert_eq!(step(CGEventType::FlagsChanged, CMD_FLAG, Some(CMD_FLAG)), None);
        assert_eq!(step(CGEventType::FlagsChanged, FN_FLAG | CMD_FLAG, Some(CMD_FLAG)), Some(Action::Press));
        assert_eq!(step(CGEventType::FlagsChanged, CMD_FLAG, Some(CMD_FLAG)), Some(Action::Release));
        // An extra modifier while both are held is somebody else's shortcut.
        assert_eq!(step(CGEventType::FlagsChanged, FN_FLAG | CMD_FLAG, Some(CMD_FLAG)), Some(Action::Press));
        assert_eq!(
            step(CGEventType::FlagsChanged, FN_FLAG | CMD_FLAG | SHIFT_FLAG, Some(CMD_FLAG)),
            Some(Action::Cancel)
        );
        assert_eq!(step(CGEventType::FlagsChanged, NONE, Some(CMD_FLAG)), None);
        // So is a real key pressed while both are held.
        assert_eq!(step(CGEventType::FlagsChanged, FN_FLAG | CMD_FLAG, Some(CMD_FLAG)), Some(Action::Press));
        assert_eq!(step(CGEventType::KeyDown, FN_FLAG | CMD_FLAG, Some(CMD_FLAG)), Some(Action::Cancel));
        reset();

        // A different companion (Control) is unaffected by Command appearing,
        // since Command is now the "extra" modifier.
        assert_eq!(step(CGEventType::FlagsChanged, FN_FLAG | CTRL_FLAG, Some(CTRL_FLAG)), Some(Action::Press));
        assert_eq!(
            step(CGEventType::FlagsChanged, FN_FLAG | CTRL_FLAG | CMD_FLAG, Some(CTRL_FLAG)),
            Some(Action::Cancel)
        );
        reset();
    }

    #[test]
    fn stored_hotkeys_map_to_the_right_companion() {
        assert_eq!(companion(FN), None);
        assert_eq!(companion(FN_CONTROL), Some(CTRL_FLAG));
        assert_eq!(companion(FN_ALT), Some(CGEventFlags::CGEventFlagAlternate));
        assert_eq!(companion(FN_COMMAND), Some(CMD_FLAG));
        assert!(is_variant(FN) && is_variant(FN_CONTROL) && is_variant(FN_ALT) && is_variant(FN_COMMAND));
        assert!(!is_variant("Control+Alt+Space"));
    }
}
