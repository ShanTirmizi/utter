import { selectedModel } from "../readiness";
import { shortName, useApp } from "../state";
import { IconCheck, Notice, WaveProgress } from "../ui";
import DictionaryCard from "./DictionaryCard";

export default function ModelSection() {
  const app = useApp();
  const selected = selectedModel(app);
  const name = selected ? shortName(selected.label) : "";
  const downloadingSelected = app.progress?.name === selected?.name;

  return (
    <>
      <div className="sechead">
        <h2 className="h">Speech model</h2>
      </div>
      <p className="sub">
        Turns your voice into text, here on this Mac. Bigger is more accurate and a little slower.
      </p>

      {selected && !selected.downloaded && !app.progress && (
        <Notice
          tone="warn"
          action={
            <button className="btn primary sm" onClick={() => app.download(selected.name)}>
              Download {name}
            </button>
          }
        >
          <b>{name} is selected but not downloaded.</b> Dictation won’t work until it is.
        </Notice>
      )}

      <ModelRows />

      {downloadingSelected && (
        <Notice tone="info">
          Dictation starts working when {name} finishes. You can close this window; the download carries on.
        </Notice>
      )}
      <p className="foot">
        Models download once from Hugging Face and are saved on this Mac. Switching takes effect from the
        next dictation.
      </p>

      <DictionaryCard />
    </>
  );
}

/** The three speech models as selectable rows. Shared with the welcome screen. */
export function ModelRows({ suggest = false }: { suggest?: boolean }) {
  const app = useApp();
  const { models, progress, downloadFault, settings } = app;

  return (
    <div className="models" role="radiogroup" aria-label="Speech model">
      {models.map((m, index) => {
        const name = shortName(m.label);
        const selected = settings.model === m.name;
        const busy = progress?.name === m.name;
        const fault = downloadFault?.name === m.name && !m.downloaded ? downloadFault : null;
        const total = (busy && progress.total) || fault?.total || m.sizeMb * 1_000_000;
        const received = busy ? progress.received : (fault?.received ?? 0);
        // "(English)" in the label becomes a plain sentence in the description.
        const englishOnly = /\(English\)/.test(m.label) ? " English only." : "";

        return (
          <div className={`mrow ${selected ? "sel" : ""}`} key={m.name}>
            <div className="mtop">
              <button
                className="pick"
                role="radio"
                aria-checked={selected}
                onClick={() => !selected && app.update({ model: m.name })}
              >
                <span className="radio" />
                <span className="mtext">
                  <span className="mname">
                    {name}
                    <span className="msize">{m.sizeMb} MB</span>
                  </span>
                  <span className="mdesc">
                    {m.note}
                    {englishOnly}
                  </span>
                </span>
              </button>

              {busy ? (
                <span className="chip accent">Downloading</span>
              ) : m.downloaded ? (
                selected ? (
                  <span className="chip ok">
                    <IconCheck width={2.4} />
                    In use
                  </span>
                ) : (
                  <span className="chip plain">Downloaded</span>
                )
              ) : fault ? (
                <button className="btn sm" disabled={!!progress} onClick={() => app.download(m.name)}>
                  Retry
                </button>
              ) : suggest && index === 0 ? (
                <button className="btn primary sm" disabled={!!progress} onClick={() => app.download(m.name)}>
                  Download
                </button>
              ) : (
                <>
                  {selected && !progress && <span className="chip warn">Not downloaded</span>}
                  <button className="btn sm" disabled={!!progress} onClick={() => app.download(m.name)}>
                    Download
                  </button>
                </>
              )}
            </div>

            {(busy || fault) && (
              <div className="dl">
                <WaveProgress fraction={total > 0 ? received / total : 0} failed={!!fault} label={`${name} download`} />
                <span className="dlnum">
                  {Math.round(received / 1_000_000)} of {m.sizeMb} MB
                </span>
              </div>
            )}
            {fault && (
              <div className="mnote">
                <p className="hint" style={{ color: "var(--bad)" }}>
                  <b>Download stopped:</b> {fault.message.replace(/^Download (interrupted|failed): /, "")} Retry starts
                  again from the beginning.
                </p>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
