//! How accurate dictation is, measured the way it is felt: did the user have
//! to change what Utter typed?
//!
//! Right after a paste, `learn::watch` follows the text box. When the user is
//! done with it, `edit` works out how many words of the pasted text they
//! changed. Most dictations should need none; the share that needed none is
//! the number worth improving. The replay harness
//! (`examples/replay.rs`) scores other speech models and prompts with the
//! same counts, plus a word error rate.

use serde::{Deserialize, Serialize};

/// What the user did to one pasted dictation once they had finished with it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Edit {
    /// Words in what Utter pasted.
    pub words: usize,
    /// How many words the user changed, added or took out inside it: 0 when
    /// they left it as it was.
    pub changed: usize,
    /// The pasted text as the user left it. Kept only next to a saved
    /// recording, so the recording can be scored against it later.
    pub corrected: Option<String>,
    /// True when Utter never saw how the user left it in the text box, and
    /// all it knows is a word fixed later with Fix a word. Such a dictation
    /// is not one Utter followed, so the edit rate leaves it out.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unwatched: bool,
}

/// Word-sized pieces of text, split at white space.
fn tokens(text: &str) -> Vec<&str> {
    text.split_whitespace().collect()
}

/// The fewest words to add, remove or swap to turn `from` into `to`.
pub fn word_distance<T: PartialEq>(from: &[T], to: &[T]) -> usize {
    // One row of the usual table at a time.
    let mut above: Vec<usize> = (0..=to.len()).collect();
    for (i, a) in from.iter().enumerate() {
        let mut row = vec![i + 1; to.len() + 1];
        for (j, b) in to.iter().enumerate() {
            let swap = above[j] + usize::from(a != b);
            row[j + 1] = swap.min(above[j + 1] + 1).min(row[j] + 1);
        }
        above = row;
    }
    above[to.len()]
}

/// Longest stretch of words compared. Past this the box was replaced, not
/// edited, and lining it up would take too long.
const MOST_WORDS: usize = 1_500;

/// The fewest words to add, remove or swap to turn `from` into some run of
/// words in `to`, and where that run is: words of `to` before or after it
/// cost nothing. Of runs that cost the same, the longest is taken, so a word
/// swapped at either end counts as swapped rather than as taken out.
fn closest_run<T: PartialEq>(from: &[T], to: &[T]) -> (usize, std::ops::Range<usize>) {
    // The usual table one row at a time, each cell with the cost so far and
    // where in `to` its run starts. The first row is free anywhere.
    let mut above: Vec<(usize, usize)> = (0..=to.len()).map(|j| (0, j)).collect();
    for (i, a) in from.iter().enumerate() {
        let mut row = vec![(i + 1, 0); to.len() + 1];
        for (j, b) in to.iter().enumerate() {
            let swap = (above[j].0 + usize::from(a != b), above[j].1);
            let take_out = (above[j + 1].0 + 1, above[j + 1].1);
            let add = (row[j].0 + 1, row[j].1);
            // Cheapest first, then the earliest start: the longer run.
            row[j + 1] = swap.min(take_out).min(add);
        }
        above = row;
    }
    let (end, &(cost, start)) = above
        .iter()
        .enumerate()
        .min_by_key(|&(end, &(cost, start))| (cost, std::cmp::Reverse(end - start)))
        .expect("the row always has a first cell");
    (cost, start..end)
}

/// What the user changed in the part of a text box Utter pasted. `before` is
/// the box just after the paste and `after` its final state. `None` when that
/// cannot be told: the paste is not in `before`, or the user also changed
/// their own text around it, so where the dictation ends is unclear.
///
/// Carrying on typing before or after the dictation is not an edit of it,
/// even when the user also fixed a word inside it.
pub fn edit(pasted: &str, before: &str, after: &str) -> Option<Edit> {
    let pasted_words = tokens(pasted);
    if pasted_words.is_empty() || pasted_words.len() > MOST_WORDS {
        return None;
    }
    let (old, new) = (tokens(before), tokens(after));
    // Where the paste sits in the box, in words: the last place it appears,
    // since it was just added and is most likely the end of what is there.
    let at = (0..=old.len().checked_sub(pasted_words.len())?)
        .rev()
        .find(|&at| old[at..at + pasted_words.len()] == pasted_words[..])?;
    let (ahead, behind) = (at, old.len() - at - pasted_words.len());

    // The user's own text before and after the paste must be as it was.
    let same_start = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    if same_start < ahead {
        return None;
    }
    let same_end = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(new.len() - ahead)
        .take_while(|(a, b)| a == b)
        .count();
    if same_end < behind {
        return None;
    }
    // What is now between the user's own text: the dictation, and anything
    // typed straight before or after it. Only this is lined up, so a long
    // box with a short dictation in it is quick.
    let now = &new[ahead..new.len() - behind];
    if now.len() > MOST_WORDS {
        return None;
    }

    // Words typed straight before or after the dictation are the user
    // carrying on, not correcting it, so they cost nothing.
    let (changed, run) = closest_run(&pasted_words, now);
    Some(Edit {
        words: pasted_words.len(),
        changed,
        corrected: Some(now[run].join(" ")),
        unwatched: false,
    })
}

/// The edit after the user also corrected a dictation with Fix a word, which
/// turned `before` into `after`. Fixing it means it was not right as typed.
/// With no earlier edit, Utter never saw the user finish with it in the text
/// box, so the edit is marked as unwatched and left out of the edit rate.
pub fn fixed_later(edit: Option<Edit>, before: &str, after: &str, keep_text: bool) -> Edit {
    let (from, to) = (tokens(before), tokens(after));
    let mut edit = edit.unwrap_or_else(|| Edit { words: from.len(), unwatched: true, ..Edit::default() });
    edit.changed += word_distance(&from, &to);
    edit.corrected = keep_text.then(|| to.join(" "));
    edit
}

/// What the watch saw, added to what was already known: a word fixed with
/// Fix a word before the user had finished with the text box. The counts add
/// up. The text kept is the box's when the user changed it there, since that
/// is what they ended with; otherwise the fixed one.
pub fn merge(earlier: Option<Edit>, watched: Edit) -> Edit {
    let Some(earlier) = earlier else { return watched };
    let corrected = if watched.changed > 0 { watched.corrected } else { earlier.corrected.or(watched.corrected) };
    Edit {
        words: watched.words,
        changed: earlier.changed + watched.changed,
        corrected,
        // Watched now, so it counts.
        unwatched: false,
    }
}

/// Words as they are compared for a word error rate: lower case, without
/// the punctuation around them, so "Hello," and "hello" are the same word,
/// and without hyphens, so "stand-up" and "standup" are too.
pub fn normalise(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            word.replace(['’', '‘'], "'")
                .replace('-', "")
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

/// How far one piece of text is from what it should have been.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Score {
    /// Words in the reference, after `normalise`.
    pub words: usize,
    /// Word errors, ignoring case and punctuation: the top of a word error rate.
    pub errors: usize,
    /// Words the user would have to change, exactly as typed: what the app
    /// counts as an edit.
    pub edits: usize,
}

impl Score {
    pub fn of(written: &str, reference: &str) -> Self {
        let (heard, meant) = (normalise(written), normalise(reference));
        Self {
            words: meant.len(),
            errors: word_distance(&meant, &heard),
            edits: word_distance(&tokens(reference), &tokens(written)),
        }
    }

    /// Word errors per reference word, over everything added together.
    pub fn error_rate(&self) -> f64 {
        if self.words == 0 { 0.0 } else { self.errors as f64 / self.words as f64 }
    }
}

impl std::ops::AddAssign for Score {
    fn add_assign(&mut self, other: Self) {
        self.words += other.words;
        self.errors += other.errors;
        self.edits += other.edits;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed(pasted: &str, before: &str, after: &str) -> Option<(usize, String)> {
        edit(pasted, before, after).map(|e| (e.changed, e.corrected.unwrap()))
    }

    #[test]
    fn distance_counts_added_removed_and_swapped_words() {
        let words = |text: &str| tokens(text).into_iter().map(String::from).collect::<Vec<_>>();
        assert_eq!(word_distance(&words("a b c"), &words("a b c")), 0);
        assert_eq!(word_distance(&words("ask cloud now"), &words("ask Claude now")), 1);
        assert_eq!(word_distance(&words("a b c"), &words("a c")), 1);
        assert_eq!(word_distance(&words(""), &words("a b")), 2);
        assert_eq!(word_distance(&words("a b"), &words("")), 2);
    }

    #[test]
    fn a_dictation_left_alone_has_no_edits() {
        let pasted = "Can you ask Claude to summarise the notes? ";
        let box_text = "Hi Sam. Can you ask Claude to summarise the notes? ";
        assert_eq!(changed(pasted, box_text, box_text), Some((0, pasted.trim().to_string())));
        // Line breaks and spacing that the app changed are no edit.
        assert_eq!(changed(pasted, box_text, "Hi Sam.\nCan you ask Claude  to summarise the notes?").unwrap().0, 0);
    }

    #[test]
    fn edits_inside_the_paste_are_counted() {
        let pasted = "Can you ask cloud to summarise the notes?";
        let before = "Hi Sam. Can you ask cloud to summarise the notes?";
        assert_eq!(
            changed(pasted, before, "Hi Sam. Can you ask Claude to summarise the notes?"),
            Some((1, "Can you ask Claude to summarise the notes?".to_string()))
        );
        // Punctuation counts: it is still something the user had to fix.
        assert_eq!(changed(pasted, before, "Hi Sam. Can you ask cloud to summarise the notes.").unwrap().0, 1);
        // Taking words out.
        assert_eq!(changed(pasted, before, "Hi Sam. Can you summarise the notes?").unwrap().0, 3);
        // Rewriting all of it.
        assert_eq!(changed(pasted, before, "Hi Sam. Notes please.").unwrap().0, 8);
    }

    #[test]
    fn carrying_on_typing_is_not_an_edit() {
        let pasted = "Ask Claude about it.";
        let before = "Hi Sam. Ask Claude about it.";
        assert_eq!(changed(pasted, before, "Hi Sam. Ask Claude about it. Thanks!").unwrap().0, 0);
        assert_eq!(changed(pasted, pasted, "Quick one: Ask Claude about it.").unwrap().0, 0);
    }

    #[test]
    fn typing_around_a_fixed_dictation_is_not_counted_or_kept() {
        let pasted = "Ask cloud about it.";
        let before = "Hi Sam. Ask cloud about it.";
        // A word fixed, and a sentence typed after it.
        assert_eq!(
            changed(pasted, before, "Hi Sam. Ask Claude about it. Thanks Sam, see you tomorrow at the office."),
            Some((1, "Ask Claude about it.".to_string()))
        );
        // Or before it.
        assert_eq!(
            changed(pasted, pasted, "Quick one for you. Ask Claude about it."),
            Some((1, "Ask Claude about it.".to_string()))
        );
        // A word swapped at the very end is a swap, not a word taken out.
        assert_eq!(changed(pasted, pasted, "Ask cloud about this."), Some((1, "Ask cloud about this.".to_string())));
    }

    #[test]
    fn a_short_dictation_in_a_long_box_is_still_measured() {
        let own = "word ".repeat(MOST_WORDS * 2);
        let before = format!("{own}Ask cloud about it.");
        let after = format!("{own}Ask Claude about it. Thanks!");
        assert_eq!(changed("Ask cloud about it.", &before, &after), Some((1, "Ask Claude about it.".to_string())));
    }

    #[test]
    fn unclear_boxes_say_nothing() {
        // The paste is not there.
        assert_eq!(edit("ask Claude", "something else", "something new"), None);
        // The user changed their own text before the paste too.
        assert_eq!(edit("ask cloud about it", "Hi Sam, ask cloud about it", "Hi Samantha, ask Claude about it"), None);
        // Or after it.
        assert_eq!(edit("ask cloud.", "ask cloud. Thanks, Sam", "ask Claude. Thanks"), None);
        assert_eq!(edit("", "a", "b"), None);
        // The box emptied: everything is gone, including the paste.
        assert_eq!(edit("ask Claude", "ask Claude", "").unwrap().changed, 2);
    }

    #[test]
    fn the_last_copy_of_a_repeated_paste_is_the_one_compared() {
        let before = "Thanks. Thanks.";
        assert_eq!(changed("Thanks.", before, "Thanks. Thanks!").unwrap(), (1, "Thanks!".to_string()));
    }

    #[test]
    fn a_later_fix_counts_as_an_edit() {
        let watched = Edit { words: 3, changed: 0, corrected: None, unwatched: false };
        let edit = fixed_later(Some(watched), "ask cloud now", "ask Claude now", true);
        assert_eq!(edit, Edit { words: 3, changed: 1, corrected: Some("ask Claude now".into()), unwatched: false });
        let again = fixed_later(Some(edit), "ask Claude now", "ask Claude right now", false);
        assert_eq!(again.changed, 2);
        assert_eq!(again.corrected, None);
        assert!(!again.unwatched);
    }

    #[test]
    fn a_fix_to_a_dictation_nobody_watched_is_marked_so() {
        let edit = fixed_later(None, "ask cloud now", "ask Claude now", true);
        assert_eq!(edit, Edit { words: 3, changed: 1, corrected: Some("ask Claude now".into()), unwatched: true });
        // A second fix leaves it unwatched.
        assert!(fixed_later(Some(edit), "ask Claude now", "ask Claude right now", true).unwatched);
        // And it is not saved as watched, but an ordinary edit says nothing.
        let saved = serde_json::to_string(&Edit { unwatched: true, ..Edit::default() }).unwrap();
        assert!(saved.contains("\"unwatched\":true"));
        assert!(!serde_json::to_string(&Edit::default()).unwrap().contains("unwatched"));
        let old: Edit = serde_json::from_str(r#"{"words":3,"changed":0,"corrected":null}"#).unwrap();
        assert!(!old.unwatched);
    }

    #[test]
    fn what_the_watch_saw_adds_to_an_earlier_fix() {
        let watched = |changed: usize, corrected: &str| Edit {
            words: 3,
            changed,
            corrected: Some(corrected.into()),
            unwatched: false,
        };
        // Nothing known before: the watch's edit as it is.
        assert_eq!(merge(None, watched(1, "ask Claude now")), watched(1, "ask Claude now"));
        // Fixed in Utter first, then left alone in the box: the fix stays.
        let fixed = fixed_later(None, "ask cloud now", "ask Claude now", true);
        assert_eq!(merge(Some(fixed.clone()), watched(0, "ask cloud now")), watched(1, "ask Claude now"));
        // Changed in the box too: the counts add up, and the box's text is kept.
        assert_eq!(merge(Some(fixed), watched(2, "ask Claude right now")), watched(3, "ask Claude right now"));
    }

    #[test]
    fn word_error_rate_ignores_case_and_punctuation() {
        assert_eq!(normalise("Hello, World! It’s “fine”."), ["hello", "world", "it's", "fine"]);
        assert_eq!(normalise("the stand-up - at 9"), ["the", "standup", "at", "9"]);
        let score = Score::of("hello world, it's fine", "Hello, world. It's fine.");
        assert_eq!(score.errors, 0);
        assert_eq!(score.words, 4);
        // But the edits a user would make do: every word differs as typed.
        assert_eq!(score.edits, 4);
        let wrong = Score::of("ask cloud to help", "Ask Claude to help.");
        assert_eq!((wrong.errors, wrong.words), (1, 4));
        assert!((wrong.error_rate() - 0.25).abs() < 1e-9);
        let mut total = Score::default();
        total += wrong;
        total += score;
        assert_eq!((total.errors, total.words), (1, 8));
        assert_eq!(Score::default().error_rate(), 0.0);
    }
}
