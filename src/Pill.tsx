import { Fragment, useEffect, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import type { Learned, PipelineUpdate } from "./api";
import { useElapsed, useLevels } from "./state";
import { Bars, clock, IconInfoLine, IconOkFill, IconWarnFill, LevelBars } from "./ui";

/** Something to read: one line, or what happened and then why. */
type Note = {
  kind: "note" | "learned" | "paste" | "problem";
  title: string;
  detail: string | null;
};

type View =
  | { kind: "hidden" }
  | { kind: "listen"; rewrite: boolean; handsFree: boolean; since: number }
  | { kind: "work"; label: string; chip: string | null }
  | { kind: "say"; note: Note };

const HOLD: Record<Note["kind"], number> = { note: 2500, learned: 4000, paste: 8000, problem: 8000 };
const TWO_LINES = 4000;

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
 * The small status at the bottom of the screen. Its window is see-through
 * and ignores the mouse, so showing a status never moves keyboard focus away
 * from the app being dictated into. Success is silent.
 */
export default function Pill() {
  const [view, setView] = useState<View>({ kind: "hidden" });
  // The last thing shown, kept while the pill fades out.
  const [shown, setShown] = useState<View>({ kind: "hidden" });
  const listening = view.kind === "listen";
  const levels = useLevels(listening, 5);
  const elapsed = useElapsed(listening, listening ? view.since : 0);

  useEffect(() => {
    if (view.kind !== "hidden") setShown(view);
  }, [view]);

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let before: PipelineUpdate["phase"] = "idle";
    let current: View["kind"] = "hidden";
    const go = (next: View) => {
      current = next.kind;
      setView(next);
    };
    const hide = () => go({ kind: "hidden" });
    const say = (note: Note) => {
      clearTimeout(timer);
      go({ kind: "say", note });
      const hold = note.kind === "note" && note.detail ? TWO_LINES : HOLD[note.kind];
      timer = setTimeout(hide, hold);
    };
    // When the recording under way began; a tap that turns it hands-free keeps the clock.
    let began = 0;

    const stop = listen<PipelineUpdate>("pipeline", ({ payload }) => {
      // A new press interrupts whatever was showing.
      clearTimeout(timer);
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
          if (payload.dictation?.note) say(readNote(payload.dictation.note, !payload.dictation.inserted));
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
      if (current === "listen" || current === "work") return;
      say({
        kind: "learned",
        title: payload.newWord ? `Learned “${payload.word}”` : `“${payload.word}” can sound like “${payload.heard}”`,
        detail: payload.newWord ? `You fixed “${payload.heard}”. Undo in the menu bar.` : "Undo in the menu bar.",
      });
    });
    return () => {
      clearTimeout(timer);
      stop.then((unlisten) => unlisten());
      stopLearned.then((unlisten) => unlisten());
    };
  }, []);

  const v = view.kind === "hidden" ? shown : view;
  let body: ReactNode = null;
  let two = false;
  let label = "";
  if (v.kind === "listen") {
    label = v.handsFree ? "Listening, hands-free. Tap the hotkey to finish." : v.rewrite ? "Listening for a rewrite" : "Listening";
    body = (
      <>
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
      note.kind === "learned" ? (
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
  }

  return (
    <div className="pillwin">
      <div
        className={`qpill ${two ? "tw" : ""} ${view.kind === "hidden" ? "off" : ""}`}
        role="status"
        aria-live="polite"
        aria-label={view.kind === "hidden" ? "" : label}
      >
        {body}
      </div>
    </div>
  );
}
