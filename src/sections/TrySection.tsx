import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { lastDictation, type Dictation, type PipelineUpdate } from "../api";
import { keycaps } from "../hotkey";

const LABELS: Record<PipelineUpdate["phase"], string> = {
  idle: "Ready",
  recording: "Listening…",
  processing: "Working…",
  done: "Done",
  error: "Problem",
};

export default function TrySection({ hotkey }: { hotkey: string }) {
  const [phase, setPhase] = useState<PipelineUpdate["phase"]>("idle");
  const [message, setMessage] = useState<string | null>(null);
  const [dictation, setDictation] = useState<Dictation | null>(null);

  useEffect(() => {
    lastDictation().then(setDictation);
    const stop = listen<PipelineUpdate>("pipeline", (e) => {
      setPhase(e.payload.phase);
      setMessage(e.payload.message);
      if (e.payload.dictation) setDictation(e.payload.dictation);
    });
    return () => {
      stop.then((unlisten) => unlisten());
    };
  }, []);

  return (
    <section className="card">
      <h2>Try it</h2>
      <p className="muted">Hold {keycaps(hotkey).join(" ")} and say something.</p>
      <textarea rows={3} placeholder="Click here, then dictate. The text lands wherever your cursor is." />
      <div className="row">
        <span className={`status status-${phase}`}>{LABELS[phase]}</span>
        {message && <span className="muted small">{message}</span>}
      </div>
      {dictation && (
        <div className="result">
          <p>{dictation.text}</p>
          <p className="muted small">
            {dictation.cleaned
              ? `Cleaned up for ${dictation.app.name} (${dictation.app.category} style)`
              : "Raw transcription"}
            {" · "}transcribed in {dictation.transcribeMs} ms
            {dictation.cleanupMs !== null && ` · cleanup ${dictation.cleanupMs} ms`}
          </p>
          {dictation.cleaned && (
            <p className="muted small">Heard: {dictation.raw}</p>
          )}
          {dictation.note && <p className="warn small">{dictation.note}</p>}
        </div>
      )}
    </section>
  );
}
