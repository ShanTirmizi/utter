import PermissionList, { usePermissions } from "./PermissionList";

/** First-run screen: what Utter needs from macOS, and why. */
export default function Welcome({ onDone }: { onDone: () => void }) {
  const status = usePermissions();
  const micReady = status?.microphone === "granted";

  return (
    <main className="page">
      <header>
        <h1>Welcome to Utter</h1>
        <p className="muted">
          Hold a hotkey, speak, release. Your words appear in whatever app you are using.
          Before the first dictation, macOS needs your go-ahead for two things.
        </p>
      </header>
      <section className="card">
        <PermissionList status={status} />
      </section>
      <div className="row">
        <span className="muted small">
          {micReady
            ? "You can change these later in System Settings."
            : "Allow the microphone to continue."}
        </span>
        <button className="primary" disabled={!micReady} onClick={onDone}>
          Continue
        </button>
      </div>
    </main>
  );
}
