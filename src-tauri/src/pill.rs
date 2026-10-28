//! The status pill's window: a small panel at the bottom of the screen that
//! floats above every app and can take a click without taking the keyboard.
//!
//! A click normally brings the clicked app to the front, which would pull the
//! cursor out of the app being dictated into. A non-activating panel takes
//! the click and leaves the app in front where it is, so the pill's window
//! is turned into one. Even then the window lets the mouse straight through,
//! except while the pointer is over a pill that has a button on it: whatever
//! lies under the empty space around the pill is never blocked.

use objc2::{
    class, msg_send,
    runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel},
    sel,
};
use objc2_app_kit::NSWindowStyleMask;
use serde::Deserialize;
use std::{
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

/// How often the pointer is checked while the pill has a button.
const POLL: Duration = Duration::from_millis(30);
/// Extra room around the pill that still counts as over it, in points.
const SLOP: f64 = 2.0;

/// The pill's place in its window, in the page's points.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Area {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Where the pill is while it has a button to press; `None` while it has
/// none, or is hidden.
static AREA: Mutex<Option<Area>> = Mutex::new(None);
/// Set while a thread is following the pointer.
static WATCHING: AtomicBool = AtomicBool::new(false);

/// An `NSPanel` that never becomes the key or main window, so the keyboard
/// stays with the app in front. It keeps the window's one variable, which
/// the window code reads and writes by name.
fn panel_class() -> &'static AnyClass {
    static CLASS: OnceLock<&'static AnyClass> = OnceLock::new();
    CLASS.get_or_init(|| {
        extern "C-unwind" fn refuse(_: &AnyObject, _: Sel) -> Bool {
            Bool::NO
        }
        let mut builder = ClassBuilder::new(c"UtterPillPanel", class!(NSPanel)).expect("class name is free");
        builder.add_ivar::<Bool>(c"focusable");
        unsafe {
            builder.add_method(sel!(canBecomeKeyWindow), refuse as extern "C-unwind" fn(_, _) -> _);
            builder.add_method(sel!(canBecomeMainWindow), refuse as extern "C-unwind" fn(_, _) -> _);
        }
        builder.register()
    })
}

/// Make the pill's window a panel that takes clicks without bringing Utter
/// to the front. Main thread only.
pub fn make_panel(pill: &WebviewWindow) {
    let Ok(window) = pill.ns_window() else {
        return;
    };
    let class = panel_class();
    unsafe {
        let window = &*(window as *const AnyObject);
        // Only a class of exactly the same size can take over a window
        // already made; otherwise the pill stays as it is, and a click on it
        // brings Utter forward.
        if window.class().instance_size() != class.instance_size() {
            eprintln!("[utter] the pill could not become a panel");
            return;
        }
        AnyObject::set_class(window, class);
        let mask: NSWindowStyleMask = msg_send![window, styleMask];
        let _: () = msg_send![window, setStyleMask: mask | NSWindowStyleMask::NonactivatingPanel];
        // A panel hides whenever its app is not in front, and Utter never is.
        let _: () = msg_send![window, setHidesOnDeactivate: false];
        let _: () = msg_send![window, setBecomesKeyOnlyIfNeeded: true];
    }
}

/// The page says where the pill is while it has a button, or `None` once it
/// has none. While there is one, the pointer is followed: over the pill, the
/// window takes the mouse; anywhere else, the mouse goes through.
pub fn set_area(app: &AppHandle, area: Option<Area>) {
    *AREA.lock().unwrap() = area;
    if area.is_some() && !WATCHING.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        std::thread::spawn(move || follow_pointer(&app));
    }
}

fn follow_pointer(app: &AppHandle) {
    let Some(pill) = app.get_webview_window("pill") else {
        WATCHING.store(false, Ordering::SeqCst);
        return;
    };
    let mut over = false;
    loop {
        let area = *AREA.lock().unwrap();
        let now = area.is_some_and(|area| pointer_over(&pill, area));
        if now != over {
            over = now;
            let _ = pill.set_ignore_cursor_events(!over);
            let _ = app.emit_to("pill", "pill-hover", over);
        }
        if area.is_none() {
            // Stop, unless a new area came in just now and found this
            // thread still running.
            WATCHING.store(false, Ordering::SeqCst);
            if AREA.lock().unwrap().is_none() || WATCHING.swap(true, Ordering::SeqCst) {
                return;
            }
            continue;
        }
        std::thread::sleep(POLL);
    }
}

fn pointer_over(pill: &WebviewWindow, area: Area) -> bool {
    let (Ok(pointer), Ok(origin), Ok(scale)) = (pill.cursor_position(), pill.inner_position(), pill.scale_factor())
    else {
        return false;
    };
    let x = (pointer.x - origin.x as f64) / scale;
    let y = (pointer.y - origin.y as f64) / scale;
    x >= area.x - SLOP && x <= area.x + area.width + SLOP && y >= area.y - SLOP && y <= area.y + area.height + SLOP
}
