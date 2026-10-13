import { useEffect, useState } from "react";
import { rewriteStyles, testRewrite, type RewriteStyle, type Settings } from "../api";
import { HotkeyField } from "./HotkeySection";
import "./rewrite.css";

type Props = {
  settings: Settings;
  update: (change: Partial<Settings>) => Promise<void>;
};

/**
 * The second hotkey. Tidying keeps your wording; this one rewrites what you
 * said in a style you pick or write yourself.
 */
export default function RewriteSection({ settings, update }: Props) {
  const [styles, setStyles] = useState<RewriteStyle[]>([]);
  const [draft, setDraft] = useState(settings.rewritePrompt);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    rewriteStyles().then(setStyles);
  }, []);

  const preset = styles.find((s) => s.id === settings.rewriteStyle);
  const isCustom = settings.rewriteStyle === "custom";
  const enabled = settings.rewriteHotkey !== "";

  // Start a custom style from whatever was showing, so there is something to edit.
  const startCustom = async () => {
    const seed = settings.rewritePrompt || preset?.instruction || "";
    setDraft(seed);
    await update({ rewriteStyle: "custom", rewritePrompt: seed });
  };

  const saveDraft = async () => {
    if (draft.trim() !== settings.rewritePrompt) await update({ rewritePrompt: draft.trim() });
  };

  const test = async () => {
    setMessage("Rewriting a sample…");
    try {
      await saveDraft();
      setMessage(`Sample came back as: “${await testRewrite()}”`);
    } catch (err) {
      setMessage(String(err));
    }
  };

  return (
    <section className="card">
      <h2>Rewrite</h2>
      <p className="muted">
        Hold this hotkey instead and Utter rewrites what you said so it reads better, rather
        than only tidying it. It uses the provider and model set under Cleanup.
      </p>

      <HotkeyField
        hotkey={settings.rewriteHotkey}
        onChange={(rewriteHotkey) => update({ rewriteHotkey })}
      />
      {enabled && (
        <p className="muted small">
          <a
            href="#"
            onClick={(e) => {
              e.preventDefault();
              update({ rewriteHotkey: "" });
            }}
          >
            Turn the rewrite hotkey off
          </a>
        </p>
      )}

      <h3>Style</h3>
      <div className="chips">
        {styles.map((s) => (
          <button
            key={s.id}
            className={s.id === settings.rewriteStyle ? "chip chip-on" : "chip"}
            aria-pressed={s.id === settings.rewriteStyle}
            onClick={() => update({ rewriteStyle: s.id })}
          >
            {s.label}
          </button>
        ))}
        <button
          className={isCustom ? "chip chip-on" : "chip"}
          aria-pressed={isCustom}
          onClick={startCustom}
        >
          Your own
        </button>
      </div>

      {isCustom ? (
        <textarea
          rows={4}
          value={draft}
          placeholder="Describe how you want it rewritten."
          onChange={(e) => setDraft(e.target.value)}
          onBlur={saveDraft}
        />
      ) : (
        <p className="instruction">{preset?.instruction}</p>
      )}
      <p className="muted small">
        Whatever the style, Utter keeps every point you made and adds nothing you did not say.
        “Your own” starts from the style you had selected, for you to edit.
      </p>

      <div className="row">
        <span className="muted small">Try the style on a sample sentence.</span>
        <button onClick={test}>Test</button>
      </div>
      {message && <p className="small">{message}</p>}
    </section>
  );
}
