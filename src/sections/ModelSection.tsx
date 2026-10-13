import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  downloadModel,
  modelStatuses,
  type DownloadProgress,
  type ModelStatus,
} from "../api";

type Props = {
  selected: string;
  onSelect: (name: string) => void;
};

export default function ModelSection({ selected, onSelect }: Props) {
  const [models, setModels] = useState<ModelStatus[]>([]);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => modelStatuses().then(setModels), []);

  useEffect(() => {
    refresh();
    const stop = listen<DownloadProgress>("model-download", (e) => setProgress(e.payload));
    return () => {
      stop.then((unlisten) => unlisten());
    };
  }, [refresh]);

  const download = async (name: string) => {
    setError(null);
    setProgress({ name, received: 0, total: 0 });
    try {
      await downloadModel(name);
    } catch (err) {
      setError(String(err));
    }
    setProgress(null);
    refresh();
  };

  return (
    <section className="card">
      <h2>Speech model</h2>
      <p className="muted">
        Transcription runs on this Mac. Models are downloaded once from Hugging Face.
      </p>
      <ul className="models">
        {models.map((m) => {
          const busy = progress?.name === m.name;
          const percent =
            busy && progress.total > 0
              ? Math.round((progress.received / progress.total) * 100)
              : 0;
          return (
            <li key={m.name}>
              <label>
                <input
                  type="radio"
                  name="model"
                  checked={selected === m.name}
                  onChange={() => onSelect(m.name)}
                />
                <span>
                  <strong>{m.label}</strong>
                  <span className="muted small">
                    {" "}
                    · {m.sizeMb} MB · {m.note}
                  </span>
                </span>
              </label>
              {m.downloaded ? (
                <span className="ok small">Downloaded</span>
              ) : busy ? (
                <span className="small">{percent}%</span>
              ) : (
                <button disabled={progress !== null} onClick={() => download(m.name)}>
                  Download
                </button>
              )}
            </li>
          );
        })}
      </ul>
      {models.some((m) => m.name === selected && !m.downloaded) && (
        <p className="warn small">Download the selected model to start dictating.</p>
      )}
      {error && <p className="error small">{error}</p>}
    </section>
  );
}
