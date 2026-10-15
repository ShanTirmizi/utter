import { useEffect, useRef, useState } from "react";
import { undoLearned, type Learned, type Removed, type Suggestion, type Word } from "../api";
import { useApp } from "../state";
import { IconScreen, IconSearch, IconSparkle, IconX, Notice, Switch } from "../ui";

/** The most words the backend keeps. */
const LIMIT = 200;
/** Above this many words, a search box appears. */
const SEARCH_FROM = 8;

/** A mishearing as the backend stores it: lower case, no edge punctuation. */
function heardForm(text: string): string {
  return text
    .trim()
    .replace(/\s+/g, " ")
    .replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "")
    .toLowerCase();
}

/** How a removed mishearing is kept on the forgotten list, as the backend does. */
function forgottenPair(heard: string, word: string): string {
  return `${heardForm(heard)}→${word.trim().replace(/\s+/g, " ").toLowerCase()}`;
}

/** "Removed today", "Removed yesterday", "Removed 12 Oct". */
function removedWhen(at: number): string {
  const day = (t: number) => new Date(t).toDateString();
  if (day(at) === day(Date.now())) return "Removed today";
  if (day(at) === day(Date.now() - 86_400_000)) return "Removed yesterday";
  return `Removed ${new Date(at).toLocaleDateString("en-GB", { day: "numeric", month: "short" })}`;
}

/**
 * The user's own words: names, products and jargon. The speech model is told
 * to listen for them, and cleanup puts right what it tends to hear instead.
 */
export default function WordsSection() {
  const { settings, update, adopt, addWordAsked, justLearned, dismissLearned } = useApp();
  const [draft, setDraft] = useState("");
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  // The word whose "heard as" box is open, by its spelling.
  const [teaching, setTeaching] = useState<string | null>(null);
  const [showRemoved, setShowRemoved] = useState(false);
  const addBox = useRef<HTMLInputElement>(null);
  const words = settings.dictionary;
  const full = words.length >= LIMIT;

  // "Add a Word…" from the menu bar lands here, ready to type.
  useEffect(() => {
    if (addWordAsked) addBox.current?.focus();
  }, [addWordAsked]);

  const run = (work: Promise<unknown>) =>
    work.then(
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
    run(update({ dictionary: [...fresh.map((word) => ({ word, heardAs: [], source: "added" as const, addedAt: now })), ...words] }));
    // One new word: ask straight away what Utter writes instead.
    if (fresh.length === 1) setTeaching(fresh[0]);
  };

  const remove = (word: Word) => {
    if (teaching === word.word) setTeaching(null);
    // Kept for 30 days so it can be restored, and never learned again meanwhile.
    run(
      update({
        dictionary: words.filter((w) => w !== word),
        forgotten: [...settings.forgotten, word.word.toLowerCase()],
        removed: [
          { word, removedAt: Date.now() },
          ...settings.removed.filter((r) => r.word.word.toLowerCase() !== word.word.toLowerCase()),
        ],
      }),
    );
  };

  const restore = (gone: Removed) =>
    run(
      update({
        dictionary: [gone.word, ...words],
        forgotten: settings.forgotten.filter((f) => f !== gone.word.word.toLowerCase()),
        removed: settings.removed.filter((r) => r !== gone),
      }),
    );

  const setHeard = (word: Word, heardAs: string[]) => {
    // A mishearing taken off is not learned back.
    const dropped = word.heardAs.filter((h) => !heardAs.includes(h)).map((h) => forgottenPair(h, word.word));
    run(
      update({
        dictionary: words.map((w) => (w === word ? { ...w, heardAs } : w)),
        forgotten: [...settings.forgotten, ...dropped],
      }),
    );
  };

  const undo = (learned: Learned) => run(undoLearned(learned).then(adopt));

  // Offered once the speech model got one wrong, or it was said twice.
  const offered = settings.suggested.filter((s) => s.heard !== null || s.seen >= 2);
  const accept = (suggestion: Suggestion) =>
    run(
      update({
        dictionary: [
          { word: suggestion.word, heardAs: suggestion.heard ? [suggestion.heard] : [], source: "screen", addedAt: Date.now() },
          ...words,
        ],
        suggested: settings.suggested.filter((s) => s !== suggestion),
      }),
    );
  const dismiss = (suggestion: Suggestion) =>
    run(
      update({
        suggested: settings.suggested.filter((s) => s !== suggestion),
        forgotten: [...settings.forgotten, suggestion.word.toLowerCase()],
      }),
    );

  const needle = query.trim().toLowerCase();
  const shown = needle ? words.filter((w) => `${w.word} ${w.heardAs.join(" ")}`.toLowerCase().includes(needle)) : words;
  const justNow = (w: Word) => justLearned.some((l) => l.newWord && l.word.toLowerCase() === w.word.toLowerCase());

  return (
    <>
      <div className="qhead">
        <div className="t">
          <h1>
            Your words
            {words.length > 0 && <span className="qtag nt sm">{words.length}</span>}
          </h1>
          <p>Names and terms Utter should spell your way.</p>
        </div>
      </div>

      {justLearned.length > 0 && (
        <Notice
          tone="mi"
          icon={<IconSparkle big />}
          action={
            <button className="qx" aria-label="Dismiss" onClick={dismissLearned}>
              <IconX />
            </button>
          }
        >
          <b>Learned from your corrections</b>
          <ul>
            {justLearned.map((l) => (
              <li key={`${l.word}|${l.heard}`}>
                <span>
                  {l.word}, heard as “{l.heard}”
                </span>
                <span className="sp" />
                <button className="qbtn sm ghost" onClick={() => undo(l)}>
                  Undo
                </button>
              </li>
            ))}
          </ul>
        </Notice>
      )}

      <div className="qcard tight">
        <div className="qfld" style={{ margin: 0 }}>
          <input
            ref={addBox}
            placeholder="Add a word, like Claude or Priya"
            aria-label="Add a word"
            value={draft}
            maxLength={400}
            spellCheck={false}
            disabled={full}
            onChange={(e) => setDraft(e.target.value)}
            // Enter that confirms an input-method candidate is not "add".
            onKeyDown={(e) => e.key === "Enter" && !e.nativeEvent.isComposing && draft.trim() && add()}
          />
          <button className="qbtn" disabled={!draft.trim() || full} onClick={add}>
            Add
          </button>
        </div>
        <p className="qp" style={{ fontSize: 12.5, marginTop: 8 }}>
          {full ? "That’s the most Utter keeps. Remove one to add another." : "Add several at once with commas."}
        </p>
      </div>

      {words.length === 0 ? (
        <div className="qcard">
          <div className="qbig md lt" style={{ marginTop: 0 }}>
            No words yet
          </div>
          <p className="qp">Add names, products and jargon Utter gets wrong, and it spells them your way from then on.</p>
          <p className="qp">Or fix a word right after a dictation, and Utter learns it for you.</p>
        </div>
      ) : (
        <div className="qcard">
          {words.length > SEARCH_FROM && (
            <label className="qsrch" style={{ marginTop: 0 }}>
              <IconSearch />
              <input
                type="search"
                placeholder="Search your words"
                aria-label="Search your words"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
              <span className="n">{needle ? `${shown.length} found` : `${words.length} of ${LIMIT}`}</span>
            </label>
          )}
          {shown.length === 0 ? (
            <p className="qp">Nothing matches.</p>
          ) : (
            <div className="qlist" aria-label="Your words">
              {shown.map((w) => (
                <WordRow
                  key={w.word}
                  word={w}
                  justLearned={justNow(w)}
                  teaching={teaching === w.word}
                  onTeach={(open) => setTeaching(open ? w.word : null)}
                  onHeard={(heardAs) => setHeard(w, heardAs)}
                  onRemove={() => remove(w)}
                />
              ))}
            </div>
          )}
        </div>
      )}

      {offered.length > 0 && (
        <div className="qcard">
          <div className="qlab">
            Suggested from your screen
            {offered[0].app && <span className="r">Seen in {offered[0].app}</span>}
          </div>
          <div className="qlist" aria-label="Suggested words">
            {offered.map((s) => (
              <div className="qli w" key={s.word}>
                <span className="wd">{s.word}</span>
                <span className="hv">
                  <span className="qnote" style={{ fontSize: 12.5 }}>
                    {s.heard ? `Heard as “${s.heard}”` : `Said ${s.seen} times`}
                  </span>
                </span>
                <button className="qbtn sm" onClick={() => accept(s)}>
                  Add
                </button>
                <button className="qx" aria-label={`Never suggest ${s.word}`} title="Never suggest it again" onClick={() => dismiss(s)}>
                  <IconX />
                </button>
              </div>
            ))}
          </div>
          <p className="qp" style={{ fontSize: 12.5 }}>
            × means never suggest it again.
          </p>
        </div>
      )}

      <div className="qcard tight">
        <div className="qsw">
          <div className="tx">
            <b>Learn from my corrections</b>
            <span>When you fix a word, Utter adds it here and tells you. You can always undo it.</span>
          </div>
          <Switch label="Learn from my corrections" checked={settings.learnWords} onChange={(learnWords) => run(update({ learnWords }))} />
        </div>
        <div className="qsw">
          <div className="tx">
            <b>Use names on screen</b>
            <span>Reads names in the app you’re dictating into so it spells them right. Nothing leaves this Mac.</span>
          </div>
          <Switch label="Use names on screen" checked={settings.screenWords} onChange={(screenWords) => run(update({ screenWords }))} />
        </div>
      </div>

      {settings.removed.length > 0 &&
        (showRemoved ? (
          <div className="qcard">
            <div className="qrow">
              <div className="qlab">Removed recently</div>
              <span className="sp" />
              <button className="qbtn sm ghost" onClick={() => setShowRemoved(false)}>
                Hide
              </button>
            </div>
            <div className="qlist" aria-label="Removed words">
              {settings.removed.map((r) => (
                <div className="qli w gone" key={r.word.word}>
                  <span className="wd">{r.word.word}</span>
                  <span className="hv">
                    <span className="qnote" style={{ fontSize: 12.5 }}>
                      {removedWhen(r.removedAt)}
                    </span>
                  </span>
                  <button className="qbtn sm" disabled={full} onClick={() => restore(r)}>
                    Restore
                  </button>
                </div>
              ))}
            </div>
            <p className="qp" style={{ fontSize: 12.5 }}>
              Kept for 30 days with their heard-as spellings, then gone for good.
            </p>
          </div>
        ) : (
          <div className="qcard tight">
            <div className="qrow">
              <div className="qlab">
                Removed recently
                <span className="qtag nt sm" style={{ marginLeft: 2 }}>
                  {settings.removed.length}
                </span>
              </div>
              <span className="sp" />
              <button className="qbtn sm" onClick={() => setShowRemoved(true)}>
                Show
              </button>
            </div>
          </div>
        ))}

      {error && (
        <Notice tone="pe">
          <b>Not saved.</b> <span>{error}</span>
        </Notice>
      )}

      <p className="qp" style={{ margin: "0 4px" }}>
        Tip: right after a dictation, choose Fix Last Word… in the menu bar.
      </p>
    </>
  );
}

function WordRow({
  word,
  justLearned,
  teaching,
  onTeach,
  onHeard,
  onRemove,
}: {
  word: Word;
  justLearned: boolean;
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
    <div className="qli w">
      <span className="wd" title={word.word}>
        {word.word}
      </span>
      <span className="hv">
        {word.heardAs.map((h) => (
          <span key={h} className="qhv">
            {h}
            <button className="qx" aria-label={`Remove ${h}`} onClick={() => onHeard(word.heardAs.filter((x) => x !== h))}>
              <IconX />
            </button>
          </span>
        ))}
        {!teaching && (
          <button className="qhv add" aria-label={`Add what ${word.word} is heard as`} onClick={() => onTeach(true)}>
            {word.heardAs.length ? "+" : "+ heard as…"}
          </button>
        )}
      </span>
      {justLearned ? (
        <span className="qtag sm mi">
          <IconSparkle />
          Just learned
        </span>
      ) : word.source === "learned" ? (
        <span className="qtag sm nt">
          <IconSparkle />
          Learned
        </span>
      ) : word.source === "screen" ? (
        <span className="qtag sm nt">
          <IconScreen />
          From screen
        </span>
      ) : null}
      <button className="qx" aria-label={`Remove ${word.word}`} title="Remove" onClick={onRemove}>
        <IconX />
      </button>
      {teaching && (
        <div className="under">
          <p className="qp" style={{ margin: "0 0 8px" }}>
            What does Utter write instead of {word.word}?
          </p>
          <div className="qfld mono" style={{ margin: 0, height: 38 }}>
            <input
              autoFocus
              aria-label="Heard as"
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
            <button className="qbtn sm" disabled={!draft.trim()} onClick={addHeard}>
              Add
            </button>
            <button className="qbtn sm ghost" onClick={() => onTeach(false)}>
              Done
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
