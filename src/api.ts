// Typed wrappers around the Rust commands. The UI talks to the backend only
// through this file.
import { invoke } from "@tauri-apps/api/core";

export type Settings = {
  hotkey: string;
  cleanupEnabled: boolean;
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
  /** Speech model. */
  model: string;
  onboarded: boolean;
};

export type Provider = {
  id: string;
  label: string;
  protocol: "anthropic" | "openai";
  baseUrl: string;
  defaultModel: string;
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
export type Dictation = {
  /** Exactly what the speech model heard. */
  raw: string;
  /** What Utter produced from it. */
  text: string;
  /** Which hotkey was used. */
  mode: Mode;
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
};

export type AppContext = {
  name: string;
  bundleId: string;
  category: "chat" | "email" | "code" | "notes" | "assistant" | "other";
};

/** Tidy keeps your wording; rewrite says it better. One hotkey each. */
export type Mode = "tidy" | "rewrite";

/** What "processing" is doing right now. */
export type Stage = "transcribing" | "tidying" | "rewriting";

export type PipelineUpdate = {
  phase: "idle" | "recording" | "processing" | "done" | "error";
  mode: Mode;
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

export type MicStatus = "undetermined" | "granted" | "denied";
export type PermissionStatus = { microphone: MicStatus; accessibility: boolean };

export const permissionStatus = () => invoke<PermissionStatus>("permission_status");
export const requestMicrophone = () => invoke<void>("request_microphone");
export const requestAccessibility = () => invoke<boolean>("request_accessibility");
/** Clears a stale Accessibility entry for Utter and asks macOS again. */
export const resetAccessibility = () => invoke<void>("reset_accessibility");
export const openPrivacySettings = (pane: "microphone" | "accessibility") =>
  invoke<void>("open_privacy_settings", { pane });
