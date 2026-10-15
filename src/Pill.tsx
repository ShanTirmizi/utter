import { Fragment, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import type { Learned, PipelineUpdate } from "./api";
import { useElapsed, useLevels, useReducedMotion } from "./state";
import { clock } from "./ui";

type Note = {
  /** The whole message, used when it fits on one line. */
  whole: string;
  /** What happened. */
  title: string;
  /** Why, or what to do about it. */
  detail: string | null;
  /** True when the message is always shown as two lines. */
  split: boolean;
  problem: boolean;
};

type View =
  | { kind: "hidden" }
  | { kind: "listen"; rewrite: boolean; handsFree: boolean; since: number }
  | { kind: "work"; label: string; chip: string | null }
  | ({ kind: "note" } & Note);

// The pill is at most 344 x 56 inside its 400 x 112 window, which leaves room
// all round for its shadow and for the width animation to overshoot. The badge takes the first 46 px; text ends 18 px from the edge.
const MAX_WIDTH = 344;
const BADGE = 46;
const TAIL = 18;
const WAVE_BARS = 26;
/** How the five badge bars share one loudness reading. */
const BADGE_SHAPE = [0.6, 0.85, 1, 0.85, 0.6];
const BADGE_REST = [6, 11, 15, 11, 6];

const HOLD_ONE_LINE = 2500;
const HOLD_TWO_LINES = 4000;
const HOLD_PROBLEM = 8000;

/** Turn a backend message into "what happened" and "why". */
function readNote(text: string, problem: boolean): Note {
  const copied = text.match(/^Copied to the clipboard \((.+?)\)\. Press ⌘V to paste\.\s*(.*)$/);
  if (copied) {
    const reason = copied[1].charAt(0).toUpperCase() + copied[1].slice(1);
    return {
      whole: text,
      title: "Copied. Press ⌘V to paste.",
      detail: [`${reason}.`, copied[2]].filter(Boolean).join(" "),
      split: true,
      problem,
    };
  }
  const skipped = text.match(/^(Cleanup|Rewrite) skipped: (.+)$/);
  if (skipped) {
    return {
      whole: text,
      title: `${skipped[1]} skipped, raw text inserted.`,
      detail: skipped[2],
      split: true,
      problem,
    };
  }
  const cut = text.indexOf(". ");
  if (cut > 0) {
    return { whole: text, title: text.slice(0, cut + 1), detail: text.slice(cut + 2), split: false, problem };
  }
  return { whole: text, title: text, detail: null, split: false, problem };
}

/** Draw "⌘V" as two small keys. */
function withKeys(text: string): ReactNode {
  return text.split("⌘V").map((part, i) => (
    <Fragment key={i}>
      {i > 0 && (
        <>
          <kbd className="pk">⌘</kbd>
          <kbd className="pk">V</kbd>
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
      return { label: app ? `Tidying for ${app}` : "Tidying", chip: style };
    case "rewriting":
      return { label: app ? `Rewriting for ${app}` : "Rewriting", chip: null };
    case "editing":
      return { label: "Editing your selection", chip: null };
    default:
      return { label: update.mode === "rewrite" ? "Rewriting" : "Working", chip: null };
  }
}

/**
 * The small status indicator at the bottom of the screen. Its window is always
 * present but see-through and click-through, so showing a status never moves
 * keyboard focus away from the app being dictated into.
 *
 * It is one shape that changes width, height and corner radius between
 * states, with the contents crossfading inside, so the eye never loses it.
 */
export default function Pill() {
  const [view, setView] = useState<View>({ kind: "hidden" });
  // Bars lie flat for a moment before the pill sinks away.
  const [resting, setResting] = useState(false);
  // Each layer keeps its last contents so they can fade out rather than vanish.
  const [listenView, setListenView] = useState({ rewrite: false, handsFree: false, since: 0 });
  const [workView, setWorkView] = useState<{ label: string; chip: string | null }>({ label: "", chip: null });
  const [noteView, setNoteView] = useState<Note>({ whole: "", title: "", detail: null, split: false, problem: false });
  const [box, setBox] = useState({ width: 44, height: 44, radius: 22, twoLines: false });
  const [fontsReady, setFontsReady] = useState(false);

  const listenFit = useRef<HTMLSpanElement>(null);
  const workFit = useRef<HTMLSpanElement>(null);
  const wholeRuler = useRef<HTMLSpanElement>(null);
  const titleRuler = useRef<HTMLSpanElement>(null);
  const detailRuler = useRef<HTMLSpanElement>(null);

  // What is showing now, for events that must not interrupt a dictation.
  const showing = useRef<View["kind"]>("hidden");
  showing.current = view.kind;

  const reduced = useReducedMotion();
  const listening = view.kind === "listen";
  const levels = useLevels(listening, WAVE_BARS);
  const elapsed = useElapsed(listening, listenView.since);

  useEffect(() => {
    document.fonts.ready.then(() => setFontsReady(true));
  }, []);

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let before: PipelineUpdate["phase"] = "idle";
    const hide = () => {
      setResting(false);
      setView({ kind: "hidden" });
    };
    const show = (note: Note, after: () => void = hide) => {
      setNoteView(note);
      setView({ kind: "note", ...note });
      const hold = note.problem ? HOLD_PROBLEM : note.detail ? HOLD_TWO_LINES : HOLD_ONE_LINE;
      timer = setTimeout(after, hold);
    };
    // When the recording under way began; a tap that turns it hands-free keeps the clock.
    let began = 0;

    const stop = listen<PipelineUpdate>("pipeline", ({ payload }) => {
      // A new press interrupts whatever was showing.
      clearTimeout(timer);
      setResting(false);
      switch (payload.phase) {
        case "recording": {
          if (before !== "recording") began = performance.now();
          const next = { rewrite: payload.mode === "rewrite", handsFree: !!payload.handsFree, since: began };
          setListenView(next);
          setView({ kind: "listen", ...next });
          break;
        }
        case "processing": {
          const next = workLabel(payload);
          setWorkView(next);
          setView({ kind: "work", ...next });
          break;
        }
        case "done":
          // Stay quiet on success unless there is something to say, such as
          // "copied to the clipboard" or "cleanup skipped".
          if (payload.dictation?.note) {
            show(readNote(payload.dictation.note, !payload.dictation.inserted));
          } else {
            setResting(true);
            timer = setTimeout(hide, 120);
          }
          break;
        case "error":
          show(readNote(payload.message ?? "Something went wrong.", true));
          break;
        default:
          // Back to idle straight after a press: the key was only tapped.
          if (payload.message === "") {
            // Dropped on purpose (fn turned out to be part of a shortcut).
            hide();
          } else if (before === "recording" || before === "processing") {
            show(readNote(payload.message ?? "Nothing heard. Hold the keys while you speak.", false));
          } else {
            hide();
          }
      }
      before = payload.phase;
    });
    // A word learned from a correction. Said only when nothing more
    // important is showing; the window and the menu bar have it too.
    const stopLearned = listen<Learned>("learned", ({ payload }) => {
      if (showing.current === "listen" || showing.current === "work") return;
      clearTimeout(timer);
      setResting(false);
      const title = payload.newWord ? `Learned “${payload.word}”` : `“${payload.word}” can sound like “${payload.heard}”`;
      const detail = payload.newWord
        ? `You corrected “${payload.heard}”. Undo it from the menu bar.`
        : "Learned from your correction. Undo it from the menu bar.";
      show({ whole: `${title}. ${detail}`, title, detail, split: true, problem: false });
    });
    return () => {
      clearTimeout(timer);
      stop.then((unlisten) => unlisten());
      stopLearned.then((unlisten) => unlisten());
    };
  }, []);

  // Size the pill to whatever the current layer needs.
  useLayoutEffect(() => {
    const one = (inner: number) => ({
      width: Math.min(MAX_WIDTH, BADGE + inner + TAIL),
      height: 44,
      radius: 22,
      twoLines: false,
    });
    if (view.kind === "hidden") {
      setBox((old) => ({ ...old, width: 44, height: 44, radius: 22 }));
    } else if (view.kind === "listen") {
      setBox(one(listenFit.current?.offsetWidth ?? 150));
    } else if (view.kind === "work") {
      setBox(one(workFit.current?.offsetWidth ?? 90));
    } else {
      const whole = wholeRuler.current?.offsetWidth ?? 0;
      if (!view.split && BADGE + whole + TAIL <= MAX_WIDTH) {
        setBox(one(whole));
      } else {
        const widest = Math.max(titleRuler.current?.offsetWidth ?? 0, detailRuler.current?.offsetWidth ?? 0);
        setBox({ width: Math.min(MAX_WIDTH, BADGE + widest + TAIL), height: 56, radius: 18, twoLines: true });
      }
    }
  }, [view, reduced, fontsReady, listenView.handsFree]);

  const hidden = view.kind === "hidden";
  const problem = view.kind === "note" && view.problem;
  const latest = levels[levels.length - 1];

  // The badge is the app mark, alive: it bounces with the voice, travels while
  // working, and lies flat when there is only something to read.
  let badgeBars: { height: number; opacity: number }[];
  if (listening && !reduced) {
    badgeBars = BADGE_SHAPE.map((share) => ({ height: Math.round(4 + Math.min(1, latest * share * 1.15) * 13), opacity: 1 }));
  } else if (listening) {
    // Reduced motion: a five-step level meter. Bars light up; nothing moves.
    const lit = Math.round(latest * 5);
    badgeBars = BADGE_REST.map((height, i) => ({ height, opacity: i < lit ? 1 : 0.35 }));
  } else if (view.kind === "work" && !resting) {
    badgeBars = BADGE_REST.map((height) => ({ height, opacity: 1 }));
  } else {
    badgeBars = BADGE_REST.map(() => ({ height: 3, opacity: 1 }));
  }
  const badgeClass = listening ? "talkL" : view.kind === "work" && !resting && !reduced ? "think" : "";

  const label =
    view.kind === "listen"
      ? view.handsFree
        ? "Listening, hands-free. Tap the hotkey again to finish."
        : "Listening"
      : view.kind === "work" ? view.label : view.kind === "note" ? view.whole : "";

  return (
    <div className="pillwin">
      <div
        className={`mp ${hidden ? "off" : ""} ${problem ? "prob" : ""}`}
        style={{ width: box.width, height: box.height, borderRadius: box.radius }}
        role="status"
        aria-live="polite"
        aria-label={label}
      >
        <span className={`badge mb ${badgeClass} ${problem ? "hide" : ""}`} aria-hidden="true">
          {badgeBars.map((bar, i) => (
            <i key={i} style={{ height: bar.height, opacity: bar.opacity }} />
          ))}
        </span>
        <span className={`badge alert mbalert ${problem ? "on" : ""}`} aria-hidden="true">
          !
        </span>

        <div className={`layer ${view.kind === "listen" ? "on" : ""}`} aria-hidden="true">
          <span className="fit" ref={listenFit}>
            <span className="lw">
              {levels.map((level, i) => (
                <i key={i} style={{ height: Math.round(3 + level * 19) }} />
              ))}
            </span>
            <span className="meter">Listening</span>
            <span className="ptime">{clock(elapsed)}</span>
            {listenView.rewrite && <span className="pchip2">rewrite</span>}
            {listenView.handsFree && <span className="pchip2">tap to finish</span>}
          </span>
        </div>

        <div className={`layer ${view.kind === "work" ? "on" : ""}`} aria-hidden="true">
          <span className="fit" ref={workFit}>
            <span>{workView.label}</span>
            {workView.chip && <span className="pchip2">{workView.chip}</span>}
          </span>
        </div>

        <div className={`layer ${box.twoLines ? "wrap" : ""} ${view.kind === "note" ? "on" : ""}`} aria-hidden="true">
          <span className="fit">
            {!box.twoLines ? (
              <span>{withKeys(noteView.whole)}</span>
            ) : noteView.detail ? (
              <span className="plines">
                <b>{withKeys(noteView.title)}</b>
                <small>{noteView.detail}</small>
              </span>
            ) : (
              <span className="clamp">{withKeys(noteView.whole)}</span>
            )}
          </span>
        </div>
      </div>

      {/* Never seen: the note's text at its natural width, to size the pill. */}
      <div className="rulers" aria-hidden="true">
        <span ref={wholeRuler}>{withKeys(noteView.whole)}</span>
        <b ref={titleRuler}>{withKeys(noteView.title)}</b>
        <small ref={detailRuler}>{noteView.detail}</small>
      </div>
    </div>
  );
}
