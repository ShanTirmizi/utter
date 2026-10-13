import { useCallback, useEffect, useState } from "react";
import { getSettings, saveSettings, type Settings } from "./api";
import CleanupSection from "./sections/CleanupSection";
import HotkeySection from "./sections/HotkeySection";
import ModelSection from "./sections/ModelSection";
import PermissionsSection from "./sections/PermissionsSection";
import TrySection from "./sections/TrySection";
import Welcome from "./Welcome";
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

  // First launch: explain the permissions before anything else.
  if (!settings.onboarded) {
    return <Welcome onDone={() => update({ onboarded: true })} />;
  }

  return (
    <main className="page">
      <header>
        <h1>Utter</h1>
        <p className="muted">Hold a hotkey, speak, release.</p>
      </header>
      <TrySection hotkey={settings.hotkey} />
      <HotkeySection hotkey={settings.hotkey} onChange={(hotkey) => update({ hotkey })} />
      <ModelSection selected={settings.model} onSelect={(model) => update({ model })} />
      <CleanupSection
        enabled={settings.cleanupEnabled}
        onToggle={(cleanupEnabled) => update({ cleanupEnabled })}
      />
      <PermissionsSection />
    </main>
  );
}
