import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { deleteApiKey, hasApiKey, setApiKey, testCleanup } from "../api";

type Props = {
  enabled: boolean;
  onToggle: (enabled: boolean) => void;
};

export default function CleanupSection({ enabled, onToggle }: Props) {
  const [hasKey, setHasKey] = useState(false);
  const [draft, setDraft] = useState("");
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    hasApiKey().then(setHasKey);
  }, []);

  const save = async () => {
    try {
      await setApiKey(draft);
      // Drop the key from the page as soon as it is in the Keychain.
      setDraft("");
      setHasKey(true);
      setMessage("Saved to your Keychain.");
    } catch (err) {
      setMessage(String(err));
    }
  };

  const test = async () => {
    setMessage("Testing…");
    try {
      setMessage(`Works. Sample came back as: “${await testCleanup()}”`);
    } catch (err) {
      setMessage(String(err));
    }
  };

  const remove = async () => {
    await deleteApiKey();
    setHasKey(false);
    setMessage("Key removed.");
  };

  return (
    <section className="card">
      <h2>Cleanup</h2>
      <p className="muted">
        Removes filler words, fixes punctuation and matches the style of the app you are
        typing in. Optional: without it you get the raw transcription.
      </p>

      <label className="row toggle">
        <span>Clean up dictations</span>
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => onToggle(e.target.checked)}
        />
      </label>

      <h3>Anthropic API key</h3>
      {hasKey ? (
        <div className="row">
          <span className="ok">A key is stored in your Keychain.</span>
          <span className="buttons">
            <button onClick={test}>Test</button>
            <button onClick={remove}>Remove</button>
          </span>
        </div>
      ) : (
        <div className="row">
          <input
            type="password"
            placeholder="sk-ant-…"
            value={draft}
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => setDraft(e.target.value)}
          />
          <button className="primary" disabled={!draft.trim()} onClick={save}>
            Save
          </button>
        </div>
      )}
      <p className="muted small">
        Your key is stored only in the macOS Keychain and sent only to api.anthropic.com.{" "}
        <a
          href="#"
          onClick={(e) => {
            e.preventDefault();
            openUrl("https://console.anthropic.com/settings/keys");
          }}
        >
          Get a key
        </a>
      </p>
      {enabled && !hasKey && (
        <p className="warn small">No key yet, so dictations are inserted as raw text.</p>
      )}
      {message && <p className="small">{message}</p>}
    </section>
  );
}
