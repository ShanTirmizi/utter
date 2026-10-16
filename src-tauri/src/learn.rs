//! Learning words from the user's own corrections.
//!
//! Right after Utter pastes, it notes the text box it pasted into and what
//! that box held. For a minute it keeps reading the box (only that box), and
//! once the user has stopped editing, moved elsewhere or sent the message, it
//! compares. A word of Utter's that the user replaced with something that
//! sounds like it and is a name or term rather than an everyday word ("cloud"
//! became "Claude") is added to Your words, with the mishearing, and the user
//! is told. Everything else (typo fixes, rewording, deletions, carrying on
//! typing) is ignored.
//!
//! The same watch also says how many of the pasted words the user changed,
//! for the edit rate (see `accuracy.rs`). That count is all that is kept from
//! the box, except with Keep recordings on, when the dictation's own part of
//! it, as the user left it, is kept next to its recording.

use crate::{
    accuracy::{self, Edit},
    settings,
    words::{self, Remembered, Source},
};
use serde::Serialize;
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};

/// Most words learned from one dictation. More than that is a rewrite, not a
/// correction.
const MOST_PER_DICTATION: usize = 3;
/// A correction is at most this many words on either side.
const MOST_WORDS: usize = 3;

/// One correction found in the text box.
#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    /// What Utter wrote: "cloud".
    pub wrong: String,
    /// What the user changed it to: "Claude".
    pub right: String,
}

/// Runs of white space as one space, so that an app's own line endings or
/// spacing do not count as edits.
fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Word-sized pieces of text, with their byte ranges.
fn spans(text: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut start = None;
    for (at, c) in text.char_indices() {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(at),
            (true, Some(s)) => {
                found.push((s, at));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        found.push((s, text.len()));
    }
    found
}

/// A word without the punctuation around it: "cloud," is "cloud".
fn bare(word: &str) -> &str {
    word.trim_matches(|c: char| !c.is_alphanumeric())
}

/// What the user changed in the part of the box Utter pasted. `before` is
/// the box just after the paste and `after` its final state; `pasted` must be
/// found in `before`, or nothing can be said about it.
pub fn corrections(pasted: &str, before: &str, after: &str) -> Vec<Correction> {
    let (pasted, before, after) = (squeeze(pasted), squeeze(before), squeeze(after));
    if pasted.is_empty() || before == after {
        return Vec::new();
    }
    let Some(start) = before.rfind(&pasted) else {
        return Vec::new();
    };
    let end = start + pasted.len();

    // Words on each side, and the ones that differ between the two ends.
    let old = spans(&before);
    let new = spans(&after);
    // Compared without their punctuation: "it" becoming "it." is no edit.
    let text = |s: &str, (a, b): (usize, usize)| bare(&s[a..b]).to_string();
    let old_words: Vec<String> = old.iter().map(|&r| text(&before, r)).collect();
    let new_words: Vec<String> = new.iter().map(|&r| text(&after, r)).collect();
    let mut first = 0;
    while first < old_words.len() && first < new_words.len() && old_words[first] == new_words[first] {
        first += 1;
    }
    let mut tail = 0;
    while tail < old_words.len() - first
        && tail < new_words.len() - first
        && old_words[old_words.len() - 1 - tail] == new_words[new_words.len() - 1 - tail]
    {
        tail += 1;
    }
    let old_mid = &old_words[first..old_words.len() - tail];
    let new_mid = &new_words[first..new_words.len() - tail];
    // A very long changed stretch is the box being rewritten or replaced.
    if old_mid.len() > 80 || new_mid.len() > 80 {
        return Vec::new();
    }

    // Line the middles up word by word, and collect each run of removed
    // words that was replaced by a run of new ones.
    let n = old_mid.len();
    let m = new_mid.len();
    let mut longest = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            longest[i][j] = if old_mid[i] == new_mid[j] {
                longest[i + 1][j + 1] + 1
            } else {
                longest[i + 1][j].max(longest[i][j + 1])
            };
        }
    }
    let mut found = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && old_mid[i] == new_mid[j] {
            i += 1;
            j += 1;
            continue;
        }
        let (from_i, from_j) = (i, j);
        while i < n || j < m {
            if i < n && j < m && old_mid[i] == new_mid[j] {
                break;
            }
            if j < m && (i == n || longest[i][j + 1] >= longest[i + 1][j]) {
                j += 1;
            } else {
                i += 1;
            }
        }
        let removed = &old_mid[from_i..i];
        let added = &new_mid[from_j..j];
        // Only a replacement, inside what Utter pasted.
        let inside = (first + from_i..first + i).all(|k| old[k].0 >= start && old[k].1 <= end);
        if !removed.is_empty() && !added.is_empty() && removed.len() <= MOST_WORDS && added.len() <= MOST_WORDS && inside {
            let wrong = removed.join(" ");
            let right = added.join(" ");
            if !wrong.is_empty() && !right.is_empty() && wrong != right {
                found.push(Correction { wrong, right });
            }
        }
    }
    found
}

/// Everyday words that the system word list, which is old, does not have.
const MODERN: &[&str] = &[
    "app", "apps", "email", "emails", "online", "offline", "website", "websites", "internet", "okay", "ok",
    "smartphone", "laptop", "login", "logout", "username", "password", "wifi", "blog", "podcast", "selfie",
    "emoji", "inbox", "upload", "download", "uploads", "downloads", "texting", "google", "googled", "tweet",
    "hashtag", "livestream", "update", "updates", "feedback", "checklist", "deadline", "workflow", "dashboard",
    "spreadsheet", "startup", "meetup", "signup", "setup", "gonna", "wanna", "gotta", "yeah", "yep", "nope",
    // Capitalised in the word list, but everyday all the same.
    "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday", "january", "february",
    "april", "june", "july", "august", "september", "october", "november", "december", "english", "i",
];

/// Which of `words` (lower case) are everyday English words. They are looked
/// up in the system's word list, where everyday words are written in lower
/// case and names with a capital. Without the list, none are.
pub(crate) fn everyday(words: &[String]) -> HashSet<String> {
    let mut found: HashSet<String> = words.iter().filter(|w| MODERN.contains(&w.as_str())).cloned().collect();
    let Ok(list) = std::fs::read_to_string("/usr/share/dict/words") else {
        return found;
    };
    let wanted: HashSet<&str> = words.iter().map(String::as_str).collect();
    for line in list.lines() {
        if wanted.contains(line) {
            found.insert(line.to_string());
        }
    }
    found
}

/// Is this correction one worth remembering? The new text must sound like
/// what Utter wrote (a mishearing, not a change of mind) and be a name or a
/// term, not an everyday word: "cloud" to "Claude" is; "teh" to "the",
/// "great" to "good" and "so" to "So" are not.
fn worth_learning(fix: &Correction, everyday: &HashSet<String>) -> bool {
    let right = fix.right.as_str();
    let length = right.chars().count();
    if !(2..=40).contains(&length)
        || right.contains(['@', '/', '\\', ':', '#'])
        || !right.chars().any(char::is_alphabetic)
    {
        return false;
    }
    let pieces: Vec<&str> = right.split(' ').collect();
    // Odd capitals or digits mark a product or code name: iPhone, GPT4, macOS.
    let unusual = |piece: &str| {
        let letters: Vec<char> = piece.chars().collect();
        letters.iter().skip(1).any(|c| c.is_uppercase())
            || (letters.iter().any(|c| c.is_ascii_digit()) && letters.iter().any(|c| c.is_alphabetic()))
            || !everyday.contains(&piece.to_lowercase())
    };
    if !pieces.iter().any(|p| unusual(p)) {
        return false;
    }
    // A change of case alone teaches a spelling: claude to Claude.
    if fix.wrong.to_lowercase() == right.to_lowercase() {
        return right.chars().any(char::is_uppercase);
    }
    let joined = |text: &str| text.split(' ').collect::<String>().to_lowercase();
    // Only the spacing changed: "i phone" to iPhone.
    joined(&fix.wrong) == joined(right) || words::sounds_alike(&joined(&fix.wrong), &joined(right))
}

/// The corrections in a text box that are worth learning, at most a few.
pub fn worth_keeping(found: Vec<Correction>) -> Vec<Correction> {
    let needles: Vec<String> = found
        .iter()
        .flat_map(|f| f.right.split(' ').map(str::to_lowercase).collect::<Vec<_>>())
        .collect();
    let everyday = everyday(&needles);
    found.into_iter().filter(|f| worth_learning(f, &everyday)).take(MOST_PER_DICTATION).collect()
}

/// What the pill and the window are told when a word is learned.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Learned {
    pub word: String,
    pub heard: String,
    /// True for a new word, false for a new mishearing of one already there.
    pub new_word: bool,
}

/// Remember the corrections found in one dictation's text box, skipping
/// anything the user has removed before, and say what was learned.
fn learn(app: &AppHandle, found: Vec<Correction>) {
    let settings = settings::load(app);
    if !settings.learn_words {
        return;
    }
    for fix in worth_keeping(found) {
        let pair = words::forgotten_pair(&fix.wrong, &fix.right);
        if settings.forgotten.iter().any(|f| *f == fix.right.to_lowercase() || *f == pair) {
            continue;
        }
        match crate::remember_word(app, &fix.right, Some(&fix.wrong), Source::Learned) {
            Ok((_, outcome @ (Remembered::Added | Remembered::Heard))) => {
                let learned = Learned {
                    word: fix.right.clone(),
                    heard: words::heard_form(&fix.wrong),
                    new_word: outcome == Remembered::Added,
                };
                log(&format!("learned a word from a correction ({})", if learned.new_word { "new" } else { "new mishearing" }));
                crate::tray::set_learned(app, Some(learned.clone()));
                let _ = app.emit("learned", learned);
            }
            Ok(_) => {}
            Err(e) => log(&format!("could not save a learned word: {e}")),
        }
    }
}

/// How often the box is read while watching it.
const EVERY: Duration = Duration::from_millis(300);
/// How long after the paste the box is watched at most.
const WATCH_FOR: Duration = Duration::from_secs(60);
/// The user is taken to have finished editing after this long without a change.
const SETTLED: Duration = Duration::from_secs(4);

/// Watch the text box that `pasted` just went into, and learn from what the
/// user changes in it. `still_current` says whether this dictation is still
/// the latest one; a new one ends the watch, since its own paste would look
/// like an edit. `edited` is told what the user changed, when that can be
/// told.
///
/// The box is watched while learning is on, or while recordings are kept to
/// measure accuracy; with both off, it is left alone.
pub fn watch(
    app: AppHandle,
    pasted: String,
    still_current: impl Fn() -> bool + Send + 'static,
    edited: impl FnOnce(Edit) + Send + 'static,
) {
    let settings = settings::load(&app);
    if !(settings.learn_words || settings.keep_audio) || !crate::permissions::accessibility() {
        return;
    }
    std::thread::spawn(move || {
        // Give the app a moment to take in the paste.
        std::thread::sleep(Duration::from_millis(250));
        let Some(field) = crate::selection::Field::focused() else {
            log("watch: the app gave no text box");
            return;
        };
        let Some(before) = field.value() else {
            log("watch: the text box would not say what it holds");
            return;
        };
        if !squeeze(&before).contains(&squeeze(&pasted)) {
            log("watch: the pasted text is not in the text box");
            return;
        }
        let began = Instant::now();
        let mut last = before.clone();
        let mut changed_at = Instant::now();
        let mut reason = "a minute passed";
        loop {
            std::thread::sleep(EVERY);
            if !still_current() {
                reason = "another dictation began";
                break;
            }
            if began.elapsed() > WATCH_FOR {
                break;
            }
            if !crate::selection::Field::focused().is_some_and(|now| now.same_as(&field)) {
                reason = "the user moved to another box";
                break;
            }
            let Some(now) = field.value() else {
                reason = "the box stopped answering";
                break;
            };
            // Emptied, or the text is gone: the message was sent. The last
            // reading from before that holds the edits.
            if squeeze(&now).len() * 2 < squeeze(&pasted).len() && now.len() < last.len() {
                reason = "the text was sent or cleared";
                break;
            }
            if now != last {
                last = now;
                changed_at = Instant::now();
            } else if last != before && changed_at.elapsed() > SETTLED {
                reason = "the user stopped editing";
                break;
            }
        }
        let found = corrections(&pasted, &before, &last);
        let edit = accuracy::edit(&pasted, &before, &last);
        log(&format!(
            "watch ended ({reason}): {} change(s) to consider, {}",
            found.len(),
            match &edit {
                Some(edit) => format!("{} of {} word(s) edited", edit.changed, edit.words),
                None => "edits unclear".to_string(),
            }
        ));
        if let Some(edit) = edit {
            edited(edit);
        }
        if !found.is_empty() {
            learn(&app, found);
        }
    });
}

/// A line in the same log as the fn key's, without any text from the box.
fn log(line: &str) {
    crate::fnkey::log(&format!("learn: {line}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixes(pasted: &str, before: &str, after: &str) -> Vec<(String, String)> {
        corrections(pasted, before, after).into_iter().map(|c| (c.wrong, c.right)).collect()
    }

    #[test]
    fn a_replaced_word_inside_the_paste_is_found() {
        let pasted = "Can you ask cloud to summarise the notes? ";
        let before = "Hi Sam. Can you ask cloud to summarise the notes? ";
        assert_eq!(
            fixes(pasted, before, "Hi Sam. Can you ask Claude to summarise the notes?"),
            [("cloud".to_string(), "Claude".to_string())]
        );
        // One name heard as two words.
        assert_eq!(
            fixes("Tell pre a the deploy is done.", "Tell pre a the deploy is done.", "Tell Priya the deploy is done."),
            [("pre a".to_string(), "Priya".to_string())]
        );
    }

    #[test]
    fn edits_outside_the_paste_or_additions_are_not_corrections() {
        let pasted = "ask cloud about it";
        let before = "Hi Sam, ask cloud about it";
        // The user's own text before the paste.
        assert!(fixes(pasted, before, "Hi Samantha, ask cloud about it").is_empty());
        // Carrying on typing after it.
        assert!(fixes(pasted, before, "Hi Sam, ask cloud about it. Thanks!").is_empty());
        // Deleting a word.
        assert!(fixes(pasted, before, "Hi Sam, ask about it").is_empty());
        // The paste is not there at all.
        assert!(fixes(pasted, "something else", "something new").is_empty());
    }

    #[test]
    fn only_names_that_sound_like_the_mistake_are_worth_learning() {
        let keep = |wrong: &str, right: &str| {
            let fix = Correction { wrong: wrong.into(), right: right.into() };
            let everyday = everyday(&right.split(' ').map(str::to_lowercase).collect::<Vec<_>>());
            worth_learning(&fix, &everyday)
        };
        assert!(keep("cloud", "Claude"));
        assert!(keep("pre a", "Priya"));
        assert!(keep("claude", "Claude"));
        assert!(keep("i phone", "iPhone"));
        // Everyday words, a change of mind, a change of case at a sentence start.
        assert!(!keep("teh", "the"));
        assert!(!keep("great", "good"));
        assert!(!keep("so", "So"));
        assert!(!keep("cloud", "Kubernetes"));
        assert!(!keep("emial", "email"));
        assert!(!keep("me", "me@example.com"));
    }
}
