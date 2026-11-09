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
    /// One of the tech names Utter knows (see `tech`), for one dictation
    /// only. Never saved.
    Builtin,
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
    /// Not one of the user's words but a name on screen, used as a hint for
    /// one dictation only. Never saved.
    #[serde(skip)]
    pub on_screen: bool,
}

impl Word {
    pub fn new(word: &str, source: Source) -> Self {
        Self { word: word.to_string(), heard_as: Vec::new(), source, added_at: now(), on_screen: false }
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
            Stored::Plain(word) => Word { word, heard_as: Vec::new(), source: Source::Added, added_at: 0, on_screen: false },
            Stored::Full { word, heard_as, source, added_at } => Word { word, heard_as, source, added_at, on_screen: false },
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
    for entry in words.iter().filter(|w| !w.on_screen) {
        let word: String = squeeze(&entry.word).chars().take(MAX_WORD_CHARS).collect();
        if word.is_empty() {
            continue;
        }
        let at = match out.iter().position(|w| w.word.to_lowercase() == word.to_lowercase()) {
            Some(at) => at,
            None if out.len() < MAX_WORDS => {
                out.push(Word { word, heard_as: Vec::new(), source: entry.source, added_at: entry.added_at, on_screen: false });
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

/// A word the user removed, kept for a while so it can be restored with its
/// mishearings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Removed {
    pub word: Word,
    /// Milliseconds since 1970.
    pub removed_at: u64,
}

/// How long a removed word can still be restored.
const KEEP_REMOVED_MS: u64 = 30 * 24 * 60 * 60 * 1000;

/// Removed words still within their 30 days, newest first, leaving out any
/// that are back in the list.
pub fn clean_removed(removed: &[Removed], words: &[Word], now: u64) -> Vec<Removed> {
    let mut seen = std::collections::HashSet::new();
    let mut kept: Vec<Removed> = removed
        .iter()
        .filter(|r| now.saturating_sub(r.removed_at) < KEEP_REMOVED_MS)
        .filter(|r| !words.iter().any(|w| w.word.to_lowercase() == r.word.word.to_lowercase()))
        .filter(|r| seen.insert(r.word.word.to_lowercase()))
        .cloned()
        .collect();
    kept.sort_by_key(|r| std::cmp::Reverse(r.removed_at));
    kept.truncate(MAX_WORDS);
    kept
}

/// How a removed mishearing is kept on the forgotten list: "cloud→claude".
pub fn forgotten_pair(heard: &str, word: &str) -> String {
    format!("{}→{}", heard_form(heard), squeeze(word).to_lowercase())
}

/// Removed words, kept lower case so they are never learned or suggested
/// again, and removed mishearings as pairs (see `forgotten_pair`). A word or
/// mishearing the user adds back by hand is no longer forgotten.
pub fn clean_forgotten(forgotten: &[String], words: &[Word]) -> Vec<String> {
    let kept: std::collections::HashSet<String> = words
        .iter()
        .flat_map(|w| {
            let mut kept = vec![w.word.to_lowercase()];
            kept.extend(w.heard_as.iter().map(|h| forgotten_pair(h, &w.word)));
            kept
        })
        .collect();
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

/// Where `needle` appears in `text` as a whole word (or run of words),
/// ignoring case, as byte ranges. A match inside a longer word is not one.
fn whole_matches(text: &str, needle: &str) -> Vec<(usize, usize)> {
    let needle = needle.trim();
    if needle.is_empty() {
        return Vec::new();
    }
    let lower = text.to_lowercase();
    let lower_needle = needle.to_lowercase();
    // Lower-casing can change byte lengths outside ASCII; then only exact
    // matches are safe to find.
    let (haystack, needle) = if lower.len() == text.len() && lower_needle.len() == needle.len() {
        (lower.as_str(), lower_needle)
    } else {
        (text, needle.to_string())
    };
    let is_word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = haystack[from..].find(&needle) {
        let start = from + at;
        let end = start + needle.len();
        if !is_word(text[..start].chars().last()) && !is_word(text[end..].chars().next()) {
            found.push((start, end));
        }
        from = end;
    }
    found
}

/// Does `text` hold `word` as a whole word, ignoring case?
pub fn contains_word(text: &str, word: &str) -> bool {
    !whole_matches(text, word).is_empty()
}

/// Replace whole-word occurrences of `wrong` in `text` with `right`. Used to
/// correct a past dictation. Matches ignore case; a word inside a longer word
/// is left alone.
pub fn replace_word(text: &str, wrong: &str, right: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    for (start, end) in whole_matches(text, wrong) {
        out.push_str(&text[from..start]);
        out.push_str(right);
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
    /// True when `heard` is an identifier said as its words: "fetch user
    /// profile" for fetchUserProfile.
    pub spoken: bool,
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
#[cfg(test)]
fn sound_key(word: &str) -> String {
    sound_code(word, Some(4))
}

/// The sound key of the whole word, without Soundex's cut after three
/// consonant groups. Cut, "whisper" and "Whisperflow" sound the same; whole,
/// they do not, because one goes on for two more syllables.
fn full_sound_key(word: &str) -> String {
    sound_code(word, None)
}

fn sound_code(word: &str, most: Option<usize>) -> String {
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
        if Some(key.len()) == most {
            break;
        }
    }
    if let Some(most) = most {
        while key.len() < most {
            key.push('0');
        }
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
/// other? Both the sound of the whole word and most of the spelling must
/// match: "cloud" and "Claude" do; "cold" and "Claude" do not, and nor do
/// "whisper" and "Whisperflow", which only begins the same way. A word cut
/// short or made plural by a letter or two still counts: "raghunath" for
/// Raghunathan, "terraforms" for Terraform.
pub fn sounds_alike(heard: &str, word: &str) -> bool {
    let (heard, word) = (heard.to_lowercase(), word.to_lowercase());
    if heard == word || heard.chars().count() < 3 || word.chars().count() < 4 || likeness(&heard, &word) < 0.5 {
        return false;
    }
    let (a, b) = (full_sound_key(&heard), full_sound_key(&word));
    let near_in_length = heard.chars().count().abs_diff(word.chars().count()) <= 2;
    a == b || (near_in_length && a.len() >= 3 && b.len() >= 3 && (a.starts_with(b.as_str()) || b.starts_with(a.as_str())))
}

/// The words an identifier is spoken as: fetchUserProfile is "fetch user
/// profile", user_id is "user id", GPT4 is "gpt 4".
fn spoken_parts(identifier: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut previous: Option<char> = None;
    for c in identifier.chars() {
        let boundary = match previous {
            Some(p) => {
                (p.is_lowercase() && c.is_uppercase())
                    || (p.is_alphabetic() && c.is_ascii_digit())
                    || (p.is_ascii_digit() && c.is_alphabetic())
            }
            None => false,
        };
        if !c.is_alphanumeric() || boundary {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current).to_lowercase());
            }
        }
        if c.is_alphanumeric() {
            current.push(c);
        }
        previous = Some(c);
    }
    if !current.is_empty() {
        parts.push(current.to_lowercase());
    }
    parts
}

/// How hard Utter leans on the user's words and the tech names it knows, in
/// the speech model's prompt and in what cleanup is told.
///
/// - Off: no hints and no notes; the words are only kept spelled right
///   where they were heard right.
/// - Light: a few of the user's own words as hints; only the mishearings
///   each word was taught ("cloud" for Claude) are pointed out to cleanup.
/// - Normal (the default): the user's words, the names on screen and a
///   short list of distinctive tech names as hints; words that sound like
///   one of the user's words are pointed out too, never everyday words.
/// - Strong: more hints, and sound-alikes are looked for more loosely.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Boost {
    Off,
    Light,
    #[default]
    Normal,
    Strong,
}

// An unknown value (a newer version's, or a typo by hand) reads as the
// default rather than failing the whole settings file.
impl<'de> Deserialize<'de> for Boost {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(match value.as_str().map(str::to_lowercase).as_deref() {
            Some("off") => Boost::Off,
            Some("light") => Boost::Light,
            Some("strong") => Boost::Strong,
            _ => Boost::Normal,
        })
    }
}

/// Most of the user's words given to the speech model at Light.
const LIGHT_HINTS: usize = 8;

/// The words the speech model is shown for one dictation: the user's own,
/// then the names on screen, then the tech names Utter knows, each once,
/// as far as `boost` allows. `own` may hold built-in and on-screen words
/// too; only the user's own are taken from it.
pub fn speech_hints(own: &[Word], screen: &[String], boost: Boost) -> Vec<String> {
    let own: Vec<&Word> = own.iter().filter(|w| !w.on_screen && w.source != Source::Builtin).collect();
    let mut hints: Vec<String> = Vec::new();
    let add = |hints: &mut Vec<String>, term: &str| {
        if !term.trim().is_empty() && !hints.iter().any(|h| h.to_lowercase() == term.to_lowercase()) {
            hints.push(term.to_string());
        }
    };
    match boost {
        Boost::Off => {}
        Boost::Light => {
            // Words known to be misheard first: they are why the list exists.
            let mut own = own;
            own.sort_by_key(|w| w.heard_as.is_empty());
            for word in own.iter().take(LIGHT_HINTS) {
                add(&mut hints, &word.word);
            }
        }
        Boost::Normal | Boost::Strong => {
            let screen_most = if boost == Boost::Strong { 20 } else { 12 };
            for term in own.iter().map(|w| w.word.as_str()).chain(screen.iter().take(screen_most).map(String::as_str)) {
                add(&mut hints, term);
            }
            for term in crate::tech::hints(boost) {
                add(&mut hints, &term);
            }
        }
    }
    hints
}

/// Everyday English words, never taken for one of the user's words by sound
/// alone (see `common_words.txt`).
fn common_words() -> &'static std::collections::HashSet<&'static str> {
    static WORDS: std::sync::OnceLock<std::collections::HashSet<&'static str>> = std::sync::OnceLock::new();
    WORDS.get_or_init(|| {
        include_str!("common_words.txt")
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect()
    })
}

/// Is `word` (any case) an everyday English word, or the plural of one?
pub fn is_common(word: &str) -> bool {
    let word = word.to_lowercase();
    let words = common_words();
    words.contains(word.as_str()) || word.strip_suffix('s').is_some_and(|one| words.contains(one))
}

/// Shortest a heard word may be to be taken for one of the user's words by
/// sound alone, and how many consonants it needs. Shorter words, and words
/// of vowels, sound like too many things: "pre a" may be Priya, "a" is not.
const MIN_HEARD_LETTERS: usize = 4;
const MIN_HEARD_LETTERS_STRONG: usize = 3;
const MIN_HEARD_CONSONANTS: usize = 2;

/// May `heard`, a word or words run together from a transcript, be taken
/// for one of the user's words by its sound? Never an everyday word, except
/// at Strong, and never a word too short or too soft to tell apart.
fn may_sound_like_a_word(heard: &str, pieces: &[&str], boost: Boost) -> bool {
    let letters: Vec<char> = heard.chars().filter(|c| c.is_alphabetic()).map(|c| c.to_ascii_lowercase()).collect();
    let consonants = letters.iter().filter(|c| c.is_ascii_alphabetic() && !"aeiouy".contains(**c)).count();
    let shortest = if boost == Boost::Strong { MIN_HEARD_LETTERS_STRONG } else { MIN_HEARD_LETTERS };
    if consonants < MIN_HEARD_CONSONANTS || letters.len() < shortest {
        return false;
    }
    boost == Boost::Strong || !pieces.iter().all(|p| is_common(p))
}

/// Where in `text` the speech model may have misheard one of `words`: a
/// mishearing the user taught it, or something that sounds like the word
/// without being it. Only a hint: the cleanup model decides from the
/// sentence whether the user's word was meant. As at the default boost.
pub fn suspects(text: &str, words: &[Word]) -> Vec<Suspect> {
    suspects_at(text, words, Boost::Normal)
}

/// `suspects`, as far as `boost` allows: nothing at Off, only taught
/// mishearings (and identifiers said as their words) at Light, sound-alikes
/// that are not everyday words at Normal, and looser ones at Strong.
pub fn suspects_at(text: &str, words: &[Word], boost: Boost) -> Vec<Suspect> {
    const MOST: usize = 10;
    if boost == Boost::Off {
        return Vec::new();
    }
    let said = plain_words(text);
    let mut found: Vec<Suspect> = Vec::new();
    let mut note = |heard: String, word: &str, spoken: bool| {
        if found.len() < MOST && !found.iter().any(|s| s.heard == heard && s.word == word) {
            found.push(Suspect { heard, word: word.to_string(), spoken });
        }
    };
    for entry in words {
        let own: Vec<String> = plain_words(&entry.word);
        // Already spelled right in this transcript: nothing to put right.
        if contains_word(text, &entry.word) {
            continue;
        }
        for heard in &entry.heard_as {
            let heard_words = plain_words(heard);
            if !heard_words.is_empty() && said.windows(heard_words.len()).any(|run| run == heard_words.as_slice()) {
                note(heard.clone(), &entry.word, false);
            }
        }
        // An identifier said as the words it is made of.
        let spoken = spoken_parts(&entry.word);
        let identifier = entry.word.contains('_')
            || entry.word.chars().collect::<Vec<_>>().windows(2).any(|p| p[0].is_lowercase() && p[1].is_uppercase());
        if identifier && said.windows(spoken.len()).any(|run| run == spoken.as_slice()) {
            note(spoken.join(" "), &entry.word, true);
        }
        // A word, or two run together ("pre a" for Priya), that sounds like
        // one of the word's own longer words.
        //
        // Not for the built-in tech names. They are a long list the user never
        // chose, matched against every dictation, and many sound roughly like
        // everyday words: "myself", "muscle" and "mussel" all sound like
        // MySQL, "hiking" like Hugging Face, "mister" like Mistral. Offered to
        // cleanup as possible names, the model swapped them in all over
        // ordinary dictations. A built-in name is suspected only from the
        // ways it is really misheard, listed with it ("my sequel").
        //
        // Nor at Light, which goes by taught mishearings alone. And a heard
        // word must be distinctive enough (see `may_sound_like_a_word`):
        // "make" is never Maike, however alike they sound.
        if entry.source == Source::Builtin || boost < Boost::Normal {
            continue;
        }
        for part in own.iter().filter(|p| p.chars().count() >= 4 && p.chars().all(|c| c.is_alphabetic())) {
            let length = part.chars().count();
            for (i, one) in said.iter().enumerate() {
                if sounds_alike(one, part) {
                    if may_sound_like_a_word(one, &[one], boost) {
                        note(one.clone(), &entry.word, false);
                    }
                    continue;
                }
                // Two pieces about as long as the word itself, together.
                if let Some(next) = said.get(i + 1) {
                    let joined = format!("{one}{next}");
                    let close = joined.chars().count().abs_diff(length) <= 1;
                    if close
                        && !sounds_alike(next, part)
                        && sounds_alike(&joined, part)
                        && may_sound_like_a_word(&joined, &[one, next], boost)
                    {
                        note(format!("{one} {next}"), &entry.word, false);
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
        let mut words = plain(&["Claude"]);
        words[0].heard_as = vec!["clawed".into()];
        let forgotten: Vec<String> = ["claude", "Zed ", "zed", "cloud→claude", "clawed→claude"].map(String::from).to_vec();
        assert_eq!(clean_forgotten(&forgotten, &words), ["zed", "cloud→claude"]);
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
        // An identifier said as words.
        let code = plain(&["fetchUserProfile", "user_id"]);
        let said = |text: &str| -> Vec<String> { suspects(text, &code).into_iter().map(|s| s.heard).collect() };
        assert_eq!(said("rename fetch user profile and user id"), ["fetch user profile", "user id"]);
        assert!(said("call fetchUserProfile").is_empty());
        // Spelled right already, or only vaguely alike.
        assert!(heard("Ask Claude about the cloud bill.").is_empty());
        assert!(heard("It is cold and I could go.").is_empty());
        assert!(heard("Nothing to see here.").is_empty());
    }

    #[test]
    fn a_word_is_not_suspected_of_being_a_longer_name_it_only_begins() {
        let screen = plain(&["Whisperflow", "Raghunathan"]);
        let heard = |text: &str| -> Vec<String> { suspects(text, &screen).into_iter().map(|s| s.word).collect() };
        assert!(heard("Just a question on the whisper part.").is_empty());
        assert!(!sounds_alike("whisper", "Whisperflow"));
        assert!(!sounds_alike("cloud", "Cloudflare"));
        // A real sound-alike still is one.
        assert_eq!(heard("Tell ragunathan the build passed."), ["Raghunathan"]);
        assert!(sounds_alike("whisperflo", "Whisperflow"));
        // Cut short or made plural by a letter or two.
        assert!(sounds_alike("raghunath", "Raghunathan"));
        assert!(sounds_alike("terraforms", "Terraform"));
    }

    #[test]
    fn built_in_names_are_suspected_only_where_said_the_way_they_are_misheard() {
        // Everyday words that merely sound a bit like a built-in name are
        // not that name: "myself" is not MySQL, "hiking" not Hugging Face.
        let builtin = crate::tech::words();
        let named = |text: &str| -> Vec<String> {
            suspects(text, &builtin).into_iter().map(|s| format!("{} -> {}", s.heard, s.word)).collect()
        };
        for text in [
            "I did it myself.",
            "My muscles ache.",
            "Michael said he would do it himself.",
            "I'll do that myself later.",
            "I'll sort it out myself tomorrow. My muscles are sore.",
            "We had mussels for dinner.",
            "We went hiking at the weekend.",
            "Ask mister Jones.",
            "I was perplexed by it.",
            "Just a cursory look.",
            "Post the letter.",
        ] {
            assert!(named(text).is_empty(), "{text}: {:?}", named(text));
        }
        // How a name is really misheard still points to it.
        assert_eq!(named("Move it off my sequel."), ["my sequel -> MySQL"]);
        assert_eq!(named("the my sql database"), ["my sql -> MySQL"]);
        assert_eq!(named("Deploy it with cooper netties."), ["cooper netties -> Kubernetes"]);
        // The user's own words are still matched by sound.
        let mut own = plain(&["Mysqlo"]);
        own[0].source = Source::Added;
        assert_eq!(suspects("Ask mysqlow.", &own).len(), 1);
    }

    #[test]
    fn everyday_words_are_never_taken_for_a_word_by_sound() {
        // "make" sounds just like Maike, and "pre a" like Priya. Only the
        // second is a likely mishearing: "make" is said all day.
        assert!(sounds_alike("make", "Maike"));
        let mut words = plain(&["Maike", "Priya"]);
        let heard = |text: &str, words: &[Word], boost: Boost| -> Vec<String> {
            suspects_at(text, words, boost).into_iter().map(|s| format!("{} -> {}", s.heard, s.word)).collect()
        };
        assert!(heard("I need to make sure it works.", &words, Boost::Normal).is_empty());
        assert_eq!(heard("Tell pre a it works.", &words, Boost::Normal), ["pre a -> Priya"]);
        // A word the user taught is still found, everyday or not.
        words[0].heard_as = vec!["make".into()];
        assert_eq!(heard("I need to make sure it works.", &words, Boost::Normal), ["make -> Maike"]);
        // The list holds everyday words and their plurals, not names.
        assert!(is_common("make") && is_common("Things") && is_common("people") && is_common("myself"));
        assert!(!is_common("priya") && !is_common("claude") && !is_common("kubernetes") && !is_common("cloud"));
        assert!(common_words().len() > 500 && common_words().len() < 1_500);
    }

    #[test]
    fn a_sound_alike_needs_two_consonants_and_some_length() {
        assert!(may_sound_like_a_word("ragunathan", &["ragunathan"], Boost::Normal));
        assert!(may_sound_like_a_word("prea", &["pre", "a"], Boost::Normal));
        // Too few consonants, or too short, to tell apart from other words.
        assert!(!may_sound_like_a_word("aya", &["aya"], Boost::Strong));
        assert!(!may_sound_like_a_word("eye", &["eye"], Boost::Strong));
        assert!(!may_sound_like_a_word("bra", &["bra"], Boost::Normal));
        assert!(may_sound_like_a_word("bra", &["bra"], Boost::Strong));
        // Two everyday words run together are not a name either; one that
        // is not everyday may be.
        assert!(!may_sound_like_a_word("todaynow", &["today", "now"], Boost::Normal));
        assert!(may_sound_like_a_word("todaynow", &["today", "now"], Boost::Strong));
    }

    #[test]
    fn the_boost_decides_how_far_suspects_are_looked_for() {
        let mut words = plain(&["Maike", "Priya", "fetchUserProfile"]);
        words[1].heard_as = vec!["pria".into()];
        let heard = |text: &str, boost: Boost| -> Vec<String> { suspects_at(text, &words, boost).into_iter().map(|s| s.heard).collect() };
        let text = "Make sure pre a and pria call fetch user profile.";
        assert!(heard(text, Boost::Off).is_empty());
        // Light: taught mishearings, and identifiers said as their words.
        assert_eq!(heard(text, Boost::Light), ["pria", "fetch user profile"]);
        // Normal adds distinctive sound-alikes; Strong everyday ones too.
        assert_eq!(heard(text, Boost::Normal), ["pria", "pre a", "fetch user profile"]);
        assert_eq!(heard(text, Boost::Strong), ["make", "pria", "pre a", "fetch user profile"]);
        // Built-in names go by their listed mishearings at every boost.
        let builtin = crate::tech::words();
        assert!(suspects_at("I did it myself.", &builtin, Boost::Strong).is_empty());
        assert_eq!(suspects_at("off my sequel", &builtin, Boost::Light).len(), 1);
        assert!(suspects_at("off my sequel", &builtin, Boost::Off).is_empty());
    }

    #[test]
    fn hints_for_the_speech_model_follow_the_boost() {
        let mut own = plain(&["Priya", "Kubernetes", "Maike", "Zed", "Ola", "Aoife", "Niamh", "Siobhan", "Tadhg", "Caoimhe"]);
        own[9].heard_as = vec!["keeva".into()];
        let mut builtin = own.clone();
        crate::tech::add_to(&mut builtin);
        let screen = vec!["Raghunathan".to_string(), "priya".to_string()];
        assert!(speech_hints(&own, &screen, Boost::Off).is_empty());
        // Light: a few of the user's own, the ones known to be misheard first.
        let light = speech_hints(&own, &screen, Boost::Light);
        assert_eq!(light.len(), LIGHT_HINTS);
        assert_eq!(light[0], "Caoimhe");
        assert!(!light.contains(&"Raghunathan".to_string()));
        // Normal: all their own, the names on screen, a few tech names, each
        // once; built-in words in the list are not taken as the user's.
        let normal = speech_hints(&builtin, &screen, Boost::Normal);
        assert_eq!(&normal[..10], &own.iter().map(|w| w.word.clone()).collect::<Vec<_>>()[..]);
        assert_eq!(normal[10], "Raghunathan");
        assert_eq!(normal.iter().filter(|h| h.eq_ignore_ascii_case("kubernetes") || h.eq_ignore_ascii_case("priya")).count(), 2);
        assert!(normal.len() <= 10 + 1 + crate::tech::hints(Boost::Normal).len());
        assert!(!normal.iter().any(|h| h == "Claude" || h == "MySQL"));
        assert!(speech_hints(&builtin, &screen, Boost::Strong).len() > normal.len());
        // The default prompt stays short.
        assert!(crate::transcribe::glossary(&speech_hints(&[], &[], Boost::Normal)).len() < 100);
    }

    #[test]
    fn sound_keys_follow_soundex() {
        assert_eq!(sound_key("Robert"), "R163");
        assert_eq!(sound_key("Rupert"), "R163");
        assert_eq!(sound_key("Ashcraft"), "A261");
        assert_eq!(sound_key("Claude"), sound_key("cloud"));
    }

    #[test]
    fn removed_words_are_kept_for_thirty_days_unless_restored() {
        let day = 24 * 60 * 60 * 1000;
        let now = 100 * day;
        let gone = |w: &str, at: u64| Removed { word: Word::new(w, Source::Added), removed_at: at };
        let removed = vec![gone("Kafka", now - day), gone("Jira", now - 31 * day), gone("Ola", now - 2 * day), gone("kafka", now)];
        let kept = clean_removed(&removed, &plain(&["Ola"]), now);
        let names: Vec<&str> = kept.iter().map(|r| r.word.word.as_str()).collect();
        assert_eq!(names, ["Kafka"]);
    }

    #[test]
    fn only_whole_words_are_replaced() {
        assert_eq!(replace_word("Ask cloud. The cloudy cloud!", "cloud", "Claude"), "Ask Claude. The cloudy Claude!");
        assert_eq!(replace_word("Cloud first", "cloud", "Claude"), "Claude first");
        assert_eq!(replace_word("nothing here", "", "x"), "nothing here");
    }
}
