import { useEffect, useState } from "react";
import { rewriteStyles, testRewrite, type AppRule, type RewriteStyle } from "../api";
import { hotkeyLabel } from "../hotkey";
import { cleanupGap } from "../readiness";
import { shortName, useApp } from "../state";
import { IconX, Notice, Segmented, Switch, type Tone } from "../ui";

type Result = { tone: Tone; title: string; body: string } | null;

/**
 * Cleanup: whether dictations are tidied, how, and the style each app gets,
 * plus the style the rewrite hotkey writes in. The language model both use
 * is set up under AI provider.
 */
export default function CleanupSection() {
  const app = useApp();
  const { settings, update, provider } = app;
  const rewriteOn = settings.rewriteHotkey !== "";
  const gap = cleanupGap(app);
  const model = settings.cleanupModel || provider?.defaultModel || "";
  const providerLink = (
    <button className="qlink" onClick={() => app.setTab("provider")}>
      AI provider
    </button>
  );

  return (
    <>
      <div className="qhead">
        <div className="t">
          <h1>Cleanup</h1>
          <p>Drops filler words, fixes punctuation and writes for the app you’re in.</p>
        </div>
        <Switch label="Cleanup" checked={settings.cleanupEnabled} onChange={(cleanupEnabled) => update({ cleanupEnabled }).catch(() => undefined)} />
      </div>

      {settings.cleanupEnabled && gap && (
        <Notice
          tone="pe"
          action={
            <button className="qbtn sm" onClick={() => app.setTab("provider")}>
              Open AI provider
            </button>
          }
        >
          <b>Cleanup needs {gap}.</b> <span>Until then, words go in as heard.</span>
        </Notice>
      )}

      {settings.cleanupEnabled && (
        <div className="qcard">
          <div className="qsw" style={{ padding: 0 }}>
            <div className="tx">
              <b>How to tidy</b>
              <span>
                {settings.way === "quick"
                  ? "Quick tidies as you talk, so only the last bit waits. A pause mid-sentence can end a sentence early."
                  : "Accurate tidies the whole dictation once you let go, so pauses never split a sentence. A little slower on long dictations."}
              </span>
            </div>
            <WaySwitch />
          </div>
        </div>
      )}

      <div className="qcard">
        <div className="qlab">Styles by app</div>
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
        <p className="qp">
          Utter picks chat, email, code, notes or assistant style from the app you’re in, and names it under each
          dictation on Home.
          {!settings.cleanupEnabled && " With cleanup off, it types exactly what it heard."}
        </p>
      </div>

      <AppRulesCard />

      {rewriteOn ? (
        <RewriteStyleCard />
      ) : (
        <Notice
          tone="nt"
          action={
            <button className="qbtn sm" onClick={() => app.setTab("hotkeys")}>
              Set a rewrite hotkey
            </button>
          }
        >
          <b>Rewrite is off.</b> <span>Set a rewrite hotkey to rewrite in a style, or rewrite a selection.</span>
        </Notice>
      )}

      {(settings.cleanupEnabled || rewriteOn) && provider && (
        <p className="qp" style={{ margin: "0 4px" }}>
          {gap
            ? `The language model still needs ${gap}. Set it up in`
            : `Uses ${model} from ${shortName(provider.label)}. Change it in`}{" "}
          {providerLink}.
        </p>
      )}
    </>
  );
}

/** Fixed changes Utter makes to everything written into one app, with or without cleanup. */
function AppRulesCard() {
  const { settings, update, history } = useApp();
  const rules = settings.appRules;
  const save = (appRules: AppRule[]) => update({ appRules }).catch(() => undefined);
  const change = (app: string, patch: Partial<AppRule>) => save(rules.map((r) => (r.app === app ? { ...r, ...patch } : r)));
  const taken = new Set(rules.map((r) => r.app.toLowerCase()));
  // The apps dictated into lately, newest first, that have no rule yet.
  const recent = [...new Set(history.map((d) => d.app.name))]
    .filter((name) => name && name !== "Unknown app" && !taken.has(name.toLowerCase()))
    .slice(0, 8);

  return (
    <div className="qcard">
      <div className="qlab">Rules by app</div>
      {rules.length > 0 && (
        <div className="qlist" aria-label="Rules by app">
          {rules.map((rule) => (
            <div className="qli w" key={rule.app}>
              <span className="wd">{rule.app}</span>
              <span className="sp" />
              <label className="qnote" style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
                Small letters
                <Switch label={`Small letters in ${rule.app}`} checked={rule.lowerCase} onChange={(lowerCase) => change(rule.app, { lowerCase })} />
              </label>
              <label className="qnote" style={{ display: "inline-flex", alignItems: "center", gap: 6, marginLeft: 12 }}>
                No full stop
                <Switch label={`No full stop in ${rule.app}`} checked={rule.noFullStop} onChange={(noFullStop) => change(rule.app, { noFullStop })} />
              </label>
              <button className="qx" aria-label={`Remove the rule for ${rule.app}`} title="Remove" onClick={() => save(rules.filter((r) => r !== rule))}>
                <IconX />
              </button>
            </div>
          ))}
        </div>
      )}
      <p className="qp">
        {rules.length === 0
          ? "Leave off the full stop, or start sentences with a small letter, in the apps where you type that way. These apply with cleanup off too."
          : "Small letters keeps the capitals on “I”, acronyms and your words."}
      </p>
      {recent.length > 0 ? (
        <div style={{ display: "flex", flexWrap: "wrap", gap: 6, marginTop: 8 }}>
          {recent.map((name) => (
            <button key={name} className="qbtn sm ghost" onClick={() => save([...rules, { app: name, lowerCase: false, noFullStop: true }])}>
              + {name}
            </button>
          ))}
        </div>
      ) : (
        rules.length === 0 && <p className="qp" style={{ fontSize: 12.5 }}>Dictate into an app once and it shows up here to add.</p>
      )}
    </div>
  );
}

/** How the second hotkey writes: a ready-made style, or your own. */
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
        Rewrite style
        <span className="r">Hold {hotkeyLabel(settings.rewriteHotkey)}, or select text first</span>
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
        preset && (
          <>
            <p className="qquote">{preset.instruction}</p>
            <p className="qp" style={{ fontSize: 12.5 }}>
              Say what you want written, and how: “email Sam that I can’t make Thursday, keep it short”. What you say beats this style.
            </p>
          </>
        )
      )}
      <div className="qrowb">
        <button className="qbtn" disabled={!canTest || testing} onClick={test}>
          {testing && <span className="qspin" />}Test on a sample
        </button>
        {!canTest && <span className="qnote" style={{ marginLeft: 6, fontSize: 12.5 }}>Needs a provider and model, set in AI provider.</span>}
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

/** Accurate or quick tidying, here and beside the timings on Home. */
export function WaySwitch() {
  const { settings, update } = useApp();
  return (
    <Segmented
      label="How to tidy"
      value={settings.way}
      onChange={(way) => update({ way }).catch(() => undefined)}
      options={[
        { id: "accurate", label: "Accurate" },
        { id: "quick", label: "Quick" },
      ]}
    />
  );
}
