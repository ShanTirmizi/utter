import { Fragment, useEffect, useState } from "react";
import { fixWord, type Dictation } from "../api";
import { useApp } from "../state";

/** Words and the spaces and punctuation between them. */
function tokens(text: string): { text: string; word: boolean }[] {
  return (text.match(/[\p{L}\p{N}_'’.-]*[\p{L}\p{N}]|[^\p{L}\p{N}]+|./gu) ?? []).map((t) => ({
    text: t,
    word: /[\p{L}\p{N}]/u.test(t),
  }));
}

/**
 * A dictation's text, where clicking a misheard word lets you correct it.
 * The correction is made in history and, if wanted, remembered in Your
 * words: the right spelling, plus what Utter wrote instead, so it can put
 * that right next time. Clicking a neighbouring word widens the choice, for
 * a name heard as two words ("pre a" for "Priya").
 */
export default function FixText({
  dictation,
  className,
  onClose,
}: {
  dictation: Dictation;
  className: string;
  onClose: () => void;
}) {
  const { adopt, refreshHistory } = useApp();
  const parts = tokens(dictation.text);
  // The chosen run of tokens, first and last index.
  const [range, setRange] = useState<[number, number] | null>(null);
  const [right, setRight] = useState("");
  const [remember, setRemember] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);

  // A different dictation starts afresh.
  useEffect(() => {
    setRange(null);
    setDone(null);
  }, [dictation.finishedAt]);

  const wrong = range ? parts.slice(range[0], range[1] + 1).map((p) => p.text).join("") : "";

  const pick = (i: number) => {
    setDone(null);
    setError(null);
    let next: [number, number] = [i, i];
    if (range) {
      const [a, b] = range;
      // The next word along on either side widens the choice. Clicking the
      // only chosen word again lets it go; any other word starts afresh.
      const nextBefore = i < a && parts.slice(i + 1, a).every((p) => !p.word);
      const nextAfter = i > b && parts.slice(b + 1, i).every((p) => !p.word);
      if (nextBefore) next = [i, b];
      else if (nextAfter) next = [a, i];
      else if (i === a && i === b) {
        setRange(null);
        return;
      }
    }
    setRange(next);
    setRight(parts.slice(next[0], next[1] + 1).map((p) => p.text).join(""));
  };

  const fix = () => {
    const spelled = right.trim();
    if (!range || !spelled || spelled === wrong) return;
    fixWord(dictation.finishedAt, wrong, spelled, remember).then(
      (saved) => {
        adopt(saved);
        refreshHistory();
        setRange(null);
        setError(null);
        setDone(
          remember && spelled.toLowerCase() !== wrong.toLowerCase()
            ? `Fixed. Utter will write ${spelled} when it hears “${wrong.toLowerCase()}” and the sentence fits.`
            : remember
              ? `Fixed. ${spelled} is in Your words.`
              : "Fixed here.",
        );
      },
      (err) => setError(String(err)),
    );
  };

  return (
    <div className="fixer">
      <p className={`${className} fixtext`}>
        {parts.map((p, i) =>
          p.word ? (
            <button
              key={i}
              className={`fixw ${range && i >= range[0] && i <= range[1] ? "sel" : ""}`}
              onClick={() => pick(i)}
            >
              {p.text}
            </button>
          ) : (
            <Fragment key={i}>
              {p.text.includes("\n") ? (
                p.text.split("\n").map((piece, j) => (
                  <Fragment key={j}>
                    {j > 0 && <br />}
                    {piece}
                  </Fragment>
                ))
              ) : (
                p.text
              )}
            </Fragment>
          ),
        )}
      </p>

      {range ? (
        <div className="fixbar">
          <span className="fixfrom">{wrong}</span>
          <span className="fixarrow" aria-hidden="true">
            →
          </span>
          <label className="fixto">
            <span className="sr">What it should say</span>
            <input
              className="input sm"
              type="text"
              autoFocus
              value={right}
              maxLength={60}
              spellCheck={false}
              onFocus={(e) => e.target.select()}
              onChange={(e) => setRight(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.nativeEvent.isComposing) fix();
                if (e.key === "Escape") setRange(null);
              }}
            />
          </label>
          <label className="fixmem">
            <input type="checkbox" checked={remember} onChange={(e) => setRemember(e.target.checked)} />
            Remember
          </label>
          <button className="btn sm primary" disabled={!right.trim() || right.trim() === wrong} onClick={fix}>
            Fix
          </button>
        </div>
      ) : (
        <div className="fixbar idle">
          <span className="hint" role="status">
            {done ?? "Click the word Utter got wrong. Click the next word too if it should be one name."}
          </span>
          <button className="btn sm quiet" onClick={onClose}>
            Done
          </button>
        </div>
      )}
      {error && (
        <p className="hint" role="alert" style={{ color: "var(--bad)" }}>
          {error}
        </p>
      )}
    </div>
  );
}
