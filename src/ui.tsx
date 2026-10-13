// Small building blocks shared by every screen: icons, notices, switches,
// keycaps and the sound-wave mark. Their looks are in styles/utter.css.
import logo from "./assets/logo.png";
import type { ReactNode } from "react";
import { keyParts } from "./hotkey";

type IconProps = { size?: 16 | 20; width?: number };

function Icon({ size = 20, width = 1.6, children }: IconProps & { children: ReactNode }) {
  return (
    <svg
      viewBox={`0 0 ${size} ${size}`}
      fill="none"
      stroke="currentColor"
      strokeWidth={width}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export const IconCheck = ({ width = 2.2 }: IconProps) => (
  <Icon width={width}>
    <path d="M4.5 10.5l3.5 3.5 7.5-8" />
  </Icon>
);
export const IconCross = () => (
  <Icon width={2.4}>
    <path d="M5.5 5.5l9 9M14.5 5.5l-9 9" />
  </Icon>
);
export const IconWarn = () => (
  <Icon width={1.7}>
    <path d="M10 3l7.5 13h-15z" />
    <path d="M10 8v3.5M10 14h.01" />
  </Icon>
);
export const IconAlert = () => (
  <Icon width={1.7}>
    <circle cx="10" cy="10" r="7.5" />
    <path d="M10 6v4.5M10 13.5h.01" />
  </Icon>
);
export const IconInfo = () => (
  <Icon width={1.7}>
    <circle cx="10" cy="10" r="7.5" />
    <path d="M10 9v4.5M10 6.5h.01" />
  </Icon>
);
export const IconMic = ({ off = false }: { off?: boolean }) => (
  <Icon>
    <rect x="7" y="2.5" width="6" height="10" rx="3" />
    <path d={`M4.5 9.5a5.5 5.5 0 0 0 11 0M10 15v2.5${off ? "M3 3l14 14" : ""}`} />
  </Icon>
);
export const IconAccessibility = () => (
  <Icon>
    <circle cx="10" cy="10" r="7.5" />
    <circle cx="10" cy="6.3" r="1" />
    <path d="M6.5 8.4l3.5.8 3.5-.8M10 9.2v2.5l-1.6 3M10 11.7l1.6 3" />
  </Icon>
);
export const IconKeyboard = () => (
  <Icon>
    <rect x="2" y="5" width="16" height="10.5" rx="2" />
    <path d="M5 8.5h.01M8 8.5h.01M11 8.5h.01M14 8.5h.01M6 12h8" />
  </Icon>
);
export const IconLock = () => (
  <Icon width={1.7}>
    <rect x="4.5" y="9" width="11" height="8" rx="2" />
    <path d="M7 9V6.5a3 3 0 0 1 6 0V9" />
  </Icon>
);
export const IconLaptop = () => (
  <Icon>
    <rect x="3.5" y="4" width="13" height="9" rx="1.5" />
    <path d="M1.5 16h17" />
  </Icon>
);
export const IconCursor = () => (
  <Icon>
    <path d="M7.5 3.5h5M7.5 16.5h5M10 3.5v13" />
  </Icon>
);
export const IconNoAccount = () => (
  <Icon>
    <circle cx="8" cy="7" r="3" />
    <path d="M2.5 16.5c.8-2.8 3-4.5 5.5-4.5 1.2 0 2.3.3 3.2 1M13.5 12.5l4 4M17.5 12.5l-4 4" />
  </Icon>
);

// Tab icons, drawn on a 16 px grid.
export const TabIconTry = () => (
  <Icon size={16}>
    <path d="M3 6.5v3M5.5 4.5v7M8 3v10M10.5 4.5v7M13 6.5v3" />
  </Icon>
);
export const TabIconHotkey = () => (
  <Icon size={16} width={1.5}>
    <rect x="1.5" y="3.5" width="13" height="9" rx="2" />
    <path d="M4.5 6.5h.01M7 6.5h.01M9.5 6.5h.01M12 6.5h.01M5 9.5h6" />
  </Icon>
);
export const TabIconModel = () => (
  <Icon size={16} width={1.5}>
    <rect x="4" y="4" width="8" height="8" rx="1.5" />
    <path d="M6.5 1.5v2M9.5 1.5v2M6.5 12.5v2M9.5 12.5v2M1.5 6.5h2M1.5 9.5h2M12.5 6.5h2M12.5 9.5h2" />
  </Icon>
);
export const TabIconCleanup = () => (
  <Icon size={16} width={1.5}>
    <path d="M2.5 4h11M2.5 7.5h7M2.5 11h4.5" />
    <path d="M9.5 11.5l1.8 1.8 3.2-3.6" />
  </Icon>
);
export const TabIconPermissions = () => (
  <Icon size={16} width={1.5}>
    <path d="M8 1.8l5 2v4c0 3.2-2.2 5.3-5 6.4-2.8-1.1-5-3.2-5-6.4v-4z" />
  </Icon>
);

type Tone = "ok" | "warn" | "bad" | "info" | "accent";

const TONE_ICON: Record<Tone, ReactNode> = {
  ok: <IconCheck />,
  warn: <IconWarn />,
  bad: <IconAlert />,
  info: <IconInfo />,
  accent: <IconInfo />,
};

/** A coloured box that says what happened and, optionally, offers the fix. */
export function Notice({
  tone,
  icon,
  action,
  children,
}: {
  tone: Tone;
  icon?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className={`note ${tone}`} role={tone === "bad" ? "alert" : "status"}>
      {icon ?? TONE_ICON[tone]}
      <span>{children}</span>
      {action && <span className="act">{action}</span>}
    </div>
  );
}

export function Switch({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
}) {
  return (
    <button
      className="switch"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
    />
  );
}

/** A small row of mutually exclusive choices. */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { id: T; label: string }[];
  onChange: (next: T) => void;
  label: string;
}) {
  return (
    <div className="seg" role="group" aria-label={label}>
      {options.map((o) => (
        <button key={o.id} aria-pressed={o.id === value} onClick={() => onChange(o.id)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** A hotkey drawn as inline keys inside a sentence. */
export function InlineKeys({ hotkey }: { hotkey: string }) {
  return (
    <>
      {keyParts(hotkey).map((k) => (
        <kbd className="k" key={k.id}>
          {k.cap}
        </kbd>
      ))}
    </>
  );
}

/**
 * A hotkey drawn as keycaps. Keys named in `down` are shown pressed; `look`
 * marks the whole row as rejected or freshly saved.
 */
export function Keycaps({
  hotkey,
  down,
  look,
  small = false,
  ghost = false,
  wideHint,
}: {
  hotkey: string;
  down?: ReadonlySet<string>;
  look?: "bad" | "pop";
  small?: boolean;
  /** Add a blinking empty key: one more key is still needed. */
  ghost?: boolean;
  /** Small print under the last key, such as "hold". */
  wideHint?: string;
}) {
  const parts = keyParts(hotkey);
  return (
    <div className="keys">
      {parts.map((k, i) => (
        <span key={k.id} style={{ display: "contents" }}>
          {i > 0 && <span className="plus">+</span>}
          <kbd
            className={[
              "key",
              small ? "sm" : "",
              k.wide ? "wide" : "",
              down?.has(k.id) ? "down" : "",
              look ?? "",
            ].join(" ")}
          >
            {k.cap}
            {k.name ? <small>{k.name}</small> : wideHint && i === parts.length - 1 ? <small>{wideHint}</small> : null}
          </kbd>
        </span>
      ))}
      {ghost && (
        <>
          {parts.length > 0 && <span className="plus">+</span>}
          <kbd className={`key ghost ${small ? "sm" : ""}`}>?</kbd>
        </>
      )}
    </div>
  );
}

/** The app's logo at welcome size. */
export function Mark() {
  return <img className="wmark" src={logo} alt="" width={96} height={96} />;
}

export function SmallMark() {
  return <img className="smark" src={logo} alt="" width={20} height={20} />;
}

/** Five flat dots: the mark at rest, for empty states. */
export function FlatMark() {
  return (
    <span className="flat" aria-hidden="true">
      <i />
      <i />
      <i />
      <i />
      <i />
    </span>
  );
}

const WAVE_BARS = 40;

/** Download progress as a row of wave bars with a bouncing leading edge. */
export function WaveProgress({
  fraction,
  failed = false,
  label,
}: {
  fraction: number;
  failed?: boolean;
  label: string;
}) {
  const filled = Math.round(WAVE_BARS * Math.min(1, Math.max(0, fraction)));
  return (
    <span
      className="dlw"
      role="progressbar"
      aria-label={label}
      aria-valuenow={Math.round(fraction * 100)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      {Array.from({ length: WAVE_BARS }, (_, i) => (
        <i key={i} className={i < filled - 1 ? (failed ? "x" : "f") : i === filled - 1 ? (failed ? "x" : "edge") : ""} />
      ))}
    </span>
  );
}

/** "0.38 s" */
export function seconds(ms: number, digits = 2): string {
  return `${(ms / 1000).toFixed(digits)} s`;
}

/** "0:04" */
export function clock(ms: number): string {
  const total = Math.floor(ms / 1000);
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}
