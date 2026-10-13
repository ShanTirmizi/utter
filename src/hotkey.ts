// Hotkeys are stored as "Modifier+Modifier+Code", e.g. "Control+Alt+Space",
// using the browser's physical key codes (KeyD, Digit1, F5, Space).

const MODIFIER_SYMBOLS: Record<string, string> = {
  Control: "⌃",
  Alt: "⌥",
  Shift: "⇧",
  Super: "⌘",
};

const MODIFIER_CODES = new Set([
  "ControlLeft", "ControlRight", "AltLeft", "AltRight",
  "ShiftLeft", "ShiftRight", "MetaLeft", "MetaRight",
]);

/** Human-readable keycaps for a stored hotkey: ["⌃", "⌥", "Space"]. */
export function keycaps(hotkey: string): string[] {
  return hotkey.split("+").map((part) => {
    if (MODIFIER_SYMBOLS[part]) return MODIFIER_SYMBOLS[part];
    if (part.startsWith("Key")) return part.slice(3);
    if (part.startsWith("Digit")) return part.slice(5);
    return part;
  });
}

/**
 * Turn a keydown event into a storable hotkey, or null while only modifiers
 * are held. A plain letter with no modifier is rejected because it would make
 * that letter impossible to type; function keys are allowed on their own.
 */
export function fromKeyEvent(e: KeyboardEvent): string | null {
  if (MODIFIER_CODES.has(e.code)) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Control");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Super");
  const isFunctionKey = /^F\d{1,2}$/.test(e.code);
  if (parts.length === 0 && !isFunctionKey) return null;
  parts.push(e.code);
  return parts.join("+");
}

const MODIFIER_NAMES: Record<string, string> = {
  Control: "Control",
  Alt: "Option",
  Shift: "Shift",
  Super: "Command",
};

/** One key of a hotkey, ready to draw: "⌥" with "Option" underneath. */
export type KeyPart = { id: string; cap: string; name: string | null; wide: boolean };

export function keyParts(hotkey: string): KeyPart[] {
  if (!hotkey) return [];
  const caps = keycaps(hotkey);
  return hotkey.split("+").map((id, i) => ({
    id,
    cap: caps[i],
    name: MODIFIER_NAMES[id] ?? null,
    wide: caps[i].length > 2,
  }));
}

/** "⌃ ⌥ Space", for sentences. */
export function hotkeyLabel(hotkey: string): string {
  return keycaps(hotkey).join(" ");
}

/** The parts of a hotkey that a key event shows as held, by id. */
export function heldIds(e: KeyboardEvent, down: boolean): string[] {
  const held: string[] = [];
  if (e.ctrlKey) held.push("Control");
  if (e.altKey) held.push("Alt");
  if (e.shiftKey) held.push("Shift");
  if (e.metaKey) held.push("Super");
  if (down && !MODIFIER_CODES.has(e.code)) held.push(e.code);
  return held;
}
