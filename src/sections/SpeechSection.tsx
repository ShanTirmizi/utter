import { useEffect, useRef, useState } from "react";
import type { ModelStatus } from "../api";
import { selectedModel } from "../readiness";
import { shortName, useApp } from "../state";
import { IconDown, IconTick, Notice, WaveProgress } from "../ui";

/** The languages offered first, then the rest Whisper knows. Codes are Whisper's. */
const COMMON: [string, string][] = [
  ["en", "English"],
  ["fr", "French"],
  ["de", "German"],
  ["es", "Spanish"],
  ["ur", "Urdu"],
];
const MORE: [string, string][] = [
  ["ar", "Arabic"], ["bn", "Bengali"], ["bg", "Bulgarian"], ["ca", "Catalan"], ["zh", "Chinese"], ["hr", "Croatian"],
  ["cs", "Czech"], ["da", "Danish"], ["nl", "Dutch"], ["et", "Estonian"], ["fi", "Finnish"], ["el", "Greek"],
  ["gu", "Gujarati"], ["he", "Hebrew"], ["hi", "Hindi"], ["hu", "Hungarian"], ["is", "Icelandic"], ["id", "Indonesian"],
  ["it", "Italian"], ["ja", "Japanese"], ["kn", "Kannada"], ["ko", "Korean"], ["lv", "Latvian"], ["lt", "Lithuanian"],
  ["ms", "Malay"], ["ml", "Malayalam"], ["mr", "Marathi"], ["no", "Norwegian"], ["fa", "Persian"], ["pl", "Polish"],
  ["pt", "Portuguese"], ["pa", "Punjabi"], ["ro", "Romanian"], ["ru", "Russian"], ["sr", "Serbian"], ["sk", "Slovak"],
  ["sl", "Slovenian"], ["sw", "Swahili"], ["sv", "Swedish"], ["tl", "Tagalog"], ["ta", "Tamil"], ["te", "Telugu"],
  ["th", "Thai"], ["tr", "Turkish"], ["uk", "Ukrainian"], ["vi", "Vietnamese"], ["cy", "Welsh"],
];
const ALL = [...COMMON, ...MORE];

const MB = 1_000_000;

/** "English only." in the label becomes words in the note. */
function sizeNote(m: ModelStatus): string {
  return `${m.sizeMb} MB · ${/\(English\)/.test(m.label) ? "English only" : "Any language"}`;
}

/** Seconds left, guessed from how fast it has gone so far. */
function useTimeLeft(received: number, total: number, active: boolean): string | null {
  const start = useRef<{ at: number; received: number } | null>(null);
  if (!active) start.current = null;
  else if (!start.current) start.current = { at: performance.now(), received };
  if (!active || !start.current || total <= 0) return null;
  const elapsed = (performance.now() - start.current.at) / 1000;
  const done = received - start.current.received;
  if (elapsed < 2 || done <= 0) return null;
  const left = ((total - received) / done) * elapsed;
  if (left < 10) return "A few seconds left.";
  if (left < 90) return `About ${Math.max(10, Math.round(left / 10) * 10)} seconds left.`;
  return `About ${Math.round(left / 60)} minutes left.`;
}

export default function SpeechSection() {
  const app = useApp();
  const { models, progress, downloadFault, settings } = app;
  const selected = selectedModel(app);
  const name = selected ? shortName(selected.label) : "";
  const busy = progress ? models.find((m) => m.name === progress.name) : undefined;
  const busyTotal = progress ? progress.total || (busy?.sizeMb ?? 0) * MB : 0;
  const fault = downloadFault && !models.find((m) => m.name === downloadFault.name)?.downloaded ? downloadFault : null;
  const faultModel = fault ? models.find((m) => m.name === fault.name) : undefined;
  const timeLeft = useTimeLeft(progress?.received ?? 0, busyTotal, !!progress);
  const multilingual = !!selected && !/\(English\)/.test(selected.label);

  // The "In use" card follows the selected model, or the one downloading for it.
  const showing = busy && !selected?.downloaded ? busy : selected;

  return (
    <>
      <div className="qhead">
        <div className="t">
          <h1>Speech</h1>
          <p>Turns your voice into text, here on this Mac.</p>
        </div>
      </div>

      {fault && !progress && (
        <Notice
          tone="pe"
          action={
            <button className="qbtn sm pri" onClick={() => app.download(fault.name)}>
              Retry
            </button>
          }
        >
          <b>Download stopped:</b>{" "}
          <span>{fault.message.replace(/^Download (interrupted|failed): /, "").replace(/\.?$/, ".")} Retry starts again from the beginning.</span>
        </Notice>
      )}

      {showing && (
        <div className="qcard" style={{ overflow: "visible" }}>
          <div className="qlab">
            In use
            {progress && busy && <span className="r">{shortName(busy.label)}</span>}
          </div>
          {progress && busy && busy === showing ? (
            <>
              <div className="qbig">
                {Math.round(progress.received / MB)}
                <small>of {busy.sizeMb} MB</small>
              </div>
              <WaveProgress fraction={busyTotal ? progress.received / busyTotal : 0} label={`${shortName(busy.label)} download`} />
              <div className="qtags">
                <span className="qtag nt">{busyTotal ? Math.round((progress.received / busyTotal) * 100) : 0}%</span>
                <span className="qnote">{[timeLeft, "Dictation starts when it’s done."].filter(Boolean).join(" ")}</span>
              </div>
              <div className="qrowb">
                <button className="qbtn" onClick={app.stopDownloading}>
                  Stop
                </button>
              </div>
            </>
          ) : (
            <>
              <div className={`qbig ${shortName(showing.label).length > 8 ? "md" : ""}`}>{shortName(showing.label)}</div>
              {faultModel === showing && fault && (
                <WaveProgress fraction={fault.total ? fault.received / fault.total : 0} failed label="Download stopped" />
              )}
              <div className="qtags">
                {showing.downloaded ? (
                  <span className="qtag mi">Ready</span>
                ) : faultModel === showing && fault ? (
                  <span className="qtag pe">Stopped at {Math.round(fault.received / MB)} MB</span>
                ) : (
                  <span className="qtag pe">Not downloaded</span>
                )}
                <span className="qnote">
                  {showing.downloaded || fault ? sizeNote(showing) : "Dictation can’t start without it."}
                </span>
              </div>
              {!showing.downloaded && !fault && (
                <div className="qrowb">
                  <button className="qbtn pri" disabled={!!progress} onClick={() => app.download(showing.name)}>
                    Download {name}
                  </button>
                </div>
              )}
              {showing.downloaded && multilingual && <LanguagePicker />}
            </>
          )}
        </div>
      )}

      <div className="qcard">
        <div className="qlab">Models</div>
        <div className="qlist">
          {models.map((m) => {
            const inUse = settings.model === m.name;
            const downloading = progress?.name === m.name;
            const stopped = fault?.name === m.name;
            return (
              <div className="qmrow" key={m.name}>
                <div className="tx">
                  <b>
                    {shortName(m.label)}
                    <small>{m.sizeMb} MB</small>
                  </b>
                  <span>
                    {m.note}
                    {/\(English\)/.test(m.label) ? " English only." : ""}
                  </span>
                </div>
                <div className="a">
                  {downloading ? (
                    <span className="qtag sm nt">{busyTotal ? Math.round((progress.received / busyTotal) * 100) : 0}%</span>
                  ) : stopped ? (
                    <>
                      <span className="qtag sm pe">Stopped</span>
                      {!inUse && (
                        <button className="qbtn sm" disabled={!!progress} onClick={() => app.download(m.name)}>
                          Retry
                        </button>
                      )}
                    </>
                  ) : inUse && m.downloaded ? (
                    <span className="qtag sm mi">In use</span>
                  ) : inUse ? (
                    <span className="qtag sm pe">Not downloaded</span>
                  ) : m.downloaded ? (
                    <>
                      <span className="qtag sm nt">Downloaded</span>
                      <button className="qbtn sm" onClick={() => app.update({ model: m.name }).catch(() => undefined)}>
                        Use
                      </button>
                    </>
                  ) : (
                    <button className="qbtn sm" disabled={!!progress} onClick={() => app.download(m.name)}>
                      Download
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </div>
      <p className="qp" style={{ margin: "0 4px" }}>
        Names and jargon it gets wrong? Teach it under{" "}
        <button className="qlink" onClick={() => app.setTab("words")}>
          Words
        </button>
        .
      </p>
    </>
  );
}

/** Which language the multilingual model should expect. */
function LanguagePicker() {
  const { settings, update } = useApp();
  const [open, setOpen] = useState(false);
  const [more, setMore] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const current = settings.language;
  const label = current ? (ALL.find(([code]) => code === current)?.[1] ?? current) : "Detect automatically";

  // Clicking elsewhere or pressing Escape closes the list.
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => !box.current?.contains(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("mousedown", away);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("mousedown", away);
      window.removeEventListener("keydown", esc);
    };
  }, [open]);

  const pick = (code: string) => {
    setOpen(false);
    setMore(false);
    update({ language: code }).catch(() => undefined);
  };
  const option = (code: string, name: string) => (
    <button key={code || "auto"} className="qopt" role="option" aria-selected={current === code} onClick={() => pick(code)}>
      <span className="ck">{current === code && <IconTick />}</span>
      {name}
    </button>
  );
  // A language chosen from the longer list stays in view in the short one.
  const shortList = COMMON.some(([code]) => code === current) || !current ? COMMON : [...COMMON, [current, label] as [string, string]];

  return (
    <>
      <div className="qrow" style={{ marginTop: 16, position: "relative" }} ref={box}>
        <span style={{ font: "600 14px/1 var(--qf)" }}>Language</span>
        <span className="sp" />
        <button className="qsel" aria-haspopup="listbox" aria-expanded={open} onClick={() => setOpen(!open)}>
          {label}
          <IconDown />
        </button>
        {open && (
          <div className="qpop" role="listbox" aria-label="Language">
            {option("", "Detect automatically")}
            {(more ? ALL : shortList).map(([code, name]) => option(code, name))}
            {!more && (
              <>
                <hr />
                <button className="qopt more" onClick={() => setMore(true)}>
                  <span className="ck" />
                  More languages…
                </button>
              </>
            )}
          </div>
        )}
      </div>
      <p className="qp">Pick one if it guesses wrong. Large and Parakeet understand other languages; Base and Small are English only.</p>
    </>
  );
}
