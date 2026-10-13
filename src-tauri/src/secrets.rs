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
    if let Some(known) = CACHE.lock().unwrap().as_ref().and_then(|m| m.get(provider)) {
        return known.clone();
    }
    let found = get_generic_password(SERVICE, &account(provider))
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .filter(|k| !k.is_empty());
    remember(provider, found.clone());
    found
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
