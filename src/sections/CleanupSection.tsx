import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  deleteApiKey,
  hasApiKey,
  listCleanupModels,
  providers as loadProviders,
  setApiKey,
  testCleanup,
  type Provider,
  type Settings,
} from "../api";

type Props = {
  settings: Settings;
  update: (change: Partial<Settings>) => Promise<void>;
};

/** "https://api.example.com/v1" -> "api.example.com" */
function host(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return "the address you enter";
  }
}

export default function CleanupSection({ settings, update }: Props) {
  const [catalog, setCatalog] = useState<Provider[]>([]);
  const [hasKey, setHasKey] = useState(false);
  const [keyDraft, setKeyDraft] = useState("");
  const [modelDraft, setModelDraft] = useState(settings.cleanupModel);
  const [urlDraft, setUrlDraft] = useState(settings.baseUrl);
  const [models, setModels] = useState<string[]>([]);
  const [message, setMessage] = useState<string | null>(null);

  const provider = catalog.find((p) => p.id === settings.provider);
  const ready = provider ? !provider.needsKey || hasKey : false;
  const baseUrl = provider?.baseUrl || settings.baseUrl;

  useEffect(() => {
    loadProviders().then(setCatalog);
  }, []);

  const refreshModels = useCallback(async (announce: boolean) => {
    try {
      const found = await listCleanupModels();
      setModels(found);
      if (announce) setMessage(`Found ${found.length} models.`);
    } catch (err) {
      setModels([]);
      if (announce) setMessage(String(err));
    }
  }, []);

  // Each provider has its own key, model and model list.
  useEffect(() => {
    setKeyDraft("");
    setModels([]);
    setMessage(null);
    setModelDraft(settings.cleanupModel);
    hasApiKey(settings.provider).then(setHasKey);
  }, [settings.provider, settings.cleanupModel]);

  // Offer the provider's models as soon as it can be asked for them.
  useEffect(() => {
    if (ready && baseUrl) refreshModels(false);
  }, [ready, baseUrl, settings.provider, refreshModels]);

  if (!provider) return null;

  const saveKey = async () => {
    try {
      await setApiKey(provider.id, keyDraft);
      // Drop the key from the page as soon as it is in the Keychain.
      setKeyDraft("");
      setHasKey(true);
      setMessage("Saved to your Keychain.");
    } catch (err) {
      setMessage(String(err));
    }
  };

  const removeKey = async () => {
    await deleteApiKey(provider.id);
    setHasKey(false);
    setModels([]);
    setMessage("Key removed.");
  };

  const saveModel = async (value: string) => {
    const next = value.trim();
    if (next !== settings.cleanupModel) await update({ cleanupModel: next });
  };

  const saveUrl = async () => {
    if (urlDraft.trim() === settings.baseUrl) return;
    try {
      await update({ baseUrl: urlDraft.trim() });
      setMessage(null);
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

  const model = settings.cleanupModel || provider.defaultModel;

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
          checked={settings.cleanupEnabled}
          onChange={(e) => update({ cleanupEnabled: e.target.checked })}
        />
      </label>

      <h3>Provider</h3>
      <div className="row">
        <select
          value={provider.id}
          onChange={(e) => update({ provider: e.target.value, cleanupModel: "" })}
        >
          {catalog.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
      </div>

      {provider.id === "custom" && (
        <>
          <h3>Base URL</h3>
          <div className="row">
            <input
              type="text"
              placeholder="https://api.example.com/v1"
              value={urlDraft}
              spellCheck={false}
              onChange={(e) => setUrlDraft(e.target.value)}
              onBlur={saveUrl}
            />
          </div>
        </>
      )}

      {provider.needsKey && (
        <>
          <h3>API key</h3>
          {hasKey ? (
            <div className="row">
              <span className="ok">A key is stored in your Keychain.</span>
              <button onClick={removeKey}>Remove</button>
            </div>
          ) : (
            <div className="row">
              <input
                type="password"
                placeholder="Paste your key"
                value={keyDraft}
                autoComplete="off"
                spellCheck={false}
                onChange={(e) => setKeyDraft(e.target.value)}
              />
              <button className="primary" disabled={!keyDraft.trim()} onClick={saveKey}>
                Save
              </button>
            </div>
          )}
          <p className="muted small">
            Stored only in the macOS Keychain and sent only to {host(baseUrl)}.
            {provider.keyUrl && (
              <>
                {" "}
                <a
                  href="#"
                  onClick={(e) => {
                    e.preventDefault();
                    openUrl(provider.keyUrl);
                  }}
                >
                  Get a key
                </a>
              </>
            )}
          </p>
        </>
      )}

      <h3>Model</h3>
      <div className="row">
        <input
          type="text"
          list="cleanup-models"
          placeholder={provider.defaultModel || "Choose or type a model name"}
          value={modelDraft}
          spellCheck={false}
          onChange={(e) => {
            setModelDraft(e.target.value);
            // Picking from the list should save straight away.
            if (models.includes(e.target.value)) saveModel(e.target.value);
          }}
          onBlur={(e) => saveModel(e.target.value)}
        />
        <datalist id="cleanup-models">
          {models.map((m) => (
            <option key={m} value={m} />
          ))}
        </datalist>
        <span className="buttons">
          <button disabled={!ready} onClick={() => refreshModels(true)}>
            List models
          </button>
          <button disabled={!ready || !model} onClick={test}>
            Test
          </button>
        </span>
      </div>
      <p className="muted small">
        {provider.defaultModel
          ? `Leave empty to use ${provider.defaultModel}. A small, fast model is best here.`
          : "A small, fast model is best here: cleanup is light editing, and you wait for it."}
      </p>

      {settings.cleanupEnabled && provider.needsKey && !hasKey && (
        <p className="warn small">No key yet, so dictations are inserted as raw text.</p>
      )}
      {settings.cleanupEnabled && ready && !model && (
        <p className="warn small">Choose a model to switch cleanup on.</p>
      )}
      {message && <p className="small">{message}</p>}
    </section>
  );
}
