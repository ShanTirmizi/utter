import { useEffect, useState } from "react";
import { fromKeyEvent, keycaps } from "../hotkey";

type Props = {
  hotkey: string;
  onChange: (hotkey: string) => Promise<void>;
};

export default function HotkeySection({ hotkey, onChange }: Props) {
  return (
    <section className="card">
      <h2>Hotkey</h2>
      <p className="muted">Hold it while you speak. Let go to finish.</p>
      <HotkeyField hotkey={hotkey} onChange={onChange} />
    </section>
  );
}

/** Shows a hotkey as keycaps, with a button to record a new one. */
export function HotkeyField({ hotkey, onChange }: Props) {
  const [capturing, setCapturing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // While capturing, the next valid key combination becomes the hotkey.
  useEffect(() => {
    if (!capturing) return;
    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      if (e.code === "Escape") {
        setCapturing(false);
        return;
      }
      const next = fromKeyEvent(e);
      if (!next) return;
      setCapturing(false);
      onChange(next)
        .then(() => setError(null))
        .catch((err) => setError(String(err)));
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [capturing, onChange]);

  return (
    <>
      <div className="row">
        <div className="keycaps" aria-label={`Current hotkey: ${hotkey}`}>
          {capturing ? (
            <span className="muted">Press the new combination… (Esc to cancel)</span>
          ) : hotkey ? (
            keycaps(hotkey).map((cap, i) => <kbd key={i}>{cap}</kbd>)
          ) : (
            <span className="muted">Not set</span>
          )}
        </div>
        <button onClick={() => setCapturing((c) => !c)}>
          {capturing ? "Cancel" : hotkey ? "Change" : "Set"}
        </button>
      </div>
      {error && <p className="error">{error}</p>}
    </>
  );
}
