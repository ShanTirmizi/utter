import { useEffect, useState } from "react";
import { fnKeyConflict, openPrivacySettings, type Mode } from "../api";
import { FN, fromKeyEvent, heldIds, hotkeyLabel } from "../hotkey";
import { useApp } from "../state";
import { IconInfoLine, IconOut, Keycaps, Notice, Switch } from "../ui";

/** The dictation hotkey new users start on, as in `settings.rs`. */
const DEFAULT_HOTKEY = FN;

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

/** Check a new hotkey and save it, reporting how that went. */
function useRecorder(which: Mode) {
  const app = useApp();
  const { settings } = app;
  const main = which === "tidy";
  const field = main ? "hotkey" : "rewriteHotkey";
  const other = main ? settings.rewriteHotkey : settings.hotkey;
  const capturing = app.capturing === which;
  const [outcome, setOutcome] = useState<Outcome>(null);

  const save = (next: string) => {
    const label = hotkeyLabel(next);
    const parts = next.split("+");
    if (next === other) {
      const use = main ? "rewriting" : "dictating";
      return setOutcome({ kind: "bad", attempt: next, message: `${label} is already your hotkey for ${use}.` });
    }
    if (parts.length === 2 && parts[0] === "Shift") {
      return setOutcome({ kind: "bad", attempt: next, message: `You could no longer type that character. Add ⌃, ⌥ or ⌘.` });
    }
    if (next === "Super+Space") {
      return setOutcome({ kind: "bad", attempt: next, message: "⌘Space opens Spotlight." });
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
  }, [capturing, app]); // eslint-disable-line react-hooks/exhaustive-deps

  const record = () => {
    setOutcome(null);
    app.setCapturing(which);
  };
  return { capturing, outcome, setOutcome, save, record, field };
}

export default function HotkeysSection() {
  const app = useApp();
  const { settings } = app;
  const held = useHeldKeys();
  const hk = hotkeyLabel(settings.hotkey);
  const rw = settings.rewriteHotkey ? hotkeyLabel(settings.rewriteHotkey) : null;
  return (
    <>
      <div className="qhead">
        <div className="t">
          <h1>Hotkeys</h1>
          <p>Hold to dictate. Let go to insert.</p>
        </div>
      </div>
      <MainHotkey held={held} />
      <RewriteHotkey held={held} />
      <div className="qcard tight">
        <div className="qsw">
          <div className="tx">
            <b>Hands-free</b>
            <span>
              Double-tap {hk} to start and tap once to finish. After 30 s of silence it stops and copies what it
              heard, so nothing is lost.
            </span>
          </div>
          <Switch label="Hands-free" checked={settings.handsFree} onChange={(handsFree) => app.update({ handsFree }).catch(() => undefined)} />
        </div>
      </div>
      <div className="qcard">
        <div className="qlab">How it works</div>
        <div className="qrules">
          <div>
            <b>Hold</b>
            <span>Listens while you hold {hk}.</span>
          </div>
          <div>
            <b>Let go</b>
            <span>Types where your cursor is.</span>
          </div>
          <div>
            <b>Double-tap</b>
            <span>{settings.handsFree ? "Hands-free. Tap to finish." : "Ignored while hands-free is off."}</span>
          </div>
          <div>
            <b>Quick tap</b>
            <span>Ignored, so nothing fires by accident.</span>
          </div>
          <div>
            <b>Rewrite</b>
            <span>{rw ? `Hold ${rw}, say what to write. It comes back finished.` : "Set a rewrite hotkey above."}</span>
          </div>
          <div>
            <b>Rewrite a selection</b>
            <span>{rw ? `Select text, hold ${rw}, say the change.` : "Needs the rewrite hotkey."}</span>
          </div>
        </div>
      </div>
    </>
  );
}

function MainHotkey({ held }: { held: Set<string> }) {
  const app = useApp();
  const { settings, live } = app;
  const { capturing, outcome, setOutcome, record } = useRecorder("tidy");
  const hotkey = settings.hotkey;
  const fn = hotkey === FN;
  const broken = !!app.hotkeyProblem && !capturing;

  // Keys light up while held. The system keeps the last key of a registered
  // hotkey to itself, so the pipeline starting to listen stands in for it.
  const firing = live.phase === "recording" && live.mode === "tidy";
  const down = new Set(firing ? hotkey.split("+") : held);
  const modifiersHeld = ["Control", "Alt", "Shift", "Super"].filter((m) => held.has(m));

  const label = capturing ? "Recording" : outcome?.kind === "bad" ? "Not saved" : "Dictate";
  const right = capturing ? "esc cancels" : outcome || broken ? "" : "Press it now to test it";

  const useFn = () => {
    setOutcome(null);
    // Choosing fn means wanting fn to dictate and do nothing else, so macOS's
    // own globe action is held off too. It comes back on quit.
    app
      .update({ hotkey: FN, quietGlobe: true })
      .then(() => setOutcome({ kind: "saved" }))
      .catch((err) => setOutcome({ kind: "bad", attempt: FN, message: String(err) }));
  };

  return (
    <>
      {broken && (
        <Notice
          tone="bad"
          action={
            <button className="qbtn sm" onClick={record}>
              Record new hotkey
            </button>
          }
        >
          <b>A hotkey isn’t working.</b> <span>{app.hotkeyProblem}</span>
        </Notice>
      )}
      <div className="qcard">
        <div className="qlab">
          {label}
          {right && <span className="r">{right}</span>}
        </div>
        {capturing ? (
          <Keycaps hotkey={modifiersHeld.join("+")} down={new Set(modifiersHeld)} slot />
        ) : outcome?.kind === "bad" ? (
          <Keycaps hotkey={outcome.attempt} look="bad" />
        ) : (
          <Keycaps key={hotkey} hotkey={hotkey} down={down} look={outcome?.kind === "saved" ? "pop" : undefined} />
        )}

        {outcome?.kind === "bad" ? (
          <div className="qtags">
            <span className="qtag pe">{outcome.message.replace(/\.$/, "")}</span>
            <span className="qnote">Pick another combination.</span>
          </div>
        ) : outcome?.kind === "saved" ? (
          <div className="qtags">
            <span className="qtag mi">Saved</span>
            <span className="qnote">Hold {hotkeyLabel(hotkey)} in any app.</span>
          </div>
        ) : broken ? (
          <div className="qtags">
            <span className="qtag bad">Not working</span>
            <span className="qnote">Another app may have it.</span>
          </div>
        ) : null}

        {capturing && (
          <p className="qp">
            {modifiersHeld.length > 0
              ? `Holding ${hotkeyLabel(modifiersHeld.join("+"))}. Add one more key. esc cancels.`
              : "Hold at least one of ⌃ ⌥ ⌘, then one other key. esc cancels."}
          </p>
        )}

        {fn && !capturing && outcome?.kind !== "bad" && <FnKeyAdvice />}

        <div className="qrowb">
          {capturing ? (
            <>
              <button className="qbtn" onClick={() => app.setCapturing(null)}>
                Cancel
              </button>
              <span className="qnote" style={{ marginLeft: 6 }}>
                Both hotkeys are paused while you record.
              </span>
            </>
          ) : outcome?.kind === "bad" ? (
            <>
              <button className="qbtn pri" onClick={record}>
                Try again
              </button>
              <button className="qbtn" onClick={() => setOutcome(null)}>
                Keep {hotkeyLabel(hotkey)}
              </button>
            </>
          ) : (
            <>
              {!broken && (
                <button className={`qbtn ${outcome || fn ? "" : "pri"}`} onClick={record}>
                  Record new hotkey
                </button>
              )}
              {hotkey !== DEFAULT_HOTKEY && (
                <button className="qbtn" title="Hold fn to dictate, double-tap it for hands-free. Also switches macOS's own 🌐 key action off while Utter is running." onClick={useFn}>
                  Reset to {hotkeyLabel(DEFAULT_HOTKEY)}
                </button>
              )}
            </>
          )}
        </div>
      </div>
    </>
  );
}

function RewriteHotkey({ held }: { held: Set<string> }) {
  const app = useApp();
  const { settings, live } = app;
  const { capturing, outcome, setOutcome, record } = useRecorder("rewrite");
  const hotkey = settings.rewriteHotkey;
  const otherRecording = app.capturing === "tidy";
  const firing = live.phase === "recording" && live.mode === "rewrite";
  const down = new Set(firing ? hotkey.split("+") : capturing ? held : []);
  const modifiersHeld = ["Control", "Alt", "Shift", "Super"].filter((m) => held.has(m));

  return (
    <div className={`qcard ${otherRecording ? "dim2" : ""}`}>
      <div className="qlab">
        Rewrite
        <span className="r">{otherRecording ? "Paused" : capturing ? "esc cancels" : hotkey ? "Hold" : ""}</span>
      </div>
      <div className="qrow" style={{ marginTop: 12 }}>
        {capturing ? (
          <Keycaps small hotkey={modifiersHeld.join("+")} down={new Set(modifiersHeld)} slot style={{ marginTop: 0 }} />
        ) : outcome?.kind === "bad" ? (
          <Keycaps small hotkey={outcome.attempt} look="bad" style={{ marginTop: 0 }} />
        ) : hotkey ? (
          <Keycaps small key={hotkey} hotkey={hotkey} down={down} look={outcome?.kind === "saved" ? "pop" : undefined} style={{ marginTop: 0 }} />
        ) : (
          <span className="qkey s off" style={{ width: 120 }}>
            Off
          </span>
        )}
        <span className="sp" />
        {capturing ? (
          <button className="qbtn" onClick={() => app.setCapturing(null)}>
            Cancel
          </button>
        ) : outcome?.kind === "bad" ? (
          <>
            <button className="qbtn" onClick={record}>
              Try again
            </button>
            <button className="qbtn ghost" onClick={() => setOutcome(null)}>
              {hotkey ? `Keep ${hotkeyLabel(hotkey)}` : "Leave it off"}
            </button>
          </>
        ) : hotkey ? (
          <>
            <button className="qbtn" disabled={otherRecording} onClick={record}>
              Record
            </button>
            <button
              className="qbtn ghost"
              disabled={otherRecording}
              onClick={() => {
                setOutcome(null);
                app.update({ rewriteHotkey: "" }).catch(() => undefined);
              }}
            >
              Turn off
            </button>
          </>
        ) : (
          <button className="qbtn" disabled={otherRecording} onClick={record}>
            Set a rewrite hotkey
          </button>
        )}
      </div>
      {outcome?.kind === "bad" ? (
        <div className="qtags">
          <span className="qtag pe">{outcome.message.replace(/\.$/, "")}</span>
          <span className="qnote">Pick another combination.</span>
        </div>
      ) : outcome?.kind === "saved" && hotkey ? (
        <div className="qtags">
          <span className="qtag mi">Saved</span>
          <span className="qnote">Hold {hotkeyLabel(hotkey)} to rewrite.</span>
        </div>
      ) : null}
      <p className="qp">
        {capturing
          ? "Hold at least one of ⌃ ⌥ ⌘, then one other key."
          : hotkey
            ? "Hold to dictate in your rewrite style. Select text first to rewrite that instead: hold it and say how to change it, like “make it shorter”."
            : "Rewriting, and rewriting a selection, both need this hotkey."}
      </p>
    </div>
  );
}

/**
 * With fn as the hotkey, macOS's own action for the globe key (emoji
 * picker, input source, Apple's dictation) fires too. Utter can hold that
 * setting off while it uses fn and give it back afterwards.
 */
function FnKeyAdvice() {
  const app = useApp();
  const quiet = app.settings.quietGlobe;
  const [action, setAction] = useState<string | null>(null);
  useEffect(() => {
    let current = true;
    const check = () => fnKeyConflict().then((found) => current && setAction(found), () => undefined);
    check();
    // The user may change it in System Settings while this is open.
    window.addEventListener("focus", check);
    return () => {
      current = false;
      window.removeEventListener("focus", check);
    };
  }, [quiet]);

  if (quiet) {
    return (
      <Notice
        tone="mi"
        icon={<IconInfoLine />}
        action={
          <button className="qbtn sm" onClick={() => app.update({ quietGlobe: false }).catch(() => undefined)}>
            Give it back now
          </button>
        }
      >
        The 🌐 key only dictates while Utter is running. Quit Utter and it goes back to what macOS does with it.
      </Notice>
    );
  }
  if (!action) return null;
  return (
    <Notice tone="pe">
      Pressing fn also makes macOS {action}.
      <div className="qrow" style={{ marginTop: 10, flexWrap: "wrap" }}>
        <button className="qbtn sm" onClick={() => app.update({ quietGlobe: true }).catch(() => undefined)}>
          Turn it off while Utter runs
        </button>
        <button className="qlink" style={{ fontSize: 12.5 }} onClick={() => openPrivacySettings("keyboard")}>
          Or change it in Keyboard settings
          <IconOut />
        </button>
      </div>
    </Notice>
  );
}
