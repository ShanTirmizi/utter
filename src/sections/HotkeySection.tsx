import { useEffect, useState } from "react";
import type { Mode } from "../api";
import { fromKeyEvent, heldIds, hotkeyLabel } from "../hotkey";
import { useApp } from "../state";
import { Keycaps, Notice, Switch } from "../ui";

const DEFAULT_HOTKEY: Record<Mode, string> = {
  tidy: "Control+Alt+Space",
  rewrite: "Control+Alt+KeyR",
};

type Outcome = { kind: "saved" } | { kind: "bad"; attempt: string; message: string } | null;

/** The keys held right now, by hotkey id ("Control", "Alt", "Space", "KeyR"). */
function useHeldKeys(): Set<string> {
  const [held, setHeld] = useState<Set<string>>(new Set());
  useEffect(() => {
    const onDown = (e: KeyboardEvent) => setHeld(new Set(heldIds(e, true)));
    const onUp = (e: KeyboardEvent) => setHeld(new Set(heldIds(e, false)));
    const clear = () => setHeld(new Set());
    window.addEventListener("keydown", onDown);
    window.addEventListener("keyup", onUp);
    window.addEventListener("blur", clear);
    return () => {
      window.removeEventListener("keydown", onDown);
      window.removeEventListener("keyup", onUp);
      window.removeEventListener("blur", clear);
    };
  }, []);
  return held;
}

export default function HotkeySection() {
  const app = useApp();
  const { settings } = app;
  const held = useHeldKeys();
  return (
    <>
      <div className="sechead">
        <h2 className="h">Hotkey</h2>
      </div>
      {app.hotkeyProblem && !app.capturing && (
        <Notice tone="bad">
          <b>A hotkey isn’t working.</b> {app.hotkeyProblem} Record a different combination below.
        </Notice>
      )}
      <HotkeyStage which="tidy" held={held} />
      <HotkeyStage which="rewrite" held={held} />
      <div className="card optrow">
        <span className="opttext">
          <b>Hands-free</b>
          <span className="hint">
            Tap a hotkey once to start, and again to finish. For when you have a lot to say. It stops by
            itself after 30 seconds of silence and copies what you said.
          </span>
        </span>
        <Switch
          label="Hands-free"
          checked={settings.handsFree}
          onChange={(handsFree) => {
            app.update({ handsFree }).catch(() => undefined);
          }}
        />
      </div>
      <ul className="rules">
        <li>
          <b>Hold</b>
          <span>Listening starts once every key is down.</span>
        </li>
        <li>
          <b>Let go</b>
          <span>Text goes in wherever your cursor is.</span>
        </li>
        <li>
          <b>Quick tap</b>
          <span>
            {settings.handsFree
              ? "Starts hands-free. Tap again to finish and insert."
              : "Ignored, with a “Nothing heard” note."}
          </span>
        </li>
        {settings.rewriteHotkey && (
          <li>
            <b>Rewrite</b>
            <span>Hold {hotkeyLabel(settings.rewriteHotkey)} instead and Utter rewords what you said.</span>
          </li>
        )}
        {settings.rewriteHotkey && (
          <li>
            <b>Edit</b>
            <span>
              Select some text first, hold {hotkeyLabel(settings.rewriteHotkey)} and say what to change. Utter
              replaces the selection.
            </span>
          </li>
        )}
      </ul>
    </>
  );
}

/**
 * One hotkey as keycaps, with a way to record a new one. `tidy` is the main
 * dictation hotkey; `rewrite` is the optional second one, drawn a size down.
 */
function HotkeyStage({ which, held }: { which: Mode; held: Set<string> }) {
  const app = useApp();
  const { settings, live } = app;
  const main = which === "tidy";
  const field = main ? "hotkey" : "rewriteHotkey";
  const hotkey = settings[field];
  const other = main ? settings.rewriteHotkey : settings.hotkey;
  const capturing = app.capturing === which;
  const [outcome, setOutcome] = useState<Outcome>(null);

  const save = (next: string) => {
    const label = hotkeyLabel(next);
    const parts = next.split("+");
    if (next === other) {
      const use = main ? "rewriting" : "dictating";
      return setOutcome({ kind: "bad", attempt: next, message: `It is already your hotkey for ${use}.` });
    }
    if (parts.length === 2 && parts[0] === "Shift") {
      return setOutcome({
        kind: "bad",
        attempt: next,
        message: `You could no longer type that character. Add ⌃, ⌥ or ⌘ to ${label}.`,
      });
    }
    app
      .update({ [field]: next })
      .then(() => setOutcome({ kind: "saved" }))
      .catch((err) => setOutcome({ kind: "bad", attempt: next, message: String(err) }));
  };

  // While recording, the next valid key combination becomes the hotkey.
  useEffect(() => {
    if (!capturing) return;
    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      if (e.code === "Escape") return app.setCapturing(null);
      const next = fromKeyEvent(e);
      if (!next) return;
      app.setCapturing(null);
      save(next);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
    // `app` is new on every render, so the listener always sees current settings.
  }, [capturing, app]);

  const record = () => {
    setOutcome(null);
    app.setCapturing(which);
  };

  // Keys light up while held. The system keeps the last key of a registered
  // hotkey to itself, so the pipeline starting to listen stands in for it.
  const firing = live.phase === "recording" && live.mode === which;
  const down = new Set(firing ? hotkey.split("+") : held);

  const modifiersHeld = ["Control", "Alt", "Shift", "Super"].filter((m) => held.has(m));
  const stageClass = ["card", "keystage", main ? "" : "compact", capturing ? "rec" : "", outcome?.kind === "bad" ? "bad" : ""];

  return (
    <>
      {main && (
        <p className="sub">
          {capturing || outcome?.kind === "bad"
            ? "Hold the keys you want, all together, then let go."
            : "Hold it to dictate. Let go to insert the text."}
        </p>
      )}

      <div className={stageClass.join(" ")}>
        {!main && <span className="stagelabel">Rewrite hotkey</span>}
        {capturing && <span className="recdot">Recording</span>}

        {capturing ? (
          <Keycaps hotkey={modifiersHeld.join("+")} down={new Set(modifiersHeld)} small={!main} ghost />
        ) : outcome?.kind === "bad" ? (
          <Keycaps hotkey={outcome.attempt} look="bad" small={!main} />
        ) : hotkey ? (
          <Keycaps key={hotkey} hotkey={hotkey} down={down} look={outcome?.kind === "saved" ? "pop" : undefined} small={!main} />
        ) : (
          <div className="keys">
            <kbd className="key sm wide off">Off</kbd>
          </div>
        )}

        <p className="hint">
          {capturing ? (
            <>
              {modifiersHeld.length > 0
                ? `Holding ${hotkeyLabel(modifiersHeld.join("+"))}. Add one more key. `
                : "Hold at least one of ⌃ ⌥ ⌘, then one other key. "}
              <kbd className="k">esc</kbd> cancels.
            </>
          ) : outcome?.kind === "bad" ? (
            "Not saved."
          ) : !hotkey ? (
            "Rewriting is off. Set a hotkey to turn it on."
          ) : main ? (
            "Press it now to check it. Keys light up while held."
          ) : (
            "Hold this one instead and Utter rewords what you said, in the style set under Cleanup. With text selected, it changes that text the way you ask."
          )}
        </p>

        {!main && !capturing && outcome?.kind !== "bad" && (
          <div className="stageacts">
            <button className="btn sm" onClick={record}>
              {hotkey ? "Record new hotkey" : "Set a rewrite hotkey"}
            </button>
            {hotkey && (
              <button
                className="btn sm quiet"
                onClick={() => {
                  setOutcome(null);
                  app.update({ rewriteHotkey: "" });
                }}
              >
                Turn off
              </button>
            )}
          </div>
        )}
        {!main && capturing && (
          <div className="stageacts">
            <button className="btn sm" onClick={() => app.setCapturing(null)}>
              Cancel
            </button>
          </div>
        )}
      </div>

      {outcome?.kind === "bad" && (
        <>
          <Notice tone="bad">
            <b>{hotkeyLabel(outcome.attempt)} can’t be used.</b> {outcome.message}
          </Notice>
          <div className="acts">
            <button className="btn primary" onClick={record}>
              Try again
            </button>
            <button className="btn" onClick={() => setOutcome(null)}>
              {hotkey ? `Keep ${hotkeyLabel(hotkey)}` : "Leave it off"}
            </button>
          </div>
        </>
      )}

      {outcome?.kind === "saved" && hotkey && (
        <Notice tone="ok">
          <b>Saved.</b> Hold {hotkeyLabel(hotkey)} to {main ? "dictate" : "rewrite"}, in any app.
        </Notice>
      )}

      {main && outcome?.kind !== "bad" && (
        <div className="acts">
          {capturing ? (
            <>
              <button className="btn" onClick={() => app.setCapturing(null)}>
                Cancel
              </button>
              <span className="hint">Both hotkeys are paused while you record.</span>
            </>
          ) : (
            <>
              <button className="btn" onClick={record}>
                Record new hotkey
              </button>
              {hotkey !== DEFAULT_HOTKEY.tidy && DEFAULT_HOTKEY.tidy !== other && (
                <button className="btn quiet" onClick={() => save(DEFAULT_HOTKEY.tidy)}>
                  Reset to {hotkeyLabel(DEFAULT_HOTKEY.tidy)}
                </button>
              )}
            </>
          )}
        </div>
      )}
    </>
  );
}
