// "Is it ready?" The title-bar capsule shows the single most important thing,
// and the tab that fixes it carries a dot.
import { hotkeyLabel } from "./hotkey";
import { shortName, type AppState, type TabId } from "./state";

export type Readiness = {
  /** Colour of the dot: "" is ready (green). */
  tone: "" | "warn" | "bad" | "busy" | "live";
  text: string;
  /** Orange call to action after the text, such as "Fix". */
  go: string;
  /** The section that fixes it, opened when the capsule is clicked. */
  tab: TabId | null;
};

/** Why cleanup cannot run, or null when it can (or is switched off). */
export function cleanupGap(app: AppState): string | null {
  const { settings, provider, hasKey } = app;
  if (!provider) return null;
  if (provider.id === "custom" && !settings.baseUrl) return "a URL";
  if (provider.needsKey && !hasKey) return "a key";
  if (!(settings.cleanupModel || provider.defaultModel)) return "a model";
  return null;
}

/** True when the last dictation asked for the language model and did not get it. */
export function lastCleanupFailed(app: AppState): boolean {
  const d = app.dictation;
  return !!d && app.fresh && !d.cleaned && !!d.note && /skipped|needs a model/i.test(d.note);
}

export function selectedModel(app: AppState) {
  return app.models.find((m) => m.name === app.settings.model);
}

export function readiness(app: AppState): Readiness {
  const { settings, permissions, live, progress } = app;

  // What is happening this second outranks what needs fixing.
  if (app.capturing) return { tone: "live", text: "Recording a hotkey", go: "Esc cancels", tab: null };
  if (live.phase === "recording") {
    return { tone: "live", text: live.mode === "rewrite" ? "Listening · rewrite" : "Listening", go: "", tab: null };
  }
  if (live.phase === "processing") return { tone: "busy", text: "Working", go: "", tab: null };

  // Then the first of these that applies.
  if (permissions && permissions.microphone !== "granted") {
    return { tone: "bad", text: "Allow the microphone", go: "Fix", tab: "permissions" };
  }
  const model = selectedModel(app);
  if (progress) {
    const busy = app.models.find((m) => m.name === progress.name);
    const total = progress.total || (busy ? busy.sizeMb * 1_000_000 : 0);
    const percent = total > 0 ? Math.round((progress.received / total) * 100) : 0;
    if (!model?.downloaded) {
      return { tone: "busy", text: `Downloading ${busy ? shortName(busy.label) : "model"} · ${percent}%`, go: "", tab: "model" };
    }
  } else if (model && !model.downloaded) {
    return { tone: "warn", text: "Download a speech model", go: "Fix", tab: "model" };
  }
  if (settings.cleanupEnabled) {
    const gap = cleanupGap(app);
    if (gap) return { tone: "warn", text: `Cleanup needs ${gap}`, go: "Fix", tab: "cleanup" };
    if (app.cleanupFault) return { tone: "warn", text: "Cleanup can't connect", go: "Fix", tab: "cleanup" };
    if (lastCleanupFailed(app)) return { tone: "warn", text: "Cleanup failed", go: "Fix", tab: "cleanup" };
  }
  if (permissions && !permissions.accessibility) {
    return { tone: "", text: "Ready · you paste with ⌘V", go: "", tab: "permissions" };
  }
  return { tone: "", text: `Ready · ${hotkeyLabel(settings.hotkey)}`, go: "", tab: null };
}

/** Every section with something to fix: "flagbad" blocks dictation, "flag" does not. */
export function tabFlags(app: AppState): Partial<Record<TabId, "flag" | "flagbad">> {
  const flags: Partial<Record<TabId, "flag" | "flagbad">> = {};
  const { permissions, settings } = app;
  if (permissions && permissions.microphone !== "granted") flags.permissions = "flagbad";
  else if (permissions && !permissions.accessibility) flags.permissions = "flag";
  const model = selectedModel(app);
  if (model && !model.downloaded && !app.progress) flags.model = "flag";
  if (settings.cleanupEnabled && (cleanupGap(app) || app.cleanupFault || lastCleanupFailed(app))) {
    flags.cleanup = "flag";
  }
  return flags;
}
