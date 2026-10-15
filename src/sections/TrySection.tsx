import { Fragment, useEffect, useState, type ReactNode } from "react";
import type { Dictation } from "../api";
import { diffWords } from "../diff";
import { hotkeyLabel } from "../hotkey";
import { cleanupGap, readiness, selectedModel } from "../readiness";
import { shortName, useApp, useElapsed, useLevels, type AppState } from "../state";
import { clock, FlatMark, IconCheck, InlineKeys, Notice, seconds, Segmented } from "../ui";
import FixText from "./FixText";
import HistoryList from "./HistoryList";

type Show = "heard" | "changes" | "written";

/** A place to practise, the live status, and the last dictation taken apart. */
/**
 * Can a dictation happen at all right now? Only a missing microphone
 * permission or speech model stops it. A cleanup problem does not: the text
 * simply goes in raw, and the strip under the box says so.
 */
function dictationBlocked(app: AppState): boolean {
  const ready = readiness(app);
  return ready.tone === "bad" || ready.tone === "busy" || (ready.tone === "warn" && ready.tab === "model");
}

export default function TrySection() {
  const app = useApp();
  const { live, settings, dictation } = app;
  const hotkey = live.mode === "rewrite" && settings.rewriteHotkey ? settings.rewriteHotkey : settings.hotkey;

  const recent = useRecent(live);
  const chip =
    live.phase === "recording"
      ? { tone: "accent", text: "Listening" }
      : live.phase === "processing"
        ? { tone: "plain", text: "Working" }
        : recent && live.phase === "error"
          ? { tone: "warn", text: "Problem" }
          : recent && live.phase === "done"
            ? { tone: "ok", text: "Done" }
            : dictationBlocked(app)
              ? { tone: "warn", text: "Not ready" }
              : { tone: "ok", text: "Ready" };

  return (
    <>
      <div className="sechead">
        <h2 className="h">Try it</h2>
        <span className={`chip ${chip.tone}`}>{chip.text}</span>
      </div>

      <PracticeBox placeholder={`Click here, then hold ${hotkeyLabel(settings.hotkey)} and speak. Let go to insert.`} />

      <p className="hint">
        {live.phase === "recording" && live.handsFree ? (
          <>
            Hands-free. Tap <InlineKeys hotkey={hotkey} /> again to finish.
          </>
        ) : live.phase === "recording" ? (
          <>
            Let go of <InlineKeys hotkey={hotkey} /> to insert.
          </>
        ) : live.phase === "processing" && live.stage && live.stage !== "transcribing" ? (
          "Transcribed. Waiting on the language model."
        ) : (
          <>
            Hold <InlineKeys hotkey={settings.hotkey} /> in any app. This box is only for practice.
          </>
        )}
      </p>

      {dictation ? (
        <LastDictation dictation={dictation} />
      ) : (
        <div className="card last empty">
          <FlatMark />
          <p>
            <b>No dictations yet.</b> Your last one shows here: what was inserted, what was heard, and how
            long each step took.
          </p>
        </div>
      )}

      <HistoryList />
    </>
  );
}

/**
 * The text box with a status strip under it. The strip is the window's own
 * version of the pill: the same waveform, the same words.
 */
export function PracticeBox({
  placeholder,
  autoFocus = false,
  readyText,
}: {
  placeholder: string;
  autoFocus?: boolean;
  /** Overrides the "Ready" line, for the welcome screen. */
  readyText?: string;
}) {
  const app = useApp();
  const { live } = app;
  const listening = live.phase === "recording";
  const working = live.phase === "processing";
  const levels = useLevels(listening, 24);
  const elapsed = useElapsed(listening || working, live.since);
  const [value, setValue] = useState("");

  return (
    <label className="tryarea">
      <span className="sr">Practice box</span>
      <textarea
        className="ta"
        autoFocus={autoFocus}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        placeholder={listening || working ? "" : placeholder}
      />
      <span className="tastrip" role="status">
        {listening ? (
          <>
            <span className="live real" aria-hidden="true">
              {levels.map((level, i) => (
                <i key={i} style={{ height: Math.round(3 + level * 23) }} />
              ))}
            </span>
            <span>
              {live.mode === "rewrite" ? "Listening · rewrite" : "Listening"}
              {live.handsFree && " · hands-free"}
            </span>
            <span className="t">{clock(elapsed)}</span>
          </>
        ) : working ? (
          <>
            <span className="badge sm think" aria-hidden="true">
              <i />
              <i />
              <i />
              <i />
              <i />
            </span>
            <span>{workingText(app)}</span>
            <span className="t">{seconds(elapsed, 1)}</span>
          </>
        ) : (
          <RestingStrip app={app} readyText={readyText} />
        )}
      </span>
    </label>
  );
}

function workingText(app: AppState): string {
  const { live } = app;
  const style = live.app && live.app.category !== "other" ? ` · ${live.app.category} style` : "";
  if (live.stage === "tidying") return `Tidying${style}`;
  if (live.stage === "rewriting") return "Rewriting";
  if (live.stage === "editing") return "Editing your selection";
  if (live.stage === "transcribing") return "Transcribing";
  return live.mode === "rewrite" ? "Rewriting" : "Working";
}

/** True for a few seconds after a dictation ends; then "Done" and "Problem" give way to "Ready". */
function useRecent(live: AppState["live"]): boolean {
  const [recent, setRecent] = useState(false);
  useEffect(() => {
    if (live.phase !== "done" && live.phase !== "error") return setRecent(false);
    setRecent(true);
    const timer = setTimeout(() => setRecent(false), 8000);
    return () => clearTimeout(timer);
  }, [live]);
  return recent;
}

/** The strip when nothing is being recorded or processed. */
function RestingStrip({ app, readyText }: { app: AppState; readyText?: string }) {
  const { live, dictation, settings, fresh } = app;
  const recent = useRecent(live);

  let dot: "" | "warn" | "bad" | "busy" = "";
  let text: ReactNode;

  if (recent && live.phase === "error") {
    dot = "bad";
    text = live.message ?? "Something went wrong.";
  } else if (recent && live.phase === "done" && dictation && fresh) {
    if (!dictation.inserted) {
      dot = "warn";
      text = "Copied to the clipboard. Press ⌘V to paste.";
    } else {
      return (
        <>
          <span className="okc">
            <IconCheck />
          </span>
          {dictation.totalMs ? `Inserted ${seconds(dictation.totalMs)} after you let go.` : "Inserted."}
        </>
      );
    }
  } else {
    const ready = readiness(app);
    const model = selectedModel(app);
    if (ready.tone === "bad" || (ready.tone === "warn" && ready.tab === "model")) {
      dot = ready.tone;
      text = `${ready.text} first.`;
    } else if (ready.tone === "busy") {
      dot = "busy";
      text = `${ready.text}. Dictation works when it finishes.`;
    } else if (settings.cleanupEnabled && cleanupGap(app)) {
      dot = "warn";
      text = `Dictation works. Cleanup needs ${cleanupGap(app)}, so text goes in raw.`;
    } else {
      text =
        readyText ??
        `Ready. ${model ? shortName(model.label) : "Speech"} model, cleanup ${settings.cleanupEnabled ? "on" : "off"}.`;
    }
  }
  return (
    <>
      <span className={`dot8 ${dot}`} />
      <span>{text}</span>
    </>
  );
}

/** The last dictation: what went in, what was heard, and where the time went. */
function LastDictation({ dictation }: { dictation: Dictation }) {
  const { setTab, settings } = useApp();
  const rewritten = dictation.mode === "rewrite";
  // With text selected, the words spoken were an instruction for changing it.
  const edited = dictation.selection !== null && dictation.cleaned;
  const [show, setShow] = useState<Show>(rewritten && !edited ? "written" : "changes");
  const [fixing, setFixing] = useState(false);
  // A new dictation starts on the view that suits it.
  useEffect(() => {
    setShow(dictation.mode === "rewrite" && !(dictation.selection !== null && dictation.cleaned) ? "written" : "changes");
    setFixing(false);
  }, [dictation.finishedAt]);

  const failed = !dictation.cleaned && !!dictation.note && /skipped|needs a model/i.test(dictation.note);
  const what = edited || dictation.selection !== null ? "Edit" : rewritten ? "Rewrite" : "Cleanup";

  const meta = [
    dictation.app.name || null,
    // "other" is the backend's word for an app it has no particular style for.
    dictation.cleaned
      ? edited
        ? "selection edited"
        : rewritten
        ? "rewritten"
        : dictation.app.category === "other"
          ? "tidied"
          : `${dictation.app.category} style`
      : "as heard",
    dictation.finishedAt
      ? new Date(dictation.finishedAt).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }).toLowerCase()
      : null,
  ].filter(Boolean);

  const waited = dictation.totalMs || dictation.transcribeMs + (dictation.cleanupMs ?? 0);
  const share = (ms: number) => `${Math.max(0, Math.min(100, (ms / waited) * 100))}%`;
  const pasteMs = Math.max(0, waited - dictation.transcribeMs - (dictation.cleanupMs ?? 0));

  return (
    <div className="card last">
      <div className="lhead">
        <span className="label">Last dictation</span>
        <span className="meta">{meta.join(" · ")}</span>
        {!edited && dictation.selection === null && !fixing && (
          <button className="btn sm quiet" onClick={() => setFixing(true)} title="Fix a misheard word">
            Fix a word
          </button>
        )}
      </div>

      {failed && (
        <Notice
          tone="bad"
          action={
            <button className="btn sm" onClick={() => setTab("cleanup")}>
              Open cleanup
            </button>
          }
        >
          <b>{what} failed.</b> {reason(dictation.note)}{" "}
          {dictation.inserted
            ? "The text was inserted as heard."
            : "The text was copied as heard. Press ⌘V to paste it."}
        </Notice>
      )}
      {!failed && dictation.note && <Notice tone="warn">{dictation.note}</Notice>}

      {fixing ? (
        <FixText dictation={dictation} className="final" onClose={() => setFixing(false)} />
      ) : edited ? (
        <>
          <p className="asked">
            <span className="label">You said</span> {dictation.raw}
          </p>
          <Segmented
            label="Show"
            value={show}
            onChange={setShow}
            options={[
              { id: "heard", label: "Before" },
              { id: "changes", label: "Changes" },
              { id: "written", label: "After" },
            ]}
          />
          {show === "written" && <p className="final">{dictation.text}</p>}
          {show === "heard" && <p className="heard">{dictation.selection}</p>}
          {show === "changes" && <Changes heard={dictation.selection ?? ""} written={dictation.text} />}
        </>
      ) : dictation.cleaned ? (
        <>
          <Segmented
            label="Show"
            value={show}
            onChange={setShow}
            options={[
              { id: "heard", label: "Heard" },
              { id: "changes", label: "Changes" },
              { id: "written", label: rewritten ? "Rewritten" : "Cleaned" },
            ]}
          />
          {show === "written" && <p className="final">{dictation.text}</p>}
          {show === "heard" && <p className="heard">{dictation.raw}</p>}
          {show === "changes" && <Changes heard={dictation.raw} written={dictation.text} />}
        </>
      ) : failed ? (
        <p className="heard">{dictation.raw}</p>
      ) : (
        <p className="final">{dictation.text}</p>
      )}

      <dl className="times">
        <div>
          <dt>You spoke</dt>
          <dd>{dictation.spokenMs ? seconds(dictation.spokenMs, 1) : "–"}</dd>
        </div>
        <div>
          <dt>Transcribe</dt>
          <dd>{seconds(dictation.transcribeMs)}</dd>
        </div>
        <div>
          <dt>{what}</dt>
          {failed ? (
            <dd className="bad">failed</dd>
          ) : (
            <dd>
              {dictation.cleanupMs !== null
                ? seconds(dictation.cleanupMs)
                : rewritten || settings.cleanupEnabled
                  ? "not run"
                  : "off"}
            </dd>
          )}
        </div>
        <div>
          <dt>After let go</dt>
          <dd>{dictation.totalMs ? seconds(dictation.totalMs) : "–"}</dd>
        </div>
      </dl>
      <div
        className="tbar"
        role="img"
        aria-label={`Of the wait: transcribe ${seconds(dictation.transcribeMs)}, ${what.toLowerCase()} ${
          dictation.cleanupMs !== null ? seconds(dictation.cleanupMs) : "none"
        }, paste ${seconds(pasteMs)}`}
      >
        <b className="s1" style={{ width: share(dictation.transcribeMs) }} />
        {dictation.cleanupMs !== null && <b className="s2" style={{ width: share(dictation.cleanupMs) }} />}
        <b className="s3" style={{ width: share(pasteMs) }} />
      </div>
    </div>
  );
}

/** "Cleanup skipped: The reply was empty." -> "The reply was empty." */
function reason(note: string | null): string {
  if (!note) return "";
  const found = note.match(/skipped: (.+)$/);
  return found ? found[1] : note;
}

/** Heard and written laid over each other: removals struck out, additions marked. */
function Changes({ heard, written }: { heard: string; written: string }) {
  const words = diffWords(heard, written);
  return (
    <>
      <p className="diff">
        {words.map((word, i) =>
          word[0].kind === "break" ? (
            // A new line, or a blank line for a new paragraph.
            <Fragment key={i}>
              <br />
              {word[0].text.length > 1 && <br />}
            </Fragment>
          ) : (
          <Fragment key={i}>
            {i > 0 && words[i - 1][0].kind !== "break" && " "}
            {word.map((piece, j) =>
              piece.kind === "del" ? (
                <del key={j}>{piece.text}</del>
              ) : piece.kind === "ins" ? (
                <ins key={j}>{piece.text}</ins>
              ) : (
                <Fragment key={j}>{piece.text}</Fragment>
              ),
            )}
          </Fragment>
          ),
        )}
      </p>
      <div className="legend" aria-hidden="true">
        <del>removed</del>
        <ins>added</ins>
      </div>
    </>
  );
}
