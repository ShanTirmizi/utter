import { Fragment, useEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { cancelDictation, copyText, setPillArea, type Learned, type PipelineUpdate } from "./api";
import { useElapsed, useLevels } from "./state";
import { Bars, clock, IconInfoLine, IconOkFill, IconWarnFill, IconX, LevelBars } from "./ui";

/** Something to read: one line, or what happened and then why. */
type Note = {
  kind: "note" | "learned" | "paste" | "problem" | "copied";
  title: string;
  detail: string | null;
};

type View =
  | { kind: "hidden" }
  | { kind: "listen"; rewrite: boolean; handsFree: boolean; since: number }
  | { kind: "work"; label: string; chip: string | null }
  | { kind: "say"; note: Note }
  // Nothing that takes text had focus, so it was not typed: offer to copy it.
  | { kind: "copy"; text: string };

const HOLD: Record<Note["kind"], number> = { note: 2500, learned: 4000, paste: 8000, problem: 8000, copied: 2500 };
const TWO_LINES = 4000;
/** How long the offer to copy stays, not counting while the pointer is on it. */
const COPY_HOLD = 8000;
const COPY_TICK = 100;
/** The backend's note on a dictation that had nowhere to go (`pipeline::NOWHERE`). */
const NOWHERE = "Nowhere to type it.";

/** Turn a backend message into what the pill says. */
function readNote(text: string, problem: boolean): Note {
  const copied = text.match(/^Copied to the clipboard \((.+?)\)\. Press ⌘V to paste\.\s*(.*)$/);
  if (copied) {
    const why = copied[1].charAt(0).toUpperCase() + copied[1].slice(1);
    return { kind: "paste", title: "Copied, not typed", detail: `Press ⌘V to paste it. ${why}.` };
  }
  if (/^Copied to the clipboard/.test(text)) return { kind: "paste", title: "Copied, not typed", detail: "Press ⌘V to paste it." };
  const skipped = text.match(/^(Cleanup|Rewrite) skipped: (.+)$/);
  if (skipped) {
    return {
      kind: problem ? "paste" : "note",
      title: skipped[1] === "Cleanup" ? "Inserted without cleanup" : "Inserted without rewriting",
      detail: skipped[2],
    };
  }
  if (/^Microphone access is off/.test(text)) return { kind: "problem", title: "Utter can’t hear you", detail: "Allow the microphone in Utter." };
  const cut = text.indexOf(". ");
  if (problem && cut > 0) return { kind: "problem", title: text.slice(0, cut + 1), detail: text.slice(cut + 2) };
  if (problem) return { kind: "problem", title: text, detail: null };
  return { kind: "note", title: text, detail: null };
}

/** Draw "⌘V" as two small keys. */
function withKeys(text: string): ReactNode {
  return text.split("⌘V").map((part, i) => (
    <Fragment key={i}>
      {i > 0 && (
        <>
          <span className="k">⌘</span>
          <span className="k">V</span>
        </>
      )}
      {part}
    </Fragment>
  ));
}

function workLabel(update: PipelineUpdate): { label: string; chip: string | null } {
  const app = update.app?.name;
  const style = update.app && update.app.category !== "other" ? update.app.category : null;
  switch (update.stage) {
    case "transcribing":
      return { label: "Transcribing", chip: null };
    case "tidying":
      return { label: "Tidying", chip: style };
    case "rewriting":
      return { label: app ? `Rewriting for ${app}` : "Rewriting", chip: null };
    case "editing":
      return { label: "Editing your selection", chip: null };
    default:
      return { label: update.mode === "rewrite" ? "Rewriting" : "Working", chip: null };
  }
}

/**
 * The small status at the bottom of the screen. Its window never takes
 * keyboard focus, and lets the mouse through except over a pill with a
 * button on it, so showing a status never moves the cursor away from the app
 * being dictated into. Success is silent.
 */
export default function Pill() {
  const [view, setView] = useState<View>({ kind: "hidden" });
  // The last thing shown, kept while the pill fades out.
  const [shown, setShown] = useState<View>({ kind: "hidden" });
  // The pointer is over the pill. Told by the backend, which follows it.
  const [hovered, setHovered] = useState(false);
  // What is left of the offer to copy.
  const [left, setLeft] = useState(COPY_HOLD);
  const pill = useRef<HTMLDivElement>(null);
  const current = useRef<View["kind"]>("hidden");
  const listening = view.kind === "listen";
  const levels = useLevels(listening, 5);
  const elapsed = useElapsed(listening, listening ? view.since : 0);
  const clickable = view.kind === "listen" || view.kind === "work" || view.kind === "copy";

  useEffect(() => {
    current.current = view.kind;
    if (view.kind !== "hidden") setShown(view);
  }, [view]);

  // A note goes by itself; anything new replaces it first.
  useEffect(() => {
    if (view.kind !== "say") return;
    const { note } = view;
    const timer = setTimeout(() => setView({ kind: "hidden" }), note.kind === "note" && note.detail ? TWO_LINES : HOLD[note.kind]);
    return () => clearTimeout(timer);
  }, [view]);

  // The offer to copy runs down, except while the pointer is on it.
  useEffect(() => {
    if (view.kind !== "copy" || hovered) return;
    const tick = setInterval(() => setLeft((ms) => ms - COPY_TICK), COPY_TICK);
    return () => clearInterval(tick);
  }, [view, hovered]);
  useEffect(() => {
    if (view.kind === "copy" && left <= 0) setView({ kind: "hidden" });
  }, [view, left]);

  // Tell the backend where the pill is while it has a button, so its window
  // takes the mouse there and nowhere else. Layout sizes, not the box on
  // screen: the pill may still be sliding in.
  useEffect(() => {
    const element = pill.current;
    if (!clickable || !element) {
      setPillArea(null).catch(() => {});
      setHovered(false);
      return;
    }
    const report = () =>
      setPillArea({ x: element.offsetLeft, y: element.offsetTop, width: element.offsetWidth, height: element.offsetHeight }).catch(() => {});
    report();
    const watch = new ResizeObserver(report);
    watch.observe(element);
    return () => watch.disconnect();
  }, [clickable]);

  useEffect(() => {
    const stop = listen<boolean>("pill-hover", ({ payload }) => setHovered(payload));
    return () => {
      stop.then((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    let before: PipelineUpdate["phase"] = "idle";
    const go = (next: View) => setView(next);
    const hide = () => go({ kind: "hidden" });
    const say = (note: Note) => go({ kind: "say", note });
    // When the recording under way began; a tap that turns it hands-free keeps the clock.
    let began = 0;

    const stop = listen<PipelineUpdate>("pipeline", ({ payload }) => {
      // A new press interrupts whatever was showing.
      switch (payload.phase) {
        case "recording":
          if (before !== "recording") began = performance.now();
          go({ kind: "listen", rewrite: payload.mode === "rewrite", handsFree: !!payload.handsFree, since: began });
          break;
        case "processing":
          go({ kind: "work", ...workLabel(payload) });
          break;
        case "done":
          // Silent on success, unless there is something to say.
          if (payload.dictation?.note === NOWHERE) {
            setLeft(COPY_HOLD);
            go({ kind: "copy", text: payload.dictation.text });
          } else if (payload.dictation?.note) say(readNote(payload.dictation.note, !payload.dictation.inserted));
          else hide();
          break;
        case "error":
          say(readNote(payload.message ?? "Something went wrong.", true));
          break;
        default:
          // Back to idle straight after a press: the key was only tapped.
          if (payload.message === "") hide();
          else if (before === "recording" || before === "processing") {
            say(readNote(payload.message ?? "Nothing heard. Hold the keys while you talk.", false));
          } else hide();
      }
      before = payload.phase;
    });
    // A word learned from a correction. Never over a dictation in progress.
    const stopLearned = listen<Learned>("learned", ({ payload }) => {
      if (current.current === "listen" || current.current === "work") return;
      say({
        kind: "learned",
        title: payload.newWord ? `Learned “${payload.word}”` : `“${payload.word}” can sound like “${payload.heard}”`,
        detail: payload.newWord ? `You fixed “${payload.heard}”. Undo in the menu bar.` : "Undo in the menu bar.",
      });
    });
    return () => {
      stop.then((unlisten) => unlisten());
      stopLearned.then((unlisten) => unlisten());
    };
  }, []);

  const drop = () => {
    cancelDictation().catch(() => {});
    setView({ kind: "hidden" });
  };
  const copy = (text: string) => {
    copyText(text)
      .then(() => setView({ kind: "say", note: { kind: "copied", title: "Copied. Press ⌘V to paste.", detail: null } }))
      .catch(() => setView({ kind: "say", note: { kind: "problem", title: "Couldn’t copy it.", detail: "It’s in History." } }));
  };
  const cancel = (
    <button type="button" className="px" aria-label="Cancel dictation" title="Cancel" onClick={drop}>
      <IconX />
    </button>
  );

  const v = view.kind === "hidden" ? shown : view;
  let body: ReactNode = null;
  let two = false;
  let label = "";
  if (v.kind === "listen") {
    label = v.handsFree ? "Listening, hands-free. Tap the hotkey to finish." : v.rewrite ? "Listening for a rewrite" : "Listening";
    body = (
      <>
        {cancel}
        <LevelBars levels={levels} height={21} />
        Listening
        {v.rewrite && <span className="pc">rewrite</span>}
        {v.handsFree && <span className="pc">tap to finish</span>}
        <span className="tm">{clock(elapsed)}</span>
      </>
    );
  } else if (v.kind === "work") {
    label = v.label;
    body = (
      <>
        {cancel}
        <Bars mode="work" />
        {v.label}
        {v.chip && <span className="pc">{v.chip}</span>}
      </>
    );
  } else if (v.kind === "say") {
    const { note } = v;
    two = !!note.detail;
    label = [note.title, note.detail].filter(Boolean).join(". ");
    const icon =
      note.kind === "learned" || note.kind === "copied" ? (
        <span className="ok" style={{ display: "inline-flex" }}>
          <IconOkFill />
        </span>
      ) : note.kind === "paste" ? (
        <span className="pe" style={{ display: "inline-flex" }}>
          <IconWarnFill />
        </span>
      ) : note.kind === "problem" ? (
        <span className="bdg bad" aria-hidden="true">
          !
        </span>
      ) : (
        <span className="nt" style={{ display: "inline-flex" }}>
          <IconInfoLine />
        </span>
      );
    body = (
      <>
        {icon}
        {note.detail ? (
          <span className="ln">
            <span>{withKeys(note.title)}</span>
            <small>{withKeys(note.detail)}</small>
          </span>
        ) : (
          withKeys(note.title)
        )}
      </>
    );
  } else if (v.kind === "copy") {
    label = "Nowhere to type it. Copy it?";
    const share = Math.max(0, left) / COPY_HOLD;
    body = (
      <>
        <svg className="ring" viewBox="0 0 20 20" aria-hidden="true">
          <circle cx="10" cy="10" r="8" />
          <circle cx="10" cy="10" r="8" pathLength={100} strokeDasharray="100" strokeDashoffset={100 * (1 - share)} />
        </svg>
        Nowhere to type it
        <button type="button" className="pb" onClick={() => copy(v.text)}>
          Copy
        </button>
      </>
    );
  }

  return (
    <div className="pillwin">
      <div
        ref={pill}
        className={`qpill ${two ? "tw" : ""} ${clickable ? "hx" : ""} ${hovered ? "hv" : ""} ${view.kind === "hidden" ? "off" : ""}`}
        role="status"
        aria-live="polite"
        aria-label={view.kind === "hidden" ? "" : label}
      >
        {body}
      </div>
    </div>
  );
}
