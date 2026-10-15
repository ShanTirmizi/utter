import { useCallback, useEffect, useRef, useState } from "react";
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
import { IconOut, IconTick, Notice, seconds, Switch, type Tone } from "../ui";

type Result = { tone: Tone; title: string; body: string } | null;

/** "https://api.example.com/v1" -> "api.example.com" */
function host(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return "the address you enter";
  }
}

/** What each provider is, in three words. */
const NOTES: Record<string, string> = {
  anthropic: "Claude models",
  openai: "GPT models",
  gemini: "Gemini models",
  groq: "Fastest replies",
  openrouter: "Many models, one key",
  ollama: "This Mac, no key",
  custom: "Any compatible URL",
};

function providerNote(p: Provider): string {
  return NOTES[p.id] ?? (p.needsKey ? "API key" : "No key");
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
  const ready = provider ? !provider.needsKey || hasKey === true : false;
  const baseUrl = provider?.baseUrl || settings.baseUrl;
  const model = settings.cleanupModel || provider?.defaultModel || "";
  const gap = cleanupGap(app);

  // The provider on screen now. An answer that comes back after the user has
  // switched provider belongs to the old one and is dropped.
  const shown = useRef(settings.provider);
  shown.current = settings.provider;

  const refreshModels = useCallback(async (announce: boolean) => {
    const asked = shown.current;
    if (announce) setBusy("list");
    try {
      const found = await listCleanupModels();
      if (shown.current !== asked) return;
      setModels(found);
      if (announce) {
        setResult({ tone: "nt", title: `Found ${found.length} ${found.length === 1 ? "model" : "models"}.`, body: "Click the model box to pick one." });
      }
    } catch (err) {
      if (shown.current !== asked) return;
      setModels([]);
      if (announce) setResult({ tone: "pe", title: "Couldn’t list models.", body: String(err) });
    } finally {
      if (announce) setBusy(null);
    }
  }, []);

  // Each provider has its own key, model and model list.
  useEffect(() => {
    setKeyDraft("");
    setModels([]);
    setResult(null);
    setBusy(null);
    setUrlDraft(settings.baseUrl);
  }, [settings.provider]); // eslint-disable-line react-hooks/exhaustive-deps

  // Follow the saved model when it changes from elsewhere, without disturbing
  // what is being typed.
  useEffect(() => {
    setModelDraft((draft) => (draft.trim() === settings.cleanupModel ? draft : settings.cleanupModel));
  }, [settings.provider, settings.cleanupModel]);

  // Offer the provider's models as soon as it can be asked for them.
  useEffect(() => {
    if (inUse && ready && baseUrl) refreshModels(false);
  }, [inUse, ready, baseUrl, settings.provider, refreshModels]);

  if (!provider) return null;
  const name = shortName(provider.label);

  const saveKey = async () => {
    const asked = provider.id;
    try {
      await setApiKey(asked, keyDraft);
      if (shown.current !== asked) return;
      // Drop the key from the page as soon as it is in the Keychain.
      setKeyDraft("");
      setHasKey(true);
      setCleanupFault(null);
      setResult(null);
    } catch (err) {
      setResult({ tone: "pe", title: "The key wasn’t saved.", body: String(err) });
    }
  };

  const removeKey = async () => {
    const asked = provider.id;
    try {
      await deleteApiKey(asked);
      if (shown.current !== asked) return;
      setHasKey(false);
      setModels([]);
      setResult(null);
    } catch (err) {
      setResult({ tone: "pe", title: "The key wasn’t removed.", body: String(err) });
    }
  };

  const saveModel = async (value: string) => {
    const next = value.trim();
    if (next === settings.cleanupModel) return;
    try {
      await update({ cleanupModel: next });
      setCleanupFault(null);
    } catch (err) {
      setResult({ tone: "pe", title: "The model wasn’t saved.", body: String(err) });
    }
  };

  const saveUrl = async () => {
    if (urlDraft.trim() === settings.baseUrl) return;
    try {
      await update({ baseUrl: urlDraft.trim() });
      setResult(null);
      setCleanupFault(null);
      // A key belongs to the host it was saved for; a new host starts without one.
      setHasKey(await hasApiKey(provider.id));
    } catch (err) {
      setResult({ tone: "pe", title: "That address can’t be used.", body: String(err) });
    }
  };

  const test = async () => {
    const asked = provider.id;
    setBusy("test");
    setResult(null);
    const started = performance.now();
    try {
      await testCleanup();
      if (shown.current !== asked) return;
      setCleanupFault(null);
      setResult({ tone: "mi", title: "Works.", body: `${name} replied in ${seconds(performance.now() - started)}.` });
    } catch (err) {
      if (shown.current !== asked) return;
      setCleanupFault(String(err));
      setResult({ tone: "pe", title: String(err), body: settings.cleanupEnabled ? "Until this works, words go in as heard." : "" });
    } finally {
      if (shown.current === asked) setBusy(null);
    }
  };

  const failedTest = result?.tone === "pe" && !!app.cleanupFault;

  return (
    <>
      <div className="qhead">
        <div className="t">
          <h1>Cleanup</h1>
          <p>Drops filler words, fixes punctuation and writes for the app you’re in.</p>
        </div>
        <Switch label="Cleanup" checked={settings.cleanupEnabled} onChange={(cleanupEnabled) => update({ cleanupEnabled }).catch(() => undefined)} />
      </div>

      {!settings.cleanupEnabled && (
        <div className="qcard">
          <div className="qlab">One dictation, three apps</div>
          <dl className="qex" style={{ gridTemplateColumns: "58px 1fr" }}>
            <dt>You said</dt>
            <dd className="said">um so can we uh move the call to tomorrow</dd>
            <dt>Slack</dt>
            <dd>
              can we move the call to tomorrow? <span className="qtag sm nt" style={{ marginLeft: 6 }}>chat</span>
            </dd>
            <dt>Mail</dt>
            <dd>
              Could we move the call to tomorrow? <span className="qtag sm nt" style={{ marginLeft: 6 }}>email</span>
            </dd>
            <dt>Notes</dt>
            <dd>
              Move the call to tomorrow. <span className="qtag sm nt" style={{ marginLeft: 6 }}>notes</span>
            </dd>
          </dl>
          <p className="qp">Utter picks chat, email, code or notes style from the app you’re in. With cleanup off, it types exactly what it heard.</p>
        </div>
      )}

      <div className="qcard">
        <div className="qlab">
          Provider
          {!settings.cleanupEnabled && rewriteOn && <span className="r">For rewrite</span>}
        </div>
        <div className={`qprov ${inUse ? "" : "dim"}`} role="radiogroup" aria-label="Provider">
          {app.catalog.map((p) => (
            <button
              key={p.id}
              className="qpv"
              role="radio"
              aria-checked={inUse && p.id === provider.id}
              disabled={!inUse}
              onClick={() => p.id !== provider.id && update({ provider: p.id, cleanupModel: "" }).catch(() => undefined)}
            >
              <b>{shortName(p.label)}</b>
              <span>{providerNote(p)}</span>
            </button>
          ))}
        </div>
        {!inUse && <p className="qp">Turn on cleanup, or set a rewrite hotkey, to choose one.</p>}
        {inUse && provider.id === "custom" && (
          <label className="qfld mono">
            <input
              placeholder="https://api.example.com/v1"
              aria-label="Base URL"
              value={urlDraft}
              spellCheck={false}
              onChange={(e) => setUrlDraft(e.target.value)}
              onBlur={saveUrl}
              onKeyDown={(e) => e.key === "Enter" && saveUrl()}
            />
            <span className="qnote" style={{ paddingRight: 8, fontSize: 12 }}>
              Base URL
            </span>
          </label>
        )}
      </div>

      {inUse && provider.needsKey && (
        <div className="qcard">
          <div className="qlab">
            API key
            {hasKey === false && provider.keyUrl && (
              <span className="r">
                <button className="qlink" onClick={() => openUrl(provider.keyUrl)}>
                  Get {/^[AEIOU]/.test(name) ? "an" : "a"} {name} key
                  <IconOut />
                </button>
              </span>
            )}
          </div>
          {hasKey ? (
            <>
              <div className="qtags">
                <span className="qtag mi">
                  <IconTick />A key is stored for {name}.
                </span>
                <button className="qbtn sm ghost" onClick={removeKey}>
                  Remove
                </button>
              </div>
              <p className="qp" style={{ fontSize: 12.5 }}>
                Kept in the macOS Keychain and sent only to {host(baseUrl)}.
              </p>
            </>
          ) : hasKey === false ? (
            <>
              <div className="qfld">
                <input
                  type="password"
                  placeholder={provider.id === "custom" ? "Paste your key" : `Paste your ${name} key`}
                  aria-label={provider.id === "custom" ? "API key" : `${name} API key`}
                  autoComplete="off"
                  spellCheck={false}
                  value={keyDraft}
                  onChange={(e) => setKeyDraft(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && keyDraft.trim() && saveKey()}
                />
                <button className="qbtn" disabled={!keyDraft.trim()} onClick={saveKey}>
                  Save
                </button>
              </div>
              {settings.cleanupEnabled && (
                <div className="qtags">
                  <span className="qtag pe">Needs a key</span>
                  <span className="qnote">Until then, words go in as heard.</span>
                </div>
              )}
              <p className="qp" style={{ fontSize: 12.5 }}>
                Kept in the macOS Keychain and sent only to {host(baseUrl)}. Once saved, it can’t be shown again.
              </p>
            </>
          ) : null}
        </div>
      )}

      {inUse && (
        <div className={`qcard ${ready ? "" : "dim2"}`}>
          <div className="qlab">Model</div>
          {ready ? (
            <>
              <div className="qfld mono">
                <input
                  list="cleanup-models"
                  aria-label="Model"
                  placeholder={provider.defaultModel || "Type a name, or list them"}
                  value={modelDraft}
                  spellCheck={false}
                  onChange={(e) => {
                    setModelDraft(e.target.value);
                    // Picking from the list saves straight away.
                    if (models.includes(e.target.value)) saveModel(e.target.value);
                  }}
                  onBlur={(e) => saveModel(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && saveModel(modelDraft)}
                />
                <datalist id="cleanup-models">
                  {models.map((m) => (
                    <option key={m} value={m} />
                  ))}
                </datalist>
                <button className="qbtn sm ghost" disabled={busy !== null} onClick={() => refreshModels(true)}>
                  {busy === "list" && <span className="qspin" />}List models
                </button>
              </div>
              <p className="qp" style={{ fontSize: 12.5 }}>
                {provider.defaultModel
                  ? `Leave it empty to use ${provider.defaultModel}. A small, fast model is best: you wait for it.`
                  : "A small, fast model is best: cleanup is light editing, and you wait for it."}
              </p>
              <div className="qrowb">
                <button className={`qbtn ${gap ? "" : "pri"}`} disabled={!model || busy !== null} onClick={test}>
                  {busy === "test" && <span className="qspin" />}
                  {failedTest ? "Test again" : "Test"}
                </button>
              </div>
              {result && (
                <Notice tone={result.tone}>
                  <b>{result.title}</b> {result.body && <span>{result.body}</span>}
                </Notice>
              )}
            </>
          ) : (
            <div className="qmono" style={{ color: "var(--light)" }}>
              Add a key first
            </div>
          )}
        </div>
      )}

      {rewriteOn && inUse ? (
        <RewriteStyleCard />
      ) : !rewriteOn ? (
        <Notice
          tone="nt"
          action={
            <button className="qbtn sm" onClick={() => app.setTab("hotkey")}>
              Set a rewrite hotkey
            </button>
          }
        >
          <b>Rewrite is off.</b> <span>Set a rewrite hotkey to rewrite in a style or edit a selection.</span>
        </Notice>
      ) : null}
    </>
  );
}

/** How the second hotkey rewords a dictation: a ready-made style, or your own. */
function RewriteStyleCard() {
  const { settings, update, provider, hasKey } = useApp();
  const [styles, setStyles] = useState<RewriteStyle[]>([]);
  const [draft, setDraft] = useState(settings.rewritePrompt);
  const [result, setResult] = useState<Result>(null);
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    rewriteStyles().then(setStyles, () => undefined);
  }, []);

  const preset = styles.find((s) => s.id === settings.rewriteStyle);
  const isCustom = settings.rewriteStyle === "custom";
  const canTest = !!provider && (!provider.needsKey || hasKey === true) && !!(settings.cleanupModel || provider.defaultModel);

  // Start your own style from whatever was showing, so there is something to edit.
  const startCustom = async () => {
    const seed = settings.rewritePrompt || preset?.instruction || "";
    setDraft(seed);
    setResult(null);
    await update({ rewriteStyle: "custom", rewritePrompt: seed }).catch(() => undefined);
  };

  const saveDraft = async () => {
    if (draft.trim() !== settings.rewritePrompt) await update({ rewritePrompt: draft.trim() });
  };

  const test = async () => {
    setTesting(true);
    setResult(null);
    try {
      await saveDraft();
      setResult({ tone: "nt", title: "Sample", body: await testRewrite() });
    } catch (err) {
      setResult({ tone: "pe", title: "The test failed.", body: String(err) });
    }
    setTesting(false);
  };

  return (
    <div className="qcard">
      <div className="qlab">
        Rewrite style<span className="r">Hold {hotkeyLabel(settings.rewriteHotkey)}</span>
      </div>
      <div className="qchips" role="radiogroup" aria-label="Rewrite style">
        {styles.map((s) => (
          <button
            key={s.id}
            className="qch"
            role="radio"
            aria-pressed={s.id === settings.rewriteStyle}
            aria-checked={s.id === settings.rewriteStyle}
            onClick={() => {
              setResult(null);
              update({ rewriteStyle: s.id }).catch(() => undefined);
            }}
          >
            {s.label}
          </button>
        ))}
        <button className="qch" role="radio" aria-pressed={isCustom} aria-checked={isCustom} onClick={startCustom}>
          Your own
        </button>
      </div>
      {isCustom ? (
        <>
          <label>
            <span className="sr">Your own style</span>
            <textarea
              className="qarea"
              rows={3}
              value={draft}
              placeholder="Write like me: short, warm, British spelling."
              onChange={(e) => setDraft(e.target.value)}
              onBlur={() => saveDraft().catch(() => undefined)}
            />
          </label>
          <p className="qp" style={{ fontSize: 12.5 }}>
            Tell it how you write. This goes with every rewrite, and is followed as written, so it can translate or change the tone too.
          </p>
        </>
      ) : (
        preset && <p className="qquote">{preset.instruction}</p>
      )}
      <div className="qrowb">
        <button className="qbtn" disabled={!canTest || testing} onClick={test}>
          {testing && <span className="qspin" />}Test on a sample
        </button>
        {!canTest && <span className="qnote" style={{ marginLeft: 6, fontSize: 12.5 }}>Needs the provider and model above.</span>}
      </div>
      {result && (
        <Notice tone={result.tone}>
          <b>{result.title}</b>
          <br />
          <span>{result.body}</span>
        </Notice>
      )}
    </div>
  );
}
