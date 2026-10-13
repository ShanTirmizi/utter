import { useEffect, useState } from "react";
import {
  openPrivacySettings,
  permissionStatus,
  requestAccessibility,
  requestMicrophone,
  type PermissionStatus,
} from "./api";

/** Poll the system once a second, since permissions change outside the app. */
export function usePermissions(): PermissionStatus | null {
  const [status, setStatus] = useState<PermissionStatus | null>(null);
  useEffect(() => {
    let alive = true;
    const check = () => permissionStatus().then((s) => alive && setStatus(s));
    check();
    const timer = setInterval(check, 1000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);
  return status;
}

/** The three permissions people ask about, what each is for, and its state. */
export default function PermissionList({ status }: { status: PermissionStatus | null }) {
  const mic = status?.microphone ?? "undetermined";
  const accessibility = status?.accessibility ?? false;

  return (
    <ul className="permissions">
      <li>
        <div>
          <strong>Microphone</strong>
          <p className="muted small">
            To hear you while the hotkey is held. Audio is transcribed on this Mac and is
            not kept.
          </p>
        </div>
        {mic === "granted" ? (
          <span className="ok">Allowed</span>
        ) : mic === "denied" ? (
          <button onClick={() => openPrivacySettings("microphone")}>Open Settings</button>
        ) : (
          <button className="primary" onClick={() => requestMicrophone()}>
            Allow
          </button>
        )}
      </li>
      <li>
        <div>
          <strong>Accessibility</strong>
          <p className="muted small">
            To press ⌘V for you in the app you are typing in. Without it, Utter copies the
            text to your clipboard and you paste it yourself.
          </p>
        </div>
        {accessibility ? (
          <span className="ok">Allowed</span>
        ) : (
          <button
            className="primary"
            onClick={async () => {
              await requestAccessibility();
              await openPrivacySettings("accessibility");
            }}
          >
            Open Settings
          </button>
        )}
      </li>
      <li>
        <div>
          <strong>Input Monitoring</strong>
          <p className="muted small">
            Not needed. Utter registers one hotkey with macOS and never sees your other
            keystrokes.
          </p>
        </div>
        <span className="muted">Not required</span>
      </li>
    </ul>
  );
}
