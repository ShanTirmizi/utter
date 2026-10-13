// Typed wrappers around the Rust commands. The UI talks to the backend only
// through this file.
import { invoke } from "@tauri-apps/api/core";

export type Settings = {
  hotkey: string;
  cleanupEnabled: boolean;
  model: string;
  onboarded: boolean;
};

export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) =>
  invoke<Settings>("save_settings", { settings });

// The key goes one way only: into the Keychain. It is never read back.
export const hasApiKey = () => invoke<boolean>("has_api_key");
export const setApiKey = (key: string) => invoke<void>("set_api_key", { key });
export const deleteApiKey = () => invoke<void>("delete_api_key");
/** Runs one fixed sentence through cleanup and returns the result. */
export const testCleanup = () => invoke<string>("test_cleanup");

// Progress reports from the dictation pipeline, sent on the "pipeline" event.
export type Dictation = {
  /** Exactly what the speech model heard. */
  raw: string;
  /** What Utter produced from it. */
  text: string;
  /** True when the text went through the cleanup model. */
  cleaned: boolean;
  note: string | null;
  app: AppContext;
  transcribeMs: number;
  cleanupMs: number | null;
};

export type AppContext = {
  name: string;
  bundleId: string;
  category: "chat" | "email" | "code" | "notes" | "assistant" | "other";
};

export type PipelineUpdate = {
  phase: "idle" | "recording" | "processing" | "done" | "error";
  message: string | null;
  dictation: Dictation | null;
};

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
export const openPrivacySettings = (pane: "microphone" | "accessibility") =>
  invoke<void>("open_privacy_settings", { pane });
