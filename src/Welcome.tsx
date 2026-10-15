import { useEffect, useState } from "react";
import { openPrivacySettings, requestMicrophone } from "./api";
import { hotkeyLabel } from "./hotkey";
import { cleanupGap, selectedModel } from "./readiness";
import { askAccessibility } from "./sections/PermissionsSection";
import { PracticeBox } from "./sections/TrySection";
import { shortName, useApp } from "./state";
import {
  Bars,
  IconCursor,
  IconLock,
  IconNoAccount,
  Keycaps,
  Logo,
  MenuGlyph,
  WaveProgress,
} from "./ui";

const STEPS = 4;
const MB = 1_000_000;

/**
 * First run: a picture on the left, words and actions on the right. Hello,
 * the two permissions, the speech model, then a first dictation. Only the
 * microphone can hold things up; everything else can be skipped.
 */
export default function Welcome() {
  const app = useApp();
  const { settings, permissions, progress, live, dictation, fresh, models } = app;
  const [step, setStep] = useState(0);
  const [askedMic, setAskedMic] = useState(false);
  const [askedAx, setAskedAx] = useState(false);
  const [finishError, setFinishError] = useState<string | null>(null);

  const model = selectedModel(app);
  const modelName = model ? shortName(model.label) : "Speech model";
  const mic = permissions?.microphone ?? "undetermined";
  const micOn = mic === "granted";
  const axOn = permissions?.accessibility ?? false;
  const downloading = !!progress && progress.name === model?.name;
  const total = progress ? progress.total || (model?.sizeMb ?? 0) * MB : 0;
  const fraction = model?.downloaded ? 1 : downloading && total ? Math.min(1, progress.received / total) : 0;
  const percent = Math.round(fraction * 100);
  const holding = live.phase === "recording";
  const working = live.phase === "processing";
  // The first dictation made on this screen.
  const worked = step === 3 && fresh && !holding && !working && !!dictation;
  const pasted = !!dictation?.inserted;

  // Keep the chosen model coming: start it once nothing else is downloading.
  useEffect(() => {
    if (step >= 1 && model && !model.downloaded && !progress) app.download(model.name);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step, model?.name, model?.downloaded, progress]);

  // The microphone answer arrived.
  useEffect(() => {
    if (mic !== "undetermined") setAskedMic(false);
  }, [mic]);

  const pickModel = (name: string) => {
    if (name === settings.model) return;
    // One download at a time: the old one stops, the new one starts after it.
    if (progress && progress.name !== name) app.stopDownloading();
    app.update({ model: name }).catch(() => undefined);
  };

  const finish = (openCleanup = false) => {
    if (openCleanup) app.setTab("cleanup");
    app.update({ onboarded: true }).catch((err) => setFinishError(String(err)));
  };

  // Keys light up while the hotkey is held.
  const [modifiers, setModifiers] = useState<string[]>([]);
  useEffect(() => {
    const read = (e: KeyboardEvent) =>
      setModifiers([e.ctrlKey && "Control", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean) as string[]);
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

  const nextLabel = step === 0 ? "Get started" : step === 3 ? (worked ? "Finish" : "Skip for now") : "Continue";
  const primary = step === 0 || (step === 1 && micOn) || step === 2 || (step === 3 && worked);
  const next = () => (step < 3 ? setStep(step + 1) : finish());

  return (
    <div className="qwin">
      <div className="qdrag" data-tauri-drag-region />

      <div className="ob-vis" aria-hidden="true">
        {step === 0 && (
          <>
            <Logo size={104} />
            <Bars style={{ height: 30 }} />
          </>
        )}
        {step === 1 && !micOn && (
          <>
            <Bars size="xl" mode="off" />
            <span className="qtag nt">{askedMic ? "Waiting for your answer" : "Asleep until it can hear you"}</span>
          </>
        )}
        {step === 1 && micOn && !axOn && (
          <>
            <div className="ob-ax">
              <span className="h">Privacy &amp; Security › Accessibility</span>
              <div className="row">
                <span className="ic" />
                <span className="nm" style={{ width: 70 }} />
                <span style={{ flex: 1 }} />
                <span className="qtog" role="presentation" aria-checked="false" />
              </div>
              <div className="row me">
                <Logo size={24} />
                <span style={{ font: "600 13.5px/1 var(--qf)" }}>Utter</span>
                <span style={{ flex: 1 }} />
                <span className="qtog" role="presentation" aria-checked="false" />
              </div>
              <div className="row">
                <span className="ic" />
                <span className="nm" style={{ width: 52 }} />
                <span style={{ flex: 1 }} />
                <span className="qtog" role="presentation" aria-checked="true" />
              </div>
            </div>
            <span className="qnote">Switch on Utter here</span>
          </>
        )}
        {step === 1 && micOn && axOn && (
          <>
            <span className="ob-done">
              <svg viewBox="0 0 40 40" width="40" height="40" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round">
                <path d="M11 20.5l6 6 12-13" />
              </svg>
            </span>
            <span className="qtag mi">Both allowed</span>
          </>
        )}
        {step === 2 && (
          <div style={{ width: 250, display: "flex", flexDirection: "column", alignItems: "center", gap: 14 }}>
            <WaveProgress fraction={fraction} label={`${modelName} download`} bars={40} style={{ height: 110, margin: 0, gap: 3, width: 250 }} />
            <span style={{ font: "600 30px/1 var(--qfd)", letterSpacing: "-.03em", fontVariantNumeric: "tabular-nums" }}>
              {model?.downloaded ? `${modelName} is ready` : `${modelName} ${percent}%`}
            </span>
            <span className="qnote">
              {model?.downloaded
                ? `${model.sizeMb} MB, on this Mac`
                : `${Math.round((downloading ? progress.received : 0) / MB)} of ${model?.sizeMb ?? 0} MB`}
            </span>
          </div>
        )}
        {step === 3 && (
          <>
            <Bars mode={holding ? "live" : working ? "work" : "rest"} style={{ height: 30 }} />
            <Keycaps hotkey={settings.hotkey} down={new Set(holding ? settings.hotkey.split("+") : modifiers)} style={{ margin: 0, gap: 8 }} />
            <span className="qnote">{holding ? "Listening" : working ? "Typing" : "Hold the keys"}</span>
          </>
        )}
      </div>

      <div className="ob-body">
        <span className="ob-step">
          Step {step + 1} of {STEPS}
        </span>

        {step === 0 && (
          <>
            <h1 className="ob-h1" style={{ fontSize: 36, lineHeight: "42px" }}>
              Welcome to Utter
            </h1>
            <p className="ob-lead">Hold a key, say what you want to write, and let go. Your words appear wherever you’re typing.</p>
            <div style={{ display: "flex", flexDirection: "column", gap: 16, marginTop: 28 }}>
              <div className="ob-pt">
                <span className="qicc" style={{ width: 40, height: 40 }} aria-hidden="true">
                  <IconLock />
                </span>
                <span>
                  <b>Stays on your Mac</b>
                  <span className="qnote">Speech becomes text here. Audio never leaves.</span>
                </span>
              </div>
              <div className="ob-pt">
                <span className="qicc" style={{ width: 40, height: 40 }} aria-hidden="true">
                  <IconCursor />
                </span>
                <span>
                  <b>Works in any app</b>
                  <span className="qnote">Anywhere there’s a text cursor.</span>
                </span>
              </div>
              <div className="ob-pt">
                <span className="qicc" style={{ width: 40, height: 40 }} aria-hidden="true">
                  <IconNoAccount />
                </span>
                <span>
                  <b>No account</b>
                  <span className="qnote">No sign-up, no analytics, free and open source.</span>
                </span>
              </div>
            </div>
          </>
        )}

        {step === 1 && (
          <>
            <h1 className="ob-h1">{micOn ? (axOn ? "All set" : "Utter can hear you") : "Two permissions"}</h1>
            <p className="ob-lead">
              {micOn
                ? axOn
                  ? "Utter can hear you and type for you."
                  : "Now let it type for you. You can skip this one."
                : "macOS asks once for each. The microphone is the one Utter needs."}
            </p>
            <div className="ob-pm" style={{ display: "flex", flexDirection: "column", gap: 12, marginTop: 20 }}>
              <div className={`qcard tight ${micOn ? "" : "hl"}`}>
                <div className="qlab">
                  Microphone<span className="r">Required</span>
                </div>
                <div className={`qbig sm ${mic === "denied" ? "bad" : ""}`}>
                  {micOn ? "Allowed" : mic === "denied" ? "Not allowed" : askedMic ? "Asking…" : "Not allowed yet"}
                </div>
                {mic === "undetermined" && !askedMic && (
                  <div className="qrowb" style={{ marginTop: 12 }}>
                    <button
                      className="qbtn pri"
                      onClick={() => {
                        setAskedMic(true);
                        requestMicrophone().catch(() => setAskedMic(false));
                      }}
                    >
                      Allow microphone
                    </button>
                  </div>
                )}
                {mic === "denied" && (
                  <>
                    <p className="qp" style={{ fontSize: 12.5 }}>
                      It was turned off. Switch on Utter in System Settings, then come back.
                    </p>
                    <div className="qrowb" style={{ marginTop: 12 }}>
                      <button className="qbtn pri" onClick={() => openPrivacySettings("microphone")}>
                        Open System Settings
                      </button>
                    </div>
                  </>
                )}
              </div>
              <div className={`qcard tight ${micOn && !axOn ? "hl" : ""}`}>
                <div className="qlab">
                  Accessibility<span className="r">Recommended</span>
                </div>
                <div className="qrow">
                  <div className="qbig sm">{axOn ? "Allowed" : askedAx ? "Waiting for you" : "Off"}</div>
                  {askedAx && !axOn && <span className="qspin" aria-label="Waiting" style={{ margin: "6px 0 0 6px" }} />}
                </div>
                <p className="qp" style={{ fontSize: 12.5 }}>
                  Lets Utter type for you. Without it, Utter copies and you press ⌘V.
                </p>
                {!axOn && !askedAx && (
                  <div className="qrowb" style={{ marginTop: 12 }}>
                    <button
                      className={`qbtn ${micOn ? "pri" : ""}`}
                      onClick={() => {
                        setAskedAx(true);
                        askAccessibility();
                      }}
                    >
                      Open System Settings
                    </button>
                  </div>
                )}
              </div>
            </div>
          </>
        )}

        {step === 2 && (
          <>
            <h1 className="ob-h1">Pick a speech model</h1>
            <p className="ob-lead">It turns your voice into text on this Mac. You can change it later.</p>
            <div className="qcard tight" style={{ marginTop: 20, paddingTop: 6, paddingBottom: 6 }} role="radiogroup" aria-label="Speech model">
              {models.map((m) => {
                const chosen = settings.model === m.name;
                const busy = progress?.name === m.name;
                const pct = busy ? Math.round((progress.received / (progress.total || m.sizeMb * MB)) * 100) : 0;
                return (
                  <button
                    key={m.name}
                    className="qmrow"
                    role="radio"
                    aria-checked={chosen}
                    onClick={() => pickModel(m.name)}
                    style={{ appearance: "none", border: 0, background: "none", width: "100%", textAlign: "left", font: "inherit", color: "inherit" }}
                  >
                    <span className={`rd ${chosen ? "on" : ""}`} aria-hidden="true" />
                    <div className="tx">
                      <b>
                        {shortName(m.label)}
                        <small>{m.sizeMb} MB</small>
                      </b>
                      <span>
                        {m.note}
                        {/\(English\)/.test(m.label) ? " English only." : ""}
                      </span>
                    </div>
                    <div className="a">
                      {m.downloaded ? (
                        <span className="qtag sm mi">Ready</span>
                      ) : busy ? (
                        <span className="qtag sm nt">{pct}%</span>
                      ) : null}
                    </div>
                  </button>
                );
              })}
            </div>
            <p className="qp" style={{ margin: "12px 2px 0" }}>
              Speak something other than English? Pick Large. You can carry on while it downloads.
            </p>
          </>
        )}

        {step === 3 && (
          <>
            <h1 className="ob-h1">{!worked ? "Try it" : pasted ? "It works." : "It heard you."}</h1>
            <p className="ob-lead">
              {!worked
                ? `Click the box, hold ${hotkeyLabel(settings.hotkey)}, say a sentence, then let go.`
                : pasted
                  ? `Hold ${hotkeyLabel(settings.hotkey)} in any app, wherever you can type.`
                  : "It’s on your clipboard. Allow Accessibility later and Utter types it for you."}
            </p>
            <div style={{ marginTop: 18 }}>
              <PracticeBox autoFocus placeholder="Your words appear here." hints={false} />
              {!worked && (
                <div className="qhints">
                  <span>Later: double-tap for hands-free</span>
                  <span>Teach it names under Words</span>
                </div>
              )}
            </div>
            {worked && (!settings.cleanupEnabled || cleanupGap(app)) && (
              <div className="qcard ob-card" style={{ marginTop: 12 }}>
                <div className="tx">
                  <b>Want it tidied too?</b>
                  <span>Cleanup drops filler words and fixes punctuation.</span>
                </div>
                <button className="qbtn" onClick={() => finish(true)}>
                  Set up cleanup
                </button>
              </div>
            )}
            {worked && (
              <div className="qcard ob-card" style={{ marginTop: 10 }}>
                <div className="tx">
                  <b>Utter lives in the menu bar</b>
                  <span>Your last dictation, Fix Last Word and settings.</span>
                </div>
                <span className="mbi" aria-hidden="true">
                  <span>
                    <MenuGlyph />
                  </span>
                </span>
              </div>
            )}
          </>
        )}

        <div className="ob-foot">
          <span className="ob-dots" role="img" aria-label={`Step ${step + 1} of ${STEPS}`}>
            {Array.from({ length: STEPS }, (_, i) => (
              <i key={i} className={i < step ? "done" : i === step ? "now" : ""} />
            ))}
          </span>
          {step >= 1 && step !== 2 && model && (
            <span
              className="qnote dl"
              title={model.downloaded ? "Speech model ready" : `Speech model ${percent}%`}
              style={{ fontSize: 12.5, marginLeft: 6, fontVariantNumeric: "tabular-nums", whiteSpace: "nowrap" }}
            >
              {model.downloaded ? `${modelName} ready` : downloading ? `${modelName} ${percent}%` : ""}
            </span>
          )}
          <span style={{ flex: 1 }} />
          {finishError && (
            <span className="qnote" style={{ color: "var(--bad-fg)" }}>
              Couldn’t save: {finishError}
            </span>
          )}
          {step > 0 && (
            <button className="qbtn lg ghost" onClick={() => setStep(step - 1)}>
              Back
            </button>
          )}
          <button className={`qbtn lg ${primary ? "pri" : ""}`} disabled={step === 1 && !micOn} onClick={next} autoFocus={step === 0}>
            {nextLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
