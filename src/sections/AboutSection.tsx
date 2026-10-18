import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { checkForUpdates } from "../api";
import { useApp } from "../state";
import { IconOut, Logo, Notice, Switch, when } from "../ui";

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
  const version = update?.current ?? "";

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
            <div className="qrowb">
              <button className="qbtn pri" onClick={() => openUrl(update.url ?? `${REPO}/releases/latest`)}>
                Get {update.latest}
              </button>
              <span className="qnote" style={{ fontSize: 12.5, marginLeft: 4 }}>
                Opens the release on GitHub
              </span>
            </div>
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
