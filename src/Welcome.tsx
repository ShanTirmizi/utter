import { useEffect, useState } from "react";
import { cleanupGap, selectedModel } from "./readiness";
import { ModelRows } from "./sections/ModelSection";
import { PermissionCards } from "./sections/PermissionsSection";
import { PracticeBox } from "./sections/TrySection";
import { shortName, useApp } from "./state";
import { IconCursor, IconLaptop, IconNoAccount, InlineKeys, Keycaps, Mark, Notice, seconds, SmallMark } from "./ui";

const STEPS = 4;

/**
 * First run, as an introduction rather than a form: hello, the two
 * permissions, the speech model, then a first dictation. Only the microphone
 * can hold things up; everything else can be skipped.
 */
export default function Welcome() {
  const app = useApp();
  const { settings, permissions, progress, live, dictation, fresh } = app;
  const [step, setStep] = useState(0);

  const model = selectedModel(app);
  const modelName = model ? shortName(model.label) : "speech";
  const micReady = permissions?.microphone === "granted";
  const downloading = progress && model && progress.name === model.name;
  const percent = downloading
    ? Math.round((progress.received / (progress.total || model.sizeMb * 1_000_000)) * 100)
    : 0;
  // The first dictation made on this screen.
  const worked = step === 3 && fresh && live.phase !== "recording" && live.phase !== "processing" && !!dictation;
  const pasted = !!dictation?.inserted;

  const start = () => {
    // Fetch the selected model in the background; it is usually there by step 3.
    if (model && !model.downloaded && !progress) app.download(model.name);
    setStep(1);
  };
  const [finishError, setFinishError] = useState<string | null>(null);
  const finish = (openCleanup = false) => {
    if (openCleanup) app.setTab("cleanup");
    app.update({ onboarded: true }).catch((err) => setFinishError(String(err)));
  };

  // Keys light up on the practice step while the hotkey is held.
  const holding = live.phase === "recording";
  const [modifiers, setModifiers] = useState<string[]>([]);
  useEffect(() => {
    const read = (e: KeyboardEvent) =>
      setModifiers(
        [e.ctrlKey && "Control", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(
          Boolean,
        ) as string[],
      );
    // Switching apps with a key held never sends its key-up here.
    const clear = () => setModifiers([]);
    window.addEventListener("keydown", read);
    window.addEventListener("keyup", read);
    window.addEventListener("blur", clear);
    return () => {
      window.removeEventListener("keydown", read);
      window.removeEventListener("keyup", read);
      window.removeEventListener("blur", clear);
    };
  }, []);

  const ticker = downloading && (
    <span className="ticker">
      <span className="bar">
        <b style={{ width: `${percent}%` }} />
      </span>
      {modelName} {percent}%
    </span>
  );
  const steps = (
    <span className="steps" role="img" aria-label={`Step ${step + 1} of ${STEPS}`}>
      {Array.from({ length: STEPS }, (_, i) => (
        <i key={i} className={i < step ? "done" : i === step ? "now" : ""} />
      ))}
    </span>
  );

  return (
    <div className="win">
      <div className="tb" data-tauri-drag-region />

      {step === 0 && (
        <>
          <div className="wbody">
            <div className="hello">
              <Mark />
              <h1 className="wh">Talk, and Utter types it.</h1>
              <p className="wp">Hold a key, speak, let go. Your words appear wherever you were typing.</p>
              <ul className="facts">
                <li>
                  <IconLaptop />
                  <span>Speech becomes text on this Mac. Audio never leaves it.</span>
                </li>
                <li>
                  <IconCursor />
                  <span>Works in any app with a text cursor.</span>
                </li>
                <li>
                  <IconNoAccount />
                  <span>No account, no analytics, no telemetry. Free and open source.</span>
                </li>
              </ul>
            </div>
          </div>
          <div className="wfoot">
            {steps}
            {model && !model.downloaded && (
              <span className="hint">Also starts the {model.sizeMb} MB speech model download.</span>
            )}
            <span className="spacer" />
            <button className="btn primary lg" autoFocus onClick={start}>
              Get started
            </button>
          </div>
        </>
      )}

      {step === 1 && (
        <>
          <div className="wbody">
            <div className="whead">
              <span className="kicker">Step 2 of 4</span>
              <h1 className="wh2">Two permissions</h1>
              <p className="wp">
                macOS asks before any app can hear you or type for you. These update by themselves.
              </p>
            </div>
            <PermissionCards firstRun />
          </div>
          <div className="wfoot">
            {steps}
            {ticker}
            <span className="spacer" />
            {!micReady && <span className="hint">Allow the microphone first.</span>}
            <button className="btn primary" disabled={!micReady} onClick={() => setStep(2)}>
              Continue
            </button>
          </div>
        </>
      )}

      {step === 2 && (
        <>
          <div className="wbody">
            <div className="whead">
              <span className="kicker">Step 3 of 4</span>
              <h1 className="wh2">Speech model</h1>
              <p className="wp">
                Turns your voice into text, here on this Mac.{" "}
                {downloading
                  ? `${modelName} started downloading when you began. `
                  : model?.downloaded
                    ? `${modelName} is ready. `
                    : ""}
                You can switch any time.
              </p>
            </div>
            <ModelRows suggest />
          </div>
          <div className="wfoot">
            {steps}
            <span className="spacer" />
            {downloading && <span className="hint">You can go on while it finishes.</span>}
            {model && !model.downloaded && !progress && <span className="hint">Dictation needs one model.</span>}
            <button className="btn primary" onClick={() => setStep(3)}>
              Continue
            </button>
          </div>
        </>
      )}

      {step === 3 && (
        <>
          <div className="wbody">
            <div className="whead">
              <span className="kicker">Step 4 of 4</span>
              <h1 className="wh2">{!worked ? "Try it" : pasted ? "It works." : "It heard you."}</h1>
              <p className="wp">
                {worked && dictation
                  ? pasted
                    ? dictation.totalMs
                      ? `The text arrived ${seconds(dictation.totalMs, 1)} after you let go.`
                      : "Your words arrived."
                    : "Utter could not paste for you, so the text is on your clipboard. Press ⌘V in the box to paste it."
                  : holding
                    ? "Keep holding while you talk. Let go when you’re done."
                    : "Click the box, hold the keys, say a sentence, then let go."}
              </p>
            </div>
            {!worked && (
              <div style={{ padding: "6px 0 4px" }}>
                <Keycaps
                  hotkey={settings.hotkey}
                  down={new Set(holding ? settings.hotkey.split("+") : modifiers)}
                  wideHint="hold"
                />
              </div>
            )}
            {/* One box throughout, so the text that was just pasted stays put. */}
            <PracticeBox
              key="practice"
              autoFocus
              placeholder="Your words will appear here."
              readyText={`Ready. The ${modelName} model is loaded.`}
            />
            {!worked && (
              <p className="hint">
                {holding
                  ? "The same waveform shows in the pill at the bottom of your screen."
                  : live.phase === "error" && live.message
                    ? live.message
                    : "You can change the keys later, in Hotkey."}
              </p>
            )}
            {worked && (!settings.cleanupEnabled || cleanupGap(app)) && (
              <div className="card wcard">
                <div>
                  <span className="flabel">Want it tidied too?</span>
                  <p className="hint">
                    Cleanup removes filler words, fixes punctuation and matches the app you’re in. It uses a
                    model you choose: your own API key, or one running on this Mac.
                  </p>
                </div>
                <button className="btn" onClick={() => finish(true)}>
                  Set up cleanup
                </button>
              </div>
            )}
            {worked && (
              <Notice tone="info" icon={<SmallMark />}>
                Utter now lives in the menu bar. Hold <InlineKeys hotkey={settings.hotkey} /> in any app.
              </Notice>
            )}
          </div>
          <div className="wfoot">
            {steps}
            {ticker}
            <span className="spacer" />
            {finishError && (
              <span className="hint" style={{ color: "var(--bad)" }}>
                Could not save: {finishError}
              </span>
            )}
            {worked ? (
              <button className="btn primary lg" onClick={() => finish()}>
                Finish
              </button>
            ) : (
              <button className="btn quiet" onClick={() => finish()}>
                Skip for now
              </button>
            )}
          </div>
        </>
      )}
    </div>
  );
}
