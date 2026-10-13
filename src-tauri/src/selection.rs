//! The text currently selected in the focused app.
//!
//! Used by "rewrite selected text": select something, hold the rewrite
//! hotkey, and say what to do with it. The selection is read through the
//! Accessibility API where the app supports that, and otherwise by copying
//! it, the way a person would.

use core_foundation::{
    base::{CFGetTypeID, CFRelease, CFTypeRef, TCFType},
    boolean::CFBoolean,
    string::{CFString, CFStringGetTypeID, CFStringRef},
};
use objc2_app_kit::NSWorkspace;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateSystemWide() -> CFTypeRef;
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(element: CFTypeRef, attribute: CFStringRef, value: CFTypeRef) -> i32;
    fn AXUIElementSetMessagingTimeout(element: CFTypeRef, seconds: f32) -> i32;
}

/// How long to wait for an app to answer before treating it as unable to.
const TIMEOUT_SECONDS: f32 = 0.3;

/// What the focused app says about its selection.
#[derive(Debug, PartialEq)]
pub enum Selection {
    /// This text is selected.
    Text(String),
    /// The app answered: nothing is selected.
    Nothing,
    /// The app could not be asked this way.
    Unknown,
}

/// Apps built on web technology only describe their contents to assistive
/// software once asked to. Ask the frontmost app now, at key-down, so that it
/// is ready by the time the selection is read at key-up. Apps that do not
/// know this switch ignore it.
///
/// The app is named on the main thread and asked on another: an app that is
/// slow to answer must not hold up the hotkey, which is handled on the main
/// thread too.
pub fn wake_frontmost_app() {
    let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
        return;
    };
    let pid = app.processIdentifier();
    std::thread::spawn(move || wake(pid));
}

fn wake(pid: i32) {
    let switch = CFString::from_static_string("AXManualAccessibility");
    unsafe {
        let element = AXUIElementCreateApplication(pid);
        if element.is_null() {
            return;
        }
        AXUIElementSetMessagingTimeout(element, TIMEOUT_SECONDS);
        AXUIElementSetAttributeValue(
            element,
            switch.as_concrete_TypeRef(),
            CFBoolean::true_value().as_CFTypeRef(),
        );
        CFRelease(element);
    }
}

/// Ask the focused text field for its selected text. Needs the Accessibility
/// permission; without it every app is `Unknown`.
pub fn read() -> Selection {
    let focused_attribute = CFString::from_static_string("AXFocusedUIElement");
    let selected_attribute = CFString::from_static_string("AXSelectedText");
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return Selection::Unknown;
        }
        AXUIElementSetMessagingTimeout(system, TIMEOUT_SECONDS);
        let mut focused: CFTypeRef = std::ptr::null();
        let status =
            AXUIElementCopyAttributeValue(system, focused_attribute.as_concrete_TypeRef(), &mut focused);
        CFRelease(system);
        if status != 0 || focused.is_null() {
            return Selection::Unknown;
        }

        AXUIElementSetMessagingTimeout(focused, TIMEOUT_SECONDS);
        let mut value: CFTypeRef = std::ptr::null();
        let status =
            AXUIElementCopyAttributeValue(focused, selected_attribute.as_concrete_TypeRef(), &mut value);
        CFRelease(focused);
        if status != 0 || value.is_null() {
            return Selection::Unknown;
        }
        if CFGetTypeID(value) != CFStringGetTypeID() {
            CFRelease(value);
            return Selection::Unknown;
        }
        // The "create rule" hands ownership of `value` to the wrapper.
        let text = CFString::wrap_under_create_rule(value as CFStringRef).to_string();
        if text.trim().is_empty() {
            Selection::Nothing
        } else {
            Selection::Text(text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run with `cargo test -- --ignored --nocapture`. Reports what the app
    /// in front right now says about its selection. It only reads.
    #[test]
    #[ignore = "asks the focused app, and needs the Accessibility permission"]
    fn report_the_current_selection() {
        let found = read();
        let summary = match &found {
            Selection::Text(text) => format!("text selected, {} characters", text.chars().count()),
            Selection::Nothing => "the app answered: nothing selected".to_string(),
            Selection::Unknown => "the app could not be asked (or permission is missing)".to_string(),
        };
        eprintln!("selection: {summary}");
    }
}
