import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { openUrl } from "@tauri-apps/plugin-opener";
import { deleteApiKey, hasApiKey, listCleanupModels, setApiKey, testCleanup, type Provider } from "../api";
import { ProviderMark } from "../providerMarks";
import { cleanupGap, lastCleanupFailed } from "../readiness";
import { shortName, useApp } from "../state";
import { IconDown, IconOut, IconTick, Notice, seconds, type Tone } from "../ui";

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
 * The language model that cleanup and rewrite share: which service, its key,
 * and which model. The key field is one-way; a stored key is never shown,
 * only removed.
 */
export default function ProviderSection() {
  const app = useApp();
  const { settings, update, provider, hasKey, setHasKey, setCleanupFault } = app;
  const [keyDraft, setKeyDraft] = useState("");
  const [modelDraft, setModelDraft] = useState(settings.cleanupModel);
  const [urlDraft, setUrlDraft] = useState(settings.baseUrl);
  const [models, setModels] = useState<string[]>([]);
  const [result, setResult] = useState<Result>(null);
  const [busy, setBusy] = useState<"test" | "list" | null>(null);
  // Goes up each time "List models" finds some, which opens the list.
  const [listedAt, setListedAt] = useState(0);

  const rewriteOn = settings.rewriteHotkey !== "";
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
      if (announce && found.length) setListedAt((n) => n + 1);
      if (announce) {
        if (!found.length) setResult({ tone: "nt", title: "No models found.", body: "Type the name of the model you want." });
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
          <h1>AI provider</h1>
          <p>The language model that cleans up and rewrites your words.</p>
        </div>
      </div>

      <div className="qtags" style={{ margin: "0 0 14px 2px" }}>
        <span className="qtag sm nt">Cleanup {settings.cleanupEnabled ? "on" : "off"}</span>
        <span className="qtag sm nt">Rewrite {rewriteOn ? "on" : "off"}</span>
        {!inUse && <span className="qnote">Nothing uses it yet. Turn on cleanup, or set a rewrite hotkey.</span>}
      </div>

      {settings.cleanupEnabled && lastCleanupFailed(app) && !result && (
        <Notice tone="pe">
          <b>The last cleanup failed.</b>{" "}
          <span>{(app.dictation?.note ?? "").replace(/^.*skipped: /, "")} Test below to check the setup.</span>
        </Notice>
      )}

      <div className="qcard">
        <div className="qlab">Provider</div>
        <div className="qprov" role="radiogroup" aria-label="Provider">
          {app.catalog.map((p) => (
            <button
              key={p.id}
              className="qpv"
              role="radio"
              aria-checked={p.id === provider.id}
              onClick={() => p.id !== provider.id && update({ provider: p.id, cleanupModel: "" }).catch(() => undefined)}
            >
              <ProviderMark id={p.id} />
              <b>{shortName(p.label)}</b>
              <span>{providerNote(p)}</span>
            </button>
          ))}
        </div>
        {provider.id === "custom" && (
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

      {provider.needsKey && (
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

      <div className={`qcard ${ready ? "" : "dim2"}`}>
        <div className="qlab">Model</div>
        {ready ? (
          <>
            <ModelField
              value={modelDraft}
              onChange={setModelDraft}
              models={models}
              current={settings.cleanupModel}
              placeholder={provider.defaultModel || "Type a name, or list them"}
              onSave={saveModel}
              listing={busy === "list"}
              disabled={busy !== null}
              onList={() => refreshModels(true)}
              openAsked={listedAt}
            />
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
    </>
  );
}

/**
 * The model name: type one, or pick from what the provider offers. The list
 * opens under the field in the app's own style, filters as you type, and
 * works from the keyboard.
 */
function ModelField({
  value,
  onChange,
  models,
  current,
  placeholder,
  onSave,
  listing,
  disabled,
  onList,
  openAsked,
}: {
  value: string;
  onChange: (next: string) => void;
  models: string[];
  current: string;
  placeholder: string;
  onSave: (value: string) => void;
  listing: boolean;
  disabled: boolean;
  onList: () => void;
  openAsked: number;
}) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const input = useRef<HTMLInputElement>(null);
  const field = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState<React.CSSProperties | null>(null);

  // Typing filters the list; the saved name, untouched, shows everything.
  const needle = value.trim().toLowerCase();
  const shown = !needle || value.trim() === current ? models : models.filter((m) => m.toLowerCase().includes(needle));

  // "List models" found some: show them.
  useEffect(() => {
    if (openAsked) {
      setOpen(true);
      setActive(-1);
      input.current?.focus();
    }
  }, [openAsked]);

  // The card clips what overflows it, so the list floats over the page, just
  // under the field, or just above it when there is more room there.
  const showing = open && shown.length > 0;
  useLayoutEffect(() => {
    if (!showing) return setPlace(null);
    const measure = () => {
      const box = field.current?.getBoundingClientRect();
      if (!box) return;
      const gap = 6;
      const edge = 12;
      const below = window.innerHeight - box.bottom - gap - edge;
      const above = box.top - gap - edge;
      const want = Math.min(240, shown.length * 30 + 10);
      const up = below < want && above > below;
      const maxHeight = Math.max(80, Math.min(240, up ? above : below));
      setPlace(
        up
          ? { left: box.left, width: box.width, bottom: window.innerHeight - box.top + gap, maxHeight }
          : { left: box.left, width: box.width, top: box.bottom + gap, maxHeight },
      );
    };
    measure();
    window.addEventListener("resize", measure);
    // Scrolling the page moves the field; scrolling the list itself does not.
    const onScroll = (e: Event) => e.target !== list.current && measure();
    window.addEventListener("scroll", onScroll, true);
    return () => {
      window.removeEventListener("resize", measure);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [showing, shown.length]);

  // Keep the highlighted option in view.
  useEffect(() => {
    list.current?.querySelector<HTMLElement>(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const pick = (name: string) => {
    onChange(name);
    onSave(name);
    setOpen(false);
    setActive(-1);
  };

  const onKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.nativeEvent.isComposing) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!shown.length) return;
      setOpen(true);
      const step = e.key === "ArrowDown" ? 1 : -1;
      setActive((i) => (i + step + shown.length) % shown.length);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (open && active >= 0 && shown[active]) pick(shown[active]);
      else {
        onSave(value);
        setOpen(false);
      }
    } else if (e.key === "Escape" && open) {
      e.preventDefault();
      setOpen(false);
    }
  };

  return (
    <div>
      <div className="qfld mono" ref={field}>
        <input
          ref={input}
          role="combobox"
          aria-label="Model"
          aria-expanded={open && shown.length > 0}
          aria-controls="model-list"
          aria-autocomplete="list"
          aria-activedescendant={open && active >= 0 ? `model-${active}` : undefined}
          placeholder={placeholder}
          value={value}
          spellCheck={false}
          autoComplete="off"
          onChange={(e) => {
            onChange(e.target.value);
            setOpen(models.length > 0);
            setActive(-1);
          }}
          onFocus={() => models.length > 0 && setOpen(true)}
          onClick={() => models.length > 0 && setOpen(true)}
          onBlur={() => {
            setOpen(false);
            onSave(value);
          }}
          onKeyDown={onKey}
        />
        {models.length > 0 && (
          <button
            className="qx"
            tabIndex={-1}
            aria-label="Show models"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => {
              setOpen(!open);
              input.current?.focus();
            }}
          >
            <IconDown />
          </button>
        )}
        <button className="qbtn sm ghost" disabled={disabled} onMouseDown={(e) => e.preventDefault()} onClick={onList}>
          {listing && <span className="qspin" />}List models
        </button>
      </div>
      {showing &&
        place &&
        createPortal(
        <div className="qpop float" role="listbox" id="model-list" aria-label="Models" ref={list} style={place}>
          {shown.map((m, i) => (
            <button
              key={m}
              id={`model-${i}`}
              data-i={i}
              className={`qopt mono ${i === active ? "on" : ""}`}
              role="option"
              aria-selected={m === current}
              tabIndex={-1}
              // Keep focus in the field, so its blur does not close the list first.
              onMouseDown={(e) => e.preventDefault()}
              onMouseEnter={() => setActive(i)}
              onClick={() => pick(m)}
            >
              <span className="ck">{m === current && <IconTick />}</span>
              {m}
            </button>
          ))}
        </div>,
          document.body,
        )}
    </div>
  );
}
