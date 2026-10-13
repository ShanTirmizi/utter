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

// Progress reports from the dictation pipeline, sent on the "pipeline" event.
export type PipelineUpdate = {
  phase: "idle" | "recording" | "processing" | "done" | "error";
  message: string | null;
};
