// Development aid only: lets `pnpm dev` show the settings UI in an ordinary
// browser, where the Rust backend does not exist. Every command gets a canned
// answer. This file is never loaded inside the real app or in production
// builds (see main.tsx).
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { Dictation, ModelStatus, PermissionStatus, Settings } from "./api";

let settings: Settings = {
  hotkey: "Control+Alt+Space",
  cleanupEnabled: true,
  model: "base.en",
  // Open the page with "?welcome" to preview the first-run screen.
  onboarded: !window.location.search.includes("welcome"),
};
let hasKey = false;

const permissions: PermissionStatus = { microphone: "granted", accessibility: false };

const models: ModelStatus[] = [
  { name: "base.en", label: "Base (English)", note: "Fastest. Good for clear speech.", sizeMb: 148, downloaded: true },
  { name: "small.en", label: "Small (English)", note: "More accurate, a little slower.", sizeMb: 488, downloaded: false },
  { name: "large-v3-turbo-q5_0", label: "Large v3 Turbo (multilingual)", note: "Most accurate. Any language.", sizeMb: 574, downloaded: false },
];

const sample: Dictation = {
  raw: "um so can you send me the like the budget spreadsheet before wednesday",
  text: "Can you send me the budget spreadsheet before Wednesday?",
  cleaned: true,
  inserted: true,
  note: null,
  app: { name: "Slack", bundleId: "com.tinyspeck.slackmacgap", category: "chat" },
  transcribeMs: 310,
  cleanupMs: 540,
};

export function installMock() {
  mockWindows("main");
  mockIPC(
    (command, args) => {
      const payload = args as Record<string, unknown> | undefined;
      switch (command) {
        case "get_settings":
          return settings;
        case "save_settings":
          settings = payload?.settings as Settings;
          return settings;
        case "has_api_key":
          return hasKey;
        case "set_api_key":
          hasKey = true;
          return;
        case "delete_api_key":
          hasKey = false;
          return;
        case "test_cleanup":
          return "This is a quick test of the cleanup step.";
        case "permission_status":
          return permissions;
        case "model_statuses":
          return models;
        case "last_dictation":
          return sample;
        default:
          return;
      }
    },
    { shouldMockEvents: true },
  );
}
