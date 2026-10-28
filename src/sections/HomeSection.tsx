import { Fragment, useEffect, useRef, useState, type ReactNode } from "react";
import { copyText, fixWord, type Dictation } from "../api";
import { diffWords } from "../diff";
import { hotkeyLabel } from "../hotkey";
import { readiness } from "../readiness";
import { useApp, useElapsed, useLevels, type AppState } from "../state";
import { WaySwitch } from "./CleanupSection";
import {
  Bars,
  clock,
  IconOkFill,
  IconWarnFill,
  LevelBars,
  Notice,
  Segmented,
  secondsBare,
  seconds,
  when,
} from "../ui";

/** What the dictation was for, in the meta line: "Slack · chat style · 3:38 pm". */
export function meta(d: Dictation): string {
  const what =
    d.selection !== null
      ? "rewrote your selection"
      : cleanupFailed(d)
        ? "cleanup failed"
        : d.mode === "rewrite"
          ? "rewritten"
          : d.cleaned
            ? d.app.category === "other"
              ? "tidied"
              : `${d.app.category} style`
            : "as heard";
  return [d.app.name || null, what, edited(d), when(d.finishedAt)].filter(Boolean).join(" · ");
}

/** "you changed 2 words", once you have finished with it and did. */
function edited(d: Dictation): string | null {
  if (!d.edit || d.edit.changed === 0) return null;
  return `you changed ${d.edit.changed} ${d.edit.changed === 1 ? "word" : "words"}`;
}

function cleanupFailed(d: Dictation): boolean {
  return !d.cleaned && !!d.note && /skipped|needs a model/i.test(d.note);
}

/** "Cleanup skipped: The reply was empty." -> "The reply was empty." */
function reason(note: string | null): string {
  if (!note) return "";
  const found = note.match(/skipped: (.+)$/);
  return found ? found[1] : note;
}

export type Fixed = { d: Dictation; wrong: string; right: string; remembered: boolean };

export default function HomeSection() {
  const app = useApp();
  const { settings, dictation, history, fixLastAsked } = app;
  const earlier = history.filter((d) => d.finishedAt !== dictation?.finishedAt).length;
  // The dictation whose words are being fixed, and the one just fixed.
  const [fixing, setFixing] = useState<Dictation | null>(null);
  const [fixed, setFixed] = useState<Fixed | null>(null);
  const top = useRef<HTMLDivElement>(null);

  // "Fix Last Word…" in the menu bar.
  useEffect(() => {
    if (fixLastAsked && dictation && dictation.selection === null) {
      setFixed(null);
      setFixing(dictation);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fixLastAsked]);

  // A new dictation clears a finished fix.
  const newest = dictation?.finishedAt;
  const seen = useRef(newest);
  useEffect(() => {
    if (seen.current !== newest) {
      seen.current = newest;
      setFixed(null);
    }
  }, [newest]);

  const startFix = (d: Dictation) => {
    setFixed(null);
    setFixing(d);
    top.current?.scrollIntoView({ block: "start" });
  };

  const failed = dictation && app.fresh && cleanupFailed(dictation);

  return (
    <>
      <div className="qhead" ref={top}>
        <div className="t">
          <h1>Home</h1>
          <p>Hold {hotkeyLabel(settings.hotkey)} in any app, or practise here.</p>
        </div>
      </div>

      {failed && !fixing && (
        <Notice
          tone="pe"
          action={
            <button className="qbtn sm" onClick={() => app.setTab("provider")}>
              Open AI provider
            </button>
          }
        >
          <b>Cleanup failed.</b> <span>{reason(dictation.note)} Your words went in as heard.</span>
        </Notice>
      )}

      {!fixing && <PracticeBox />}

      {fixing ? (
        <FixCard
          dictation={fixing}
          onCancel={() => setFixing(null)}
          onFixed={(done) => {
            setFixing(null);
            setFixed(done);
          }}
        />
      ) : fixed ? (
        <FixedCard {...fixed} onDone={() => setFixed(null)} />
      ) : dictation ? (
        <LastDictation dictation={dictation} onFix={() => startFix(dictation)} />
      ) : (
        <div className="qcard">
          <div className="qlab">Last dictation</div>
          <div className="qbig md lt">Nothing yet</div>
          <p className="qp">Dictate anywhere and it shows up here: what Utter heard, what it changed and how long it took.</p>
        </div>
      )}

      {dictation && !fixing && !fixed && <Timings dictation={dictation} />}

      {earlier > 0 && !fixing && (
        <div className="qcard tight">
          <div className="qrow">
            <span>
              <b style={{ font: "600 14px/1 var(--qf)" }}>
                {earlier} earlier {earlier === 1 ? "dictation" : "dictations"}
              </b>
              <span className="qnote" style={{ marginLeft: 6 }}>
                Search, copy and fix them.
              </span>
            </span>
            <span className="sp" />
            <button className="qbtn sm" onClick={() => app.setTab("history")}>
              Open history
            </button>
          </div>
        </div>
      )}
    </>
  );
}

/**
 * The practice box, with a status strip joined to it. The strip is the
 * window's version of the pill: the same bars and the same words.
 */
export function PracticeBox({
  placeholder,
  autoFocus = false,
  hints = true,
}: {
  placeholder?: string;
  autoFocus?: boolean;
  hints?: boolean;
}) {
  const app = useApp();
  const { settings } = app;
  const [value, setValue] = useState("");
  return (
    <div>
      <div className="qprac">
        <label>
          <span className="sr">Practice box</span>
          <textarea
            autoFocus={autoFocus}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder={placeholder ?? `Click here, then hold ${hotkeyLabel(settings.hotkey)} and speak.`}
          />
        </label>
        <Strip app={app} />
      </div>
      {hints && (
        <div className="qhints">
          <span>Hold to dictate</span>
          {settings.handsFree && <span>Double-tap for hands-free</span>}
          {settings.rewriteHotkey && <span>Select text, hold {hotkeyLabel(settings.rewriteHotkey)} to rewrite it</span>}
        </div>
      )}
    </div>
  );
}

/** True for a few seconds after a dictation ends. */
function useRecent(live: AppState["live"]): boolean {
  const [recent, setRecent] = useState(false);
  useEffect(() => {
    if (live.phase !== "done" && live.phase !== "error") return setRecent(false);
    setRecent(true);
    const timer = setTimeout(() => setRecent(false), 8000);
    return () => clearTimeout(timer);
  }, [live]);
  return recent;
}

function workWord(app: AppState): string {
  const { live } = app;
  const style = live.app && live.app.category !== "other" ? ` · ${live.app.category} style` : "";
  if (live.stage === "tidying") return `Tidying${style}`;
  if (live.stage === "rewriting") return "Rewriting";
  if (live.stage === "editing") return "Rewriting your selection";
  if (live.stage === "transcribing") return "Transcribing";
  return live.mode === "rewrite" ? "Rewriting" : "Working";
}

export function Strip({ app, readyText }: { app: AppState; readyText?: string }) {
  const { live, settings, dictation, fresh } = app;
  const listening = live.phase === "recording";
  const working = live.phase === "processing";
  const levels = useLevels(listening, 14);
  const elapsed = useElapsed(listening || working, live.since);
  const recent = useRecent(live);
  const hotkey = live.mode === "rewrite" && settings.rewriteHotkey ? settings.rewriteHotkey : settings.hotkey;

  let tone = "";
  let icon: ReactNode = <Bars size="sm" />;
  let word = "Ready";
  let time = "";
  let right = readyText ?? `Hold ${hotkeyLabel(settings.hotkey)}`;

  if (listening) {
    icon = <LevelBars levels={levels} />;
    word = live.handsFree ? "Listening, hands-free" : live.mode === "rewrite" ? "Listening, rewrite" : "Listening";
    time = clock(elapsed);
    right = live.handsFree ? `Tap ${hotkeyLabel(hotkey)} to finish` : "Let go to insert";
  } else if (working) {
    icon = <Bars size="sm" mode="work" />;
    word = workWord(app);
    time = seconds(elapsed, 1);
    right = live.stage && live.stage !== "transcribing" ? "Waiting on the language model" : "";
  } else if (recent && live.phase === "error") {
    tone = "pe";
    icon = <IconWarnFill hole="var(--bg)" />;
    word = live.message ?? "Something went wrong.";
    right = "";
  } else if (recent && live.phase === "done" && dictation && fresh) {
    if (!dictation.inserted && dictation.note === "Nowhere to type it.") {
      tone = "pe";
      icon = <IconWarnFill hole="var(--bg)" />;
      word = "Not typed";
      right = "Nowhere to type it";
    } else if (!dictation.inserted) {
      tone = "pe";
      icon = <IconWarnFill hole="var(--bg)" />;
      word = "Copied";
      right = "Press ⌘V to paste";
    } else if (cleanupFailed(dictation)) {
      tone = "pe";
      icon = <IconWarnFill hole="var(--bg)" />;
      word = "Inserted as heard";
      right = "Cleanup failed";
    } else {
      tone = "mi";
      icon = <IconOkFill hole="var(--bg)" />;
      word = "Inserted";
      right = dictation.totalMs ? `${seconds(dictation.totalMs)} after you let go` : "";
    }
  } else {
    const ready = readiness(app);
    if (ready.tone === "bad" || (ready.tone === "pe" && ready.tab === "speech")) {
      tone = "pe";
      icon = <IconWarnFill hole="var(--bg)" />;
      word = ready.title;
      right = ready.detail;
    } else if (ready.tone === "live" && ready.bar !== null) {
      word = ready.title;
      right = ready.detail;
    } else if (ready.tone === "pe") {
      right = `${ready.title}, so words go in as heard`;
    }
  }

  return (
    <div className={`qstrip ${tone}`} role="status">
      {icon}
      <b>{word}</b>
      {time && <span className="tm">{time}</span>}
      <span className="sp" />
      <span className="rt">{right}</span>
    </div>
  );
}

type View = "heard" | "changes" | "written";

/** Heard and written laid over each other: removals struck out, additions marked. */
function Changes({ heard, written }: { heard: string; written: string }) {
  const words = diffWords(heard, written);
  return (
    <p className="qsaid">
      {words.map((word, i) =>
        word[0].kind === "break" ? (
          <Fragment key={i}>
            <br />
            {word[0].text.length > 1 && <span className="br" />}
          </Fragment>
        ) : (
          <Fragment key={i}>
            {i > 0 && words[i - 1][0].kind !== "break" && " "}
            {word.map((piece, j) =>
              piece.kind === "del" ? (
                <del key={j}>{piece.text}</del>
              ) : piece.kind === "ins" ? (
                <ins key={j}>{piece.text}</ins>
              ) : (
                <Fragment key={j}>{piece.text}</Fragment>
              ),
            )}
          </Fragment>
        ),
      )}
    </p>
  );
}

/** Text with its line breaks kept. */
function Lines({ text, className = "qsaid" }: { text: string; className?: string }) {
  const paragraphs = text.split(/\n{2,}/);
  return (
    <p className={className}>
      {paragraphs.map((para, i) => (
        <Fragment key={i}>
          {i > 0 && (
            <>
              <br />
              <span className="br" />
            </>
          )}
          {para.split("\n").map((line, j) => (
            <Fragment key={j}>
              {j > 0 && <br />}
              {line}
            </Fragment>
          ))}
        </Fragment>
      ))}
    </p>
  );
}

export function useCopy() {
  const [copied, setCopied] = useState<number | null>(null);
  const copy = (d: Dictation) =>
    copyText(d.text).then(() => {
      setCopied(d.finishedAt);
      setTimeout(() => setCopied((now) => (now === d.finishedAt ? null : now)), 1500);
    });
  return { copied, copy };
}

/** The last dictation: what went in and what was heard. */
function LastDictation({ dictation, onFix }: { dictation: Dictation; onFix: () => void }) {
  const edit = dictation.selection !== null;
  const failed = cleanupFailed(dictation);
  const rewritten = dictation.mode === "rewrite" && !edit;
  const start: View = !dictation.cleaned ? "heard" : rewritten ? "written" : "changes";
  const [view, setView] = useState<View>(start);
  const { copied, copy } = useCopy();
  useEffect(() => setView(start), [dictation.finishedAt]); // eslint-disable-line react-hooks/exhaustive-deps

  const before = edit ? (dictation.selection ?? "") : dictation.raw;
  return (
    <div className="qcard">
      <div className="qrow">
        <div className="qlab">{edit ? "Last rewrite" : "Last dictation"}</div>
        <span className="sp" />
        <Segmented
          label="View"
          value={dictation.cleaned ? view : "heard"}
          onChange={setView}
          options={[
            { id: "heard", label: edit ? "Before" : "Heard" },
            { id: "changes", label: "Changes", disabled: !dictation.cleaned },
            { id: "written", label: edit ? "After" : rewritten ? "Rewritten" : "Cleaned", disabled: !dictation.cleaned },
          ]}
        />
      </div>
      <p className="qsub" style={{ marginTop: 2 }}>
        {meta(dictation)}
      </p>
      {edit && (
        <div className="qinstr">
          <span>You said</span>
          <q>{dictation.raw}</q>
        </div>
      )}
      {!dictation.cleaned ? (
        <Lines text={dictation.text} className={`qsaid ${failed ? "heard" : ""}`} />
      ) : view === "heard" ? (
        <Lines text={before} className="qsaid heard" />
      ) : view === "changes" ? (
        <Changes heard={before} written={dictation.text} />
      ) : (
        <Lines text={dictation.text} />
      )}
      {dictation.note && !failed && <p className="qp">{dictation.note}</p>}
      <div className="qrowb">
        {edit ? (
          <>
            <button className="qbtn" onClick={() => copy(dictation)}>
              {copied === dictation.finishedAt ? "Copied" : "Copy"}
            </button>
            <span className="qnote" style={{ marginLeft: 6 }}>
              Rewrites replace your selection, so there’s nothing to fix here.
            </span>
          </>
        ) : (
          <>
            <button className="qbtn" onClick={onFix}>
              Fix a word
            </button>
            <button className="qbtn ghost" onClick={() => copy(dictation)}>
              {copied === dictation.finishedAt ? "Copied" : "Copy"}
            </button>
          </>
        )}
      </div>
    </div>
  );
}

/** Where the wait went. The big number is the one people feel. */
function Timings({ dictation }: { dictation: Dictation }) {
  const { settings } = useApp();
  const failed = cleanupFailed(dictation);
  const edit = dictation.selection !== null;
  const total = dictation.totalMs || dictation.transcribeMs + (dictation.cleanupMs ?? 0);
  const model = failed ? 0 : (dictation.cleanupMs ?? 0);
  const inserting = Math.max(0, total - dictation.transcribeMs - model);
  const label = edit || dictation.mode === "rewrite" ? "Rewrite" : "Cleanup";
  const second = failed
    ? { text: "Failed", cls: "no pe" }
    : dictation.cleanupMs !== null
      ? { text: null, cls: "" }
      : { text: dictation.mode === "rewrite" || settings.cleanupEnabled ? "Not needed" : "Off", cls: "no" };
  return (
    <div className="qcard">
      <div className="qlab">
        After you let go
        {dictation.spokenMs > 0 && (
          <span className="r">
            You spoke for {seconds(dictation.spokenMs, 1)}
            {dictation.way && ` · ${dictation.way === "quick" ? "Quick" : "Accurate"}`}
          </span>
        )}
      </div>
      <div className="qbig">
        {secondsBare(total)}
        <small>s</small>
      </div>
      <div className="qsplit" aria-hidden="true">
        <i className="c1" style={{ flex: Math.max(1, dictation.transcribeMs) }} />
        {model > 0 && <i className="c2" style={{ flex: model }} />}
        <i className="c3" style={{ flex: Math.max(1, inserting) }} />
      </div>
      <div className="qtiles">
        <div>
          <span>
            <i className="c1" />
            Transcribe
          </span>
          <b>
            {secondsBare(dictation.transcribeMs)}
            <small>s</small>
          </b>
        </div>
        <div className={second.cls}>
          <span>
            <i className="c2" />
            {label}
          </span>
          <b>
            {second.text ?? (
              <>
                {secondsBare(dictation.cleanupMs ?? 0)}
                <small>s</small>
              </>
            )}
          </b>
        </div>
        <div>
          <span>
            <i className="c3" />
            Inserting
          </span>
          <b>
            {secondsBare(inserting)}
            <small>s</small>
          </b>
        </div>
      </div>
      {settings.cleanupEnabled && !edit && dictation.mode === "tidy" && (
        <div className="qsw" style={{ padding: "12px 0 0" }}>
          <div className="tx">
            <b>How to tidy</b>
            <span>Switch, dictate the same thing again, and compare the time.</span>
          </div>
          <WaySwitch />
        </div>
      )}
    </div>
  );
}

/** Words and the spaces and punctuation between them. */
function tokens(text: string): { text: string; word: boolean }[] {
  return (text.match(/[\p{L}\p{N}_'’.-]*[\p{L}\p{N}]|[^\p{L}\p{N}]+|./gu) ?? []).map((t) => ({
    text: t,
    word: /[\p{L}\p{N}]/u.test(t),
  }));
}

/**
 * Fix a word: click the word that came out wrong, or drag across a few. The
 * correction is made in history and, if wanted, remembered in Your words.
 */
export function FixCard({
  dictation,
  onCancel,
  onFixed,
}: {
  dictation: Dictation;
  onCancel: () => void;
  onFixed: (done: Fixed) => void;
}) {
  const { adopt, refreshHistory } = useApp();
  const parts = tokens(dictation.text);
  const [range, setRange] = useState<[number, number] | null>(null);
  const [right, setRight] = useState("");
  const [remember, setRemember] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // Where a drag across words began, and whether it has moved off that word.
  const drag = useRef<{ from: number; moved: boolean } | null>(null);
  const input = useRef<HTMLInputElement>(null);

  const textOf = (r: [number, number]) => parts.slice(r[0], r[1] + 1).map((p) => p.text).join("");
  const wrong = range ? textOf(range) : "";

  const choose = (next: [number, number] | null) => {
    setError(null);
    setRange(next);
    if (next) {
      setRight(textOf(next));
      requestAnimationFrame(() => {
        input.current?.focus({ preventScroll: true });
        input.current?.select();
      });
    }
  };

  // A click picks one word; a click on the next word along widens the choice.
  const click = (i: number) => {
    if (range) {
      const [a, b] = range;
      const nextBefore = i < a && parts.slice(i + 1, a).every((p) => !p.word);
      const nextAfter = i > b && parts.slice(b + 1, i).every((p) => !p.word);
      if (nextBefore) return choose([i, b]);
      if (nextAfter) return choose([a, i]);
      if (i === a && i === b) return choose(null);
    }
    choose([i, i]);
  };

  useEffect(() => {
    const end = () => {
      // Let the click that ends a drag see that it was one.
      setTimeout(() => (drag.current = null), 0);
    };
    window.addEventListener("mouseup", end);
    return () => window.removeEventListener("mouseup", end);
  }, []);

  const spelled = right.trim();
  const heard = spelled && spelled.toLowerCase() !== wrong.toLowerCase() ? wrong.toLowerCase() : null;

  const fix = () => {
    if (!range || !spelled || spelled === wrong) return;
    fixWord(dictation.finishedAt, wrong, spelled, remember).then(
      (saved) => {
        adopt(saved);
        refreshHistory();
        onFixed({ d: dictation, wrong, right: spelled, remembered: remember });
      },
      (err) => setError(String(err)),
    );
  };

  return (
    <div className="qcard">
      <div className="qrow">
        <div className="qlab">Fix a word</div>
        <span className="sp" />
        <button className="qbtn sm ghost" onClick={onCancel}>
          Cancel
        </button>
      </div>
      <p className="qsub" style={{ marginTop: 2 }}>
        {meta(dictation)}
      </p>
      <p className="qp">Click the word that’s wrong. Drag across to fix a few words together.</p>
      <p className="qsaid" style={{ userSelect: "none" }}>
        {parts.map((p, i) =>
          p.word ? (
            <button
              key={i}
              className={`qwd ${range && i >= range[0] && i <= range[1] ? "sel" : ""}`}
              onMouseDown={(e) => {
                if (e.button === 0) drag.current = { from: i, moved: false };
              }}
              onMouseEnter={() => {
                const d = drag.current;
                if (d && d.from !== i) {
                  d.moved = true;
                  choose([Math.min(d.from, i), Math.max(d.from, i)]);
                }
              }}
              onClick={() => {
                if (drag.current?.moved) return;
                click(i);
              }}
            >
              {p.text}
            </button>
          ) : (
            <Fragment key={i}>
              {p.text.includes("\n")
                ? p.text.split("\n").map((piece, j) => (
                    <Fragment key={j}>
                      {j > 0 && <br />}
                      {piece}
                    </Fragment>
                  ))
                : p.text}
            </Fragment>
          ),
        )}
      </p>
      {range && (
        <>
          <div className="qfix">
            <s>{wrong}</s>
            <span className="ar" aria-hidden="true">
              →
            </span>
            <div className="qfld">
              <input
                ref={input}
                value={right}
                aria-label="Write it as"
                maxLength={60}
                spellCheck={false}
                onChange={(e) => setRight(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.nativeEvent.isComposing) fix();
                  if (e.key === "Escape") choose(null);
                }}
              />
            </div>
            <button className="qbtn pri" disabled={!spelled || spelled === wrong} onClick={fix}>
              Fix
            </button>
          </div>
          <div className="qrow" style={{ marginTop: 10, gap: 10 }}>
            <button
              className={`qchk ${remember ? "on" : ""}`}
              role="checkbox"
              aria-checked={remember}
              onClick={() => setRemember(!remember)}
            >
              <i />
              Remember
            </button>
            <span className="qnote" style={{ fontSize: 12.5 }}>
              {!remember
                ? "Fixes it here only."
                : heard
                  ? `Adds ${spelled} to your words, heard as “${heard}”.`
                  : spelled
                    ? `Adds ${spelled} to your words.`
                    : ""}
            </span>
          </div>
        </>
      )}
      {error && (
        <Notice tone="pe">
          <b>Not fixed.</b> <span>{error}</span>
        </Notice>
      )}
    </div>
  );
}

/** Escape text for use inside a regular expression. */
function escape(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export function FixedCard({
  d,
  wrong,
  right,
  remembered,
  onDone,
  title = "Last dictation",
}: Fixed & { onDone: () => void; title?: string }) {
  const caseOnly = wrong.toLowerCase() === right.toLowerCase();
  // The text as it now reads, with each fix marked.
  const pieces = d.text.split(new RegExp(`(?<![\\p{L}\\p{N}_])${escape(wrong)}(?![\\p{L}\\p{N}_])`, "iu"));
  return (
    <div className="qcard">
      <div className="qlab">
        {title}
        <span className="r">{meta(d)}</span>
      </div>
      <p className="qsaid">
        {pieces.map((piece, i) => (
          <Fragment key={i}>
            {i > 0 && <ins>{right}</ins>}
            {piece}
          </Fragment>
        ))}
      </p>
      <Notice
        tone="mi"
        action={
          <button className="qbtn sm" onClick={onDone}>
            Done
          </button>
        }
      >
        {remembered && !caseOnly ? (
          <>
            Fixed. Utter will write <b>{right}</b> when it hears “{wrong.toLowerCase()}” and the sentence fits.
          </>
        ) : remembered ? (
          <>
            Fixed. <b>{right}</b> is in your words.
          </>
        ) : (
          "Fixed here."
        )}
      </Notice>
    </div>
  );
}
