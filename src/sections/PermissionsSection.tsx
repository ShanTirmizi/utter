import { useState } from "react";
import { openPrivacySettings, requestAccessibility, requestMicrophone, resetAccessibility } from "../api";
import { useApp } from "../state";
import { IconAccess, IconMic } from "../ui";

/** Ask for Accessibility and open the pane where it is switched on. */
export async function askAccessibility() {
  await requestAccessibility().catch(() => undefined);
  await openPrivacySettings("accessibility").catch(() => undefined);
}

export default function PermissionsSection() {
  const { permissions } = useApp();
  const mic = permissions?.microphone ?? "undetermined";
  const accessibility = permissions?.accessibility ?? false;
  // Once System Settings has been opened, the card says it is waiting.
  const [asked, setAsked] = useState(false);
  const [resetError, setResetError] = useState<string | null>(null);
  const waiting = asked && !accessibility;

  const reset = async () => {
    try {
      await resetAccessibility();
      setResetError(null);
    } catch (err) {
      setResetError(String(err));
    }
    // Open the pane either way; the switch can be set by hand.
    await openPrivacySettings("accessibility").catch(() => undefined);
  };

  return (
    <>
      <div className="qhead">
        <div className="t">
          <h1>Permissions</h1>
          <p>macOS asks once. Checked live, so these update by themselves.</p>
        </div>
      </div>

      <div className={`qcard ${mic === "granted" ? "" : "hl"}`}>
        <div className="qlab">
          <span style={{ color: "var(--mid)", display: "inline-flex" }}>
            <IconMic />
          </span>
          Microphone<span className="r">Required</span>
        </div>
        <div className={`qbig md ${mic === "denied" ? "bad" : ""}`}>
          {mic === "granted" ? "Allowed" : mic === "denied" ? "Not allowed" : "Not allowed yet"}
        </div>
        <p className="qp">
          {mic === "granted"
            ? "Hears you only while you hold the hotkey or go hands-free."
            : mic === "denied"
              ? "It was turned off. Switch on Utter in System Settings, then come back."
              : "Utter can’t hear you until you allow it. macOS asks once."}
        </p>
        {mic === "undetermined" && (
          <div className="qrowb">
            <button className="qbtn pri" onClick={() => requestMicrophone()}>
              Allow microphone
            </button>
          </div>
        )}
        {mic === "denied" && (
          <div className="qrowb">
            <button className="qbtn pri" onClick={() => openPrivacySettings("microphone")}>
              Open System Settings
            </button>
            <span className="qnote" style={{ fontSize: 12.5, marginLeft: 4 }}>
              Privacy &amp; Security › Microphone
            </span>
          </div>
        )}
      </div>

      <div className="qcard">
        <div className="qlab">
          <span style={{ color: "var(--mid)", display: "inline-flex" }}>
            <IconAccess />
          </span>
          Accessibility<span className="r">Recommended</span>
        </div>
        {waiting ? (
          <div className="qbig md">
            <span className="qspin" style={{ width: 26, height: 26, borderWidth: 3, alignSelf: "center", marginRight: 6 }} />
            Waiting for you
          </div>
        ) : (
          <div className="qbig md">{accessibility ? "Allowed" : "Off"}</div>
        )}
        <p className="qp">
          {accessibility
            ? "Types for you. Without it, you press ⌘V."
            : waiting
              ? "Switch on Utter in System Settings. This page updates by itself."
              : "Types for you. Without it, Utter copies and you press ⌘V."}
        </p>
        {!accessibility && (
          <div className="qrowb">
            <button
              className={`qbtn ${mic === "granted" && !waiting ? "pri" : ""}`}
              onClick={() => {
                setAsked(true);
                askAccessibility();
              }}
            >
              Open System Settings
            </button>
          </div>
        )}
        {!accessibility && (
          <p className="qp" style={{ fontSize: 12.5 }}>
            Already on but not working?{" "}
            <button className="qlink" style={{ fontSize: 12.5 }} onClick={reset}>
              Reset it
            </button>
          </p>
        )}
        {resetError && (
          <p className="qp" style={{ fontSize: 12.5, color: "var(--pe-fg)" }}>
            The reset didn’t work ({resetError}). In System Settings, select Utter, remove it with the minus button, then add it again.
          </p>
        )}
      </div>

      <div className="qcard tight">
        <div className="qrow">
          <b style={{ font: "600 14px/1 var(--qf)" }}>Input Monitoring</b>
          <span className="sp" />
          <span className="qtag sm nt">Not needed</span>
        </div>
        <p className="qp" style={{ fontSize: 12.5 }}>
          Utter doesn’t use it. If it shows up there, leave it off.
        </p>
      </div>
    </>
  );
}
