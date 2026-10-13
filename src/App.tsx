import { useCallback, useEffect, useState } from "react";
import { getSettings, saveSettings, type Settings } from "./api";
import CleanupSection from "./sections/CleanupSection";
import HotkeySection from "./sections/HotkeySection";
import "./App.css";

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    getSettings().then(setSettings);
  }, []);

  // Save a partial change and keep the screen in step with what was stored.
  const update = useCallback(
    async (change: Partial<Settings>) => {
      if (!settings) return;
      setSettings(await saveSettings({ ...settings, ...change }));
    },
    [settings],
  );

  if (!settings) return null;

  return (
    <main className="page">
      <header>
        <h1>Utter</h1>
        <p className="muted">Hold a hotkey, speak, release.</p>
      </header>
      <HotkeySection hotkey={settings.hotkey} onChange={(hotkey) => update({ hotkey })} />
      <CleanupSection
        enabled={settings.cleanupEnabled}
        onToggle={(cleanupEnabled) => update({ cleanupEnabled })}
      />
    </main>
  );
}
