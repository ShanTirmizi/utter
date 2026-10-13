import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { PipelineUpdate } from "./api";
import "./Pill.css";

type View =
  | { kind: "hidden" }
  | { kind: "recording"; rewrite: boolean }
  | { kind: "working"; rewrite: boolean }
  | { kind: "note"; text: string; bad: boolean };

/**
 * The small status indicator at the bottom of the screen. Its window is always
 * present but see-through and click-through, so showing a status never moves
 * keyboard focus away from the app being dictated into.
 */
export default function Pill() {
  const [view, setView] = useState<View>({ kind: "hidden" });

  useEffect(() => {
    let hideTimer: ReturnType<typeof setTimeout> | undefined;
    const showBriefly = (text: string, bad: boolean, ms: number) => {
      setView({ kind: "note", text, bad });
      hideTimer = setTimeout(() => setView({ kind: "hidden" }), ms);
    };

    const stop = listen<PipelineUpdate>("pipeline", ({ payload }) => {
      clearTimeout(hideTimer);
      switch (payload.phase) {
        case "recording":
          setView({ kind: "recording", rewrite: payload.mode === "rewrite" });
          break;
        case "processing":
          setView({ kind: "working", rewrite: payload.mode === "rewrite" });
          break;
        case "done":
          // Stay quiet on success unless there is something to say, such as
          // "copied to the clipboard" or "cleanup skipped".
          if (payload.dictation?.note) {
            showBriefly(payload.dictation.note, !payload.dictation.inserted, 4000);
          } else {
            setView({ kind: "hidden" });
          }
          break;
        case "error":
          showBriefly(payload.message ?? "Something went wrong.", true, 4000);
          break;
        default:
          setView({ kind: "hidden" });
      }
    });
    return () => {
      clearTimeout(hideTimer);
      stop.then((unlisten) => unlisten());
    };
  }, []);

  if (view.kind === "hidden") return null;

  return (
    <div className="pill-wrap">
      <div className={`pill ${view.kind === "note" && view.bad ? "pill-bad" : ""}`}>
        {view.kind === "recording" && (
          <>
            <span className="bars" aria-hidden="true">
              <i /><i /><i /><i /><i />
            </span>
            <span>{view.rewrite ? "Listening · rewrite" : "Listening"}</span>
          </>
        )}
        {view.kind === "working" && (
          <>
            <span className="spinner" aria-hidden="true" />
            <span>{view.rewrite ? "Rewriting" : "Working"}</span>
          </>
        )}
        {view.kind === "note" && <span className="pill-text">{view.text}</span>}
      </div>
    </div>
  );
}
