import { useEffect, useRef, useState } from "react";
import type { Word } from "../api";
import { useApp } from "../state";

/** The most words the backend keeps. */
const LIMIT = 200;
/** Above this many words, a search box appears. */
const SEARCH_FROM = 8;

const SOURCE_LABEL: Record<Word["source"], string> = {
  added: "added by you",
  learned: "learned",
  screen: "from screen",
};

/** A mishearing as the backend stores it: lower case, no edge punctuation. */
function heardForm(text: string): string {
  return text
    .trim()
    .replace(/\s+/g, " ")
    .replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "")
    .toLowerCase();
}

/**
 * The user's own words: names, products and jargon. The speech model is told
 * to listen for them, and cleanup puts right what it tends to hear instead.
 */
export default function WordsSection() {
  const { settings, update, addWordAsked, setTab } = useApp();
  const [draft, setDraft] = useState("");
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  // The word whose "heard as" box is open, by its spelling.
  const [teaching, setTeaching] = useState<string | null>(null);
  const addBox = useRef<HTMLInputElement>(null);
  const words = settings.dictionary;
  const full = words.length >= LIMIT;

  // "Add a Word…" from the menu bar lands here, ready to type.
  useEffect(() => {
    if (addWordAsked) addBox.current?.focus();
  }, [addWordAsked]);

  const save = (dictionary: Word[], forgotten = settings.forgotten) =>
    update({ dictionary, forgotten }).then(
      () => setError(null),
      (err) => setError(String(err)),
    );

  const add = () => {
    // Several at once, separated by commas or new lines.
    const typed = draft
      .split(/[,\n]/)
      .map((t) => t.trim().replace(/\s+/g, " "))
      .filter(Boolean);
    const fresh = typed.filter(
      (t, i) =>
        !words.some((have) => have.word.toLowerCase() === t.toLowerCase()) &&
        typed.findIndex((other) => other.toLowerCase() === t.toLowerCase()) === i,
    );
    setDraft("");
    if (!fresh.length) return;
    const now = Date.now();
    save([...fresh.map((word) => ({ word, heardAs: [], source: "added" as const, addedAt: now })), ...words]);
    // One new word: ask straight away what Utter writes instead.
    if (fresh.length === 1) setTeaching(fresh[0]);
  };

  const remove = (word: Word) => {
    if (teaching === word.word) setTeaching(null);
    // A removed word is never learned or suggested again.
    save(
      words.filter((w) => w !== word),
      [...settings.forgotten, word.word.toLowerCase()],
    );
  };

  const setHeard = (word: Word, heardAs: string[]) =>
    save(words.map((w) => (w === word ? { ...w, heardAs } : w)));

  const needle = query.trim().toLowerCase();
  const shown = needle
    ? words.filter((w) => `${w.word} ${w.heardAs.join(" ")}`.toLowerCase().includes(needle))
    : words;

  return (
    <>
      <div className="sechead">
        <h2 className="h">Your words</h2>
        <span className="chip plain">
          {words.length} {words.length === 1 ? "word" : "words"}
        </span>
      </div>
      <p className="sub">
        Names and jargon Utter should spell your way. Tell it what it writes by mistake, and it fixes that
        whenever the sentence is about your word.
      </p>

      <div className="card stack">
        <div className="frow">
          <label style={{ flex: 1 }}>
            <span className="sr">A name or term to add</span>
            <input
              ref={addBox}
              className="input"
              type="text"
              placeholder="Add a word, like Claude or Priya"
              value={draft}
              maxLength={400}
              spellCheck={false}
              disabled={full}
              onChange={(e) => setDraft(e.target.value)}
              // Enter that confirms an input-method candidate is not "add".
              onKeyDown={(e) => e.key === "Enter" && !e.nativeEvent.isComposing && draft.trim() && add()}
            />
          </label>
          <button className="btn primary" disabled={!draft.trim() || full} onClick={add}>
            Add
          </button>
        </div>
        {full && <p className="hint">That is the most Utter keeps. Remove one to add another.</p>}

        {words.length > SEARCH_FROM && (
          <label>
            <span className="sr">Search your words</span>
            <input
              className="input"
              type="search"
              placeholder="Search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </label>
        )}

        {words.length === 0 ? (
          <div className="wempty">
            <p>
              <b>No words yet.</b> Add a name Utter gets wrong. Say it writes <span className="mono">cloud</span>{" "}
              when you mean <b>Claude</b>: add Claude, then tell it “heard as cloud”.
            </p>
          </div>
        ) : shown.length === 0 ? (
          <p className="hint">Nothing matches.</p>
        ) : (
          <ul className="wlist" aria-label="Your words">
            {shown.map((w) => (
              <WordRow
                key={w.word}
                word={w}
                teaching={teaching === w.word}
                onTeach={(open) => setTeaching(open ? w.word : null)}
                onHeard={(heardAs) => setHeard(w, heardAs)}
                onRemove={() => remove(w)}
              />
            ))}
          </ul>
        )}

        {error && (
          <p className="hint" role="alert" style={{ color: "var(--bad)" }}>
            Not saved: {error}
          </p>
        )}
      </div>

      <p className="foot">
        Utter got a word wrong just now? In{" "}
        <button className="link" onClick={() => setTab("try")}>
          Try it
        </button>
        , press Fix on that dictation and click the word. It is corrected there and remembered here.
      </p>
    </>
  );
}

function WordRow({
  word,
  teaching,
  onTeach,
  onHeard,
  onRemove,
}: {
  word: Word;
  teaching: boolean;
  onTeach: (open: boolean) => void;
  onHeard: (heardAs: string[]) => void;
  onRemove: () => void;
}) {
  const [draft, setDraft] = useState("");

  const addHeard = () => {
    const fresh = draft
      .split(",")
      .map(heardForm)
      .filter((h) => h && h !== word.word.toLowerCase() && !word.heardAs.includes(h));
    setDraft("");
    if (fresh.length) onHeard([...word.heardAs, ...fresh]);
  };

  return (
    <li className="wrow">
      <div className="wmain">
        <span className="wword">{word.word}</span>
        <span className="wheard">
          {word.heardAs.length > 0 && <span className="wlab">heard as</span>}
          {word.heardAs.map((h) => (
            <span key={h} className="wh">
              {h}
              <button
                aria-label={`Stop treating ${h} as ${word.word}`}
                onClick={() => onHeard(word.heardAs.filter((x) => x !== h))}
              >
                ×
              </button>
            </span>
          ))}
          {!teaching && (
            <button className="wadd" onClick={() => onTeach(true)} aria-label={`Add what ${word.word} is heard as`}>
              {word.heardAs.length ? "+" : "+ heard as…"}
            </button>
          )}
        </span>
        {word.source !== "added" && <span className={`wsrc ${word.source}`}>{SOURCE_LABEL[word.source]}</span>}
        <button className="wdel" aria-label={`Remove ${word.word}`} onClick={onRemove}>
          ×
        </button>
      </div>
      {teaching && (
        <div className="wteach">
          <label style={{ flex: 1 }}>
            <span className="sr">What Utter writes instead of {word.word}</span>
            <input
              className="input sm"
              type="text"
              autoFocus
              placeholder={`What does Utter write instead of ${word.word}?`}
              value={draft}
              maxLength={200}
              spellCheck={false}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") onTeach(false);
                if (e.key === "Enter" && !e.nativeEvent.isComposing) {
                  if (draft.trim()) addHeard();
                  else onTeach(false);
                }
              }}
            />
          </label>
          <button className="btn sm" disabled={!draft.trim()} onClick={addHeard}>
            Add
          </button>
          <button className="btn sm quiet" onClick={() => onTeach(false)}>
            Done
          </button>
        </div>
      )}
    </li>
  );
}
