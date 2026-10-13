import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { PipelineUpdate } from "../api";
import { keycaps } from "../hotkey";

const LABELS: Record<PipelineUpdate["phase"], string> = {
  idle: "Ready",
  recording: "Listening…",
  processing: "Working…",
  done: "Done",
  error: "Problem",
};

export default function TrySection({ hotkey }: { hotkey: string }) {
  const [update, setUpdate] = useState<PipelineUpdate>({ phase: "idle", message: null });

  useEffect(() => {
    const stop = listen<PipelineUpdate>("pipeline", (e) => setUpdate(e.payload));
    return () => {
      stop.then((unlisten) => unlisten());
    };
  }, []);

  return (
    <section className="card">
      <h2>Try it</h2>
      <p className="muted">
        Hold {keycaps(hotkey).join(" ")} and say something.
      </p>
      <div className="row">
        <span className={`status status-${update.phase}`}>{LABELS[update.phase]}</span>
        {update.message && <span className="muted small">{update.message}</span>}
      </div>
    </section>
  );
}
