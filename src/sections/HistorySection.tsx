import { useRef, useState } from "react";
import { clearHistory, forgetDictation, type Dictation } from "../api";
import { useApp } from "../state";
import { IconSearch, IconX, secondsBare } from "../ui";
import { FixCard, FixedCard, meta, useCopy, type Fixed } from "./HomeSection";

/**
 * Of the dictations Utter could follow after pasting, how many you left as
 * they were. Fewer edits is the measure of accuracy that matters. A word
 * fixed later on a dictation Utter did not follow does not count.
 */
export function editRate(history: Dictation[]): { followed: number; untouched: number } {
  const followed = history.filter((d) => d.edit !== null && !d.edit.unwatched);
  return { followed: followed.length, untouched: followed.filter((d) => d.edit!.changed === 0).length };
}

/** The middle wait after letting go, in milliseconds, or null with none timed. */
function typicalWait(history: Dictation[]): number | null {
  const waits = history.map((d) => d.totalMs).filter((ms) => ms > 0).sort((a, b) => a - b);
  if (!waits.length) return null;
  const mid = Math.floor(waits.length / 2);
  return waits.length % 2 ? waits[mid] : (waits[mid - 1] + waits[mid]) / 2;
}

/** Every dictation kept on this Mac: how they went, and each one to search, copy or fix. */
export default function HistorySection() {
  const app = useApp();
  const { settings, history, refreshHistory, permissions } = app;
  const [query, setQuery] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The dictation whose words are being fixed, and the one just fixed.
  const [fixing, setFixing] = useState<Dictation | null>(null);
  const [fixed, setFixed] = useState<Fixed | null>(null);
  const top = useRef<HTMLDivElement>(null);
  const { copied, copy } = useCopy();

  const { followed, untouched } = editRate(history);
  const wait = typicalWait(history);
  // Edits are counted only while Utter follows the text box after pasting.
  const following =
    (settings.learnWords || (settings.keepAudio && settings.keepHistory)) && permissions?.accessibility !== false;

  const needle = query.trim().toLowerCase();
  const shown = needle ? history.filter((d) => `${d.text} ${d.raw} ${d.app.name}`.toLowerCase().includes(needle)) : history;

  const run = (work: Promise<unknown>) =>
    work.then(
      () => setError(null),
      (err) => setError(String(err)),
    );

  const startFix = (d: Dictation) => {
    setFixed(null);
    setFixing(d);
    top.current?.scrollIntoView({ block: "start" });
  };

  const privacyLink = (
    <button className="qlink" onClick={() => app.setTab("privacy")}>
      Privacy
    </button>
  );

  return (
    <>
      <div className="qhead" ref={top}>
        <div className="t">
          <h1>History</h1>
          <p>Your dictations, kept on this Mac only.</p>
        </div>
      </div>

      {fixing ? (
        <FixCard
          dictation={fixing}
          onCancel={() => setFixing(null)}
          onFixed={(done) => {
            setFixing(null);
            setFixed(done);
          }}
        />
      ) : fixed ? (
        <FixedCard {...fixed} title="Dictation" onDone={() => setFixed(null)} />
      ) : null}

      {history.length > 0 && (
        <div className="qcard tight">
          <div className="qtiles" style={{ marginTop: 0 }}>
            <div className={followed ? "" : "no"}>
              <span>Left as typed</span>
              <b>
                {followed ? (
                  <>
                    {Math.round((untouched / followed) * 100)}
                    <small>%</small>
                  </>
                ) : (
                  "Not counted yet"
                )}
              </b>
            </div>
            <div>
              <span>Dictations</span>
              <b>{history.length}</b>
            </div>
            <div className={wait === null ? "no" : ""}>
              <span>Typical wait</span>
              <b>
                {wait === null ? (
                  "None timed"
                ) : (
                  <>
                    {secondsBare(wait, 1)}
                    <small>s</small>
                  </>
                )}
              </b>
            </div>
          </div>
          <p className="qp" style={{ fontSize: 12.5 }}>
            {followed > 0
              ? `You changed ${followed - untouched} of your last ${followed} dictations after Utter pasted them.`
              : "Left as typed counts the dictations you didn’t change after Utter pasted them."}
            {!following &&
              " Utter counts only while it follows the text box after pasting: with Learn from my corrections or Keep recordings on, and Accessibility allowed."}
          </p>
        </div>
      )}

      <div className={`qcard ${history.length ? "" : "tight"}`}>
        {!settings.keepHistory && (
          <p className="qp" style={{ marginTop: 0 }}>
            History is off, so new dictations aren’t kept.
            {history.length > 0 && " Ones from before stay here until you clear them."} Turn it on in {privacyLink}.
          </p>
        )}

        {history.length === 0 ? (
          settings.keepHistory && (
            <p className="qp" style={{ marginTop: 0 }}>
              Nothing yet. Past dictations stay here, on this Mac only.
            </p>
          )
        ) : (
          <>
            <label className="qsrch" style={{ marginTop: settings.keepHistory ? 0 : undefined }}>
              <IconSearch />
              <input
                type="search"
                placeholder={`Search ${history.length} ${history.length === 1 ? "dictation" : "dictations"}`}
                aria-label="Search history"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
              {needle && <span className="n">{shown.length}</span>}
            </label>
            {shown.length === 0 ? (
              <p className="qp">Nothing matches.</p>
            ) : (
              <div className="qlist">
                {shown.map((d) => (
                  <div className="qli" key={d.finishedAt}>
                    <div className="tx">
                      <p>{d.text}</p>
                      <small>{meta(d)}</small>
                    </div>
                    <div className="a">
                      {d.selection === null && (
                        <button className="qbtn sm ghost" onClick={() => startFix(d)}>
                          Fix
                        </button>
                      )}
                      <button className="qbtn sm ghost" onClick={() => run(copy(d))}>
                        {copied === d.finishedAt ? "Copied" : "Copy"}
                      </button>
                      <button
                        className="qx"
                        aria-label="Delete this dictation"
                        title="Delete"
                        onClick={() => {
                          if (fixing?.finishedAt === d.finishedAt) setFixing(null);
                          run(forgetDictation(d.finishedAt).then(refreshHistory));
                        }}
                      >
                        <IconX />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </>
        )}

        {history.length > 0 &&
          (confirming ? (
            <div className="qfoot" role="alert">
              <span>
                <b style={{ color: "var(--ink)" }}>
                  Clear all {history.length} {history.length === 1 ? "dictation" : "dictations"}?
                </b>{" "}
                This can’t be undone.
              </span>
              <span className="sp" />
              <button className="qbtn sm" autoFocus onClick={() => setConfirming(false)}>
                Keep them
              </button>
              <button
                className="qbtn sm pri"
                onClick={() => {
                  setConfirming(false);
                  setFixing(null);
                  setFixed(null);
                  run(clearHistory().then(refreshHistory));
                }}
              >
                Clear all {history.length}
              </button>
            </div>
          ) : (
            <div className="qfoot">
              <span>Up to 200 kept. Keep history and recordings are in {privacyLink}.</span>
              <span className="sp" />
              <button className="qbtn sm ghost" onClick={() => setConfirming(true)}>
                Clear history
              </button>
            </div>
          ))}
        <span className="sr" role="status">
          {copied !== null ? "Copied to the clipboard." : ""}
        </span>
        {error && (
          <p className="qp" role="alert" style={{ color: "var(--bad-fg)" }}>
            {error}
          </p>
        )}
      </div>
    </>
  );
}
