import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  deleteApiKey,
  hasApiKey,
  listCleanupModels,
  rewriteStyles,
  setApiKey,
  testCleanup,
  testRewrite,
  type Provider,
  type RewriteStyle,
} from "../api";
import { hotkeyLabel } from "../hotkey";
import { cleanupGap } from "../readiness";
import { shortName, useApp } from "../state";
import { IconLock, Notice, seconds, Switch } from "../ui";

type Result = { tone: "ok" | "bad" | "info"; title: string; body: string } | null;

/** "https://api.example.com/v1" -> "api.example.com" */
function host(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return "the address you enter";
  }
}

/** What each provider asks of you, in three words. */
function providerNote(p: Provider): string {
  if (p.id === "custom") return "Any compatible URL";
  return p.needsKey ? "API key" : "This Mac, no key";
}

/**
 * The language model: whether dictations are tidied, which service does it,
 * and how the rewrite hotkey rewords things. The key field is one-way; a
 * stored key is never shown, only removed.
 */
export default function CleanupSection() {
  const app = useApp();
  const { settings, update, provider, hasKey, setHasKey, setCleanupFault } = app;
  const [keyDraft, setKeyDraft] = useState("");
  const [modelDraft, setModelDraft] = useState(settings.cleanupModel);
  const [urlDraft, setUrlDraft] = useState(settings.baseUrl);
  const [models, setModels] = useState<string[]>([]);
  const [result, setResult] = useState<Result>(null);
  const [busy, setBusy] = useState<"test" | "list" | null>(null);

  const rewriteOn = settings.rewriteHotkey !== "";
  // The rewrite hotkey uses this setup too, so it stays live while either is on.
  const inUse = settings.cleanupEnabled || rewriteOn;
  const ready = provider ? !provider.needsKey || hasKey : false;
  const baseUrl = provider?.baseUrl || settings.baseUrl;
  const model = settings.cleanupModel || provider?.defaultModel || "";
  const gap = cleanupGap(app);

  const refreshModels = useCallback(async (announce: boolean) => {
    if (announce) setBusy("list");
    try {
      const found = await listCleanupModels();
      setModels(found);
      if (announce) {
        setResult({
          tone: "info",
          title: `Found ${found.length} ${found.length === 1 ? "model" : "models"}.`,
          body: "Click the model box to pick one.",
        });
      }
    } catch (err) {
      setModels([]);
      if (announce) setResult({ tone: "bad", title: "Couldn’t list models.", body: String(err) });
    }
    if (announce) setBusy(null);
  }, []);

  // Each provider has its own key, model and model list.
  useEffect(() => {
    setKeyDraft("");
    setModels([]);
    setResult(null);
    setModelDraft(settings.cleanupModel);
  }, [settings.provider, settings.cleanupModel]);

  // Offer the provider's models as soon as it can be asked for them.
  useEffect(() => {
    if (inUse && ready && baseUrl) refreshModels(false);
  }, [inUse, ready, baseUrl, settings.provider, refreshModels]);

  if (!provider) return null;
  const name = shortName(provider.label);

  const saveKey = async () => {
    try {
      await setApiKey(provider.id, keyDraft);
      // Drop the key from the page as soon as it is in the Keychain.
      setKeyDraft("");
      setHasKey(true);
      setCleanupFault(null);
      setResult(null);
    } catch (err) {
      setResult({ tone: "bad", title: "The key was not saved.", body: String(err) });
    }
  };

  const removeKey = async () => {
    await deleteApiKey(provider.id);
    setHasKey(false);
    setModels([]);
    setResult(null);
  };

  const saveModel = async (value: string) => {
    const next = value.trim();
    if (next !== settings.cleanupModel) await update({ cleanupModel: next });
  };

  const saveUrl = async () => {
    if (urlDraft.trim() === settings.baseUrl) return;
    try {
      await update({ baseUrl: urlDraft.trim() });
      setResult(null);
      // A key belongs to the host it was saved for; a new host starts without one.
      setHasKey(await hasApiKey(provider.id));
    } catch (err) {
      setResult({ tone: "bad", title: "That address can’t be used.", body: String(err) });
    }
  };

  const test = async () => {
    setBusy("test");
    setResult(null);
    const started = performance.now();
    try {
      const sample = await testCleanup();
      setCleanupFault(null);
      setResult({
        tone: "ok",
        title: "Works.",
        body: `${name} tidied a test sentence in ${seconds(performance.now() - started)}: “${sample}”`,
      });
    } catch (err) {
      setCleanupFault(String(err));
      setResult({ tone: "bad", title: "The test failed.", body: String(err) });
    }
    setBusy(null);
  };

  return (
    <>
      <div className="sechead">
        <h2 className="h">Cleanup</h2>
        <Switch
          label="Clean up dictations"
          checked={settings.cleanupEnabled}
          onChange={(cleanupEnabled) => update({ cleanupEnabled })}
        />
      </div>
      <p className="sub">
        A language model tidies each dictation: drops filler words, fixes punctuation and matches the app
        you’re in. When it’s off or it fails, the raw text goes in.
      </p>

      {!settings.cleanupEnabled && <Example />}

      <div className="field">
        <span className="flabel">Provider</span>
        <div className={`provs ${inUse ? "" : "dimmed"}`} role="group" aria-label="Provider" aria-hidden={!inUse}>
          {app.catalog.map((p) => (
            <button
              key={p.id}
              className="pchip"
              aria-pressed={inUse && p.id === provider.id}
              tabIndex={inUse ? 0 : -1}
              onClick={() => p.id !== provider.id && update({ provider: p.id, cleanupModel: "" })}
            >
              <b>{shortName(p.label)}</b>
              <span>{providerNote(p)}</span>
            </button>
          ))}
        </div>
      </div>

      {!inUse ? (
        <p className="hint">Turn cleanup on to choose a provider.</p>
      ) : (
        <>
          {!settings.cleanupEnabled && (
            <p className="hint">
              Cleanup is off, but the rewrite hotkey ({hotkeyLabel(settings.rewriteHotkey)}) uses this provider
              and model.
            </p>
          )}

          {provider.id === "custom" && (
            <label className="field">
              <span className="flabel">Base URL</span>
              <input
                className="input mono"
                type="text"
                placeholder="https://api.example.com/v1"
                value={urlDraft}
                spellCheck={false}
                onChange={(e) => setUrlDraft(e.target.value)}
                onBlur={saveUrl}
              />
            </label>
          )}

          {provider.needsKey && (
            <div className="field">
              <div className="flabel">
                API key
                {!hasKey && provider.keyUrl && (
                  <button className="link" onClick={() => openUrl(provider.keyUrl)}>
                    Get {/^[AEIOU]/.test(name) ? "an" : "a"} {name} key ↗
                  </button>
                )}
              </div>
              {hasKey ? (
                <div className="stored">
                  <IconLock />
                  <span>A key is stored for {name}.</span>
                  <button className="btn sm" onClick={removeKey}>
                    Remove
                  </button>
                </div>
              ) : (
                <div className="frow">
                  <label style={{ flex: 1 }}>
                    <span className="sr">{name} API key</span>
                    <input
                      className="input mono"
                      type="password"
                      placeholder={`Paste your ${name} key`}
                      value={keyDraft}
                      autoComplete="off"
                      spellCheck={false}
                      onChange={(e) => setKeyDraft(e.target.value)}
                      onKeyDown={(e) => e.key === "Enter" && keyDraft.trim() && saveKey()}
                    />
                  </label>
                  <button className="btn primary" disabled={!keyDraft.trim()} onClick={saveKey}>
                    Save
                  </button>
                </div>
              )}
              <p className="hint">
                Kept in the macOS Keychain and sent only to {host(baseUrl)}. Once saved it can’t be shown
                again, only removed.
              </p>
            </div>
          )}

          <div className="field">
            <div className="flabel">Model</div>
            <div className="frow">
              <label style={{ flex: 1 }}>
                <span className="sr">Model name</span>
                <input
                  className="input mono"
                  type="text"
                  list="cleanup-models"
                  placeholder={provider.defaultModel || "Type a name, or list them"}
                  value={modelDraft}
                  spellCheck={false}
                  disabled={!ready}
                  onChange={(e) => {
                    setModelDraft(e.target.value);
                    // Picking from the list should save straight away.
                    if (models.includes(e.target.value)) saveModel(e.target.value);
                  }}
                  onBlur={(e) => saveModel(e.target.value)}
                />
              </label>
              <datalist id="cleanup-models">
                {models.map((m) => (
                  <option key={m} value={m} />
                ))}
              </datalist>
              <button className="btn" disabled={!ready || busy !== null} onClick={() => refreshModels(true)}>
                {busy === "list" ? <span className="spin" /> : null}List models
              </button>
              <button className="btn" disabled={!ready || !model || busy !== null} onClick={test}>
                {busy === "test" ? <span className="spin" /> : null}Test
              </button>
            </div>
            <p className="hint">
              {provider.defaultModel
                ? `Leave empty to use ${provider.defaultModel}. A small, fast model is best: you wait for it.`
                : "A small, fast model is best: cleanup is light editing, and you wait for it."}
            </p>
          </div>

          {result ? (
            <Notice tone={result.tone}>
              <b>{result.title}</b> {result.body}
              {result.tone === "bad" && settings.cleanupEnabled && " Until this works, dictations go in raw."}
            </Notice>
          ) : (
            settings.cleanupEnabled &&
            gap && (
              <Notice tone="warn">
                <b>Cleanup is on, but it needs {gap}.</b> Until it has one, dictations go in raw.
              </Notice>
            )
          )}

          <RewriteStyleCard />
        </>
      )}
    </>
  );
}

/** Shown while cleanup is off: what it would do, without a wall of settings. */
function Example() {
  return (
    <div className="card stack">
      <span className="label">One dictation, three apps</span>
      <dl className="example">
        <dt>Heard</dt>
        <dd className="mono">um so can we uh move the call to tomorrow</dd>
        <dt>Mail</dt>
        <dd>Could we move the call to tomorrow?</dd>
        <dt>Slack</dt>
        <dd>can we move the call to tomorrow?</dd>
        <dt>Notes</dt>
        <dd>Move the call to tomorrow.</dd>
      </dl>
      <div className="styles">
        <span className="chip plain">chat</span>
        <span className="chip plain">email</span>
        <span className="chip plain">code</span>
        <span className="chip plain">notes</span>
      </div>
    </div>
  );
}

/**
 * How the second hotkey rewords a dictation: one of the ready-made styles, or
 * an instruction of your own.
 */
function RewriteStyleCard() {
  const { settings, update, setTab, provider, hasKey } = useApp();
  const [styles, setStyles] = useState<RewriteStyle[]>([]);
  const [draft, setDraft] = useState(settings.rewritePrompt);
  const [result, setResult] = useState<Result>(null);
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    rewriteStyles().then(setStyles);
  }, []);

  const preset = styles.find((s) => s.id === settings.rewriteStyle);
  const isCustom = settings.rewriteStyle === "custom";
  const canTest = !!provider && (!provider.needsKey || hasKey) && !!(settings.cleanupModel || provider.defaultModel);

  if (!settings.rewriteHotkey) {
    return (
      <div className="card wcard">
        <div>
          <span className="flabel">Rewrite</span>
          <p className="hint">
            A second hotkey that rewords what you said so it reads better, instead of only tidying it. It is
            off.
          </p>
        </div>
        <button className="btn" onClick={() => setTab("hotkey")}>
          Set a hotkey
        </button>
      </div>
    );
  }

  // Start a custom style from whatever was showing, so there is something to edit.
  const startCustom = async () => {
    const seed = settings.rewritePrompt || preset?.instruction || "";
    setDraft(seed);
    setResult(null);
    await update({ rewriteStyle: "custom", rewritePrompt: seed });
  };

  const saveDraft = async () => {
    if (draft.trim() !== settings.rewritePrompt) await update({ rewritePrompt: draft.trim() });
  };

  const test = async () => {
    setTesting(true);
    setResult(null);
    try {
      await saveDraft();
      setResult({ tone: "ok", title: "A sample came back as:", body: `“${await testRewrite()}”` });
    } catch (err) {
      setResult({ tone: "bad", title: "The test failed.", body: String(err) });
    }
    setTesting(false);
  };

  return (
    <div className="card stack">
      <div className="cardhead">
        <span className="label">Rewrite style</span>
        <span className="meta">hold {hotkeyLabel(settings.rewriteHotkey)}</span>
      </div>
      <div className="schips" role="group" aria-label="Rewrite style">
        {styles.map((s) => (
          <button
            key={s.id}
            className="schip"
            aria-pressed={s.id === settings.rewriteStyle}
            onClick={() => {
              setResult(null);
              update({ rewriteStyle: s.id });
            }}
          >
            {s.label}
          </button>
        ))}
        <button className="schip" aria-pressed={isCustom} onClick={startCustom}>
          Your own
        </button>
      </div>

      {isCustom ? (
        <label>
          <span className="sr">Your rewrite instruction</span>
          <textarea
            className="input area"
            rows={4}
            value={draft}
            placeholder="Describe how you want it rewritten."
            onChange={(e) => setDraft(e.target.value)}
            onBlur={saveDraft}
          />
        </label>
      ) : (
        preset && <p className="quote">{preset.instruction}</p>
      )}

      <p className="hint">
        {isCustom
          ? "Your own instruction is followed as written, so it can also translate, expand or change the tone. It starts from the style you had selected."
          : "The ready-made styles keep every point you made and add nothing you did not say."}
      </p>

      <div className="acts">
        <button className="btn sm" disabled={!canTest || testing} onClick={test}>
          {testing ? <span className="spin" /> : null}Test on a sample
        </button>
        {!canTest && <span className="hint">Needs the provider and model above.</span>}
      </div>
      {result && (
        <Notice tone={result.tone}>
          <b>{result.title}</b> {result.body}
        </Notice>
      )}
    </div>
  );
}
