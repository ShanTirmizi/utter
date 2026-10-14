//! macOS privacy permissions Utter depends on, and how to check each one.
//!
//! - Microphone: to hear you while the hotkey is held.
//! - Accessibility: to press Cmd+V in the app you are using.
//! - Input Monitoring: NOT needed. The hotkey is a registered system hotkey,
//!   which macOS delivers without it. It would only be required to watch every
//!   keystroke (for example, to use a lone modifier key as the trigger).

use core_foundation::{
    base::TCFType,
    boolean::CFBoolean,
    dictionary::{CFDictionary, CFDictionaryRef},
    string::CFString,
};
use block2::RcBlock;
use objc2::{
    msg_send,
    runtime::{AnyClass, Bool},
};
use objc2_foundation::NSString;
use serde::Serialize;
use std::process::Command;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> u8;
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn IsSecureEventInputEnabled() -> u8;
}

#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {
    static AVMediaTypeAudio: &'static NSString;
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MicStatus {
    /// Never asked; macOS will prompt on first use.
    Undetermined,
    Granted,
    Denied,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    pub microphone: MicStatus,
    pub accessibility: bool,
}

pub fn microphone() -> MicStatus {
    let Some(class) = AnyClass::get(c"AVCaptureDevice") else {
        return MicStatus::Undetermined;
    };
    // AVAuthorizationStatus: 0 not determined, 1 restricted, 2 denied, 3 authorised.
    let status: isize =
        unsafe { msg_send![class, authorizationStatusForMediaType: AVMediaTypeAudio] };
    match status {
        3 => MicStatus::Granted,
        0 => MicStatus::Undetermined,
        _ => MicStatus::Denied,
    }
}

/// Ask macOS to show its microphone permission dialog. The answer is picked
/// up by the next `microphone()` check. If the user has already answered,
/// macOS shows nothing.
pub fn request_microphone() {
    let Some(class) = AnyClass::get(c"AVCaptureDevice") else {
        return;
    };
    let on_answer = RcBlock::new(|_granted: Bool| {});
    let _: () = unsafe {
        msg_send![
            class,
            requestAccessForMediaType: AVMediaTypeAudio,
            completionHandler: &*on_answer
        ]
    };
}

pub fn accessibility() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

/// Ask macOS to show its "would like to control this computer" dialog, which
/// links to the right System Settings pane. Does nothing if already trusted.
pub fn prompt_accessibility() -> bool {
    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let options =
        CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) != 0 }
}

/// Remove every Accessibility entry macOS holds for this app.
///
/// macOS ties the permission to the app's code signature. An app without a
/// Developer ID gets a new signature on every build, so after an update the
/// switch can show as on in System Settings while the running app is still
/// refused. Clearing the entry lets the user grant it afresh.
pub fn reset_accessibility(bundle_id: &str) -> Result<(), String> {
    let status = Command::new("/usr/bin/tccutil")
        .args(["reset", "Accessibility", bundle_id])
        .status()
        .map_err(|e| format!("Could not run tccutil: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("macOS would not reset the permission.".into())
    }
}

/// True while a password field (or similar) has locked keyboard input. Utter
/// does not paste in that state.
pub fn secure_input_active() -> bool {
    unsafe { IsSecureEventInputEnabled() != 0 }
}

pub fn status() -> PermissionStatus {
    PermissionStatus {
        microphone: microphone(),
        accessibility: accessibility(),
    }
}

/// Open the matching pane of System Settings > Privacy & Security.
pub fn open_settings(pane: &str) -> Result<(), String> {
    if pane == "keyboard" {
        return Command::new("open")
            .arg("x-apple.systempreferences:com.apple.Keyboard-Settings.extension")
            .status()
            .map(|_| ())
            .map_err(|e| e.to_string());
    }
    let anchor = match pane {
        "microphone" => "Privacy_Microphone",
        "accessibility" => "Privacy_Accessibility",
        other => return Err(format!("Unknown settings pane: {other}")),
    };
    Command::new("open")
        .arg(format!(
            "x-apple.systempreferences:com.apple.preference.security?{anchor}"
        ))
        .status()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
