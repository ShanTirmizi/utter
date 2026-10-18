import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  loginItem,
  microphones,
  openPrivacySettings,
  requestAccessibility,
  requestMicrophone,
  resetAccessibility,
  setLoginItem,
  type LoginItem,
  type Microphones,
} from "../api";
import { useApp } from "../state";
import { IconAccess, IconDown, IconHotkey, IconMic, IconTick, Segmented, Switch } from "../ui";

/** Ask for Accessibility and open the pane where it is switched on. */
export async function askAccessibility() {
  await requestAccessibility().catch(() => undefined);
  await openPrivacySettings("accessibility").catch(() => undefined);
}

/** How Utter looks, starts and listens, and the macOS permissions it needs. */
export default function GeneralSection() {
  const app = useApp();
  const { settings, permissions } = app;
  // Without the microphone nothing works, so that comes first.
  const blocked = !!permissions && permissions.microphone !== "granted";
  return (
    <>
      <div className="qhead">
        <div className="t">
          <h1>General</h1>
          <p>How Utter looks, starts and listens, and what macOS lets it do.</p>
        </div>
      </div>

      {blocked && <Permissions />}

      <div className="qcard" style={{ overflow: "visible" }}>
        <div className="qsw" style={{ padding: "0 0 12px" }}>
          <div className="tx">
            <b>Appearance</b>
            <span>System follows your Mac. Light or Dark keeps Utter that way.</span>
          </div>
          <Segmented
            label="Appearance"
            value={settings.appearance}
            onChange={(appearance) => app.update({ appearance }).catch(() => undefined)}
            options={[
              { id: "system", label: "System" },
              { id: "light", label: "Light" },
              { id: "dark", label: "Dark" },
            ]}
          />
        </div>
        <OpenAtLogin />
        <MicPicker />
      </div>

      {!blocked && <Permissions />}
    </>
  );
}

/** Re-run `check` whenever the window comes back to the front. */
function useOnFocus(check: () => void) {
  useEffect(() => {
    check();
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}

/** Open at login, read from macOS each time so a change made there shows here. */
function OpenAtLogin() {
  const [state, setState] = useState<LoginItem | null>(null);
  const [error, setError] = useState<string | null>(null);
  useOnFocus(() => {
    loginItem().then(setState, () => setState("unavailable"));
  });

  const change = (on: boolean) =>
    setLoginItem(on).then(
      (now) => {
        setState(now);
        setError(null);
      },
      (err) => setError(String(err)),
    );

  const note =
    state === "unavailable"
      ? "Only the installed Utter app can open at login, on macOS 13 or later."
      : state === "needsApproval"
        ? "Allow Utter under System Settings › General › Login Items, and it opens from then on."
        : "Starts in the menu bar when you log in, so the hotkey always works.";
  return (
    <div className="qsw">
      <div className="tx">
        <b>Open at login</b>
        <span style={state === "needsApproval" || error ? { color: "var(--pe-fg)" } : undefined}>{error ?? note}</span>
      </div>
      <Switch
        label="Open at login"
        checked={state === "on" || state === "needsApproval"}
        disabled={state === null || state === "unavailable"}
        onChange={change}
      />
    </div>
  );
}

/** Which microphone to record from: the system's default, or a particular one. */
function MicPicker() {
  const { settings, update } = useApp();
  const [mics, setMics] = useState<Microphones | null>(null);
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const refresh = () => microphones().then(setMics, () => undefined);
  useOnFocus(refresh);

  // Clicking elsewhere or pressing Escape closes the list.
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => !box.current?.contains(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("mousedown", away);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("mousedown", away);
      window.removeEventListener("keydown", esc);
    };
  }, [open]);

  const current = settings.microphone;
  const picked = mics?.devices.find((m) => m.id === current);
  const missing = current !== "" && mics !== null && !picked;
  const label = current === "" ? "System default" : picked ? picked.name : "Not plugged in";

  const pick = (id: string) => {
    setOpen(false);
    update({ microphone: id }).catch(() => undefined);
  };
  const option = (id: string, name: string) => (
    <button
      key={id || "default"}
      className="qopt"
      role="option"
      aria-selected={current === id}
      // Device names can be long; let them wrap rather than spill.
      style={{ height: "auto", minHeight: 28, padding: "5px 9px", lineHeight: "18px" }}
      onClick={() => pick(id)}
    >
      <span className="ck">{current === id && <IconTick />}</span>
      {name}
    </button>
  );

  return (
    <div className="qsw" style={{ paddingBottom: 0, position: "relative" }} ref={box}>
      <div className="tx">
        <b>Microphone</b>
        <span style={missing ? { color: "var(--pe-fg)" } : undefined}>
          {missing
            ? "The one you picked isn’t plugged in, so Utter uses the system default until it’s back."
            : current === ""
              ? mics?.defaultName
                ? `Follows macOS, which is using ${mics.defaultName}.`
                : "Follows the microphone chosen in macOS."
              : "Utter records from this one, whatever macOS is set to."}
        </span>
      </div>
      <button
        className="qsel"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`Microphone: ${label}`}
        style={{ maxWidth: 230 }}
        onClick={() => {
          if (!open) refresh();
          setOpen(!open);
        }}
      >
        <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{label}</span>
        <IconDown />
      </button>
      {open && (
        <div className="qpop" role="listbox" aria-label="Microphone" style={{ top: "100%", marginTop: 6, width: 280 }}>
          {option("", "System default")}
          {(mics?.devices.length ?? 0) > 0 && <hr />}
          {mics?.devices.map((m) => option(m.id, m.name))}
          {missing && option(current, "Not plugged in")}
        </div>
      )}
    </div>
  );
}

/** One permission: a single row once allowed, the way to allow it while not. */
function Permission({
  icon,
  name,
  need,
  allowed,
  status,
  tone,
  highlight = false,
  children,
}: {
  icon: ReactNode;
  name: string;
  need: string;
  allowed: boolean;
  status: string;
  tone: "mi" | "pe" | "bad" | "nt";
  highlight?: boolean;
  children?: ReactNode;
}) {
  return (
    <div className={`qcard ${allowed || tone === "nt" ? "tight" : ""} ${highlight ? "hl" : ""}`}>
      <div className="qrow">
        <span className="qpi">{icon}</span>
        <b style={{ font: "600 14px/1 var(--qf)" }}>{name}</b>
        {need && (
          <span className="qnote" style={{ fontSize: 12.5 }}>
            {need}
          </span>
        )}
        <span className="sp" />
        <span className={`qtag sm ${tone}`}>
          {allowed && <IconTick />}
          {status}
        </span>
      </div>
      {children}
    </div>
  );
}

function Permissions() {
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
      <div className="qlab" style={{ margin: "6px 4px 10px" }}>
        Permissions<span className="r">Checked live</span>
      </div>

      <Permission
        icon={<IconMic />}
        name="Microphone"
        need="Required"
        allowed={mic === "granted"}
        status={mic === "granted" ? "Allowed" : mic === "denied" ? "Not allowed" : "Not allowed yet"}
        tone={mic === "granted" ? "mi" : "bad"}
        highlight={mic !== "granted"}
      >
        {mic !== "granted" && (
          <>
            <p className="qp">
              {mic === "denied"
                ? "It was turned off. Switch on Utter in System Settings, then come back."
                : "Utter can’t hear you until you allow it. macOS asks once."}
            </p>
            <div className="qrowb">
              {mic === "undetermined" ? (
                <button className="qbtn pri" onClick={() => requestMicrophone()}>
                  Allow microphone
                </button>
              ) : (
                <>
                  <button className="qbtn pri" onClick={() => openPrivacySettings("microphone")}>
                    Open System Settings
                  </button>
                  <span className="qnote" style={{ fontSize: 12.5, marginLeft: 4 }}>
                    Privacy &amp; Security › Microphone
                  </span>
                </>
              )}
            </div>
          </>
        )}
        {mic === "granted" && (
          <p className="qp" style={{ fontSize: 12.5 }}>
            Hears you only while you hold the hotkey or go hands-free.
          </p>
        )}
      </Permission>

      <Permission
        icon={<IconAccess />}
        name="Accessibility"
        need="Recommended"
        allowed={accessibility}
        status={accessibility ? "Allowed" : waiting ? "Waiting for you" : "Off"}
        tone={accessibility ? "mi" : "pe"}
      >
        <p className="qp" style={accessibility ? { fontSize: 12.5 } : undefined}>
          {accessibility
            ? "Types for you. Without it, you press ⌘V."
            : waiting
              ? "Switch on Utter in System Settings. This page updates by itself."
              : "Types for you. Without it, Utter copies and you press ⌘V."}
        </p>
        {!accessibility && (
          <>
            <div className="qrowb">
              <button
                className={`qbtn ${mic === "granted" && !waiting ? "pri" : ""}`}
                onClick={() => {
                  setAsked(true);
                  askAccessibility();
                }}
              >
                {waiting && <span className="qspin" />}
                Open System Settings
              </button>
            </div>
            <p className="qp" style={{ fontSize: 12.5 }}>
              Already on but not working?{" "}
              <button className="qlink" style={{ fontSize: 12.5 }} onClick={reset}>
                Reset it
              </button>
            </p>
            {resetError && (
              <p className="qp" style={{ fontSize: 12.5, color: "var(--pe-fg)" }}>
                The reset didn’t work ({resetError}). In System Settings, select Utter, remove it with the minus button, then
                add it again.
              </p>
            )}
          </>
        )}
      </Permission>

      <Permission icon={<IconHotkey />} name="Input Monitoring" need="" allowed={false} status="Not needed" tone="nt">
        <p className="qp" style={{ fontSize: 12.5 }}>
          Utter doesn’t use it. If it shows up there, leave it off.
        </p>
      </Permission>
    </>
  );
}
