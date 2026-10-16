import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { checkForUpdates } from "../api";
import { useApp } from "../state";
import { IconOut, Logo, Notice, Segmented, Switch, when } from "../ui";
import { editRate } from "./TrySection";

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
        <div className="qsw" style={{ padding: 0 }}>
          <div className="tx">
            <b>Appearance</b>
            <span>System follows your Mac. Light or Dark keeps Utter that way.</span>
          </div>
          <Segmented
            label="Appearance"
            value={settings.appearance}
            onChange={(appearance) => app.update({ appearance }).catch(() => undefined)}
            options={[
              { id: "system", label: "System" },
              { id: "light", label: "Light" },
              { id: "dark", label: "Dark" },
            ]}
          />
        </div>
      </div>

      <Accuracy />

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
          </div>
        </div>
      </div>
    </>
  );
}

/**
 * How often a dictation went in right: the share you left as typed. Utter
 * can only tell while it follows the text box after pasting, which it does
 * while learning is on or recordings are kept.
 */
function Accuracy() {
  const app = useApp();
  const { settings, history, permissions } = app;
  const [error, setError] = useState<string | null>(null);
  const { followed, untouched } = editRate(history);
  const kept = history.filter((d) => d.recording !== null).length;
  const following =
    (settings.learnWords || (settings.keepAudio && settings.keepHistory)) && permissions?.accessibility !== false;
  // A recording belongs to a history entry, so it can only be switched on
  // with history kept. Switching it off is always allowed.
  const needsHistory = !settings.keepHistory && !settings.keepAudio;

  // Switching it off deletes the recordings, so the history is read again
  // for the count below.
  const setKeepAudio = (keepAudio: boolean) =>
    app.update({ keepAudio }).then(
      () => {
        setError(null);
        return app.refreshHistory();
      },
      (err) => setError(String(err)),
    );

  return (
    <div className="qcard">
      <div className="qlab">
        Accuracy{followed > 0 && <span className="r">Last {followed} dictations</span>}
      </div>
      {followed > 0 ? (
        <>
          <div className="qbig sm">
            {Math.round((untouched / followed) * 100)}
            <small>% left as typed</small>
          </div>
          <p className="qp">
            You changed {followed - untouched} of your last {followed} dictations after Utter pasted them.
          </p>
        </>
      ) : (
        <div className="qbig sm lt">Nothing counted yet</div>
      )}
      {!following && (
        <p className="qp">
          Utter counts your edits only while it follows the text box after pasting: with Learn from my corrections or Keep
          recordings on, and Accessibility allowed.
        </p>
      )}
      <div className="qsw" style={{ borderTop: "1px solid var(--hair)", marginTop: 14, paddingBottom: 0 }}>
        <div className="tx">
          <b>Keep recordings</b>
          <span>
            {settings.keepHistory
              ? "Off by default. Keeps the audio of your last 50 dictations on this Mac, with what you changed, to test other models on. Never uploaded."
              : "Needs Keep history, under Try it: each recording belongs to a dictation there."}
            {settings.keepAudio && kept > 0 && ` ${kept} kept now; switching this off deletes them.`}
          </span>
        </div>
        <Switch
          small
          label="Keep recordings"
          checked={settings.keepAudio}
          disabled={needsHistory}
          onChange={setKeepAudio}
        />
      </div>
      {error && (
        <p className="qp" role="alert" style={{ color: "var(--bad-fg)" }}>
          {error}
        </p>
      )}
    </div>
  );
}
