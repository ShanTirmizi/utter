// Small building blocks shared by every screen: the logo, the voice bars,
// notices, switches, keycaps and icons. Their looks are in styles/quiet.css.
import type { ReactNode } from "react";
import { keyParts } from "./hotkey";

// ---------------------------------------------------------------- the mark

/** The logo: voice bars trailing into a text cursor, on a graphite squircle. */
export function Logo({ size = 46 }: { size?: number }) {
  return (
    <div className="qhex" style={{ width: size, height: size }} aria-hidden="true">
      <b>
        <i />
        <i />
        <i />
        <i />
        <i />
      </b>
    </div>
  );
}

/**
 * The five chrome bars. At rest they breathe; `live` follows a voice, `work`
 * runs a wave through them, `off` lies flat.
 */
export function Bars({
  mode = "rest",
  size,
  style,
}: {
  mode?: "rest" | "live" | "work" | "off";
  size?: "sm" | "xl";
  style?: React.CSSProperties;
}) {
  return (
    <span className={["qchr", size ?? "", mode === "rest" ? "" : mode].join(" ")} style={style} aria-hidden="true">
      <i />
      <i />
      <i />
      <i />
      <i />
    </span>
  );
}

/** Voice level bars that follow the microphone, oldest first. */
export function LevelBars({ levels, height = 13 }: { levels: number[]; height?: number }) {
  return (
    <span className="lw" aria-hidden="true">
      {levels.map((level, i) => (
        <i key={i} style={{ height: Math.round(3 + level * height) }} />
      ))}
    </span>
  );
}

// ---------------------------------------------------------------- icons

type SvgProps = { children: ReactNode; box?: number; size?: number; stroke?: number; fill?: boolean; className?: string };

function Svg({ children, box = 20, size, stroke = 1.6, fill = false, className }: SvgProps) {
  return (
    <svg
      viewBox={`0 0 ${box} ${box}`}
      width={size}
      height={size}
      className={className}
      fill={fill ? "currentColor" : "none"}
      stroke={fill ? undefined : "currentColor"}
      strokeWidth={fill ? undefined : stroke}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export const IconWords = () => (
  <Svg>
    <path d="M3 15.5L7 4.5l4 11M4.4 11.5h5.2" />
    <path d="M13 9.2c.5-.8 1.3-1.2 2.2-1.2 1.5 0 2.3.9 2.3 2.3v5.2M17.5 12.3c-2.8 0-4.6.7-4.6 2 0 .9.7 1.4 1.7 1.4 1.6 0 2.9-1.2 2.9-3.4" />
  </Svg>
);
export const IconHome = () => (
  <Svg>
    <path d="M3.5 9L10 3.5 16.5 9M5.5 7.5v8.5h9V7.5" />
    <path d="M8.5 16v-4h3v4" />
  </Svg>
);
export const IconHistory = () => (
  <Svg>
    <path d="M3.6 10a6.4 6.4 0 1 0 1.9-4.6M3.4 3.6v2.6H6" />
    <path d="M10 6.6V10l2.3 1.6" />
  </Svg>
);
export const IconSettings = () => (
  <Svg box={24} stroke={1.8}>
    <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
    <circle cx="12" cy="12" r="3" />
  </Svg>
);
export const IconChevron = () => (
  <svg className="chev" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M3 7.5L6 4.5l3 3" />
  </svg>
);
export const IconDown = () => (
  <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M3 4.5l3 3 3-3" />
  </svg>
);
// Shown for the settings pages only when the sidebar is folded to icons.
export const IconHotkey = () => (
  <Svg stroke={1.5}>
    <rect x="2.5" y="5" width="15" height="10.5" rx="2.2" />
    <path d="M5.5 8.5h.01M8.5 8.5h.01M11.5 8.5h.01M14.5 8.5h.01M6.5 12h7" />
  </Svg>
);
export const IconChip = () => (
  <Svg stroke={1.5}>
    <rect x="5" y="5" width="10" height="10" rx="2" />
    <path d="M8 2.5v2.5M12 2.5v2.5M8 15v2.5M12 15v2.5M2.5 8H5M2.5 12H5M15 8h2.5M15 12h2.5" />
  </Svg>
);
export const IconCleanup = () => (
  <Svg stroke={1.5}>
    <path d="M3 5h14M3 9.5h9M3 14h5.5" />
    <path d="M11.5 14.5l2 2 3.8-4.3" />
  </Svg>
);
export const IconShield = () => (
  <Svg stroke={1.5}>
    <path d="M10 2.5l6 2.4v4.8c0 3.8-2.6 6.3-6 7.6-3.4-1.3-6-3.8-6-7.6V4.9z" />
  </Svg>
);
export const IconPlug = () => (
  <Svg stroke={1.5}>
    <path d="M7.5 3v3.5M12.5 3v3.5M5 6.5h10v2.5a5 5 0 0 1-10 0z" />
    <path d="M10 14v3" />
  </Svg>
);
export const IconSliders = () => (
  <Svg stroke={1.5}>
    <path d="M3 6h7M14 6h3M3 14h3M10 14h7" />
    <circle cx="12" cy="6" r="2" />
    <circle cx="8" cy="14" r="2" />
  </Svg>
);
export const IconX = () => (
  <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true">
    <path d="M2 2l6 6M8 2L2 8" />
  </svg>
);
export const IconSearch = () => (
  <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true">
    <circle cx="6" cy="6" r="4.2" />
    <path d="M9.2 9.2l3 3" />
  </svg>
);
export const IconOut = () => (
  <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
    <path d="M3 2h5v5M8 2L2.5 7.5" />
  </svg>
);
export const IconTick = () => (
  <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M3 7.5l2.5 2.5L11 4" />
  </svg>
);
export const IconSparkle = ({ big = false }: { big?: boolean }) => (
  <svg viewBox="0 0 18 18" aria-hidden="true">
    <path d="M9 1.8l1.7 4.6 4.6 1.7-4.6 1.7L9 14.4l-1.7-4.6-4.6-1.7 4.6-1.7z" fill="currentColor" />
    {big && <path d="M14.5 12.2l.6 1.6 1.6.6-1.6.6-.6 1.6-.6-1.6-1.6-.6 1.6-.6z" fill="currentColor" />}
  </svg>
);
export const IconScreen = () => (
  <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
    <rect x="1.5" y="2.5" width="11" height="8" rx="1.5" />
    <path d="M5 12.5h4" strokeLinecap="round" />
  </svg>
);
export const IconMic = () => (
  <svg viewBox="0 0 22 22" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" aria-hidden="true">
    <path d="M11 3a3 3 0 0 1 3 3v4.5a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3z" />
    <path d="M5.5 10.5a5.5 5.5 0 0 0 11 0M11 16v3" />
  </svg>
);
export const IconAccess = () => (
  <svg viewBox="0 0 22 22" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <circle cx="11" cy="4.3" r="1.4" fill="currentColor" stroke="none" />
    <path d="M5 7.3l6 1.2 6-1.2M11 8.5v4l-2.6 5M11 12.5l2.6 5" />
  </svg>
);
export const IconLock = () => (
  <svg viewBox="0 0 22 22" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M7.5 10V7.5a3.5 3.5 0 0 1 7 0V10" />
    <rect x="5" y="10" width="12" height="8.5" rx="2" />
  </svg>
);
export const IconCursor = () => (
  <svg viewBox="0 0 22 22" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" aria-hidden="true">
    <path d="M8 4h6M8 18h6M11 4v14" />
  </svg>
);
export const IconNoAccount = () => (
  <svg viewBox="0 0 22 22" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true">
    <circle cx="11" cy="8" r="3" />
    <path d="M5 18c.9-3 3.2-4.6 6-4.6s5.1 1.6 6 4.6M4 4l14 14" />
  </svg>
);

/** The menu bar glyph, as drawn in the window. */
export const MenuGlyph = () => (
  <svg viewBox="0 0 100 100" width="16" height="16" fill="currentColor" aria-hidden="true">
    <rect x="13" y="40" width="10" height="20" rx="5" />
    <rect x="27" y="29" width="10" height="42" rx="5" />
    <rect x="41" y="36" width="10" height="28" rx="5" />
    <rect x="55" y="44" width="10" height="12" rx="5" />
    <rect x="72" y="20" width="8" height="60" rx="4" />
    <rect x="64" y="18" width="24" height="8" rx="4" />
    <rect x="64" y="74" width="24" height="8" rx="4" />
  </svg>
);

/** Filled status icons used by notices and the practice strip. `hole` is the colour cut out of them. */
export const IconWarnFill = ({ hole = "var(--card)" }: { hole?: string }) => (
  <svg viewBox="0 0 18 18" aria-hidden="true">
    <path d="M9 2l7.4 13a1 1 0 0 1-.9 1.5h-13a1 1 0 0 1-.9-1.5z" fill="currentColor" />
    <path d="M9 7v3.8" stroke={hole} strokeWidth="1.7" strokeLinecap="round" />
    <circle cx="9" cy="13.3" r="1" fill={hole} />
  </svg>
);
export const IconOkFill = ({ hole = "var(--card)" }: { hole?: string }) => (
  <svg viewBox="0 0 18 18" aria-hidden="true">
    <circle cx="9" cy="9" r="8" fill="currentColor" />
    <path d="M5.3 9.3l2.4 2.4 5-5.4" fill="none" stroke={hole} strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
export const IconInfoLine = () => (
  <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true">
    <circle cx="9" cy="9" r="7" />
    <path d="M9 8.2v4.3" />
    <circle cx="9" cy="5.6" r=".6" fill="currentColor" />
  </svg>
);

// ---------------------------------------------------------------- controls

export type Tone = "mi" | "pe" | "bad" | "nt";

const TONE_ICON: Record<Tone, ReactNode> = {
  mi: <IconOkFill />,
  pe: <IconWarnFill />,
  bad: <IconWarnFill />,
  nt: <IconInfoLine />,
};

/**
 * One line saying what happened, then the fix. Mint is good or info, peach
 * needs you, red means dictation can't happen.
 */
export function Notice({
  tone,
  icon,
  action,
  children,
  style,
}: {
  tone: Tone;
  icon?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
  style?: React.CSSProperties;
}) {
  return (
    <div className={`qntc ${tone}`} role={tone === "bad" ? "alert" : "status"} style={style}>
      {icon ?? TONE_ICON[tone]}
      <div className="tx">{children}</div>
      {action && <div className="a">{action}</div>}
    </div>
  );
}

export function Switch({
  checked,
  onChange,
  label,
  small = false,
  disabled = false,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
  small?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      className={`qtog ${small ? "sm" : ""}`}
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    />
  );
}

/** A row of mutually exclusive views: Heard | Changes | Cleaned. */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { id: T; label: string; disabled?: boolean }[];
  onChange: (next: T) => void;
  label: string;
}) {
  return (
    <div className="qseg" role="tablist" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.id}
          role="tab"
          aria-selected={o.id === value}
          disabled={o.disabled}
          onClick={() => onChange(o.id)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** A hotkey drawn as small keys inside a sentence. */
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
 * A hotkey drawn as keycaps: the symbol at the top right, the name at the
 * bottom left. Keys named in `down` are shown pressed; `look` marks the row
 * as refused or freshly saved; `slot` adds an empty key still to be pressed.
 */
export function Keycaps({
  hotkey,
  down,
  look,
  small = false,
  slot = false,
  style,
}: {
  hotkey: string;
  down?: ReadonlySet<string>;
  look?: "bad" | "pop";
  small?: boolean;
  slot?: boolean;
  style?: React.CSSProperties;
}) {
  const parts = keyParts(hotkey);
  return (
    <div className="qkeys" style={style} role="img" aria-label={parts.map((p) => p.name ?? p.cap).join(", ") || "No keys yet"}>
      {parts.map((k) => {
        const space = k.id === "Space";
        const word = !k.name && k.cap.length > 1 && !space;
        return (
          <span
            key={k.id}
            className={[
              "qkey",
              small ? "s" : "",
              space ? (small ? "wide" : "sp") : "",
              word && !small ? "" : "",
              down?.has(k.id) ? "dn" : "",
              look ?? "",
            ].join(" ")}
          >
            {!space && (
              <span className={`sy ${k.id === "Fn" ? "gl" : ""} ${word ? "tx" : ""}`}>{k.id === "Fn" ? "🌐" : k.cap}</span>
            )}
            {k.name && <span className="nm">{k.name}</span>}
          </span>
        );
      })}
      {slot && <span className={`qkey slot ${small ? "s" : ""}`} />}
    </div>
  );
}

/** Wave bars of a fixed shape, for download progress. */
const WAVE_SHAPE = [
  55, 60, 40, 100, 30, 45, 65, 45, 55, 50, 30, 65, 75, 30, 45, 40, 40, 45, 75, 45, 65, 40, 30, 50, 45, 75, 100, 100, 50, 30,
  50, 50, 40, 30, 75, 30, 65, 60, 90, 40, 60, 65, 45, 50, 90, 65, 100, 60, 45, 50, 50, 100, 75, 55, 45, 65,
];

/** Download progress: a waveform that fills from the left. */
export function WaveProgress({
  fraction,
  failed = false,
  label,
  bars = WAVE_SHAPE.length,
  style,
}: {
  fraction: number;
  failed?: boolean;
  label: string;
  bars?: number;
  style?: React.CSSProperties;
}) {
  const filled = Math.round(bars * Math.min(1, Math.max(0, fraction)));
  return (
    <div
      className={`qwave ${failed ? "pe" : ""}`}
      style={style}
      role="progressbar"
      aria-label={label}
      aria-valuenow={Math.round(fraction * 100)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      {Array.from({ length: bars }, (_, i) => (
        <i key={i} className={i < filled ? "f" : ""} style={{ height: `${WAVE_SHAPE[i % WAVE_SHAPE.length]}%` }} />
      ))}
    </div>
  );
}

// ---------------------------------------------------------------- numbers

/** "0.38 s" */
export function seconds(ms: number, digits = 2): string {
  return `${(ms / 1000).toFixed(digits)} s`;
}

/** "0.38" without the unit, for a big number with a small "s". */
export function secondsBare(ms: number, digits = 2): string {
  return (ms / 1000).toFixed(digits);
}

/** "0:04" */
export function clock(ms: number): string {
  const total = Math.floor(ms / 1000);
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

/** "3:38 pm", or "14 Oct, 3:38 pm" before today. */
export function when(at: number): string {
  if (!at) return "";
  const date = new Date(at);
  const today = new Date().toDateString() === date.toDateString();
  const time = date.toLocaleTimeString("en-GB", { hour: "numeric", minute: "2-digit", hour12: true }).toLowerCase();
  return today ? time : `${date.toLocaleDateString("en-GB", { day: "numeric", month: "short" })}, ${time}`;
}
