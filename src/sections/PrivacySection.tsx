import { useState } from "react";
import { shortName, useApp } from "../state";
import { Switch } from "../ui";

/** "https://api.example.com/v1" -> "api.example.com", or "" when it can't be read. */
function host(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return "";
  }
}

/** True for an address on this Mac, as Ollama's is. */
function onThisMac(url: string): boolean {
  return /^(localhost|127\.0\.0\.1|\[::1\])(:\d+)?$/.test(host(url));
}

/**
 * What Utter keeps, what it reads, and what leaves this Mac, in one place.
 * The switches here are the same settings they always were; only where they
 * are shown changed.
 */
export default function PrivacySection() {
  const app = useApp();
  const { settings, update, history, provider } = app;
  const [error, setError] = useState<string | null>(null);

  const run = (work: Promise<unknown>) =>
    work.then(
      () => setError(null),
      (err) => setError(String(err)),
    );

  const kept = history.filter((d) => d.recording !== null).length;
  // A recording belongs to a history entry, so it can only be switched on
  // with history kept. Switching it off is always allowed.
  const needsHistory = !settings.keepHistory && !settings.keepAudio;

  // Switching recordings off deletes them, so the history is read again.
  const setKeepAudio = (keepAudio: boolean) => run(update({ keepAudio }).then(() => app.refreshHistory()));

  // Where text goes when the language model is used.
  const rewriteOn = settings.rewriteHotkey !== "";
  const modelUsed = settings.cleanupEnabled || rewriteOn;
  const baseUrl = provider?.baseUrl || settings.baseUrl;
  const local = onThisMac(baseUrl);
  const name = provider ? (provider.id === "custom" ? host(baseUrl) || "your provider" : shortName(provider.label)) : "your provider";
  const leaves = modelUsed && !local;

  const said = !modelUsed
    ? "Never. Cleanup and rewrite are off."
    : local
      ? `Stays on this Mac, with ${name}.`
      : settings.cleanupEnabled
        ? `To ${name}, to tidy it.`
        : `To ${name}, only when you rewrite.`;
  const names = !settings.screenWords
    ? "Not read."
    : leaves && settings.cleanupEnabled
      ? `To ${name}, with each cleanup.`
      : "Read on this Mac, never sent.";
  const nearby =
    settings.screenWords && settings.screenContext && leaves && settings.cleanupEnabled
      ? `To ${name}, with each cleanup.`
      : "Never.";

  const rows: [string, string][] = [
    ["Your voice", "Never. It’s turned into text on this Mac."],
    ["What you said", said],
    ["Names on screen", names],
    ["Text near the cursor", nearby],
    ["History and recordings", settings.keepHistory || history.length ? "Stay on this Mac." : "Not kept."],
    ["Update checks", settings.checkUpdates ? "Asks GitHub once a day." : "Only when you check, in About."],
  ];

  return (
    <>
      <div className="qhead">
        <div className="t">
          <h1>Privacy</h1>
          <p>What Utter keeps, what it reads, and what leaves this Mac.</p>
        </div>
      </div>

      <div className="qcard">
        <div className="qlab">Kept on this Mac</div>
        <div className="qsw" style={{ paddingTop: 10, borderTop: 0 }}>
          <div className="tx">
            <b>Keep history</b>
            <span>Your last 200 dictations, so you can search, copy and fix them in History.</span>
          </div>
          <Switch label="Keep history" checked={settings.keepHistory} onChange={(keepHistory) => run(update({ keepHistory }))} />
        </div>
        <div className="qsw" style={{ paddingBottom: 0 }}>
          <div className="tx">
            <b>Keep recordings</b>
            <span>
              {settings.keepHistory || settings.keepAudio
                ? "Off by default. Keeps the audio of your last 50 dictations, with what you changed, to test other models on. Never uploaded."
                : "Needs Keep history: each recording belongs to a dictation there."}
              {settings.keepAudio && kept > 0 && ` ${kept} kept now; switching this off deletes them.`}
            </span>
          </div>
          <Switch label="Keep recordings" checked={settings.keepAudio} disabled={needsHistory} onChange={setKeepAudio} />
        </div>
      </div>

      <div className="qcard">
        <div className="qlab">Reading your screen</div>
        <div className="qsw" style={{ paddingTop: 10, borderTop: 0 }}>
          <div className="tx">
            <b>Use what’s on screen</b>
            <span>
              Reads the app you’re dictating into, so names on screen are spelled right and suggested in Words. Only the
              names go to your cleanup model, and nothing is saved.
            </span>
          </div>
          <Switch label="Use what’s on screen" checked={settings.screenWords} onChange={(screenWords) => run(update({ screenWords }))} />
        </div>
        <div className="qsw" style={{ paddingBottom: 0 }}>
          <div className="tx">
            <b>Send nearby text to cleanup</b>
            <span>
              {settings.screenWords
                ? "Also gives your cleanup model the text around the cursor, so it can fix misheard words from what you’re writing about. With a hosted provider that text leaves your Mac. Never from a terminal or code editor, and never saved."
                : "Needs Use what’s on screen."}
            </span>
          </div>
          <Switch
            label="Send nearby text to cleanup"
            checked={settings.screenWords && settings.screenContext}
            disabled={!settings.screenWords}
            onChange={(screenContext) => run(update({ screenContext }))}
          />
        </div>
      </div>

      <div className="qcard">
        <div className="qlab">What leaves this Mac</div>
        <dl className="qleave">
          {rows.map(([what, where]) => (
            <div key={what}>
              <dt>{what}</dt>
              <dd>{where}</dd>
            </div>
          ))}
        </dl>
        <p className="qp" style={{ fontSize: 12.5 }}>
          Your API key is kept in the macOS Keychain{provider?.needsKey && host(baseUrl) ? ` and sent only to ${host(baseUrl)}` : ""}.{" "}
          <button className="qlink" onClick={() => app.setTab("provider")}>
            AI provider
          </button>
        </p>
      </div>

      {error && (
        <p className="qp" role="alert" style={{ color: "var(--bad-fg)", margin: "0 4px" }}>
          {error}
        </p>
      )}
    </>
  );
}
