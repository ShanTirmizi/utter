import { useRef, type KeyboardEvent, type ReactNode } from "react";
import { readiness, tabFlags } from "./readiness";
import CleanupSection from "./sections/CleanupSection";
import HotkeySection from "./sections/HotkeySection";
import ModelSection from "./sections/ModelSection";
import PermissionsSection from "./sections/PermissionsSection";
import TrySection from "./sections/TrySection";
import { AppContextProvider, useApp, useAppState, type TabId } from "./state";
import { TabIconCleanup, TabIconHotkey, TabIconModel, TabIconPermissions, TabIconTry } from "./ui";
import Welcome from "./Welcome";

const TABS: { id: TabId; label: string; icon: ReactNode }[] = [
  { id: "try", label: "Try it", icon: <TabIconTry /> },
  { id: "hotkey", label: "Hotkey", icon: <TabIconHotkey /> },
  { id: "model", label: "Model", icon: <TabIconModel /> },
  { id: "cleanup", label: "Cleanup", icon: <TabIconCleanup /> },
  { id: "permissions", label: "Permissions", icon: <TabIconPermissions /> },
];

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
  const { tab, setTab } = app;
  const ready = readiness(app);
  const flags = tabFlags(app);
  const tabRefs = useRef<Partial<Record<TabId, HTMLButtonElement | null>>>({});

  // Arrow keys move between tabs, as in any macOS segmented control.
  const onTabKey = (e: KeyboardEvent) => {
    const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const at = TABS.findIndex((t) => t.id === tab);
    const next = TABS[(at + step + TABS.length) % TABS.length].id;
    setTab(next);
    tabRefs.current[next]?.focus();
  };

  return (
    <div className="win">
      {/* The title bar is hidden, so this strip is what drags the window. */}
      <div className="tb" data-tauri-drag-region>
        <button
          className={`state ${ready.tone}`}
          disabled={!ready.tab}
          onClick={() => ready.tab && setTab(ready.tab)}
          aria-label={`${ready.text}${ready.go ? `. ${ready.go}` : ""}`}
        >
          <span className="dot" />
          <span>{ready.text}</span>
          <span className="go">{ready.go}</span>
        </button>
      </div>

      <div className="tabs" role="tablist" aria-label="Sections" onKeyDown={onTabKey}>
        {TABS.map((t) => (
          <button
            key={t.id}
            ref={(el) => {
              tabRefs.current[t.id] = el;
            }}
            className={`tab ${flags[t.id] ?? ""}`}
            role="tab"
            id={`tab-${t.id}`}
            aria-selected={t.id === tab}
            aria-controls="panel"
            tabIndex={t.id === tab ? 0 : -1}
            title={t.label}
            onClick={() => setTab(t.id)}
          >
            {t.icon}
            <span className="tl">{t.label}</span>
          </button>
        ))}
      </div>

      <div className="body" role="tabpanel" id="panel" aria-labelledby={`tab-${tab}`}>
        {tab === "try" && <TrySection />}
        {tab === "hotkey" && <HotkeySection />}
        {tab === "model" && <ModelSection />}
        {tab === "cleanup" && <CleanupSection />}
        {tab === "permissions" && <PermissionsSection />}
      </div>
    </div>
  );
}
