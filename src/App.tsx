import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { setTrayStatus } from "./api";
import { readiness, tabFlags } from "./readiness";
import AboutSection from "./sections/AboutSection";
import CleanupSection from "./sections/CleanupSection";
import GeneralSection from "./sections/GeneralSection";
import HistorySection from "./sections/HistorySection";
import HomeSection from "./sections/HomeSection";
import HotkeysSection from "./sections/HotkeysSection";
import PrivacySection from "./sections/PrivacySection";
import ProviderSection from "./sections/ProviderSection";
import SpeechSection from "./sections/SpeechSection";
import WordsSection from "./sections/WordsSection";
import { AppContextProvider, useApp, useAppState, type TabId } from "./state";
import {
  IconChevron,
  IconChip,
  IconCleanup,
  IconHistory,
  IconHome,
  IconHotkey,
  IconPlug,
  IconSettings,
  IconShield,
  IconSliders,
  IconWords,
  Logo,
} from "./ui";
import Welcome from "./Welcome";

const SETTINGS: { id: TabId; label: string; icon: ReactNode }[] = [
  { id: "hotkeys", label: "Hotkeys", icon: <IconHotkey /> },
  { id: "speech", label: "Speech", icon: <IconChip /> },
  { id: "cleanup", label: "Cleanup", icon: <IconCleanup /> },
  { id: "provider", label: "AI provider", icon: <IconPlug /> },
  { id: "privacy", label: "Privacy", icon: <IconShield /> },
  { id: "general", label: "General", icon: <IconSliders /> },
];

/** Every section in sidebar order, for the arrow keys. */
const ORDER: TabId[] = ["home", "history", "words", "hotkeys", "speech", "cleanup", "provider", "privacy", "general", "about"];

/** Whether the settings group is open, kept between launches. Open unless folded. */
const FOLD_KEY = "utter.settingsFolded";
function savedOpen(): boolean {
  try {
    return localStorage.getItem(FOLD_KEY) !== "1";
  } catch {
    return true;
  }
}

/** True while the window is narrow enough for the sidebar to fold to icons (see app.css). */
function useNarrow(): boolean {
  const query = "(max-width: 679px)";
  const [narrow, setNarrow] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const media = window.matchMedia(query);
    const onChange = () => setNarrow(media.matches);
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, []);
  return narrow;
}

export default function App() {
  const app = useAppState();
  if (!app) return null;
  return (
    <AppContextProvider.Provider value={app}>
      {/* First launch: introduce Utter and get it working before anything else. */}
      {app.settings.onboarded ? <MainWindow /> : <Welcome />}
    </AppContextProvider.Provider>
  );
}

function MainWindow() {
  const app = useApp();
  const { tab, setTab, updateInfo } = app;
  const ready = readiness(app);
  const flags = tabFlags(app);
  const [settingsOpen, setSettingsOpen] = useState(savedOpen);
  // Folded to icons there is no group header to unfold with, so every
  // settings page shows.
  const narrow = useNarrow();
  const groupOpen = settingsOpen || narrow;
  const navRefs = useRef<Partial<Record<TabId, HTMLButtonElement | null>>>({});
  const main = useRef<HTMLElement>(null);

  // The menu bar says what the status card says.
  const trayKey = `${ready.tone}|${ready.title}|${ready.detail}|${ready.action}|${ready.tab}`;
  useEffect(() => {
    setTrayStatus({
      title: ready.title,
      detail: ready.tone === "" ? `${ready.detail} in any app` : ready.detail,
      tone: ready.tone === "note" ? "" : ready.tone,
      action: ready.action,
      fix: ready.tab,
    }).catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [trayKey]);

  useEffect(() => {
    try {
      localStorage.setItem(FOLD_KEY, settingsOpen ? "0" : "1");
    } catch {
      // Remembering is a nicety; the group still works without it.
    }
  }, [settingsOpen]);

  // A new section starts at the top.
  useEffect(() => {
    main.current?.scrollTo({ top: 0 });
  }, [tab]);

  // Up and down arrows move through the sidebar, as in a macOS source list.
  const onNavKey = (e: KeyboardEvent) => {
    const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (!step || !(e.target instanceof HTMLElement) || !e.target.dataset.section) return;
    e.preventDefault();
    const shown = (id: TabId) => groupOpen || !!flags[id] || tab === id;
    const visible = ORDER.filter((id) => !SETTINGS.some((s) => s.id === id) || shown(id));
    // Move from the item that has focus, which is not always the one selected.
    const at = visible.indexOf(e.target.dataset.section as TabId);
    const next = visible[(at + step + visible.length) % visible.length];
    setTab(next);
    navRefs.current[next]?.focus();
  };

  const nav = (id: TabId, label: string, icon: ReactNode | null, tree = false) => (
    <button
      key={id}
      ref={(el) => {
        navRefs.current[id] = el;
      }}
      data-section={id}
      className={`qnav ${tab === id ? "sel" : ""}`}
      aria-current={tab === id ? "page" : undefined}
      title={label}
      onClick={() => setTab(id)}
    >
      {tree ? <span className="ico">{icon}</span> : icon}
      <span className="lbl">{label}</span>
      {flags[id] && (
        <span className={`qbdg ${flags[id]}`} aria-label="Needs you">
          !
        </span>
      )}
    </button>
  );

  return (
    <div className="qwin">
      {/* The title bar is hidden; this strip drags the window. */}
      <div className="qdrag" data-tauri-drag-region />
      <nav className="qside" aria-label="Sections" onKeyDown={onNavKey}>
        <div className="qbrand" data-tauri-drag-region>
          <Logo />
          <span>Utter</span>
        </div>
        {nav("home", "Home", <IconHome />)}
        {nav("history", "History", <IconHistory />)}
        {nav("words", "Words", <IconWords />)}
        <button
          className="qnav on qgroup"
          aria-expanded={settingsOpen}
          onClick={() => setSettingsOpen((open) => !open)}
        >
          <IconSettings />
          <span className="lbl">Settings</span>
          <IconChevron />
        </button>
        {(groupOpen || SETTINGS.some((s) => flags[s.id] || tab === s.id)) && (
          <div className="qtree">
            {SETTINGS.filter((s) => groupOpen || flags[s.id] || tab === s.id).map((s) => nav(s.id, s.label, s.icon, true))}
          </div>
        )}
        <button
          className="qstat"
          aria-label={`${ready.title}. ${ready.detail}`}
          title={`${ready.title}. ${ready.detail}`}
          disabled={!ready.tab}
          onClick={() => ready.tab && setTab(ready.tab)}
        >
          <span className="r">
            <span className={`d ${ready.tone === "note" ? "" : ready.tone}`} />
            <span className="t">{ready.title}</span>
          </span>
          <span className={`s ${ready.tone === "bad" || ready.tone === "pe" ? ready.tone : ""}`}>{ready.detail}</span>
          {ready.bar !== null && (
            <span className="bar">
              <b style={{ width: `${Math.round(ready.bar * 100)}%` }} />
            </span>
          )}
        </button>
        <button
          ref={(el) => {
            navRefs.current.about = el;
          }}
          data-section="about"
          className={`qabout ${tab === "about" ? "sel" : ""}`}
          aria-current={tab === "about" ? "page" : undefined}
          onClick={() => setTab("about")}
        >
          <span className="lbl">About</span>
          <span className="sp" />
          {updateInfo?.newer && <span className="dot" aria-label="Update available" />}
          {updateInfo?.current ?? ""}
        </button>
      </nav>

      <main className="qmain" ref={main} aria-label={tab}>
        {tab === "home" && <HomeSection />}
        {tab === "history" && <HistorySection />}
        {tab === "words" && <WordsSection />}
        {tab === "hotkeys" && <HotkeysSection />}
        {tab === "speech" && <SpeechSection />}
        {tab === "cleanup" && <CleanupSection />}
        {tab === "provider" && <ProviderSection />}
        {tab === "privacy" && <PrivacySection />}
        {tab === "general" && <GeneralSection />}
        {tab === "about" && <AboutSection />}
      </main>
    </div>
  );
}
