//! Names and terms on screen.
//!
//! When a dictation starts, Utter reads the text of the window in front (its
//! title, labels, text boxes, headings and links) and picks out what looks
//! like a name or term: words with odd capitals or digits (fetchUserProfile,
//! iPhone, GPT4), snake_case identifiers, and capitalised words that are not
//! everyday English (Raghunathan). For that one dictation they are hints, for
//! the speech model and for cleanup, so a name on screen is spelled the way
//! the screen spells it. The window's text itself is never kept.
//!
//! A term that then turns up in what the user said is remembered as a
//! suggestion for Your words, never added on its own.

use crate::words::{self, Word};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Most terms taken from one window.
const MOST_TERMS: usize = 40;
/// Most suggestions kept; the oldest go first.
const MOST_SUGGESTIONS: usize = 30;

/// Does this look like a name or a term rather than an ordinary word?
/// `everyday` holds the lower-case words known to be ordinary.
fn looks_like_a_name(token: &str, everyday: &std::collections::HashSet<String>) -> bool {
    let chars: Vec<char> = token.chars().collect();
    let length = chars.len();
    if !(3..=40).contains(&length) || !chars.iter().any(|c| c.is_alphabetic()) {
        return false;
    }
    let has_digit = chars.iter().any(|c| c.is_ascii_digit());
    // A commit hash or an id, not a word anyone says.
    if has_digit && length >= 7 && chars.iter().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    if has_digit {
        return chars.iter().filter(|c| c.is_alphabetic()).count() >= 2;
    }
    // Capitals inside the word: camelCase, iPhone, macOS.
    if chars.windows(2).any(|pair| pair[0].is_lowercase() && pair[1].is_uppercase()) {
        return true;
    }
    if token.contains('_') {
        return chars.iter().filter(|c| c.is_alphabetic()).count() >= 3;
    }
    // A capitalised word or an acronym that is not an everyday word.
    chars[0].is_uppercase() && !everyday.contains(&token.to_lowercase())
}

/// Split text into word-like tokens: letters, digits and underscores, and
/// the hyphens and dots inside them.
fn tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '\''))
        .map(|t| t.trim_matches(|c: char| !(c.is_alphanumeric() || c == '_')))
        .filter(|t| !t.is_empty())
}

/// The names and terms in `text`, the most frequent first, leaving out the
/// user's own words and anything they removed. Part of a web address, an
/// email address or a file name is not a term.
pub fn terms(text: &str, mine: &[Word], forgotten: &[String]) -> Vec<String> {
    let candidates: Vec<&str> = text
        .split_whitespace()
        .filter(|chunk| !(chunk.contains("://") || chunk.contains('@') || chunk.starts_with("www.")))
        .flat_map(tokens)
        .filter(|t| !t.contains('.') && !t.contains('\''))
        .collect();
    let lower: Vec<String> = candidates.iter().map(|t| t.to_lowercase()).collect();
    let everyday = crate::learn::everyday(&lower);
    let known: std::collections::HashSet<String> = mine
        .iter()
        .flat_map(|w| std::iter::once(w.word.to_lowercase()).chain(w.word.split(' ').map(str::to_lowercase)))
        .chain(forgotten.iter().cloned())
        .collect();
    // Count each spelling, remembering where it first appeared.
    let mut counts: HashMap<&str, (usize, usize)> = HashMap::new();
    for (at, token) in candidates.iter().enumerate() {
        if known.contains(&token.to_lowercase()) || !looks_like_a_name(token, &everyday) {
            continue;
        }
        counts.entry(token).or_insert((0, at)).0 += 1;
    }
    let mut ranked: Vec<(&str, (usize, usize))> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.1.1.cmp(&b.1.1)));
    ranked.into_iter().take(MOST_TERMS).map(|(t, _)| t.to_string()).collect()
}

/// Terms from the screen, as words for this one dictation.
pub fn as_words(terms: &[String]) -> Vec<Word> {
    terms
        .iter()
        .map(|t| {
            let mut word = Word::new(t, words::Source::Screen);
            word.on_screen = true;
            word
        })
        .collect()
}

/// A term seen on screen that the user then said: offered for Your words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub word: String,
    /// What the speech model wrote for it, when it misheard it; lower case.
    #[serde(default)]
    pub heard: Option<String>,
    /// How many dictations used it.
    #[serde(default)]
    pub seen: u32,
    /// The app it was last seen in.
    #[serde(default)]
    pub app: String,
    /// Milliseconds since 1970.
    #[serde(default)]
    pub last_seen: u64,
}

impl Suggestion {
    /// Worth showing: the speech model got it wrong at least once (so the
    /// screen saved the day), or the user has said it more than once.
    pub fn worth_offering(&self) -> bool {
        self.heard.is_some() || self.seen >= 2
    }
}

/// Does `text` contain `term` as a whole word?
fn has_word(text: &str, term: &str, ignore_case: bool) -> bool {
    tokens(text).any(|t| if ignore_case { t.eq_ignore_ascii_case(term) } else { t == term })
}

/// After a dictation: note each screen term that made it into the text.
/// Returns the suggestions that just became worth offering.
pub fn note_used(
    suggestions: &mut Vec<Suggestion>,
    terms: &[String],
    raw: &str,
    written: &str,
    app: &str,
) -> Vec<Suggestion> {
    let mut fresh = Vec::new();
    for term in terms {
        if !has_word(written, term, false) {
            continue;
        }
        // Not in what the speech model heard: then it misheard it, and the
        // screen put it right. Find what it wrote instead.
        let heard = if has_word(raw, term, true) {
            None
        } else {
            words::suspects(raw, &as_words(std::slice::from_ref(term))).into_iter().next().map(|s| s.heard)
        };
        let at = suggestions.iter().position(|s| s.word.to_lowercase() == term.to_lowercase());
        let entry = match at {
            Some(at) => &mut suggestions[at],
            None => {
                suggestions.push(Suggestion {
                    word: term.clone(),
                    heard: None,
                    seen: 0,
                    app: String::new(),
                    last_seen: 0,
                });
                suggestions.last_mut().unwrap()
            }
        };
        let was_offered = entry.worth_offering();
        entry.seen += 1;
        entry.heard = entry.heard.take().or(heard);
        entry.app = app.to_string();
        entry.last_seen = words::now();
        if !was_offered && entry.worth_offering() {
            fresh.push(entry.clone());
        }
    }
    if suggestions.len() > MOST_SUGGESTIONS {
        suggestions.sort_by_key(|s| std::cmp::Reverse(s.last_seen));
        suggestions.truncate(MOST_SUGGESTIONS);
    }
    fresh
}

/// Suggestions that are not already the user's words or ones they removed.
pub fn clean_suggestions(suggestions: &[Suggestion], mine: &[Word], forgotten: &[String]) -> Vec<Suggestion> {
    let mut seen = std::collections::HashSet::new();
    suggestions
        .iter()
        .filter(|s| {
            let lower = s.word.to_lowercase();
            !mine.iter().any(|w| w.word.to_lowercase() == lower) && !forgotten.contains(&lower) && seen.insert(lower)
        })
        .take(MOST_SUGGESTIONS)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_terms_are_picked_out_of_window_text() {
        let text = "Slack | #platform\nPriya Raghunathan: the fetchUserProfile call fails on iOS 18 with GPT4.\n\
                    Raghunathan said Kubernetes is fine. See https://example.com/Docs and README.md, commit a3f9c2d1.\n\
                    The Team will Meet on Monday";
        let mine = vec![Word::new("Kubernetes", words::Source::Added)];
        let found = terms(text, &mine, &["priya".to_string()]);
        assert_eq!(found[0], "Raghunathan", "{found:?}");
        for expected in ["fetchUserProfile", "iOS", "GPT4"] {
            assert!(found.contains(&expected.to_string()), "{expected} missing from {found:?}");
        }
        // Everyday words, even capitalised; the user's own and forgotten words;
        // web addresses, file names and hashes.
        for unwanted in ["Slack", "The", "Team", "Meet", "Monday", "Kubernetes", "Priya", "Docs", "README", "a3f9c2d1", "https"] {
            assert!(!found.contains(&unwanted.to_string()), "{unwanted} in {found:?}");
        }
    }

    #[test]
    fn a_term_said_is_suggested_once_misheard_or_said_twice() {
        let terms = vec!["Raghunathan".to_string(), "fetchUserProfile".to_string()];
        let mut list = Vec::new();
        // Misheard and put right from the screen: offered straight away.
        let fresh = note_used(&mut list, &terms, "ask ragunathan", "Ask Raghunathan.", "Slack");
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].heard.as_deref(), Some("ragunathan"));
        // Heard right: offered the second time only.
        assert!(note_used(&mut list, &terms, "call fetchUserProfile", "Call fetchUserProfile.", "Code").is_empty());
        let fresh = note_used(&mut list, &terms, "call fetchUserProfile", "Call fetchUserProfile.", "Code");
        assert_eq!(fresh[0].word, "fetchUserProfile");
        // Not said: nothing.
        assert!(note_used(&mut list, &terms, "hello", "Hello.", "Mail").is_empty());
        assert_eq!(list.len(), 2);
    }
}
