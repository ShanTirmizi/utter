//! The user's API key. It is stored only in the macOS Keychain: never in a
//! file, never in settings, never logged, and never sent back to the UI.

use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};
use std::sync::Mutex;

const SERVICE: &str = "dev.utter.app";
const ACCOUNT: &str = "anthropic-api-key";

/// Keychain "item not found" status code (errSecItemNotFound).
const NOT_FOUND: i32 = -25300;

/// Read the Keychain at most once per launch. `None` = not looked up yet,
/// `Some(None)` = looked up and absent.
static CACHE: Mutex<Option<Option<String>>> = Mutex::new(None);

pub fn set(key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("The key is empty.".into());
    }
    set_generic_password(SERVICE, ACCOUNT, key.as_bytes())
        .map_err(|e| format!("Could not save to the Keychain: {e}"))?;
    *CACHE.lock().unwrap() = Some(Some(key.to_string()));
    Ok(())
}

pub fn get() -> Option<String> {
    let mut cache = CACHE.lock().unwrap();
    if let Some(known) = cache.as_ref() {
        return known.clone();
    }
    let found = get_generic_password(SERVICE, ACCOUNT)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .filter(|k| !k.is_empty());
    *cache = Some(found.clone());
    found
}

pub fn exists() -> bool {
    get().is_some()
}

pub fn delete() -> Result<(), String> {
    match delete_generic_password(SERVICE, ACCOUNT) {
        Ok(()) => {}
        Err(e) if e.code() == NOT_FOUND => {}
        Err(e) => return Err(format!("Could not remove the key: {e}")),
    }
    *CACHE.lock().unwrap() = Some(None);
    Ok(())
}
