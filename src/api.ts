// Typed wrappers around the Rust commands. The UI talks to the backend only
// through this file.
import { invoke } from "@tauri-apps/api/core";

export type Settings = {
  hotkey: string;
  cleanupEnabled: boolean;
  /** Tidy the whole dictation at once ("accurate") or piece by piece while you talk ("quick"). */
  way: Way;
  /** Which service runs cleanup; an id from `providers()`. */
  provider: string;
  /** Empty means the provider's default model, if it has one. */
  cleanupModel: string;
  /** Used only by the "custom" provider. */
  baseUrl: string;
  /** Second hotkey: hold it to have the dictation rewritten. Empty = off. */
  rewriteHotkey: string;
  /** A preset id from `rewriteStyles()`, or "custom". */
  rewriteStyle: string;
  /** The user's own rewrite instruction, used when the style is "custom". */
  rewritePrompt: string;
  /** Model for the rewrite hotkey. Empty means the provider's `rewriteModel`, else the cleanup model. */
  rewriteModel: string;
  /** Names and terms to be spelled exactly this way. */
  dictionary: Word[];
  /** Removed words, lower case, and removed mishearings as "cloud→claude": never learned again. */
  forgotten: string[];
  /** Learn a word when you correct it in what Utter just typed. */
  learnWords: boolean;
  /** Use names and terms in the window in front as hints while dictating. */
  screenWords: boolean;
  screenContext: boolean;
  /** Names seen on screen that you then said, offered for Your words. */
  suggested: Suggestion[];
  /** Keep past dictations on this Mac. */
  keepHistory: boolean;
  /** Also keep the audio of the last 50, on this Mac, to replay through other models. Off by default. */
  keepAudio: boolean;
  /** A quick tap starts a recording that runs until the next tap. */
  handsFree: boolean;
  /** While fn dictates, switch macOS's own globe-key action off, and back afterwards. */
  quietGlobe: boolean;
  /** Speech model. */
  model: string;
  /** The language the multilingual model expects, a Whisper code; "" detects it. */
  language: string;
  /** Words removed in the last 30 days, newest first. */
  removed: Removed[];
  /** Ask GitHub once a day for a newer release. Off by default. */
  checkUpdates: boolean;
  /** Follow the Mac's appearance, or fix it to light or dark. */
  appearance: "system" | "light" | "dark";
  /** The microphone to record from, by Core Audio UID; "" for the system default. */
  microphone: string;
  onboarded: boolean;
};

/** A removed word, restorable for 30 days. */
export type Removed = { word: Word; removedAt: number };

/** A name seen on screen that you then said. */
export type Suggestion = {
  word: string;
  /** What the speech model wrote for it, if it misheard it; lower case. */
  heard: string | null;
  /** How many dictations used it. */
  seen: number;
  /** The app it was last seen in. */
  app: string;
  lastSeen: number;
};

/** One of the user's own words. */
export type Word = {
  /** How the user writes it. */
  word: string;
  /** What the speech model wrote instead, lower case: "cloud" for "Claude". */
  heardAs: string[];
  source: "added" | "learned" | "screen";
  /** Milliseconds since 1970; 0 if unknown. */
  addedAt: number;
};

export type Provider = {
  id: string;
  label: string;
  protocol: "anthropic" | "openai";
  baseUrl: string;
  defaultModel: string;
  /** What the rewrite hotkey uses unless one is chosen. Empty means the cleanup model. */
  rewriteModel: string;
  needsKey: boolean;
  keyUrl: string;
};

export const providers = () => invoke<Provider[]>("providers");

export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) =>
  invoke<Settings>("save_settings", { settings });

// A key goes one way only: into the Keychain. It is never read back. Each
// provider keeps its own key.
export const hasApiKey = (provider: string) => invoke<boolean>("has_api_key", { provider });
export const setApiKey = (provider: string, key: string) =>
  invoke<void>("set_api_key", { provider, key });
export const deleteApiKey = (provider: string) => invoke<void>("delete_api_key", { provider });
/** Runs one fixed sentence through cleanup and returns the result. */
export const testCleanup = () => invoke<string>("test_cleanup");
export type RewriteStyle = { id: string; label: string; instruction: string };
/** The ready-made rewrite styles. */
export const rewriteStyles = () => invoke<RewriteStyle[]>("rewrite_styles");
/** Rewrites one fixed, rambling sentence with the current style. */
export const testRewrite = () => invoke<string>("test_rewrite");

/** Models the chosen provider offers for the saved key. */
export const listCleanupModels = () => invoke<string[]>("list_cleanup_models");

// Progress reports from the dictation pipeline, sent on the "pipeline" event.
export type Way = "accurate" | "quick";

export type Dictation = {
  /** Exactly what the speech model heard. */
  raw: string;
  /** What Utter produced from it. */
  text: string;
  /** Which hotkey was used. */
  mode: Mode;
  /**
   * The selected text this dictation edited, or null. When set, `raw` is the
   * spoken instruction and `text` is what replaced the selection.
   */
  selection: string | null;
  /** True when the text went through the cleanup model. */
  cleaned: boolean;
  /** True when pasted for you; false when only copied to the clipboard. */
  inserted: boolean;
  note: string | null;
  app: AppContext;
  transcribeMs: number;
  cleanupMs: number | null;
  /** How long the hotkey was held. */
  spokenMs: number;
  /** From letting go of the hotkey to the text being delivered. */
  totalMs: number;
  /** When it finished, in milliseconds since 1970. */
  finishedAt: number;
  /** How it was tidied; null when it was not, or from before there was a choice. */
  way?: Way | null;
  /**
   * What you changed in it after it was pasted, once you were done. Null
   * until then, and for good when Utter couldn't follow the text box.
   */
  edit: Edit | null;
  /** Its recording's file name, when Keep recordings is on. */
  recording: string | null;
};

/** What you did to a pasted dictation afterwards. */
export type Edit = {
  /** Words Utter pasted. */
  words: number;
  /** Words you changed, added or took out in it; 0 if you left it alone. */
  changed: number;
  /** The text as you left it, kept only next to a recording. */
  corrected: string | null;
  /**
   * True when Utter never saw how you left it in the text box and only knows
   * of a word you fixed later. Left out of the edit rate.
   */
  unwatched?: boolean;
};

export type AppContext = {
  name: string;
  bundleId: string;
  category: "chat" | "email" | "code" | "notes" | "assistant" | "other";
};

/** Tidy keeps your wording; rewrite says it better. One hotkey each. */
export type Mode = "tidy" | "rewrite";

/** What "processing" is doing right now. */
export type Stage = "transcribing" | "tidying" | "rewriting" | "editing";

export type PipelineUpdate = {
  phase: "idle" | "recording" | "processing" | "done" | "error";
  mode: Mode;
  /** True while a tap-started recording runs until the next tap. */
  handsFree: boolean;
  /** Set while processing. */
  stage: Stage | null;
  /** The app being dictated into, once known. */
  app: AppContext | null;
  message: string | null;
  dictation: Dictation | null;
};

// While recording, the "audio-level" event carries the microphone level about
// twenty times a second: one number from 0 (silence) to 1 (loud speech).
export type AudioLevel = number;

export const lastDictation = () => invoke<Dictation | null>("last_dictation");

/** Past dictations kept on this Mac, newest first. */
export const history = () => invoke<Dictation[]>("history");
export const clearHistory = () => invoke<void>("clear_history");
/** Forget one past dictation, named by its `finishedAt`. */
export const forgetDictation = (finishedAt: number) => invoke<void>("forget_dictation", { finishedAt });
/**
 * Correct a misheard word in a past dictation. With `remember`, the right
 * spelling joins Your words, and the wrong one is kept as how it was heard.
 * Resolves to the settings as saved.
 */
export const fixWord = (finishedAt: number, wrong: string, right: string, remember: boolean) =>
  invoke<Settings>("fix_word", { finishedAt, wrong, right, remember });
/** A word learned from a correction, sent on the "learned" event. */
export type Learned = {
  word: string;
  /** What Utter had written, lower case. */
  heard: string;
  /** True for a new word; false for a new mishearing of a word already there. */
  newWord: boolean;
};
/** Take back a learned word (or mishearing) and never learn it again. */
export const undoLearned = (learned: Learned) => invoke<Settings>("undo_learned", { learned });
/** Put text on the clipboard. */
export const copyText = (text: string) => invoke<void>("copy_text", { text });

export type ModelStatus = {
  name: string;
  label: string;
  note: string;
  sizeMb: number;
  downloaded: boolean;
};
export type DownloadProgress = { name: string; received: number; total: number };

export const modelStatuses = () => invoke<ModelStatus[]>("model_statuses");
export const downloadModel = (name: string) => invoke<void>("download_model", { name });
/** Stop a running download; it then fails with "Stopped." */
export const stopDownload = (name: string) => invoke<void>("stop_download", { name });

/** The window's status, for the menu bar's header and icon. */
export type TrayStatus = { title: string; detail: string; tone: string; action: string | null; fix: string | null };
export const setTrayStatus = (status: TrayStatus) => invoke<void>("set_tray_status", { status });

export type UpdateStatus = {
  current: string;
  latest: string | null;
  newer: boolean;
  summary: string | null;
  url: string | null;
  /** Milliseconds since 1970; 0 if never checked. */
  checkedAt: number;
  error: string | null;
};
export const updateStatus = () => invoke<UpdateStatus>("update_status");
export const checkForUpdates = () => invoke<UpdateStatus>("check_for_updates");

export type MicStatus = "undetermined" | "granted" | "denied";
export type PermissionStatus = { microphone: MicStatus; accessibility: boolean };

export const permissionStatus = () => invoke<PermissionStatus>("permission_status");

export type Microphone = { id: string; name: string };
export type Microphones = { defaultName: string | null; devices: Microphone[] };
/** The microphones plugged in now. */
export const microphones = () => invoke<Microphones>("microphones");

/** Whether Utter opens at login, as macOS has it. "unavailable" before macOS 13 or outside the installed app. */
export type LoginItem = "off" | "on" | "needsApproval" | "unavailable";
export const loginItem = () => invoke<LoginItem>("login_item");
export const setLoginItem = (enabled: boolean) => invoke<LoginItem>("set_login_item", { enabled });
export const requestMicrophone = () => invoke<void>("request_microphone");
export const requestAccessibility = () => invoke<boolean>("request_accessibility");
/** Stop the global hotkeys while a new one is being recorded. */
export const pauseHotkeys = () => invoke<void>("pause_hotkeys");
/** Start them again from the saved settings. Resolves to a problem, if any. */
export const resumeHotkeys = () => invoke<string | null>("resume_hotkeys");
/** Why a hotkey is not working right now, or null when both are. */
export const hotkeyProblem = () => invoke<string | null>("hotkey_problem");

/** Clears a stale Accessibility entry for Utter and asks macOS again. */
export const resetAccessibility = () => invoke<void>("reset_accessibility");
/** What macOS also does when fn is pressed ("show emoji and symbols"), or null. */
export const fnKeyConflict = () => invoke<string | null>("fn_key_conflict");
export const openPrivacySettings = (pane: "microphone" | "accessibility" | "keyboard") =>
  invoke<void>("open_privacy_settings", { pane });
