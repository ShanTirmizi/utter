import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { checkForUpdates, installUpdate, type UpdateProgress } from "../api";
import { useApp } from "../state";
import { IconOut, Logo, Notice, Switch, WaveProgress, when } from "../ui";

const REPO = "https://github.com/ShanTirmizi/utter";

/** "Checked today at 4:12 pm" */
function checked(at: number): string {
  if (!at) return "";
  const time = when(at);
  return new Date(at).toDateString() === new Date().toDateString() ? `Checked today at ${time}` : `Checked ${time}`;
}

/** Version, updates, and where to find the source. */
export default function AboutSection() {
  const app = useApp();
  const { updateInfo: update, setUpdateInfo: setUpdate, settings } = app;
  const [checking, setChecking] = useState(false);
  // While an update downloads: how far it has got. Utter relaunches when it's in.
  const [installing, setInstalling] = useState<UpdateProgress | null>(null);
  const [installError, setInstallError] = useState<string | null>(null);
  const version = update?.current ?? "";

  useEffect(() => {
    if (!installing) return;
    const stop = listen<UpdateProgress>("update-progress", ({ payload }) => setInstalling(payload));
    return () => {
      stop.then((unlisten) => unlisten());
    };
  }, [installing !== null]); // eslint-disable-line react-hooks/exhaustive-deps

  const install = () => {
    setInstallError(null);
    setInstalling({ received: 0, total: null });
    // Resolves only if it failed: on success Utter quits and the new one opens.
    installUpdate().catch((err) => {
      setInstalling(null);
      setInstallError(String(err));
    });
  };
  const releaseUrl = update?.url ?? `${REPO}/releases/latest`;
  const fraction = installing?.total ? Math.min(1, installing.received / installing.total) : 0;

  const check = () => {
    setChecking(true);
    checkForUpdates()
      .then(setUpdate)
      .catch(() => undefined)
      .finally(() => setChecking(false));
  };

  return (
    <>
      <div className="qcard" style={{ display: "flex", alignItems: "center", gap: 18, padding: 22 }}>
        <Logo size={72} />
        <div>
          <h1 style={{ margin: 0, font: "700 32px/36px var(--qfd)", letterSpacing: "-.03em" }}>Utter</h1>
          <p className="qp" style={{ marginTop: 4 }}>
            Version {version}, beta
          </p>
        </div>
      </div>

      <div className="qcard">
        <div className="qlab">
          Updates<span className="r">{checking ? "Checking…" : checked(update?.checkedAt ?? 0)}</span>
        </div>
        {update?.newer && update.latest ? (
          <>
            <div className="qbig sm">{update.latest} is ready</div>
            {update.summary && (
              <p className="qp">
                {update.summary}{" "}
                {update.url && (
                  <button className="qlink" onClick={() => openUrl(update.url!)}>
                    What’s new
                  </button>
                )}
              </p>
            )}
            {installing ? (
              <>
                <WaveProgress fraction={fraction} label={`Utter ${update.latest} download`} />
                <div className="qtags">
                  <span className="qtag nt">{installing.total ? `${Math.round(fraction * 100)}%` : "Starting"}</span>
                  <span className="qnote">
                    {fraction >= 1 ? "Installing. Utter restarts in a moment." : "Utter restarts when it’s in."}
                  </span>
                </div>
              </>
            ) : (
              <div className="qrowb">
                <button className="qbtn pri" onClick={install}>
                  Install and restart
                </button>
                <span className="qnote" style={{ fontSize: 12.5, marginLeft: 4 }}>
                  Takes a few seconds. Your settings and words stay as they are.
                </span>
              </div>
            )}
            {installError && (
              <Notice
                tone="pe"
                action={
                  <button className="qbtn sm" onClick={() => openUrl(releaseUrl)}>
                    Download it
                  </button>
                }
              >
                <b>Not installed.</b> <span>{installError} You can download {update.latest} from GitHub instead.</span>
              </Notice>
            )}
          </>
        ) : (
          <>
            {!(update?.error && !update.latest) && (
              <div className={`qbig sm ${update?.latest ? "" : "lt"}`}>
                {update?.latest && !update.newer ? "Up to date" : "Not checked yet"}
              </div>
            )}
            {update?.error && (
              <Notice tone="pe">
                <b>Couldn’t check.</b> <span>{update.error}</span>
              </Notice>
            )}
            <div className="qrowb">
              <button className="qbtn" disabled={checking} onClick={check}>
                {checking && <span className="qspin" />}
                Check for updates
              </button>
            </div>
          </>
        )}
        <div className="qsw" style={{ borderTop: "1px solid var(--hair)", marginTop: 14, paddingBottom: 0 }}>
          <div className="tx">
            <b>Check automatically</b>
            <span>Off by default. When on, asks GitHub once a day and sends nothing else.</span>
          </div>
          <Switch
            small
            label="Check automatically"
            checked={settings.checkUpdates}
            onChange={(checkUpdates) => app.update({ checkUpdates }).catch(() => undefined)}
          />
        </div>
      </div>

      <div className="qcard tight">
        <div className="qlist" style={{ margin: 0 }}>
          <div className="qli w">
            <span className="wd">GitHub</span>
            <span className="hv">
              <span className="qnote" style={{ fontSize: 12.5 }}>
                Source, issues and releases
              </span>
            </span>
            <button className="qlink" onClick={() => openUrl(REPO)}>
              Open
              <IconOut />
            </button>
          </div>
          <div className="qli w">
            <span className="wd">Licence</span>
            <span className="hv">
              <span className="qnote" style={{ fontSize: 12.5 }}>
                Free and open source
              </span>
            </span>
            <button className="qlink" onClick={() => openUrl(`${REPO}/blob/main/LICENSE`)}>
              Read it
              <IconOut />
            </button>
          </div>
          <div className="qli w">
            <span className="wd">Privacy</span>
            <span className="hv">
              <span className="qnote" style={{ fontSize: 12.5 }}>
                Your voice never leaves this Mac. Cleanup sends text only, to the provider you pick.
              </span>
            </span>
            <button className="qlink" onClick={() => app.setTab("privacy")}>
              Settings
            </button>
          </div>
        </div>
      </div>
    </>
  );
}
