//! The services that can run the cleanup step.
//!
//! Two wire formats cover practically every hosted or local model: Anthropic's
//! Messages API, and the OpenAI-style chat completions API that most other
//! providers (and local runners such as Ollama) also speak. A provider here is
//! just a name, a base URL and which of the two formats to use.

use crate::{secrets, settings::Settings};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Anthropic,
    /// OpenAI-style `/chat/completions`.
    OpenAi,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: &'static str,
    pub label: &'static str,
    pub protocol: Protocol,
    /// Empty for `custom`, where the user supplies it.
    pub base_url: &'static str,
    /// Empty when the user must pick one; model names change too often to
    /// hard-code for every provider, so the app lists them live instead.
    pub default_model: &'static str,
    pub needs_key: bool,
    /// Where to create a key, for the "Get a key" link.
    pub key_url: &'static str,
}

pub const CATALOG: [Provider; 7] = [
    Provider {
        id: "anthropic",
        label: "Anthropic (Claude)",
        protocol: Protocol::Anthropic,
        base_url: "https://api.anthropic.com/v1",
        default_model: "claude-haiku-5-5",
        needs_key: true,
        key_url: "https://console.anthropic.com/settings/keys",
    },
    Provider {
        id: "openai",
        label: "OpenAI",
        protocol: Protocol::OpenAi,
        base_url: "https://api.openai.com/v1",
        default_model: "",
        needs_key: true,
        key_url: "https://platform.openai.com/api-keys",
    },
    Provider {
        id: "gemini",
        label: "Google Gemini",
        protocol: Protocol::OpenAi,
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        default_model: "",
        needs_key: true,
        key_url: "https://aistudio.google.com/apikey",
    },
    Provider {
        id: "groq",
        label: "Groq",
        protocol: Protocol::OpenAi,
        base_url: "https://api.groq.com/openai/v1",
        default_model: "",
        needs_key: true,
        key_url: "https://console.groq.com/keys",
    },
    Provider {
        id: "openrouter",
        label: "OpenRouter",
        protocol: Protocol::OpenAi,
        base_url: "https://openrouter.ai/api/v1",
        default_model: "",
        needs_key: true,
        key_url: "https://openrouter.ai/keys",
    },
    Provider {
        id: "ollama",
        label: "Ollama (on this Mac, no key)",
        protocol: Protocol::OpenAi,
        base_url: "http://localhost:11434/v1",
        default_model: "",
        needs_key: false,
        key_url: "https://ollama.com",
    },
    Provider {
        id: "custom",
        label: "Other (OpenAI-compatible)",
        protocol: Protocol::OpenAi,
        base_url: "",
        default_model: "",
        needs_key: true,
        key_url: "",
    },
];

pub fn find(id: &str) -> Option<Provider> {
    CATALOG.iter().copied().find(|p| p.id == id)
}

/// Everything needed to make one request: where, how, which model, which key.
#[derive(Debug, Clone)]
pub struct Endpoint {
    /// True for a model server running on this Mac (Ollama).
    pub local: bool,
    pub protocol: Protocol,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

/// Why cleanup cannot run with the current settings.
#[derive(Debug, PartialEq)]
pub enum NotReady {
    /// The normal state before a key is added. Not worth a warning each time.
    NoKey,
    /// Something the user should fix; the text says what.
    Problem(String),
}

/// A base URL is acceptable if it is HTTPS, or plain HTTP to this Mac only.
/// A key must never travel over the network unencrypted.
pub fn check_base_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("Enter the service's base URL, for example https://api.example.com/v1".into());
    }
    let wrong = "The base URL must start with https:// (plain http is allowed only for localhost).";
    // Parse rather than compare prefixes: "http://localhost:x@evil.example"
    // starts like a local address but is not one.
    let parsed = reqwest::Url::parse(url).map_err(|_| wrong.to_string())?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("The base URL must not contain a user name or password.".into());
    }
    let host = parsed.host_str().unwrap_or("");
    let local = matches!(host, "localhost" | "127.0.0.1" | "[::1]");
    match parsed.scheme() {
        "https" if !host.is_empty() => {}
        "http" if local => {}
        _ => return Err(wrong.to_string()),
    }
    Ok(parsed.as_str().trim_end_matches('/').to_string())
}

/// The host a base URL points at, for telling when a custom provider has
/// been repointed somewhere else. Empty if the URL does not parse.
pub fn host_of(url: &str) -> String {
    reqwest::Url::parse(url.trim())
        .ok()
        .and_then(|u| u.host_str().map(str::to_lowercase))
        .unwrap_or_default()
}

/// Turn settings into a usable endpoint, without touching the Keychain. The
/// key is passed in so this stays easy to test.
pub fn endpoint(settings: &Settings, api_key: Option<String>) -> Result<Endpoint, NotReady> {
    let provider = find(&settings.provider)
        .ok_or_else(|| NotReady::Problem(format!("Unknown provider: {}", settings.provider)))?;

    let base_url = if provider.base_url.is_empty() {
        check_base_url(&settings.base_url).map_err(NotReady::Problem)?
    } else {
        provider.base_url.to_string()
    };

    if provider.needs_key && api_key.is_none() {
        return Err(NotReady::NoKey);
    }

    let chosen = settings.cleanup_model.trim();
    let model = if chosen.is_empty() { provider.default_model } else { chosen };
    if model.is_empty() {
        return Err(NotReady::Problem("Choose a cleanup model in settings.".into()));
    }

    Ok(Endpoint {
        local: provider.id == "ollama",
        protocol: provider.protocol,
        base_url,
        model: model.to_string(),
        api_key: if provider.needs_key { api_key } else { None },
    })
}

/// As `endpoint`, reading the key from the Keychain. This can block on a
/// system prompt, so call it off the async threads.
pub fn resolve(settings: &Settings) -> Result<Endpoint, NotReady> {
    endpoint(settings, secrets::get(&settings.provider))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(provider: &str, model: &str, base_url: &str) -> Settings {
        Settings {
            provider: provider.into(),
            cleanup_model: model.into(),
            base_url: base_url.into(),
            ..Settings::default()
        }
    }

    #[test]
    fn anthropic_works_with_just_a_key() {
        let e = endpoint(&settings("anthropic", "", ""), Some("k".into())).unwrap();
        assert_eq!(e.protocol, Protocol::Anthropic);
        assert_eq!(e.model, "claude-haiku-5-5");
    }

    #[test]
    fn a_missing_key_is_not_reported_as_a_problem() {
        assert_eq!(endpoint(&settings("openai", "x", ""), None).unwrap_err(), NotReady::NoKey);
    }

    #[test]
    fn other_providers_need_a_model_to_be_chosen() {
        let err = endpoint(&settings("openai", "  ", ""), Some("k".into())).unwrap_err();
        assert!(matches!(err, NotReady::Problem(_)));
        let ok = endpoint(&settings("openai", "some-model", ""), Some("k".into())).unwrap();
        assert_eq!(ok.model, "some-model");
        assert_eq!(ok.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn ollama_needs_no_key_and_never_sends_one() {
        let e = endpoint(&settings("ollama", "local-model", ""), Some("stray".into())).unwrap();
        assert_eq!(e.api_key, None);
        assert!(endpoint(&settings("ollama", "local-model", ""), None).is_ok());
    }

    #[test]
    fn custom_base_urls_must_be_https_or_local() {
        assert_eq!(check_base_url("https://api.example.com/v1/").unwrap(), "https://api.example.com/v1");
        assert!(check_base_url("http://localhost:1234/v1").is_ok());
        assert!(check_base_url("http://127.0.0.1:8080").is_ok());
        assert!(check_base_url("http://api.example.com/v1").is_err());
        assert!(check_base_url("http://localhost.evil.com/v1").is_err());
        // Looks local, but everything before the @ is a user name.
        assert!(check_base_url("http://localhost:x@evil.example/v1").is_err());
        assert!(check_base_url("http://127.0.0.1:1@evil.example").is_err());
        assert!(check_base_url("https://user:pw@api.example.com/v1").is_err());
        assert!(check_base_url("HTTPS://api.example.com/v1").is_ok());
        assert!(check_base_url("http://[::1]:8080/v1").is_ok());
        assert!(check_base_url("ftp://api.example.com").is_err());
        assert_eq!(host_of("https://API.example.com/v1"), "api.example.com");
        assert_eq!(host_of("not a url"), "");
        assert!(check_base_url("").is_err());
        assert!(check_base_url("https://").is_err());
    }

    #[test]
    fn every_built_in_base_url_passes_the_same_check() {
        for p in CATALOG.iter().filter(|p| !p.base_url.is_empty()) {
            assert!(check_base_url(p.base_url).is_ok(), "{}", p.id);
        }
    }
}
