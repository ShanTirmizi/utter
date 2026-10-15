// Everything the main window knows about the app, gathered in one place so the
// title bar, the tabs and every section agree on whether Utter is ready.
import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  downloadModel,
  getSettings,
  hasApiKey,
  history as loadHistory,
  hotkeyProblem as loadHotkeyProblem,
  lastDictation,
  modelStatuses,
  pauseHotkeys,
  permissionStatus,
  resumeHotkeys,
  providers as loadProviders,
  saveSettings,
  type AppContext,
  type AudioLevel,
  type Dictation,
  type DownloadProgress,
  type Learned,
  type Mode,
  type ModelStatus,
  type PermissionStatus,
  type PipelineUpdate,
  type Provider,
  type Settings,
  type Stage,
} from "./api";

export type TabId = "try" | "words" | "hotkey" | "model" | "cleanup" | "permissions";

/** What the dictation pipeline is doing right now. */
export type Live = {
  phase: PipelineUpdate["phase"];
  mode: Mode;
  /** A tap started this recording; it runs until the next tap. */
  handsFree: boolean;
  stage: Stage | null;
  app: AppContext | null;
  message: string | null;
  /** When this phase began, from `performance.now()`. */
  since: number;
};

export type DownloadFault = { name: string; message: string; received: number; total: number };

export type AppState = {
  settings: Settings;
  /** Save a partial change. Rejects, leaving settings untouched, if the backend refuses it. */
  update: (change: Partial<Settings>) => Promise<void>;
  /** Take settings the backend has just saved by itself. */
  adopt: (saved: Settings) => void;

  permissions: PermissionStatus | null;

  models: ModelStatus[];
  progress: DownloadProgress | null;
  downloadFault: DownloadFault | null;
  download: (name: string) => Promise<void>;

  catalog: Provider[];
  provider: Provider | undefined;
  /**
   * Whether a key is stored for the chosen provider: null until the Keychain
   * has answered. Never the key itself.
   */
  hasKey: boolean | null;
  setHasKey: (has: boolean) => void;
  /** Why the last cleanup test failed, until one passes or the setup changes. */
  cleanupFault: string | null;
  setCleanupFault: (fault: string | null) => void;

  live: Live;
  dictation: Dictation | null;
  /** Past dictations kept on this Mac, newest first. */
  history: Dictation[];
  refreshHistory: () => Promise<void>;
  /** True once a dictation has finished while this window was open. */
  fresh: boolean;

  /** Why a hotkey is not working, when macOS refused to register one. */
  hotkeyProblem: string | null;

  /** Which hotkey is being re-recorded, if any. */
  capturing: Mode | null;
  setCapturing: (which: Mode | null) => void;

  tab: TabId;
  setTab: (tab: TabId) => void;
  /** Goes up each time "Add a Word…" is chosen from the menu bar. */
  addWordAsked: number;
  /** Words learned from corrections since the window opened, newest first. */
  justLearned: Learned[];
};

const IDLE: Live = { phase: "idle", mode: "tidy", handsFree: false, stage: null, app: null, message: null, since: 0 };
const TABS: TabId[] = ["try", "words", "hotkey", "model", "cleanup", "permissions"];

/** `pnpm dev` only: "?tab=cleanup" opens the window on that section. */
function firstTab(): TabId {
  if (import.meta.env.DEV) {
    const wanted = new URLSearchParams(window.location.search).get("tab") as TabId | null;
    if (wanted && TABS.includes(wanted)) return wanted;
  }
  return "try";
}

/** Load and track everything. Returns null until the settings have arrived. */
export function useAppState(): AppState | null {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [permissions, setPermissions] = useState<PermissionStatus | null>(null);
  const [models, setModels] = useState<ModelStatus[]>([]);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [downloadFault, setDownloadFault] = useState<DownloadFault | null>(null);
  const [catalog, setCatalog] = useState<Provider[]>([]);
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  const [hotkeyProblem, setHotkeyProblem] = useState<string | null>(null);
  const [cleanupFault, setCleanupFault] = useState<string | null>(null);
  const [live, setLive] = useState<Live>(IDLE);
  const [dictation, setDictation] = useState<Dictation | null>(null);
  const [past, setPast] = useState<Dictation[]>([]);
  const [fresh, setFresh] = useState(false);
  const [capturing, setCapturing] = useState<Mode | null>(null);
  const [tab, setTabNow] = useState<TabId>(firstTab);
  const [addWordAsked, setAddWordAsked] = useState(0);
  const [justLearned, setJustLearned] = useState<Learned[]>([]);
  // Leaving a section abandons a hotkey recording begun there.
  const setTab = useCallback((next: TabId) => {
    setCapturing(null);
    setTabNow(next);
  }, []);
  const lastProgress = useRef<DownloadProgress | null>(null);
  // The model being downloaded, so a late progress event cannot look like a new download.
  const downloading = useRef<string | null>(null);
  // Saves run one after another, each starting from what the last one stored.
  const latest = useRef<Settings | null>(null);
  const saving = useRef<Promise<unknown>>(Promise.resolve());

  const refreshModels = useCallback(() => modelStatuses().then(setModels), []);
  // The last dictation comes along, since forgetting or clearing can change it.
  const refreshHistory = useCallback(
    () =>
      Promise.all([loadHistory(), lastDictation()]).then(
        ([entries, last]) => {
          setPast(entries);
          setDictation(last);
        },
        () => undefined,
      ),
    [],
  );

  useEffect(() => {
    getSettings().then((loaded) => {
      latest.current = loaded;
      setSettings(loaded);
    });
    loadProviders().then(setCatalog);
    loadHotkeyProblem().then(setHotkeyProblem);
    refreshHistory();
    refreshModels();

    const stopPipeline = listen<PipelineUpdate>("pipeline", ({ payload }) => {
      setLive((before) => ({
        phase: payload.phase,
        mode: payload.mode,
        handsFree: !!payload.handsFree,
        stage: payload.stage ?? null,
        app: payload.app ?? null,
        message: payload.message,
        // Moving from transcribing to tidying is still the same wait.
        since: before.phase === payload.phase ? before.since : performance.now(),
      }));
      if (payload.dictation) {
        setDictation(payload.dictation);
        setFresh(true);
        // Cleanup just worked, so an earlier failed test no longer describes it.
        if (payload.dictation.cleaned) setCleanupFault(null);
        refreshHistory();
      }
    });
    const stopDownload = listen<DownloadProgress>("model-download", ({ payload }) => {
      if (downloading.current !== payload.name) return;
      lastProgress.current = payload;
      setProgress(payload);
    });
    // "Add a Word…" in the menu bar opens the word list, ready to type into.
    const stopAddWord = listen("add-word", () => {
      setCapturing(null);
      setTabNow("words");
      setAddWordAsked((n) => n + 1);
    });
    // A word learned or taken back changed the saved settings behind the
    // window's back; read them again.
    const reload = () =>
      getSettings().then((loaded) => {
        latest.current = loaded;
        setSettings(loaded);
      });
    const same = (a: Learned, b: Learned) => a.word === b.word && a.heard === b.heard;
    const stopLearned = listen<Learned>("learned", ({ payload }) => {
      reload();
      setJustLearned((old) => [payload, ...old.filter((l) => !same(l, payload))].slice(0, 5));
    });
    const stopSuggested = listen("suggested", () => reload());
    const stopUnlearned = listen<Learned>("unlearned", ({ payload }) => {
      reload();
      setJustLearned((old) => old.filter((l) => !same(l, payload)));
    });
    return () => {
      stopPipeline.then((unlisten) => unlisten());
      stopDownload.then((unlisten) => unlisten());
      stopAddWord.then((unlisten) => unlisten());
      stopLearned.then((unlisten) => unlisten());
      stopUnlearned.then((unlisten) => unlisten());
      stopSuggested.then((unlisten) => unlisten());
    };
  }, [refreshModels, refreshHistory]);

  // Permissions change outside the app, so ask the system once a second.
  // Whether the hotkeys work can change with them (the fn key starts working
  // a moment after Accessibility is allowed), so ask about that too.
  useEffect(() => {
    let alive = true;
    const check = () => {
      permissionStatus().then((s) => alive && setPermissions(s));
      loadHotkeyProblem().then((problem) => alive && setHotkeyProblem(problem), () => undefined);
    };
    check();
    const timer = setInterval(check, 1000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);

  // Each provider has its own key.
  const providerId = settings?.provider;
  useEffect(() => {
    if (!providerId) return;
    setCleanupFault(null);
    // Unknown until the Keychain answers; not "no key".
    setHasKey(null);
    let current = true;
    hasApiKey(providerId).then((has) => current && setHasKey(has));
    return () => {
      current = false;
    };
  }, [providerId]);

  // While a new hotkey is being recorded the existing ones are switched off.
  // Otherwise pressing one would start a dictation instead of being captured.
  const recordingHotkey = capturing !== null;
  useEffect(() => {
    if (!recordingHotkey) return;
    pauseHotkeys().catch(() => undefined);
    // Clicking away from the window abandons the recording.
    const cancel = () => setCapturing(null);
    window.addEventListener("blur", cancel);
    return () => {
      window.removeEventListener("blur", cancel);
      resumeHotkeys().then(setHotkeyProblem, () => undefined);
    };
  }, [recordingHotkey]);

  const update = useCallback((change: Partial<Settings>) => {
    const save = async () => {
      if (!latest.current) return;
      try {
        const stored = await saveSettings({ ...latest.current, ...change });
        latest.current = stored;
        setSettings(stored);
      } finally {
        // A save may have changed which hotkeys are live, or failed to.
        if ("hotkey" in change || "rewriteHotkey" in change) {
          loadHotkeyProblem().then(setHotkeyProblem, () => undefined);
        }
      }
    };
    // Wait for any save already under way, whether it worked or not.
    const done = saving.current.then(save, save);
    saving.current = done.catch(() => undefined);
    return done;
  }, []);

  const adopt = useCallback((saved: Settings) => {
    latest.current = saved;
    setSettings(saved);
  }, []);

  const download = useCallback(
    async (name: string) => {
      setDownloadFault(null);
      downloading.current = name;
      lastProgress.current = { name, received: 0, total: 0 };
      setProgress(lastProgress.current);
      try {
        await downloadModel(name);
      } catch (err) {
        const reached = lastProgress.current ?? { received: 0, total: 0 };
        setDownloadFault({ name, message: String(err), received: reached.received, total: reached.total });
      }
      // Learn that the model is there before dropping the progress bar, so the
      // window never flashes "Download a speech model" in between.
      await refreshModels().catch(() => undefined);
      downloading.current = null;
      setProgress(null);
    },
    [refreshModels],
  );

  if (!settings) return null;

  return {
    settings,
    update,
    adopt,
    permissions,
    models,
    progress,
    downloadFault,
    download,
    catalog,
    provider: catalog.find((p) => p.id === settings.provider),
    hasKey,
    setHasKey,
    cleanupFault,
    setCleanupFault,
    hotkeyProblem,
    live,
    dictation,
    history: past,
    refreshHistory,
    fresh,
    capturing,
    setCapturing,
    tab,
    setTab,
    addWordAsked,
    justLearned,
  };
}

export const AppContextProvider = createContext<AppState | null>(null);

export function useApp(): AppState {
  const app = useContext(AppContextProvider);
  if (!app) throw new Error("useApp must be used inside the app window");
  return app;
}

/** Milliseconds since `since`, counting up while `active`. */
export function useElapsed(active: boolean, since: number): number {
  const [now, setNow] = useState(() => performance.now());
  useEffect(() => {
    if (!active) return;
    setNow(performance.now());
    const timer = setInterval(() => setNow(performance.now()), 100);
    return () => clearInterval(timer);
  }, [active, since]);
  return active ? Math.max(0, now - since) : 0;
}

const WAVE_FLOOR = 0.06;

/**
 * Recent microphone levels, oldest first, while `active`. Levels rise fast and
 * fall slower, so speech looks lively without flickering.
 */
export function useLevels(active: boolean, count: number): number[] {
  const [levels, setLevels] = useState<number[]>(() => new Array(count).fill(WAVE_FLOOR));
  const smooth = useRef(WAVE_FLOOR);

  useEffect(() => {
    smooth.current = WAVE_FLOOR;
    setLevels(new Array(count).fill(WAVE_FLOOR));
    if (!active) return;
    const stop = listen<AudioLevel>("audio-level", ({ payload }) => {
      const target = Math.min(1, Math.max(0, payload));
      const pull = target > smooth.current ? 0.7 : 0.35;
      smooth.current += (target - smooth.current) * pull;
      const next = Math.max(WAVE_FLOOR, smooth.current);
      setLevels((old) => [...old.slice(1), next]);
    });
    return () => {
      stop.then((unlisten) => unlisten());
    };
  }, [active, count]);

  return levels;
}

/** True when the system asks for less motion. */
export function useReducedMotion(): boolean {
  const query = "(prefers-reduced-motion: reduce)";
  const [reduced, setReduced] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const media = window.matchMedia(query);
    const onChange = () => setReduced(media.matches);
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, []);
  return reduced;
}

/** "Base (English)" -> "Base" */
export function shortName(label: string): string {
  return label.replace(/\s*\(.*\)\s*$/, "");
}
