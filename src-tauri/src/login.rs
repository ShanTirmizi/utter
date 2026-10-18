//! Open at login, through macOS's own login items (`SMAppService`, macOS 13
//! and later). The answer always comes from the system rather than a saved
//! setting, so switching Utter off under System Settings › General › Login
//! Items shows here too.

use objc2::{
    msg_send,
    runtime::{AnyClass, AnyObject, Bool},
};
use serde::Serialize;

#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LoginItem {
    /// Not set to open at login.
    Off,
    /// Opens at login.
    On,
    /// Registered, but the user has to allow it in System Settings first.
    NeedsApproval,
    /// This Mac or this copy of Utter can't do it: macOS 12, or a build that
    /// isn't an installed app.
    Unavailable,
}

/// `SMAppService.mainAppService`, or None before macOS 13.
fn service() -> Option<*mut AnyObject> {
    let class = AnyClass::get(c"SMAppService")?;
    let service: *mut AnyObject = unsafe { msg_send![class, mainAppService] };
    (!service.is_null()).then_some(service)
}

pub fn status() -> LoginItem {
    let Some(service) = service() else {
        return LoginItem::Unavailable;
    };
    // SMAppServiceStatus: 0 not registered, 1 enabled, 2 requires approval, 3 not found.
    let status: isize = unsafe { msg_send![service, status] };
    match status {
        1 => LoginItem::On,
        2 => LoginItem::NeedsApproval,
        3 => LoginItem::Unavailable,
        _ => LoginItem::Off,
    }
}

/// Switch opening at login on or off, and report where that left it.
pub fn set(enabled: bool) -> Result<LoginItem, String> {
    let Some(service) = service() else {
        return Err("Opening at login needs macOS 13 or later.".into());
    };
    let mut error: *mut AnyObject = std::ptr::null_mut();
    let done: Bool = unsafe {
        if enabled {
            msg_send![service, registerAndReturnError: &mut error]
        } else {
            msg_send![service, unregisterAndReturnError: &mut error]
        }
    };
    let now = status();
    // Asking for what is already so is not a failure, whatever macOS says.
    let wanted = if enabled { now == LoginItem::On || now == LoginItem::NeedsApproval } else { now == LoginItem::Off };
    if done.as_bool() || wanted {
        return Ok(now);
    }
    if now == LoginItem::Unavailable {
        return Err("Only the installed Utter app can open at login.".into());
    }
    Err(describe(error).unwrap_or_else(|| "macOS didn’t allow it.".into()))
}

/// An NSError's own description, if there is one.
fn describe(error: *mut AnyObject) -> Option<String> {
    if error.is_null() {
        return None;
    }
    let text: *mut objc2_foundation::NSString = unsafe { msg_send![error, localizedDescription] };
    (!text.is_null()).then(|| unsafe { (*text).to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_login_items_service_is_linked() {
        // On macOS 13 or later the class is there; this catches the
        // framework not being linked, which would quietly read as Unavailable.
        let version = std::process::Command::new("sw_vers").arg("-productVersion").output().unwrap();
        let major: u32 = String::from_utf8_lossy(&version.stdout).split('.').next().unwrap().trim().parse().unwrap();
        if major >= 13 {
            assert!(AnyClass::get(c"SMAppService").is_some());
        }
        // A test binary isn't an app bundle, so it can't be registered, but
        // asking must not crash.
        let _ = status();
    }
}
