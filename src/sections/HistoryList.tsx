import { useState } from "react";
import { clearHistory, copyText, forgetDictation, type Dictation } from "../api";
import { useApp } from "../state";
import { Switch } from "../ui";

function when(finishedAt: number): string {
  if (!finishedAt) return "";
  const date = new Date(finishedAt);
  const today = new Date().toDateString() === date.toDateString();
  const time = date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }).toLowerCase();
  return today ? time : `${date.toLocaleDateString([], { day: "numeric", month: "short" })}, ${time}`;
}

/**
 * Earlier dictations, kept only on this Mac. Each can be copied again or
 * forgotten; the whole list can be cleared, or not kept at all.
 */
export default function HistoryList() {
  const { settings, update, history, refreshHistory, dictation } = useApp();
  const [query, setQuery] = useState("");
  const [copied, setCopied] = useState<number | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // The newest one is already shown in full above, as "Last dictation".
  const earlier = history.filter((d) => d.finishedAt !== dictation?.finishedAt);
  const needle = query.trim().toLowerCase();
  const shown = needle
    ? earlier.filter((d) => `${d.text} ${d.raw} ${d.app.name}`.toLowerCase().includes(needle))
    : earlier;

  const run = (work: Promise<unknown>) =>
    work.then(
      () => setError(null),
      (err) => setError(String(err)),
    );

  const copy = (d: Dictation) =>
    run(
      copyText(d.text).then(() => {
        setCopied(d.finishedAt);
        setTimeout(() => setCopied((now) => (now === d.finishedAt ? null : now)), 1500);
      }),
    );

  const clear = () => {
    setConfirming(false);
    run(clearHistory().then(refreshHistory));
  };

  return (
    <div className="card stack">
      <div className="cardhead">
        <span className="label">Earlier</span>
        <span className="meta" aria-hidden="true">
          Keep history
        </span>
        <Switch
          label="Keep history"
          checked={settings.keepHistory}
          onChange={(keepHistory) => run(update({ keepHistory }))}
        />
      </div>

      {!settings.keepHistory && (
        <p className="hint">
          History is off, so new dictations are not kept.
          {history.length > 0 && " Ones from before are still below until you clear them."}
        </p>
      )}

      {earlier.length > 0 && (
        <label>
          <span className="sr">Search earlier dictations</span>
          <input
            className="input"
            type="search"
            placeholder="Search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            style={{ width: "100%" }}
          />
        </label>
      )}

      {shown.length > 0 ? (
        <ul className="past">
          {shown.map((d) => (
            <li key={d.finishedAt}>
              <div className="pasttext">
                <p>{d.text}</p>
                <span className="meta">
                  {[when(d.finishedAt), d.app.name, d.selection ? "edited selection" : d.mode === "rewrite" ? "rewritten" : null]
                    .filter(Boolean)
                    .join(" · ")}
                </span>
              </div>
              <button className="btn sm" onClick={() => copy(d)}>
                {copied === d.finishedAt ? "Copied" : "Copy"}
              </button>
              <button
                className="btn sm quiet"
                aria-label="Forget this dictation"
                onClick={() => run(forgetDictation(d.finishedAt).then(refreshHistory))}
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="hint">
          {needle
            ? "Nothing matches."
            : settings.keepHistory
              ? "Your dictations collect here, on this Mac only, so you can copy one again."
              : ""}
        </p>
      )}

      {history.length > 0 && (
        <div className="frow">
          <span className="hint" style={{ flex: 1 }}>
            Kept on this Mac only, up to 200.
          </span>
          {confirming ? (
            <>
              <button className="btn sm" onClick={() => setConfirming(false)}>
                Keep them
              </button>
              <button className="btn sm" style={{ color: "var(--bad)" }} onClick={clear}>
                Clear all {history.length}
              </button>
            </>
          ) : (
            <button className="btn sm quiet" onClick={() => setConfirming(true)}>
              Clear history
            </button>
          )}
        </div>
      )}
      <span className="sr" role="status">
        {copied !== null ? "Copied to the clipboard." : ""}
      </span>
      {error && (
        <p className="hint" role="alert" style={{ color: "var(--bad)" }}>
          {error}
        </p>
      )}
    </div>
  );
}
