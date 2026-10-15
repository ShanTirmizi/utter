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
//   ?empty            no last dictation and no history
//   ?words            a few terms already in the dictionary
//   ?learned          a word is learned from a correction shortly after loading
//   ?selected         the rewrite hotkey edits a pretend selection
//   ?fn               the fn key is the dictation hotkey
//   ?fnconflict       and macOS also shows emoji when it is pressed
//   ?fail=cleanup|test|download|hotkey   make that step fail
//   ?fail=startup-hotkey   macOS refused the hotkey at launch
//   ?pill=listen|handsfree|transcribe|tidy|rewrite|edit|note|note2|paste|blocked|learned
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
  hotkey: query.has("fn") ? "Fn" : "Control+Alt+Space",
  cleanupEnabled: query.get("cleanup") !== "off",
  provider: "anthropic",
  cleanupModel: "",
  baseUrl: "",
  rewriteHotkey: "Control+Alt+KeyR",
  rewriteStyle: "clear",
  rewritePrompt: "",
  dictionary: query.has("words")
    ? [
        { word: "Claude", heardAs: ["cloud", "clawed"], source: "learned", addedAt: Date.now() - 3_600_000 },
        { word: "Priya Raghunathan", heardAs: [], source: "added", addedAt: Date.now() - 86_400_000 },
        { word: "Kubernetes", heardAs: ["cuban eats"], source: "added", addedAt: Date.now() - 86_400_000 },
        { word: "fetchUserProfile", heardAs: [], source: "screen", addedAt: Date.now() - 86_400_000 },
      ]
    : [],
  forgotten: [],
  learnWords: true,
  keepHistory: true,
  handsFree: true,
  quietGlobe: !query.has("fnconflict"),
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
      raw: "um so here's the thing i've moved the release to thursday and like we still need to tell the design team",
      text: "So here\u2019s the thing: I\u2019ve moved the release to Thursday.\n\nWe still need to tell the design team.",
      mode: "tidy",
      selection: null,
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

/** Past dictations, newest first. */
let past: Dictation[] = last
  ? [
      last,
      { ...last, raw: lines[0].raw, text: lines[0].tidy, app: mail, finishedAt: Date.now() - 50 * 60_000 },
      {
        ...last,
        raw: "okay so the uh can you ask cloud to summarise the standup notes",
        text: "Can you ask cloud to summarise the standup notes?",
        finishedAt: Date.now() - 26 * 3_600_000,
      },
    ]
  : [];

// Other tabs of the preview (the pill at /#pill, say) hear the same events.
const channel = new BroadcastChannel("utter-dev");
channel.onmessage = ({ data }) => emit(data.event, data.payload);
function send(event: string, payload: unknown) {
  emit(event, payload);
  channel.postMessage({ event, payload });
}

function pipeline(update: Partial<PipelineUpdate> & Pick<PipelineUpdate, "phase">, mode: Mode) {
  send("pipeline", { mode, handsFree: false, stage: null, app: null, message: null, dictation: null, ...update });
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
// A double tap started the recording, so it runs until the next tap.
let latched = false;
let swallowRelease = false;
// A quick tap just ended; a second press soon makes it a double tap.
let awaiting: ReturnType<typeof setTimeout> | null = null;
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
  if (recording && awaiting) {
    // The second tap of a double tap: hands-free.
    clearTimeout(awaiting);
    awaiting = null;
    latched = true;
    swallowRelease = true;
    return pipeline(
      { phase: "recording", handsFree: true, message: "Hands-free. Tap the hotkey again to finish." },
      recording,
    );
  }
  if (recording && latched) {
    // The second tap ends a hands-free recording.
    latched = false;
    swallowRelease = true;
    finish();
    return;
  }
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

function release() {
  if (swallowRelease) {
    swallowRelease = false;
    return;
  }
  if (!recording || latched || awaiting) return;
  if (performance.now() - pressedAt < 300 && settings.handsFree) {
    const mode = recording;
    awaiting = setTimeout(() => {
      // Only a single tap: let it go, and say how hands-free starts.
      awaiting = null;
      recording = null;
      clearInterval(voice);
      pipeline({ phase: "idle", message: "Hold to talk, or double-tap for hands-free." }, mode);
    }, 400);
    return;
  }
  finish();
}

async function finish() {
  const mode = recording;
  if (!mode) return;
  recording = null;
  clearInterval(voice);
  const spokenMs = Math.round(performance.now() - pressedAt);
  const line = lines[turn % lines.length];
  pipeline({ phase: "processing", stage: "transcribing", app: line.app }, mode);
  // An accidental tap is ignored.
  if (spokenMs < 300) return pipeline({ phase: "idle" }, mode);
  if (mode === "rewrite" && query.has("selected")) return edit(spokenMs);

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
    selection: null,
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
  if (settings.keepHistory) past = [last, ...past].slice(0, 200);
  pipeline({ phase: "done", dictation: last }, mode);
}

/** The rewrite hotkey with text selected: the words spoken say how to change it. */
async function edit(spokenMs: number) {
  await wait(380);
  pipeline({ phase: "processing", stage: "editing", app: mail }, "rewrite");
  await wait(700);
  const done: Dictation = {
    raw: "make this shorter and less formal",
    text: "Can we move Thursday's review to Friday? Half the team is out.",
    mode: "rewrite",
    selection:
      "I am writing to enquire whether it would be possible to reschedule the review that is planned for Thursday to Friday instead, as approximately half of the team will be out of the office.",
    cleaned: true,
    inserted: permissions.accessibility,
    note: permissions.accessibility ? null : "Copied to the clipboard (Accessibility is off). Press ⌘V to paste.",
    app: mail,
    transcribeMs: 380,
    cleanupMs: 700,
    spokenMs,
    totalMs: 1110,
    finishedAt: Date.now(),
  };
  last = done;
  if (settings.keepHistory) past = [done, ...past].slice(0, 200);
  pipeline({ phase: "done", dictation: done }, "rewrite");
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
    handsfree: () => (pipeline({ phase: "recording", handsFree: true }, "tidy"), startVoice()),
    edit: () => pipeline({ phase: "processing", stage: "editing", app: mail }, "rewrite"),
    transcribe: () => pipeline({ phase: "processing", stage: "transcribing" }, "tidy"),
    tidy: () => pipeline({ phase: "processing", stage: "tidying", app: mail }, "tidy"),
    rewrite: () => pipeline({ phase: "processing", stage: "rewriting", app: mail }, "rewrite"),
    note: () => (pipeline({ phase: "processing" }, "tidy"), pipeline({ phase: "idle" }, "tidy")),
    note2: () => pipeline({ phase: "done", dictation: dictation("Cleanup skipped: Anthropic rejected the API key (401).", true) }, "tidy"),
    paste: () => pipeline({ phase: "done", dictation: dictation("Copied to the clipboard (Accessibility is off). Press ⌘V to paste.", false) }, "tidy"),
    blocked: () => pipeline({ phase: "error", message: "Microphone access is off. Allow Utter in System Settings." }, "tidy"),
    learned: () => send("learned", { word: "Claude", heard: "cloud", newWord: true }),
  };
  // Notes hide themselves, so show them again for as long as the page is open.
  const again = () => {
    play[state]?.();
    if (["note", "note2", "paste", "blocked", "learned"].includes(state)) at(2400, again);
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
          settings = {
            ...next,
            dictionary: next.dictionary
              .map((w) => ({ ...w, word: w.word.trim().slice(0, 60) }))
              .filter((w) => w.word)
              .slice(0, 200),
          };
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
        case "pause_hotkeys":
          return;
        case "resume_hotkeys":
        case "hotkey_problem":
          return query.get("fail") === "startup-hotkey"
            ? "The dictation hotkey is not working. Another app may be using it."
            : null;
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
        case "history":
          return past;
        case "clear_history":
          past = [];
          last = null;
          return;
        case "forget_dictation":
          past = past.filter((d) => d.finishedAt !== payload?.finishedAt);
          return;
        case "copy_text":
          return;
        case "undo_learned": {
          const learned = payload?.learned as { word: string; heard: string; newWord: boolean };
          settings = {
            ...settings,
            dictionary: learned.newWord
              ? settings.dictionary.filter((w) => w.word !== learned.word)
              : settings.dictionary.map((w) =>
                  w.word === learned.word ? { ...w, heardAs: w.heardAs.filter((h) => h !== learned.heard) } : w,
                ),
            forgotten: [...settings.forgotten, learned.newWord ? learned.word.toLowerCase() : `${learned.heard}→${learned.word.toLowerCase()}`],
          };
          send("unlearned", learned);
          return settings;
        }
        case "fix_word": {
          const { finishedAt, wrong, right, remember } = payload as { finishedAt: number; wrong: string; right: string; remember: boolean };
          const swap = (d: Dictation) =>
            d.finishedAt === finishedAt ? { ...d, text: d.text.split(wrong).join(right) } : d;
          past = past.map(swap);
          if (last) last = swap(last);
          if (!remember) return settings;
          const heard = wrong.toLowerCase() === right.toLowerCase() ? [] : [wrong.toLowerCase()];
          const have = settings.dictionary.find((w) => w.word.toLowerCase() === right.toLowerCase());
          settings = {
            ...settings,
            dictionary: have
              ? settings.dictionary.map((w) =>
                  w === have ? { ...w, word: right, heardAs: [...new Set([...w.heardAs, ...heard])] } : w,
                )
              : [{ word: right, heardAs: heard, source: "added", addedAt: Date.now() }, ...settings.dictionary],
          };
          return settings;
        }
        case "fn_key_conflict":
          return query.has("fnconflict") && !settings.quietGlobe ? "show emoji and symbols" : null;
        default:
          return;
      }
    },
    { shouldMockEvents: true },
  );

  if (query.has("learned")) {
    setTimeout(() => {
      const learned = { word: "Anthropic", heard: "and tropic", newWord: true };
      settings = {
        ...settings,
        dictionary: [{ word: "Anthropic", heardAs: ["and tropic"], source: "learned" }, ...settings.dictionary].map((w) => ({ addedAt: Date.now(), ...w })) as Settings["dictionary"],
      };
      send("learned", learned);
    }, 600);
  }

  const pinned = query.get("pill");
  if (pinned) pinPill(pinned);
  else watchHotkeys();
}
