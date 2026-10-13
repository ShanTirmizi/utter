//! API keys. They are stored only in the macOS Keychain: never in a file,
//! never in settings, never logged, and never sent back to the UI. Each
//! provider has its own entry, so switching providers does not lose a key.

use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};
use std::{collections::HashMap, sync::Mutex};

const SERVICE: &str = "dev.utter.app";

/// Keychain "item not found" status code (errSecItemNotFound).
const NOT_FOUND: i32 = -25300;

/// Read each Keychain entry at most once per launch. A missing map entry
/// means "not looked up yet"; `None` means "looked up and absent".
static CACHE: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

fn account(provider: &str) -> String {
    format!("{provider}-api-key")
}

fn remember(provider: &str, value: Option<String>) {
    CACHE
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(provider.to_string(), value);
}

pub fn set(provider: &str, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("The key is empty.".into());
    }
    set_generic_password(SERVICE, &account(provider), key.as_bytes())
        .map_err(|e| format!("Could not save to the Keychain: {e}"))?;
    remember(provider, Some(key.to_string()));
    Ok(())
}

pub fn get(provider: &str) -> Option<String> {
    // Hold the lock across the read so a key saved at the same moment cannot
    // be overwritten by a stale "absent".
    let mut cache = CACHE.lock().unwrap();
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(known) = cache.get(provider) {
        return known.clone();
    }
    match get_generic_password(SERVICE, &account(provider)) {
        Ok(bytes) => {
            let key = String::from_utf8(bytes).ok().filter(|k| !k.is_empty());
            cache.insert(provider.to_string(), key.clone());
            key
        }
        Err(e) if e.code() == NOT_FOUND => {
            cache.insert(provider.to_string(), None);
            None
        }
        // Any other failure (the Keychain is locked, or the user dismissed
        // macOS's access prompt) is not remembered, so the next attempt asks
        // again instead of behaving as if there were no key until relaunch.
        Err(_) => None,
    }
}

pub fn exists(provider: &str) -> bool {
    get(provider).is_some()
}

pub fn delete(provider: &str) -> Result<(), String> {
    match delete_generic_password(SERVICE, &account(provider)) {
        Ok(()) => {}
        Err(e) if e.code() == NOT_FOUND => {}
        Err(e) => return Err(format!("Could not remove the key: {e}")),
    }
    remember(provider, None);
    Ok(())
}
