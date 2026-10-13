import { useState } from "react";
import { useApp } from "../state";

/** The most terms the backend keeps. */
const LIMIT = 200;

/**
 * The user's own words: names, products, jargon. The speech model is told to
 * listen for them, and cleanup is told how they are spelled.
 */
export default function DictionaryCard() {
  const { settings, update } = useApp();
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const terms = settings.dictionary;
  const full = terms.length >= LIMIT;

  const save = (next: string[]) =>
    update({ dictionary: next }).then(
      () => setError(null),
      (err) => setError(String(err)),
    );

  const add = () => {
    // Several at once, separated by commas or new lines.
    const fresh = draft
      .split(/[,\n]/)
      .map((t) => t.trim())
      .filter((t) => t && !terms.some((have) => have.toLowerCase() === t.toLowerCase()));
    setDraft("");
    if (fresh.length) save([...terms, ...fresh]);
  };

  return (
    <div className="card stack">
      <div className="cardhead">
        <span className="label">Your words</span>
        <span className="meta">
          {terms.length} of {LIMIT}
        </span>
      </div>
      <p className="hint">
        Names, products and jargon that are often misheard. Utter listens for them and spells them the way
        you write them here.
      </p>
      <div className="frow">
        <label style={{ flex: 1 }}>
          <span className="sr">A name or term to add</span>
          <input
            className="input"
            type="text"
            placeholder="Priya, Kubernetes, Q3 roadmap"
            value={draft}
            maxLength={400}
            spellCheck={false}
            disabled={full}
            onChange={(e) => setDraft(e.target.value)}
            // Enter that confirms an input-method candidate is not "add".
            onKeyDown={(e) => e.key === "Enter" && !e.nativeEvent.isComposing && draft.trim() && add()}
          />
        </label>
        <button className="btn" disabled={!draft.trim() || full} onClick={add}>
          Add
        </button>
      </div>
      {terms.length > 0 && (
        <ul className="terms" aria-label="Your words">
          {terms.map((term) => (
            <li key={term} className="term">
              <span>{term}</span>
              <button aria-label={`Remove ${term}`} onClick={() => save(terms.filter((t) => t !== term))}>
                ×
              </button>
            </li>
          ))}
        </ul>
      )}
      {full && <p className="hint">That is the most Utter keeps. Remove one to add another.</p>}
      {error && (
        <p className="hint" role="alert" style={{ color: "var(--bad)" }}>
          Not saved: {error}
        </p>
      )}
    </div>
  );
}
