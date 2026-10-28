//! The text currently selected in the focused app.
//!
//! Used by "rewrite selected text": select something, hold the rewrite
//! hotkey, and say what to do with it. The selection is read through the
//! Accessibility API where the app supports that, and otherwise by copying
//! it, the way a person would.

use core_foundation::{
    base::{CFEqual, CFGetTypeID, CFRelease, CFTypeRef, TCFType},
    boolean::CFBoolean,
    string::{CFString, CFStringGetTypeID, CFStringRef},
    url::{CFURL, CFURLGetTypeID, CFURLRef},
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
    fn AXUIElementIsAttributeSettable(element: CFTypeRef, attribute: CFStringRef, settable: *mut u8) -> i32;
    fn AXUIElementSetMessagingTimeout(element: CFTypeRef, seconds: f32) -> i32;
    fn AXValueGetTypeID() -> core_foundation::base::CFTypeID;
    fn AXValueGetValue(value: CFTypeRef, kind: u32, out: *mut std::ffi::c_void) -> bool;
}

/// `kAXValueTypeCFRange`: an AXValue holding a CFRange.
const AX_VALUE_CF_RANGE: u32 = 4;

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

/// Roles of focused elements that hold no text and take none from a paste:
/// a web page with no box selected, lists of files or messages, buttons.
/// Tables are left out, since a spreadsheet pastes into its cells.
const NOT_TEXT: &[&str] = &[
    "AXWebArea", "AXList", "AXOutline", "AXBrowser", "AXButton", "AXImage", "AXLink", "AXCheckBox",
    "AXRadioButton", "AXPopUpButton", "AXMenuButton", "AXSlider", "AXTabGroup", "AXToolbar",
];

/// Is there plainly nowhere to type? True only when the app in front says
/// what has focus and it is something that holds no text: part of a web page
/// that is not editable, a list of files or messages, a button. Also when
/// Finder is in front with no name being edited. An app that will not say
/// gets the benefit of the doubt: a paste is never held back on a guess.
pub fn nowhere_to_type() -> bool {
    let finder = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .and_then(|app| app.bundleIdentifier())
        .is_some_and(|id| id.to_string() == "com.apple.finder");
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return false;
        }
        AXUIElementSetMessagingTimeout(system, TIMEOUT_SECONDS);
        let focused = copy(system, "AXFocusedUIElement");
        CFRelease(system);
        if focused.is_null() {
            #[cfg(debug_assertions)]
            eprintln!("[focus] nothing focused (finder: {finder})");
            // The desktop, with nothing of Finder's open.
            return finder;
        }
        AXUIElementSetMessagingTimeout(focused, TIMEOUT_SECONDS);
        let role = text_of(copy(focused, "AXRole")).unwrap_or_default();
        let has = |attribute| {
            let value = copy(focused, attribute);
            if !value.is_null() {
                CFRelease(value);
            }
            !value.is_null()
        };
        let mut settable = 0u8;
        let name = CFString::from_static_string("AXValue");
        let value_settable =
            AXUIElementIsAttributeSettable(focused, name.as_concrete_TypeRef(), &mut settable) == 0 && settable != 0;
        let text_role = matches!(role.as_str(), "AXTextField" | "AXTextArea" | "AXComboBox");
        let editable_ancestor = has("AXEditableAncestor");
        let cursor = has("AXSelectedTextRange");
        let in_page = in_web_page(focused);
        // Browsers give every part of a page a text range, editable or not,
        // so on a page only an editable part counts. Elsewhere, a cursor
        // means text goes here.
        let texty = text_role || value_settable || editable_ancestor || (cursor && !in_page);
        // A browser that has not described the page yet may name the page
        // when a box in it has focus. Only a page it has described counts.
        let described = role != "AXWebArea" || {
            let children = copy(focused, "AXChildren");
            let any = !children.is_null()
                && CFGetTypeID(children) == core_foundation::array::CFArrayGetTypeID()
                && core_foundation::array::CFArrayGetCount(children as _) > 0;
            if !children.is_null() {
                CFRelease(children);
            }
            any
        };
        #[cfg(debug_assertions)]
        eprintln!(
            "[focus] role {role} text role {text_role} value settable {value_settable} editable ancestor {editable_ancestor} cursor {cursor} in page {in_page} described {described} finder {finder}"
        );
        CFRelease(focused);
        !texty && described && (finder || in_page || NOT_TEXT.contains(&role.as_str()))
    }
}

/// Is `element` a web page, or part of one? Looks a few dozen levels up.
unsafe fn in_web_page(element: CFTypeRef) -> bool {
    unsafe {
        if text_of(copy(element, "AXRole")).as_deref() == Some("AXWebArea") {
            return true;
        }
        let mut current = copy(element, "AXParent");
        for _ in 0..40 {
            if current.is_null() {
                return false;
            }
            AXUIElementSetMessagingTimeout(current, TIMEOUT_SECONDS);
            let role = text_of(copy(current, "AXRole"));
            if role.as_deref() == Some("AXWebArea") {
                CFRelease(current);
                return true;
            }
            let parent = copy(current, "AXParent");
            CFRelease(current);
            current = parent;
            if role.as_deref() == Some("AXWindow") {
                break;
            }
        }
        if !current.is_null() {
            CFRelease(current);
        }
        false
    }
}

/// The web page the cursor is in, as far as the browser will say.
#[derive(Debug, Default, PartialEq)]
pub struct Page {
    pub url: Option<String>,
    pub title: Option<String>,
}

/// Ask the browser in front which page is being typed into: the address of
/// the web area holding the focused field, and the window's title. Needs the
/// Accessibility permission; without it, or in an app that will not say,
/// both are empty. Nothing is kept: the caller turns this into a site name.
pub fn focused_page() -> Page {
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return Page::default();
        }
        AXUIElementSetMessagingTimeout(system, TIMEOUT_SECONDS);
        let focused = copy(system, "AXFocusedUIElement");
        let app = copy(system, "AXFocusedApplication");
        CFRelease(system);

        let mut page = Page::default();
        // Climb from the focused field to the web page that holds it.
        let mut element = focused;
        for _ in 0..40 {
            if element.is_null() {
                break;
            }
            AXUIElementSetMessagingTimeout(element, TIMEOUT_SECONDS);
            if text_of(copy(element, "AXRole")).as_deref() == Some("AXWebArea") {
                page.url = text_of(copy(element, "AXURL"));
                CFRelease(element);
                element = std::ptr::null();
                break;
            }
            let parent = copy(element, "AXParent");
            CFRelease(element);
            element = parent;
        }
        if !element.is_null() {
            CFRelease(element);
        }

        if !app.is_null() {
            AXUIElementSetMessagingTimeout(app, TIMEOUT_SECONDS);
            let window = copy(app, "AXFocusedWindow");
            CFRelease(app);
            if !window.is_null() {
                AXUIElementSetMessagingTimeout(window, TIMEOUT_SECONDS);
                page.title = text_of(copy(window, "AXTitle"));
                if page.url.is_none() {
                    // Safari also gives the address as the window's document.
                    page.url = text_of(copy(window, "AXDocument"));
                }
                CFRelease(window);
            }
        }
        page
    }
}

/// The text box the cursor is in, held so it can be read again later. Used
/// to notice a word the user corrects in what Utter just pasted. Lives on
/// one thread: it is made, read and dropped there.
pub struct Field(CFTypeRef);

impl Field {
    /// The focused element of the focused app, unless it is a password field.
    pub fn focused() -> Option<Field> {
        unsafe {
            let system = AXUIElementCreateSystemWide();
            if system.is_null() {
                return None;
            }
            AXUIElementSetMessagingTimeout(system, TIMEOUT_SECONDS);
            let focused = copy(system, "AXFocusedUIElement");
            CFRelease(system);
            if focused.is_null() {
                return None;
            }
            AXUIElementSetMessagingTimeout(focused, TIMEOUT_SECONDS);
            let field = Field(focused);
            if text_of(copy(focused, "AXSubrole")).as_deref() == Some("AXSecureTextField") {
                return None;
            }
            Some(field)
        }
    }

    /// Everything the box holds, if it will say.
    pub fn value(&self) -> Option<String> {
        unsafe {
            let value = copy(self.0, "AXValue");
            if value.is_null() || CFGetTypeID(value) != CFStringGetTypeID() {
                if !value.is_null() {
                    CFRelease(value);
                }
                return None;
            }
            Some(CFString::wrap_under_create_rule(value as CFStringRef).to_string())
        }
    }

    /// Is this the same box as `other`?
    pub fn same_as(&self, other: &Field) -> bool {
        unsafe { CFEqual(self.0, other.0) != 0 }
    }
}

impl Drop for Field {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

/// Text visible in the focused window: its title, and what its labels, text
/// boxes, headings and links say, read through the Accessibility API within
/// a small budget of elements, time and length. Used to find names on screen
/// that the user may be about to say. Nothing here is stored.
pub fn window_text() -> String {
    const MOST_ELEMENTS: usize = 800;
    const MOST_CHARS: usize = 30_000;
    const MOST_TIME: std::time::Duration = std::time::Duration::from_millis(600);
    let began = std::time::Instant::now();
    let mut out = String::new();
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return out;
        }
        AXUIElementSetMessagingTimeout(system, TIMEOUT_SECONDS);
        let app = copy(system, "AXFocusedApplication");
        CFRelease(system);
        if app.is_null() {
            return out;
        }
        AXUIElementSetMessagingTimeout(app, TIMEOUT_SECONDS);
        let window = copy(app, "AXFocusedWindow");
        CFRelease(app);
        if window.is_null() {
            return out;
        }
        // Breadth first, so the budget is spent on the window's main parts
        // before the depths of any one of them.
        let mut queue = std::collections::VecDeque::from([window]);
        let mut seen = 0;
        while let Some(element) = queue.pop_front() {
            seen += 1;
            if seen > MOST_ELEMENTS || out.len() > MOST_CHARS || began.elapsed() > MOST_TIME {
                CFRelease(element);
                continue;
            }
            AXUIElementSetMessagingTimeout(element, 0.1);
            own_text(element, &mut out);
            queue.extend(children(element));
            CFRelease(element);
        }
    }
    out
}

/// What one element says (see `window_text`), added to `out` a line at a
/// time. A password field says nothing.
unsafe fn own_text(element: CFTypeRef, out: &mut String) {
    unsafe {
        let role = text_of(copy(element, "AXRole")).unwrap_or_default();
        let wanted: &[&str] = match role.as_str() {
            "AXStaticText" | "AXTextField" | "AXTextArea" | "AXComboBox" => &["AXValue"],
            "AXWindow" | "AXHeading" | "AXLink" | "AXButton" | "AXTab" | "AXRadioButton" | "AXMenuButton"
            | "AXCell" | "AXRow" | "AXGroup" | "AXWebArea" => &["AXTitle", "AXDescription"],
            _ => &[],
        };
        let secure = role == "AXTextField"
            && text_of(copy(element, "AXSubrole")).as_deref() == Some("AXSecureTextField");
        if secure {
            return;
        }
        for attribute in wanted {
            if let Some(text) = text_of(copy(element, attribute)) {
                let text = text.trim();
                if !text.is_empty() {
                    out.push_str(text);
                    out.push('\n');
                }
            }
        }
    }
}

/// An element's children, each owned by the caller, in order.
unsafe fn children(element: CFTypeRef) -> Vec<CFTypeRef> {
    let mut found = Vec::new();
    unsafe {
        let list = copy(element, "AXChildren");
        if list.is_null() {
            return found;
        }
        if CFGetTypeID(list) == core_foundation::array::CFArrayGetTypeID() {
            let array = list as core_foundation::array::CFArrayRef;
            for i in 0..core_foundation::array::CFArrayGetCount(array) {
                let child = core_foundation::array::CFArrayGetValueAtIndex(array, i) as CFTypeRef;
                if !child.is_null() {
                    core_foundation::base::CFRetain(child);
                    found.push(child);
                }
            }
        }
        CFRelease(list);
    }
    found
}

/// The text around where the user is typing, read when a dictation starts.
/// Only ever used for that one dictation, and never stored.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Around {
    /// What the focused text box holds before the cursor, at most
    /// `most_before` characters of it.
    pub before: String,
    /// Text near the box on screen (the conversation above a chat box, say),
    /// at most `most_nearby` characters, mostly from before the box.
    pub nearby: String,
}

/// Read the text before the cursor and the text near the focused box. An app
/// that will not say, or a password field, gives nothing.
pub fn around_cursor(most_before: usize, most_nearby: usize) -> Around {
    const MOST_TIME: std::time::Duration = std::time::Duration::from_millis(500);
    const MOST_ELEMENTS: usize = 600;
    const MOST_LEVELS: usize = 25;
    let began = std::time::Instant::now();
    let mut around = Around::default();
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return around;
        }
        AXUIElementSetMessagingTimeout(system, TIMEOUT_SECONDS);
        let focused = copy(system, "AXFocusedUIElement");
        CFRelease(system);
        if focused.is_null() {
            return around;
        }
        AXUIElementSetMessagingTimeout(focused, TIMEOUT_SECONDS);
        if text_of(copy(focused, "AXSubrole")).as_deref() == Some("AXSecureTextField") {
            CFRelease(focused);
            return around;
        }
        if let Some(value) = text_of(copy(focused, "AXValue")) {
            let cursor = cursor_at(focused);
            around.before = last_chars(&before_cursor(&value, cursor), most_before);
        }

        // The box's ancestors, nearest first, up to its window.
        let mut ancestors = Vec::new();
        let mut element = copy(focused, "AXParent");
        while !element.is_null() {
            if ancestors.len() >= MOST_LEVELS {
                // Too deep to be worth reading; this one is not kept.
                CFRelease(element);
                break;
            }
            AXUIElementSetMessagingTimeout(element, 0.1);
            let is_window = text_of(copy(element, "AXRole")).as_deref() == Some("AXWindow");
            ancestors.push(element);
            if is_window {
                break;
            }
            element = copy(element, "AXParent");
        }

        // Read ever larger parts of the window around the box until there is
        // enough text, or no time left.
        for ancestor in &ancestors {
            if began.elapsed() > MOST_TIME {
                break;
            }
            let (text, at) = read_in_order(*ancestor, focused, MOST_ELEMENTS, began, MOST_TIME);
            around.nearby = excerpt(&text, at, most_nearby);
            if text.chars().count() >= most_nearby * 2 {
                break;
            }
        }
        for ancestor in ancestors {
            CFRelease(ancestor);
        }
        CFRelease(focused);
    }
    around
}

/// Where the cursor is in a text box, in UTF-16 units, if it will say.
unsafe fn cursor_at(field: CFTypeRef) -> Option<usize> {
    unsafe {
        let value = copy(field, "AXSelectedTextRange");
        if value.is_null() {
            return None;
        }
        let mut range = core_foundation::base::CFRange { location: 0, length: 0 };
        let ok = CFGetTypeID(value) == AXValueGetTypeID()
            && AXValueGetValue(value, AX_VALUE_CF_RANGE, &mut range as *mut _ as *mut std::ffi::c_void);
        CFRelease(value);
        (ok && range.location >= 0).then_some(range.location as usize)
    }
}

/// The text of a box before the cursor at `cursor` (UTF-16 units, as macOS
/// counts). Without a cursor, the whole text: it is usually at the end.
fn before_cursor(value: &str, cursor: Option<usize>) -> String {
    match cursor {
        Some(at) => String::from_utf16_lossy(&value.encode_utf16().take(at).collect::<Vec<_>>()),
        None => value.to_string(),
    }
}

/// The last `most` characters of `text`, from the start of a line or word.
fn last_chars(text: &str, most: usize) -> String {
    let count = text.chars().count();
    if count <= most {
        return text.to_string();
    }
    let tail: String = text.chars().skip(count - most).collect();
    // Start at a whole word, unless the cut came right after one.
    let at_word = text.chars().nth(count - most - 1).is_some_and(char::is_whitespace);
    match tail.find(char::is_whitespace) {
        Some(space) if !at_word => tail[space..].trim_start().to_string(),
        _ => tail,
    }
}

/// About `most` characters of `text` around position `at` (in characters),
/// four fifths from before it: what is above a chat box matters most.
fn excerpt(text: &str, at: usize, most: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let at = at.min(chars.len());
    let before_budget = most * 4 / 5;
    let after = (chars.len() - at).min(most - before_budget.min(at));
    let start = at - at.min(most - after);
    chars[start..at + after].iter().collect::<String>().trim().to_string()
}

/// The text of `root` and everything in it, in reading order, leaving out
/// `skip` (the box being typed in), and where in the text `skip` sits.
unsafe fn read_in_order(
    root: CFTypeRef,
    skip: CFTypeRef,
    most_elements: usize,
    began: std::time::Instant,
    most_time: std::time::Duration,
) -> (String, usize) {
    let mut out = String::new();
    let mut at = None;
    unsafe {
        core_foundation::base::CFRetain(root);
        let mut stack = vec![root];
        let mut seen = 0;
        while let Some(element) = stack.pop() {
            seen += 1;
            let over = seen > most_elements || began.elapsed() > most_time;
            if over || CFEqual(element, skip) != 0 {
                if !over {
                    at = Some(out.chars().count());
                }
                CFRelease(element);
                continue;
            }
            AXUIElementSetMessagingTimeout(element, 0.1);
            own_text(element, &mut out);
            let mut kids = children(element);
            kids.reverse();
            stack.extend(kids);
            CFRelease(element);
        }
    }
    let length = out.chars().count();
    (out, at.unwrap_or(length))
}

/// One attribute of an accessibility element, owned by the caller, or null.
unsafe fn copy(element: CFTypeRef, attribute: &'static str) -> CFTypeRef {
    let name = CFString::from_static_string(attribute);
    let mut value: CFTypeRef = std::ptr::null();
    let status = unsafe { AXUIElementCopyAttributeValue(element, name.as_concrete_TypeRef(), &mut value) };
    if status != 0 { std::ptr::null() } else { value }
}

/// A string or web address as text, releasing the value either way.
unsafe fn text_of(value: CFTypeRef) -> Option<String> {
    if value.is_null() {
        return None;
    }
    unsafe {
        let kind = CFGetTypeID(value);
        if kind == CFStringGetTypeID() {
            Some(CFString::wrap_under_create_rule(value as CFStringRef).to_string())
        } else if kind == CFURLGetTypeID() {
            Some(CFURL::wrap_under_create_rule(value as CFURLRef).get_string().to_string())
        } else {
            CFRelease(value);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_before_the_cursor_counts_as_macos_does() {
        // macOS counts in UTF-16 units: the emoji is two of them.
        assert_eq!(before_cursor("Hi 👋 there", Some(5)), "Hi 👋");
        assert_eq!(before_cursor("Hi there", None), "Hi there");
        assert_eq!(last_chars("one two three four", 9), "four");
        assert_eq!(last_chars("short", 100), "short");
    }

    #[test]
    fn the_excerpt_is_mostly_from_before_the_box() {
        let text: String = ('a'..='z').cycle().take(1000).collect();
        let around = excerpt(&text, 900, 100);
        assert_eq!(around.chars().count(), 100);
        assert_eq!(around, text.chars().skip(820).take(100).collect::<String>());
        // Near the end, the rest of the budget goes before it.
        assert_eq!(excerpt(&text, 1000, 100), text.chars().skip(900).collect::<String>());
        assert_eq!(excerpt("short", 5, 100), "short");
        assert_eq!(excerpt("", 0, 100), "");
    }

    /// Run with `cargo test -- --ignored --nocapture`. Prints what Utter would
    /// read around the cursor in the app in front a few seconds from now.
    #[test]
    #[ignore = "reads the focused app, and needs the Accessibility permission"]
    fn report_the_text_around_the_cursor() {
        std::thread::sleep(std::time::Duration::from_secs(3));
        let started = std::time::Instant::now();
        let around = around_cursor(600, 1500);
        eprintln!("took {:?}\nBEFORE: {:?}\nNEARBY: {}", started.elapsed(), around.before, around.nearby);
    }

    /// Run with `cargo test -- --ignored --nocapture`. Says whether the app
    /// in front a few seconds from now has anywhere to type. It only reads.
    #[test]
    #[ignore = "asks the focused app, and needs the Accessibility permission"]
    fn report_whether_there_is_anywhere_to_type() {
        std::thread::sleep(std::time::Duration::from_secs(3));
        eprintln!("nowhere to type: {}", nowhere_to_type());
    }

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
