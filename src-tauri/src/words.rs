//! The user's own words: names, products and jargon that Utter should spell
//! their way. Each word may also remember what the speech model wrote when it
//! misheard it ("cloud" for "Claude"), which lets cleanup put it right when
//! the sentence is about that word, and leave it alone when it is not.

use serde::{Deserialize, Deserializer, Serialize};

/// Most words the list may hold, and the longest a word may be. The list is
/// given to the speech model, which only reads a few hundred words of it.
pub const MAX_WORDS: usize = 200;
const MAX_WORD_CHARS: usize = 60;
/// Most mishearings remembered for one word, and the longest one may be.
const MAX_HEARD: usize = 8;
const MAX_HEARD_CHARS: usize = 40;
/// Most removed words remembered, so they are not learned again.
const MAX_FORGOTTEN: usize = 500;

/// Where a word came from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Typed in by the user.
    #[default]
    Added,
    /// Learned from a correction the user made.
    Learned,
    /// Suggested from what was on screen, and accepted.
    Screen,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    /// How the user writes it.
    pub word: String,
    /// What the speech model wrote instead, lower case: "cloud", "clawed".
    pub heard_as: Vec<String>,
    pub source: Source,
    /// When it was added, in milliseconds since 1970; 0 if unknown.
    pub added_at: u64,
}

impl Word {
    pub fn new(word: &str, source: Source) -> Self {
        Self { word: word.to_string(), heard_as: Vec::new(), source, added_at: now() }
    }
}

// Lists saved before mishearings existed hold plain strings. Read both.
impl<'de> Deserialize<'de> for Word {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Stored {
            Plain(String),
            #[serde(rename_all = "camelCase")]
            Full {
                word: String,
                #[serde(default)]
                heard_as: Vec<String>,
                #[serde(default)]
                source: Source,
                #[serde(default)]
                added_at: u64,
            },
        }
        Ok(match Stored::deserialize(deserializer)? {
            Stored::Plain(word) => Word { word, heard_as: Vec::new(), source: Source::Added, added_at: 0 },
            Stored::Full { word, heard_as, source, added_at } => Word { word, heard_as, source, added_at },
        })
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Runs of white space become one space; the ends are trimmed.
fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A mishearing as stored: lower case, without the punctuation around it.
pub fn heard_form(text: &str) -> String {
    squeeze(text)
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
        .chars()
        .take(MAX_HEARD_CHARS)
        .collect()
}

/// Tidy a list as saved: trimmed, without empty entries or repeats (a repeat
/// donates its mishearings to the first), within the size limits.
pub fn clean(words: &[Word]) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::new();
    for entry in words {
        let word: String = squeeze(&entry.word).chars().take(MAX_WORD_CHARS).collect();
        if word.is_empty() {
            continue;
        }
        let at = match out.iter().position(|w| w.word.to_lowercase() == word.to_lowercase()) {
            Some(at) => at,
            None if out.len() < MAX_WORDS => {
                out.push(Word { word, heard_as: Vec::new(), source: entry.source, added_at: entry.added_at });
                out.len() - 1
            }
            None => continue,
        };
        for heard in &entry.heard_as {
            add_heard(&mut out[at], heard);
        }
    }
    out
}

/// Remember one more mishearing of `word`. False when there was nothing new:
/// it was already known, empty, or simply the word itself.
fn add_heard(word: &mut Word, heard: &str) -> bool {
    let heard = heard_form(heard);
    if heard.is_empty()
        || heard == word.word.to_lowercase()
        || word.heard_as.contains(&heard)
        || word.heard_as.len() >= MAX_HEARD
    {
        return false;
    }
    word.heard_as.push(heard);
    true
}

/// Just the spellings, for the speech model and for checking replies.
pub fn spellings(words: &[Word]) -> Vec<String> {
    words.iter().map(|w| w.word.clone()).collect()
}

/// What `remember` changed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Remembered {
    /// A new word.
    Added,
    /// A word already there, with a new mishearing.
    Heard,
    /// Nothing new.
    Known,
    /// The list is full.
    Full,
}

/// Add `word`, or teach a word already there that it may be `heard` as
/// something else. Matching is by spelling, ignoring case; a word that only
/// differed in case takes the new spelling, since that is the correction.
pub fn remember(words: &mut Vec<Word>, word: &str, heard: Option<&str>, source: Source) -> Remembered {
    let word: String = squeeze(word).chars().take(MAX_WORD_CHARS).collect();
    if word.is_empty() {
        return Remembered::Known;
    }
    if let Some(existing) = words.iter_mut().find(|w| w.word.to_lowercase() == word.to_lowercase()) {
        let respelled = existing.word != word;
        existing.word = word;
        let heard_new = heard.is_some_and(|h| add_heard(existing, h));
        return if heard_new { Remembered::Heard } else if respelled { Remembered::Added } else { Remembered::Known };
    }
    if words.len() >= MAX_WORDS {
        return Remembered::Full;
    }
    let mut entry = Word::new(&word, source);
    if let Some(heard) = heard {
        add_heard(&mut entry, heard);
    }
    words.push(entry);
    Remembered::Added
}

/// Removed words, kept lower case so they are never learned or suggested
/// again. A word the user adds back by hand is no longer forgotten.
pub fn clean_forgotten(forgotten: &[String], words: &[Word]) -> Vec<String> {
    let kept: std::collections::HashSet<String> = words.iter().map(|w| w.word.to_lowercase()).collect();
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<String> = forgotten
        .iter()
        .map(|f| squeeze(f).to_lowercase())
        .filter(|f| !f.is_empty() && !kept.contains(f) && seen.insert(f.clone()))
        .collect();
    // The oldest go first.
    if out.len() > MAX_FORGOTTEN {
        out.drain(..out.len() - MAX_FORGOTTEN);
    }
    out
}

/// Replace whole-word occurrences of `wrong` in `text` with `right`. Used to
/// correct a past dictation. Matches ignore case; a word inside a longer word
/// is left alone.
pub fn replace_word(text: &str, wrong: &str, right: &str) -> String {
    let wrong = wrong.trim();
    if wrong.is_empty() {
        return text.to_string();
    }
    let lower = text.to_lowercase();
    let needle = wrong.to_lowercase();
    // Lower-casing can change byte lengths outside ASCII; then only exact
    // matches are safe to find.
    let (haystack, needle) = if lower.len() == text.len() && needle.len() == wrong.len() {
        (lower.as_str(), needle)
    } else {
        (text, wrong.to_string())
    };
    let is_word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    while let Some(found) = haystack[from..].find(&needle) {
        let start = from + found;
        let end = start + needle.len();
        let before = text[..start].chars().last();
        let after = text[end..].chars().next();
        out.push_str(&text[from..start]);
        if is_word(before) || is_word(after) {
            out.push_str(&text[start..end]);
        } else {
            out.push_str(right);
        }
        from = end;
    }
    out.push_str(&text[from..]);
    out
}

/// A place in a transcript where the speech model may have misheard one of
/// the user's words.
#[derive(Debug, Clone, PartialEq)]
pub struct Suspect {
    /// What the transcript says, lower case: "cloud".
    pub heard: String,
    /// The user's word it may stand for: "Claude".
    pub word: String,
}

/// Lower-case words of a text, apostrophes dropped.
fn plain_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace(['\'', '\u{2019}'], "")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// A rough key for how an English word sounds (Soundex): the first letter,
/// then digits for the consonant groups that follow. "Claude", "cloud" and
/// "clawed" share one.
fn sound_key(word: &str) -> String {
    let code = |c: char| match c {
        'b' | 'f' | 'p' | 'v' => '1',
        'c' | 'g' | 'j' | 'k' | 'q' | 's' | 'x' | 'z' => '2',
        'd' | 't' => '3',
        'l' => '4',
        'm' | 'n' => '5',
        'r' => '6',
        _ => '0',
    };
    let letters: Vec<char> = word.chars().filter(|c| c.is_ascii_alphabetic()).map(|c| c.to_ascii_lowercase()).collect();
    let Some(&first) = letters.first() else { return String::new() };
    let mut key = String::from(first.to_ascii_uppercase());
    let mut last = code(first);
    for &c in &letters[1..] {
        let digit = code(c);
        // H and W do not separate two of the same sound; vowels do.
        if c == 'h' || c == 'w' {
            continue;
        }
        if digit != '0' && digit != last {
            key.push(digit);
        }
        last = digit;
        if key.len() == 4 {
            break;
        }
    }
    while key.len() < 4 {
        key.push('0');
    }
    key
}

/// How alike two words are in spelling, from 0 (nothing shared) to 1.
fn likeness(a: &str, b: &str) -> f32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let longest = a.len().max(b.len());
    if longest == 0 {
        return 1.0;
    }
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut diagonal = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let above = row[j];
            row[j] = (row[j] + 1).min(row[j - 1] + 1).min(diagonal + usize::from(a[i - 1] != b[j - 1]));
            diagonal = above;
        }
    }
    1.0 - row[b.len()] as f32 / longest as f32
}

/// Do these two sound alike enough that one may have been misheard as the
/// other? Both the sound and most of the spelling must match: "cloud" and
/// "Claude" do; "cold" and "Claude" do not.
pub fn sounds_alike(heard: &str, word: &str) -> bool {
    let (heard, word) = (heard.to_lowercase(), word.to_lowercase());
    heard != word
        && heard.chars().count() >= 3
        && word.chars().count() >= 4
        && sound_key(&heard) == sound_key(&word)
        && likeness(&heard, &word) >= 0.5
}

/// Where in `text` the speech model may have misheard one of `words`: a
/// mishearing the user taught it, or something that sounds like the word
/// without being it. Only a hint: the cleanup model decides from the
/// sentence whether the user's word was meant.
pub fn suspects(text: &str, words: &[Word]) -> Vec<Suspect> {
    const MOST: usize = 10;
    let said = plain_words(text);
    let mut found: Vec<Suspect> = Vec::new();
    let mut note = |heard: String, word: &str| {
        if found.len() < MOST && !found.iter().any(|s| s.heard == heard && s.word == word) {
            found.push(Suspect { heard, word: word.to_string() });
        }
    };
    for entry in words {
        let own: Vec<String> = plain_words(&entry.word);
        // Already spelled right in this transcript: nothing to put right.
        if !own.is_empty() && said.windows(own.len()).any(|run| run == own.as_slice()) {
            continue;
        }
        for heard in &entry.heard_as {
            let heard_words = plain_words(heard);
            if !heard_words.is_empty() && said.windows(heard_words.len()).any(|run| run == heard_words.as_slice()) {
                note(heard.clone(), &entry.word);
            }
        }
        // A word, or two run together ("pre a" for Priya), that sounds like
        // one of the word's own longer words.
        for part in own.iter().filter(|p| p.chars().count() >= 4 && p.chars().all(|c| c.is_alphabetic())) {
            let length = part.chars().count();
            for (i, one) in said.iter().enumerate() {
                if sounds_alike(one, part) {
                    note(one.clone(), &entry.word);
                    continue;
                }
                // Two pieces about as long as the word itself, together.
                if let Some(next) = said.get(i + 1) {
                    let joined = format!("{one}{next}");
                    let close = joined.chars().count().abs_diff(length) <= 1;
                    if close && !sounds_alike(next, part) && sounds_alike(&joined, part) {
                        note(format!("{one} {next}"), &entry.word);
                    }
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(words: &[&str]) -> Vec<Word> {
        words.iter().map(|w| Word::new(w, Source::Added)).collect()
    }

    #[test]
    fn old_lists_of_plain_strings_still_load() {
        let loaded: Vec<Word> = serde_json::from_str(r#"["Priya", {"word":"Claude","heardAs":["cloud"],"source":"learned"}]"#).unwrap();
        assert_eq!(loaded[0].word, "Priya");
        assert_eq!(loaded[0].source, Source::Added);
        assert_eq!(loaded[1].heard_as, ["cloud"]);
        assert_eq!(loaded[1].source, Source::Learned);
    }

    #[test]
    fn a_list_is_trimmed_deduplicated_and_capped() {
        let mut typed = plain(&["  Priya  ", "", "priya", "Kubernetes", "two   words", "\n"]);
        typed[2].heard_as = vec!["Pria.".into()];
        let cleaned = clean(&typed);
        let names: Vec<&str> = cleaned.iter().map(|w| w.word.as_str()).collect();
        assert_eq!(names, ["Priya", "Kubernetes", "two words"]);
        // The repeat's mishearing joins the first, tidied.
        assert_eq!(cleaned[0].heard_as, ["pria"]);
        let many: Vec<Word> = (0..500).map(|i| Word::new(&format!("term{i}"), Source::Added)).collect();
        assert_eq!(clean(&many).len(), MAX_WORDS);
        assert_eq!(clean(&plain(&[&"x".repeat(500)]))[0].word.chars().count(), MAX_WORD_CHARS);
    }

    #[test]
    fn remembering_adds_a_word_or_a_new_mishearing() {
        let mut words = plain(&["Priya"]);
        assert_eq!(remember(&mut words, "Claude", Some("cloud"), Source::Learned), Remembered::Added);
        assert_eq!(remember(&mut words, "claude", Some("Clawed"), Source::Learned), Remembered::Heard);
        assert_eq!(remember(&mut words, "claude", Some("cloud"), Source::Learned), Remembered::Known);
        // The word itself is not a mishearing of itself.
        assert_eq!(remember(&mut words, "Priya", Some("priya"), Source::Added), Remembered::Known);
        assert_eq!(words[1].word, "claude");
        assert_eq!(words[1].heard_as, ["cloud", "clawed"]);
        assert_eq!(words[1].source, Source::Learned);
    }

    #[test]
    fn forgotten_words_drop_out_when_added_back() {
        let words = plain(&["Claude"]);
        let forgotten = vec!["claude".to_string(), "Zed ".to_string(), "zed".to_string()];
        assert_eq!(clean_forgotten(&forgotten, &words), ["zed"]);
    }

    #[test]
    fn mishearings_and_sound_alikes_are_suspected() {
        let mut words = plain(&["Claude", "Priya", "Kubernetes", "Q3 roadmap"]);
        words[2].heard_as = vec!["cuban eats".into()];
        let heard = |text: &str| -> Vec<(String, String)> {
            suspects(text, &words).into_iter().map(|s| (s.heard, s.word)).collect()
        };
        assert_eq!(heard("Ask cloud to summarise this."), [("cloud".into(), "Claude".into())]);
        assert_eq!(heard("It was clawed back."), [("clawed".into(), "Claude".into())]);
        assert_eq!(heard("Ask pre a about the cuban eats migration."), [
            ("pre a".into(), "Priya".into()),
            ("cuban eats".into(), "Kubernetes".into()),
        ]);
        // Spelled right already, or only vaguely alike.
        assert!(heard("Ask Claude about the cloud bill.").is_empty());
        assert!(heard("It is cold and I could go.").is_empty());
        assert!(heard("Nothing to see here.").is_empty());
    }

    #[test]
    fn sound_keys_follow_soundex() {
        assert_eq!(sound_key("Robert"), "R163");
        assert_eq!(sound_key("Rupert"), "R163");
        assert_eq!(sound_key("Ashcraft"), "A261");
        assert_eq!(sound_key("Claude"), sound_key("cloud"));
    }

    #[test]
    fn only_whole_words_are_replaced() {
        assert_eq!(replace_word("Ask cloud. The cloudy cloud!", "cloud", "Claude"), "Ask Claude. The cloudy Claude!");
        assert_eq!(replace_word("Cloud first", "cloud", "Claude"), "Claude first");
        assert_eq!(replace_word("nothing here", "", "x"), "nothing here");
    }
}
