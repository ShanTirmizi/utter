//! Put text into whatever app has focus.
//!
//! The dependable way to do this across every kind of app is the clipboard:
//! remember what is on it, place our text there, press Cmd+V, then put the
//! original contents back. Typing character by character is slow and breaks on
//! non-Latin text; writing through accessibility APIs fails in many apps.
//!
//! Pasteboard calls here must run on the main thread.

use core_graphics::{
    event::{CGEvent, CGEventFlags, CGEventTapLocation},
    event_source::{CGEventSource, CGEventSourceStateID},
};
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString};

/// Virtual key code for "V" on every Mac keyboard layout's physical V key.
const KEY_V: u16 = 9;

/// Marker types from the nspasteboard.org convention. Clipboard managers that
/// honour it will not record the dictated text in their history.
const MARKERS: [&str; 2] = [
    "org.nspasteboard.ConcealedType",
    "org.nspasteboard.TransientType",
];

/// Everything that was on the clipboard, so it can be put back afterwards.
pub struct Saved {
    /// One entry per clipboard item; each is a list of (type, bytes) pairs.
    items: Vec<Vec<(Retained<NSString>, Retained<NSData>)>>,
}

/// Snapshot the clipboard, including images, files and rich text.
pub fn save_clipboard() -> Saved {
    let pasteboard = NSPasteboard::generalPasteboard();
    let mut items = Vec::new();
    if let Some(existing) = pasteboard.pasteboardItems() {
        for item in existing.iter() {
            let mut entries = Vec::new();
            for kind in item.types().iter() {
                if let Some(data) = item.dataForType(&kind) {
                    entries.push((kind, data));
                }
            }
            if !entries.is_empty() {
                items.push(entries);
            }
        }
    }
    Saved { items }
}

/// Replace the clipboard with `text` and return the pasteboard's change count,
/// which identifies this exact write.
pub fn set_clipboard_text(text: &str, concealed: bool) -> isize {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    unsafe {
        pasteboard.setString_forType(&NSString::from_str(text), NSPasteboardTypeString);
    }
    if concealed {
        for marker in MARKERS {
            pasteboard.setString_forType(&NSString::from_str(""), &NSString::from_str(marker));
        }
    }
    pasteboard.changeCount()
}

/// Put the saved contents back, but only if the clipboard still holds what we
/// wrote. If the user copied something else in the meantime, theirs wins.
pub fn restore_clipboard(saved: Saved, our_change_count: isize) {
    let pasteboard = NSPasteboard::generalPasteboard();
    if pasteboard.changeCount() != our_change_count {
        return;
    }
    pasteboard.clearContents();
    if saved.items.is_empty() {
        return;
    }
    let mut objects: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = Vec::new();
    for entries in saved.items {
        let item = NSPasteboardItem::new();
        for (kind, data) in entries {
            item.setData_forType(&data, &kind);
        }
        objects.push(ProtocolObject::from_retained(item));
    }
    pasteboard.writeObjects(&NSArray::from_retained_slice(&objects));
}

/// Press and release Cmd+V. Requires the Accessibility permission; without it
/// macOS silently drops the events.
pub fn press_paste() -> Result<(), String> {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| "Could not create a keyboard event source.".to_string())?;
    for key_down in [true, false] {
        let event = CGEvent::new_keyboard_event(source.clone(), KEY_V, key_down)
            .map_err(|_| "Could not create a keyboard event.".to_string())?;
        // Set the modifiers explicitly so keys the user is still holding from
        // the hotkey do not turn this into a different shortcut.
        event.set_flags(CGEventFlags::CGEventFlagCommand);
        event.post(CGEventTapLocation::HID);
    }
    Ok(())
}
