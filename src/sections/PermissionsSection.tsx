import { useState } from "react";
import {
  openPrivacySettings,
  requestAccessibility,
  requestMicrophone,
  resetAccessibility,
} from "../api";
import { useApp } from "../state";
import { IconAccessibility, IconCheck, IconCross, IconKeyboard, IconMic } from "../ui";

export default function PermissionsSection() {
  return (
    <>
      <div className="sechead">
        <h2 className="h">Permissions</h2>
        <span className="hint">Checked live</span>
      </div>
      <PermissionCards />
      <p className="foot">Status updates by itself while this window is open.</p>
    </>
  );
}

const Allowed = () => (
  <span className="chip ok">
    <IconCheck width={2.4} />
    Allowed
  </span>
);

/**
 * The three permissions people ask about, what each is for, and its state.
 * Shared by the welcome screen, where nothing has been granted yet, and the
 * settings window.
 */
export function PermissionCards({ firstRun = false }: { firstRun?: boolean }) {
  const { permissions } = useApp();
  const mic = permissions?.microphone ?? "undetermined";
  const accessibility = permissions?.accessibility ?? false;
  // Once System Settings has been opened, the card says it is waiting.
  const [askedAccessibility, setAskedAccessibility] = useState(false);
  const [resetError, setResetError] = useState<string | null>(null);

  const openAccessibility = async () => {
    setAskedAccessibility(true);
    await requestAccessibility();
    await openPrivacySettings("accessibility");
  };

  return (
    <div className="perms">
      {mic === "granted" ? (
        <div className="perm">
          <span className="pic">
            <IconMic />
          </span>
          <div className="pbody">
            <div className="ptitle">
              Microphone<span className="need-tag">required</span>
              <Allowed />
            </div>
            <p>Hears you while the hotkey is held, and only then.</p>
          </div>
        </div>
      ) : mic === "denied" ? (
        <div className="perm" style={{ boxShadow: "0 0 0 1.5px var(--bad)" }}>
          <span className="pic" style={{ background: "var(--bad-soft)", color: "var(--bad)" }}>
            <IconMic off />
          </span>
          <div className="pbody">
            <div className="ptitle">
              Microphone<span className="need-tag">required</span>
              <span className="chip bad">
                <IconCross />
                Not allowed
              </span>
            </div>
            <p>
              Utter can’t hear you, so dictation can’t start. Once refused, macOS only lets you change this in
              System Settings.
            </p>
            <div className="acts">
              <button className="btn primary" onClick={() => openPrivacySettings("microphone")}>
                Open System Settings
              </button>
              <span className="hint">Privacy &amp; Security › Microphone</span>
            </div>
          </div>
        </div>
      ) : (
        <div className="perm need">
          <span className="pic">
            <IconMic />
          </span>
          <div className="pbody">
            <div className="ptitle">
              Microphone<span className="need-tag">required</span>
              <span className="chip plain">Not allowed yet</span>
            </div>
            <p>Hears you while the hotkey is held, and only then.</p>
            <div className="acts">
              <button className="btn primary" onClick={() => requestMicrophone()}>
                Allow microphone
              </button>
            </div>
          </div>
        </div>
      )}

      {accessibility ? (
        <div className="perm">
          <span className="pic">
            <IconAccessibility />
          </span>
          <div className="pbody">
            <div className="ptitle">
              Accessibility<span className="need-tag">recommended</span>
              <Allowed />
            </div>
            <p>Pastes the text for you, right where your cursor is.</p>
          </div>
        </div>
      ) : (
        // Only one card asks for attention at a time: the microphone comes first.
        <div className={`perm ${mic === "granted" ? "need" : ""}`}>
          <span className="pic">
            <IconAccessibility />
          </span>
          <div className="pbody">
            <div className="ptitle">
              Accessibility<span className="need-tag">recommended</span>
              {askedAccessibility ? (
                <span className="chip accent">
                  <span className="spin" />
                  Waiting for you
                </span>
              ) : (
                <span className={`chip ${firstRun ? "plain" : "warn"}`}>Off</span>
              )}
            </div>
            <p>
              {askedAccessibility
                ? "In System Settings, open Privacy & Security › Accessibility and turn on Utter. This card changes when you do."
                : firstRun
                  ? "Pastes the text for you. Without it, Utter copies the text and you press ⌘V."
                  : "Utter can’t paste for you, so it copies the text and you press ⌘V."}
            </p>
            <div className="acts">
              <button className="btn" onClick={openAccessibility}>
                Open System Settings
              </button>
              {firstRun && askedAccessibility && <span className="hint">Optional. Skip it and you paste with ⌘V.</span>}
            </div>
            {(!firstRun || askedAccessibility) && (
              <>
                <p style={{ marginTop: 10 }}>
                  Already on in System Settings?{" "}
                  <button
                    className="link"
                    onClick={async () => {
                      try {
                        await resetAccessibility();
                        setResetError(null);
                      } catch (err) {
                        setResetError(String(err));
                      }
                      // Open the pane either way; the switch can be set by hand.
                      await openPrivacySettings("accessibility").catch(() => undefined);
                    }}
                  >
                    Reset it
                  </button>
                </p>
                <p className="hint">
                  macOS may be matching the switch to an older build of Utter. Reset removes that entry; then
                  turn Utter on again.
                </p>
                {resetError && (
                  <p className="hint" style={{ color: "var(--bad)" }}>
                    The reset did not work ({resetError}). In System Settings, select Utter in the list, remove
                    it with the minus button, then add it again.
                  </p>
                )}
              </>
            )}
          </div>
        </div>
      )}

      <div className="perm muted">
        <span className="pic">
          <IconKeyboard />
        </span>
        <div className="pbody">
          <div className="ptitle">
            Input Monitoring<span className="chip plain">Not needed</span>
          </div>
          <p>Utter doesn’t use it, so there’s nothing to look for or turn on.</p>
        </div>
      </div>
    </div>
  );
}
