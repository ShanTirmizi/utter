// Development aid only: lets `pnpm dev` show the UI in an ordinary browser,
// where the Rust backend does not exist. It is a small pretend backend: it
// keeps settings, grants permissions, "downloads" models and plays whole
// dictations with a made-up voice. This file is never loaded inside the real
// app or in production builds (see main.tsx).
//
// Hold the hotkey shown in the window (by default Control+Option+Space) to
// dictate; Control+Option+R rewrites. Query parameters set the scene:
//
//   ?welcome          start at the first-run screen
//   ?tab=cleanup      open the main window on a section
//   ?mic=ask|denied   microphone not yet allowed, or refused
//   ?ax=on            Accessibility already granted
//   ?model=missing    no speech model downloaded
//   ?key              a key is already stored for Anthropic
//   ?cleanup=off      cleanup switched off
//   ?empty            no last dictation
//   ?fail=cleanup|test|download|hotkey   make that step fail
//   ?pill=listen|transcribe|tidy|rewrite|note|note2|paste|blocked
//                     hold the pill (open /#pill) in one state
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type {
  AppContext,
  Dictation,
  Mode,
  ModelStatus,
  PermissionStatus,
  PipelineUpdate,
  Provider,
  RewriteStyle,
  Settings,
} from "./api";

const query = new URLSearchParams(window.location.search);
const fail = query.get("fail");
const wait = (ms: number) => new Promise((done) => setTimeout(done, ms));

let settings: Settings = {
  hotkey: "Control+Alt+Space",
  cleanupEnabled: query.get("cleanup") !== "off",
  provider: "anthropic",
  cleanupModel: "",
  baseUrl: "",
  rewriteHotkey: "Control+Alt+KeyR",
  rewriteStyle: "clear",
  rewritePrompt: "",
  model: "base.en",
  onboarded: !query.has("welcome"),
};
const keys = new Set<string>(query.has("key") ? ["anthropic"] : []);

const catalog: Provider[] = [
  { id: "anthropic", label: "Anthropic (Claude)", protocol: "anthropic", baseUrl: "https://api.anthropic.com/v1", defaultModel: "claude-haiku-5-5", needsKey: true, keyUrl: "https://console.anthropic.com/settings/keys" },
  { id: "openai", label: "OpenAI", protocol: "openai", baseUrl: "https://api.openai.com/v1", defaultModel: "", needsKey: true, keyUrl: "https://platform.openai.com/api-keys" },
  { id: "gemini", label: "Google Gemini", protocol: "openai", baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai", defaultModel: "", needsKey: true, keyUrl: "https://aistudio.google.com/apikey" },
  { id: "groq", label: "Groq", protocol: "openai", baseUrl: "https://api.groq.com/openai/v1", defaultModel: "", needsKey: true, keyUrl: "https://console.groq.com/keys" },
  { id: "openrouter", label: "OpenRouter", protocol: "openai", baseUrl: "https://openrouter.ai/api/v1", defaultModel: "", needsKey: true, keyUrl: "https://openrouter.ai/keys" },
  { id: "ollama", label: "Ollama (on this Mac, no key)", protocol: "openai", baseUrl: "http://localhost:11434/v1", defaultModel: "", needsKey: false, keyUrl: "https://ollama.com" },
  { id: "custom", label: "Other (OpenAI-compatible)", protocol: "openai", baseUrl: "", defaultModel: "", needsKey: true, keyUrl: "" },
];

const styles: RewriteStyle[] = [
  { id: "clear", label: "Clearer", instruction: "Make it clear and easy to follow. Plain words, short sentences, the main point first. Keep the speaker's own tone." },
  { id: "concise", label: "Shorter", instruction: "Make it as short as it can be without losing a point. Cut anything that repeats or adds no information." },
  { id: "professional", label: "Professional", instruction: "Make it polished and professional, fit for a colleague or a client. Courteous and direct, with no slang and no stiffness." },
  { id: "friendly", label: "Friendly", instruction: "Make it warm and friendly, the way you would write to someone you like. Relaxed and natural, without gushing." },
];

const permissions: PermissionStatus = {
  microphone: query.get("mic") === "ask" || query.has("welcome") ? "undetermined" : query.get("mic") === "denied" ? "denied" : "granted",
  accessibility: query.get("ax") === "on",
};

const models: ModelStatus[] = [
  { name: "base.en", label: "Base (English)", note: "Fastest. Good for clear speech.", sizeMb: 148, downloaded: query.get("model") !== "missing" && !query.has("welcome") },
  { name: "small.en", label: "Small (English)", note: "More accurate, a little slower.", sizeMb: 488, downloaded: false },
  { name: "large-v3-turbo-q5_0", label: "Large v3 Turbo (multilingual)", note: "Most accurate. Any language.", sizeMb: 574, downloaded: false },
];

const slack: AppContext = { name: "Slack", bundleId: "com.tinyspeck.slackmacgap", category: "chat" };
const mail: AppContext = { name: "Mail", bundleId: "com.apple.mail", category: "email" };

/** What the pretend speech model hears, in turn. */
const lines: { raw: string; tidy: string; rewrite: string; app: AppContext }[] = [
  {
    raw: "um so i think we should uh push the release to thursday and like let the design team know",
    tidy: "I think we should push the release to Thursday and let the design team know.",
    rewrite: "Let's move the release to Thursday. I'll tell the design team.",
    app: mail,
  },
  {
    raw: "um so can you send me the like the budget spreadsheet before wednesday",
    tidy: "Can you send me the budget spreadsheet before Wednesday?",
    rewrite: "Please send me the budget spreadsheet by Wednesday.",
    app: slack,
  },
];
let turn = 0;

let last: Dictation | null = query.has("empty") || query.has("welcome")
  ? null
  : {
      raw: lines[1].raw,
      text: lines[1].tidy,
      mode: "tidy",
      cleaned: true,
      inserted: true,
      note: null,
      app: slack,
      transcribeMs: 310,
      cleanupMs: 540,
      spokenMs: 4100,
      totalMs: 910,
      finishedAt: Date.now() - 4 * 60_000,
    };

// Other tabs of the preview (the pill at /#pill, say) hear the same events.
const channel = new BroadcastChannel("utter-dev");
channel.onmessage = ({ data }) => emit(data.event, data.payload);
function send(event: string, payload: unknown) {
  emit(event, payload);
  channel.postMessage({ event, payload });
}

function pipeline(update: Partial<PipelineUpdate> & Pick<PipelineUpdate, "phase">, mode: Mode) {
  send("pipeline", { mode, stage: null, app: null, message: null, dictation: null, ...update });
}

/** Whether the language-model step would run for this hotkey. */
function modelStepRuns(mode: Mode): boolean {
  const provider = catalog.find((p) => p.id === settings.provider);
  const ready = !!provider && (!provider.needsKey || keys.has(provider.id)) && !!(settings.cleanupModel || provider.defaultModel);
  return ready && (mode === "rewrite" || settings.cleanupEnabled);
}

// A made-up voice: bursts of syllables with a breath every few seconds.
let voice: ReturnType<typeof setInterval> | undefined;
let pressedAt = 0;
let recording: Mode | null = null;
function startVoice() {
  let beat = 0;
  clearInterval(voice);
  voice = setInterval(() => {
    beat += 1;
    const syllable = Math.max(0, Math.sin(beat * 0.55)) * 0.75 + Math.random() * 0.35;
    const breath = Math.floor(beat / 26) % 4 === 3 ? 0.1 : 1;
    send("audio-level", Math.min(1, syllable * breath));
  }, 50);
}

function press(mode: Mode) {
  if (recording) return;
  if (!models.find((m) => m.name === settings.model)?.downloaded) {
    return pipeline({ phase: "error", message: "Download a speech model in Utter's settings first." }, mode);
  }
  if (permissions.microphone === "denied") {
    return pipeline({ phase: "error", message: "Microphone access is off. Allow Utter in System Settings." }, mode);
  }
  recording = mode;
  pressedAt = performance.now();
  pipeline({ phase: "recording" }, mode);
  startVoice();
}

async function release() {
  const mode = recording;
  if (!mode) return;
  recording = null;
  clearInterval(voice);
  const spokenMs = Math.round(performance.now() - pressedAt);
  const line = lines[turn % lines.length];
  pipeline({ phase: "processing", stage: "transcribing", app: line.app }, mode);
  // An accidental tap is ignored.
  if (spokenMs < 300) return pipeline({ phase: "idle" }, mode);

  const transcribeMs = 380;
  await wait(transcribeMs);
  const runs = modelStepRuns(mode);
  let cleanupMs: number | null = null;
  let note: string | null = null;
  let cleaned = false;
  if (runs) {
    pipeline({ phase: "processing", stage: mode === "rewrite" ? "rewriting" : "tidying", app: line.app }, mode);
    cleanupMs = 610;
    await wait(cleanupMs);
    if (fail === "cleanup") note = `${mode === "rewrite" ? "Rewrite" : "Cleanup"} skipped: Anthropic rejected the API key (401).`;
    else cleaned = true;
  } else if (mode === "rewrite") {
    note = "Rewrite needs a model. Set one up under Cleanup in settings.";
  }
  const inserted = permissions.accessibility;
  if (!inserted) {
    const copied = "Copied to the clipboard (Accessibility is off). Press ⌘V to paste.";
    note = note ? `${copied} ${note}` : copied;
  }
  turn += 1;
  last = {
    raw: line.raw,
    text: cleaned ? (mode === "rewrite" ? line.rewrite : line.tidy) : line.raw,
    mode,
    cleaned,
    inserted,
    note,
    app: line.app,
    transcribeMs,
    cleanupMs,
    spokenMs,
    totalMs: transcribeMs + (cleanupMs ?? 0) + 30,
    finishedAt: Date.now(),
  };
  if (inserted) paste(`${last.text} `);
  pipeline({ phase: "done", dictation: last }, mode);
}

/** Stand in for the real paste: put the text into the focused text box. */
function paste(text: string) {
  const box = document.activeElement;
  if (!(box instanceof HTMLTextAreaElement)) return;
  // React tracks the value itself, so set it the way typing would.
  const setValue = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
  setValue?.call(box, box.value + text);
  box.dispatchEvent(new Event("input", { bubbles: true }));
}

/** Does this key event complete the stored hotkey ("Control+Alt+Space")? */
function matches(e: KeyboardEvent, hotkey: string): boolean {
  if (!hotkey) return false;
  const parts = hotkey.split("+");
  const code = parts[parts.length - 1];
  const wants = (m: string) => parts.includes(m);
  return (
    e.code === code &&
    e.ctrlKey === wants("Control") &&
    e.altKey === wants("Alt") &&
    e.shiftKey === wants("Shift") &&
    e.metaKey === wants("Super")
  );
}

function watchHotkeys() {
  window.addEventListener("keydown", (e) => {
    const mode = matches(e, settings.hotkey) ? "tidy" : matches(e, settings.rewriteHotkey) ? "rewrite" : null;
    if (!mode) return;
    e.preventDefault();
    if (!e.repeat) press(mode);
  });
  // Letting go of any key of the combination ends the dictation.
  window.addEventListener("keyup", () => release());
  window.addEventListener("blur", () => release());
}

async function download(name: string) {
  const model = models.find((m) => m.name === name);
  if (!model || model.downloaded) return;
  const total = model.sizeMb * 1_000_000;
  const steps = 60;
  for (let i = 1; i <= steps; i++) {
    await wait(90);
    send("model-download", { name, received: Math.round((total * i) / steps), total });
    if (fail === "download" && i === 22) throw "Download interrupted: the network connection was lost.";
  }
  model.downloaded = true;
}

/** Hold the pill in one state, for looking at it or taking screenshots. */
function pinPill(state: string) {
  const at = (ms: number, fn: () => void) => setTimeout(fn, ms);
  const dictation = (note: string, inserted: boolean): Dictation => ({ ...(last ?? ({} as Dictation)), note, inserted });
  const play: Record<string, () => void> = {
    listen: () => (pipeline({ phase: "recording" }, "tidy"), startVoice()),
    transcribe: () => pipeline({ phase: "processing", stage: "transcribing" }, "tidy"),
    tidy: () => pipeline({ phase: "processing", stage: "tidying", app: mail }, "tidy"),
    rewrite: () => pipeline({ phase: "processing", stage: "rewriting", app: mail }, "rewrite"),
    note: () => (pipeline({ phase: "processing" }, "tidy"), pipeline({ phase: "idle" }, "tidy")),
    note2: () => pipeline({ phase: "done", dictation: dictation("Cleanup skipped: Anthropic rejected the API key (401).", true) }, "tidy"),
    paste: () => pipeline({ phase: "done", dictation: dictation("Copied to the clipboard (Accessibility is off). Press ⌘V to paste.", false) }, "tidy"),
    blocked: () => pipeline({ phase: "error", message: "Microphone access is off. Allow Utter in System Settings." }, "tidy"),
  };
  // Notes hide themselves, so show them again for as long as the page is open.
  const again = () => {
    play[state]?.();
    if (["note", "note2", "paste", "blocked"].includes(state)) at(2400, again);
  };
  at(300, again);
}

export function installMock() {
  mockWindows("main");
  mockIPC(
    async (command, args) => {
      const payload = args as Record<string, unknown> | undefined;
      switch (command) {
        case "get_settings":
          return settings;
        case "save_settings": {
          const next = payload?.settings as Settings;
          if (fail === "hotkey" && next.hotkey !== settings.hotkey) {
            throw "macOS would not register it. Another app may be using this combination.";
          }
          if (next.baseUrl && !/^https:\/\/|^http:\/\/(localhost|127\.0\.0\.1)/.test(next.baseUrl)) {
            throw "The base URL must use https, or http on localhost.";
          }
          settings = next;
          return settings;
        }
        case "providers":
          return catalog;
        case "has_api_key":
          return keys.has(payload?.provider as string);
        case "set_api_key":
          keys.add(payload?.provider as string);
          return;
        case "delete_api_key":
          keys.delete(payload?.provider as string);
          return;
        case "rewrite_styles":
          return styles;
        case "test_rewrite":
          await wait(900);
          if (fail === "test") throw "Could not reach api.anthropic.com (connection refused).";
          return "I think we should move the meeting to Friday, since half the team is out on Thursday.";
        case "list_cleanup_models":
          await wait(400);
          if (fail === "test") throw "Could not reach api.anthropic.com (connection refused).";
          return ["example-fast-model", "example-large-model"];
        case "test_cleanup":
          await wait(580);
          if (fail === "test") throw "Could not reach api.anthropic.com (connection refused).";
          return "This is a quick test of the cleanup step.";
        case "permission_status":
          return { ...permissions };
        case "request_microphone":
          // Stand in for the person clicking "Allow" on the system prompt.
          setTimeout(() => (permissions.microphone = "granted"), 900);
          return;
        case "request_accessibility":
          return permissions.accessibility;
        case "open_privacy_settings":
          // Stand in for the person finding the switch in System Settings.
          if (payload?.pane === "accessibility") setTimeout(() => (permissions.accessibility = true), 3500);
          else setTimeout(() => (permissions.microphone = "granted"), 3500);
          return;
        case "reset_accessibility":
          permissions.accessibility = false;
          return;
        case "model_statuses":
          return models.map((m) => ({ ...m }));
        case "download_model":
          return download(payload?.name as string);
        case "last_dictation":
          return last;
        default:
          return;
      }
    },
    { shouldMockEvents: true },
  );

  const pinned = query.get("pill");
  if (pinned) pinPill(pinned);
  else watchHotkeys();
}
