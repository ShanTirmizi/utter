// "Is it ready?" The status card at the foot of the sidebar shows the single
// most important thing, the section that fixes it carries a ! badge, and the
// menu bar shows the same words. One table for all three.
import { hotkeyLabel } from "./hotkey";
import { shortName, type AppState, type TabId } from "./state";

export type Readiness = {
  /** "" ready (mint), "live" under way, "bad" dictation can't happen, "pe" needs you, "note" ready with a caveat. */
  tone: "" | "live" | "bad" | "pe" | "note";
  title: string;
  /** The second line: what it means, or what to do. */
  detail: string;
  /** The section that fixes it, opened when the card is clicked. */
  tab: TabId | null;
  /** Download progress, 0 to 1, when the card shows a bar. */
  bar: number | null;
  /** The menu bar item that fixes it. */
  action: string | null;
};

/** Why cleanup cannot run, or null when it can (or is switched off). */
export function cleanupGap(app: AppState): string | null {
  const { settings, provider, hasKey } = app;
  if (!provider) return null;
  if (provider.id === "custom" && !settings.baseUrl) return "a URL";
  // `hasKey` is null while the Keychain is still being asked: not a gap yet.
  if (provider.needsKey && hasKey === false) return "a key";
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
  const base = { bar: null, action: null };

  // What is happening this second outranks what needs fixing.
  if (app.capturing) return { ...base, tone: "live", title: "Recording hotkey", detail: "esc cancels", tab: null };
  if (live.phase === "recording") {
    return {
      ...base,
      tone: "live",
      title: "Listening",
      detail: live.handsFree ? "Hands-free" : live.mode === "rewrite" ? "Rewrite" : "Let go to insert",
      tab: null,
    };
  }
  if (live.phase === "processing") return { ...base, tone: "live", title: "Working", detail: live.app?.name ?? "", tab: null };

  // Then the first of these that applies. Red only when dictation can't happen.
  if (permissions && permissions.microphone !== "granted") {
    return { ...base, tone: "bad", title: "Allow the microphone", detail: "Dictation can’t start", tab: "general", action: "Allow Microphone…" };
  }
  if (app.hotkeyProblem) {
    return { ...base, tone: "bad", title: "A hotkey isn’t working", detail: "Fix it in Hotkeys", tab: "hotkeys", action: "Fix Hotkey…" };
  }
  const model = selectedModel(app);
  if (progress && !model?.downloaded) {
    const busy = app.models.find((m) => m.name === progress.name);
    const total = progress.total || (busy ? busy.sizeMb * 1_000_000 : 0);
    const fraction = total > 0 ? Math.min(1, progress.received / total) : 0;
    return {
      ...base,
      tone: "live",
      title: `Downloading ${busy ? shortName(busy.label) : "a model"}`,
      detail: `${Math.round(fraction * 100)}%`,
      tab: "speech",
      bar: fraction,
    };
  }
  if (!progress && model && !model.downloaded) {
    return { ...base, tone: "pe", title: "Download a speech model", detail: "Dictation can’t start yet", tab: "speech", action: "Download a Speech Model…" };
  }
  if (settings.cleanupEnabled) {
    const gap = cleanupGap(app);
    if (gap) return { ...base, tone: "pe", title: `Cleanup needs ${gap}`, detail: "Words go in as heard", tab: "provider", action: "Set Up Cleanup…" };
    if (app.cleanupFault) return { ...base, tone: "pe", title: "Cleanup can’t connect", detail: "Words go in as heard", tab: "provider", action: "Fix Cleanup…" };
    if (lastCleanupFailed(app)) return { ...base, tone: "pe", title: "Cleanup failed", detail: "Words went in as heard", tab: "provider", action: "Fix Cleanup…" };
  }
  if (permissions && !permissions.accessibility) {
    return { ...base, tone: "note", title: "Ready", detail: "You paste with ⌘V", tab: "general" };
  }
  return { ...base, tone: "", title: "Ready", detail: `Hold ${hotkeyLabel(settings.hotkey)}`, tab: null };
}

/** The ! badge on each section with something to fix: "bad" blocks dictation, "pe" does not. */
export function tabFlags(app: AppState): Partial<Record<TabId, "bad" | "pe">> {
  const flags: Partial<Record<TabId, "bad" | "pe">> = {};
  const { permissions, settings } = app;
  if (permissions && permissions.microphone !== "granted") flags.general = "bad";
  if (app.hotkeyProblem) flags.hotkeys = "bad";
  const model = selectedModel(app);
  if (model && !model.downloaded && !app.progress) flags.speech = "pe";
  if (app.downloadFault && !model?.downloaded) flags.speech = "pe";
  if (settings.cleanupEnabled && (cleanupGap(app) || app.cleanupFault || lastCleanupFailed(app))) {
    flags.provider = "pe";
  }
  return flags;
}
