/* The Utter site's script. Plain JavaScript, no dependencies.
   It makes no network requests and stores nothing.

   Most of it runs the demo in the hero: a recreation of one dictation with
   canned text. Nothing is recorded and no speech recognition runs here. The
   example sentences live in index.html; this file only decides what is showing
   and animates the pill and the menu bar icon the way the app does. */
(function () {
  "use strict";

  var doc = document;
  function $(id) {
    return doc.getElementById(id);
  }
  function each(list, fn) {
    Array.prototype.forEach.call(list, fn);
  }

  /* ---------- the version, copied from its one home in the hero ---------- */
  each(doc.querySelectorAll("[data-copy]"), function (el) {
    var source = $(el.getAttribute("data-copy"));
    var text = source && source.textContent.trim();
    if (text) el.textContent = text;
  });

  /* ---------- "Copy" buttons beside commands ---------- */
  var notice = doc.createElement("p");
  notice.className = "sr";
  notice.setAttribute("role", "status");
  doc.body.appendChild(notice);

  each(doc.querySelectorAll("[data-copy-from]"), function (button) {
    var source = $(button.getAttribute("data-copy-from"));
    if (!source) return;
    var reset;
    // The commands without their "$ " prompts.
    function text() {
      var copy = source.cloneNode(true);
      each(copy.querySelectorAll(".pr"), function (pr) {
        pr.remove();
      });
      return copy.textContent;
    }
    function report(ok) {
      button.textContent = ok ? "Copied" : "Selected";
      notice.textContent = ok ? "Copied to the clipboard." : "Selected. Press Command C to copy.";
      clearTimeout(reset);
      reset = setTimeout(function () {
        button.textContent = "Copy";
        notice.textContent = "";
      }, 2000);
    }
    function selectIt() {
      var range = doc.createRange();
      range.selectNodeContents(source);
      var selection = window.getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
      report(false);
    }
    button.addEventListener("click", function () {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text()).then(function () {
          report(true);
        }, selectIt);
      } else {
        selectIt();
      }
    });
  });

  /* ---------- "same words, right for every app" ---------- */
  var styler = $("styler");
  if (styler) {
    var exButtons = styler.querySelectorAll("[data-ex]:not(.ex)");
    var examples = styler.querySelectorAll(".ex");
    each(exButtons, function (button) {
      button.addEventListener("click", function () {
        var want = button.getAttribute("data-ex");
        each(exButtons, function (b) {
          b.setAttribute("aria-pressed", String(b === button));
        });
        each(examples, function (ex) {
          var on = ex.getAttribute("data-ex") === want;
          ex.classList.toggle("cur", on);
          if (on && $("exapp")) $("exapp").textContent = ex.getAttribute("data-app");
        });
      });
    });
  }

  /* ---------- the demo ---------- */
  var demo = $("demo");
  if (!demo) return;

  var pill = $("pill");
  var pin = $("pin");
  var pbars = $("pbars");
  var lw = $("lw");
  var ptext = $("ptext");
  var pchip = $("pchip");
  var ptime = $("ptime");
  var glyph = $("mglyph");
  var mbarApp = $("mbar-app");
  var hold = $("hold");
  var autobtn = $("autobtn");
  var saidK = $("said-k");
  var saidW = $("said-w");
  var live = $("demo-live");
  var holdKeys = hold.querySelectorAll("[data-keys]");

  var wins = {};
  each(demo.querySelectorAll(".win"), function (win) {
    wins[win.getAttribute("data-app")] = win;
  });
  var outs = demo.querySelectorAll(".out");
  var appButtons = demo.querySelectorAll(".apps button");
  var modeButtons = demo.querySelectorAll(".modes button");

  var APPS = ["slack", "mail", "notes", "xcode"];
  var reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // Timings, in milliseconds. They pace the demo; they are not measurements.
  var WORD_MS = 205; // one spoken word
  var LEAD_MS = 260; // a breath before the first word
  var TAIL_MS = 600; // a beat after the last word
  var TRANSCRIBE_MS = 650;
  var WORK_MS = 1050;
  var TAP_MS = 400; // shorter than this is a tap, not a hold
  var MAX_HOLD_MS = 15000;
  var REST_MS = 4200; // how long a finished result stays up between autoplay runs
  var AUTO_RUNS = 8; // autoplay stops by itself after two rounds of the four apps
  var LW_BARS = 5;

  var state = { app: "slack", mode: "tidy", phase: "done" };
  var hasResult = false;
  var running = false;
  var byUser = false;
  var timed = false;
  var holding = false;
  var lastHoldEnd = 0;
  var t0 = 0;
  var level = 0;
  var ticker = null;
  var timers = [];
  var auto = { on: !reduce, runs: 0, visible: false };

  var i;
  for (i = 0; i < LW_BARS; i++) lw.appendChild(doc.createElement("i"));
  var lwBars = lw.children;
  var lwShape = [0.55, 0.85, 1, 0.8, 0.5];

  // Wrap each heard word so it can appear as it is "spoken".
  var words = [];
  (function wrap(node) {
    each(Array.prototype.slice.call(node.childNodes), function (child) {
      if (child.nodeType === 1) return wrap(child);
      if (child.nodeType !== 3) return;
      var parts = doc.createDocumentFragment();
      child.nodeValue.split(/(\s+)/).forEach(function (part) {
        if (!part) return;
        if (/^\s+$/.test(part)) return parts.appendChild(doc.createTextNode(" "));
        var word = doc.createElement("span");
        word.className = "w on";
        word.textContent = part;
        words.push(word);
        parts.appendChild(word);
      });
      node.replaceChild(parts, child);
    });
  })(saidW);

  /* ----- helpers ----- */
  function later(ms, fn) {
    timers.push(setTimeout(fn, ms));
  }
  function clearTimers() {
    timers.forEach(clearTimeout);
    timers = [];
    clearInterval(ticker);
    ticker = null;
  }
  function clock(ms) {
    var total = Math.floor(ms / 1000);
    var secs = total % 60;
    return Math.floor(total / 60) + ":" + (secs < 10 ? "0" : "") + secs;
  }
  // Tell screen-reader users what a run they started is doing. Autoplay stays quiet.
  function say(text) {
    if (byUser) live.textContent = text;
  }
  function currentOut() {
    return wins[state.app].querySelector('.out[data-mode="' + state.mode + '"]');
  }
  function outText(out) {
    var lines = [];
    each(out.querySelectorAll("p, li"), function (el) {
      lines.push(el.textContent.trim());
    });
    return lines.length ? lines.join(" ") : out.textContent.trim();
  }

  function setPhase(phase) {
    state.phase = phase;
    demo.setAttribute("data-phase", phase);
    each(outs, function (out) {
      out.classList.remove("cur", "flash");
    });
    if (phase === "done") currentOut().classList.add("cur");
  }

  function showWords(on) {
    words.forEach(function (word) {
      word.classList.toggle("on", on);
    });
  }

  // Which app and which hotkey: windows, buttons, menu bar and the third keycap follow.
  function syncChoice() {
    demo.setAttribute("data-app", state.app);
    demo.setAttribute("data-mode", state.mode);
    APPS.forEach(function (app) {
      wins[app].classList.toggle("cur", app === state.app);
    });
    each(appButtons, function (button) {
      button.setAttribute("aria-pressed", String(button.getAttribute("data-app") === state.app));
    });
    each(modeButtons, function (button) {
      button.setAttribute("aria-pressed", String(button.getAttribute("data-mode") === state.mode));
    });
    mbarApp.textContent = wins[state.app].getAttribute("data-name");
    // fn to tidy, as in the app. A page cannot see fn, so Space stands in for it.
    each(holdKeys, function (key) {
      key.hidden = key.getAttribute("data-keys") !== state.mode;
    });
  }

  /* ----- the pill and the menu bar icon ----- */
  function fitPill() {
    if (pill.getAttribute("data-kind") === "off") return;
    pill.style.width = pin.offsetWidth + 32 + "px";
  }
  function pillTo(kind, text, chip) {
    pill.setAttribute("data-kind", kind);
    pill.classList.toggle("off", kind === "off");
    glyph.classList.toggle("live", kind === "listen");
    glyph.classList.toggle("work", kind === "work");
    pbars.classList.toggle("work", kind === "work");
    pbars.hidden = kind === "note";
    if (kind === "off") return;
    ptext.textContent = text;
    pchip.textContent = chip || "";
    pchip.hidden = !chip;
    ptime.hidden = kind !== "listen";
    fitPill();
  }

  /* ----- listening: the bars, the timer, the words ----- */
  function drawLevel() {
    for (var b = 0; b < LW_BARS; b++) {
      var h = reduce ? 10 : Math.round(4 + Math.min(1, level * lwShape[b] * 1.2) * 20);
      lwBars[b].style.height = h + "px";
    }
  }

  function tick() {
    var elapsed = performance.now() - t0;
    var spoken = elapsed < LEAD_MS ? 0 : Math.min(words.length, Math.floor((elapsed - LEAD_MS) / WORD_MS) + 1);
    var speaking = elapsed >= LEAD_MS && spoken < words.length;
    if (!reduce) {
      for (var w = 0; w < spoken; w++) words[w].classList.add("on");
    }
    // A made-up loudness reading: lively while "speaking", near silence otherwise.
    var target = !speaking
      ? 0.03 + Math.random() * 0.06
      : Math.random() < 0.26
        ? 0.08 + Math.random() * 0.16
        : 0.4 + Math.random() * 0.6;
    level += (target - level) * (target > level ? 0.7 : 0.3);
    drawLevel();
    ptime.textContent = clock(elapsed);

    if (timed && elapsed > LEAD_MS + words.length * WORD_MS + TAIL_MS) return finishListen();
    if (!timed && elapsed > MAX_HOLD_MS) endHold();
  }

  function startListen(user, isTimed) {
    clearTimers();
    running = true;
    byUser = user;
    timed = isTimed;
    setPhase("listen");
    saidK.textContent = "You say";
    showWords(reduce);
    hold.classList.add("down");
    level = 0;
    t0 = performance.now();
    ptime.textContent = "0:00";
    pillTo("listen", "Listening", state.mode === "rewrite" ? "rewrite" : "");
    ticker = setInterval(tick, 50);
    say("Listening.");
  }

  function finishListen() {
    clearInterval(ticker);
    ticker = null;
    hold.classList.remove("down");
    showWords(true);
    saidK.textContent = "Heard";
    setPhase("transcribe");
    pillTo("work", "Transcribing");
    say("Transcribing.");

    later(TRANSCRIBE_MS, function () {
      var win = wins[state.app];
      var tidy = state.mode === "tidy";
      setPhase("work");
      if (tidy) pillTo("work", "Tidying", win.getAttribute("data-kind"));
      else pillTo("work", "Rewriting for " + win.getAttribute("data-name"));
      say(ptext.textContent + ".");
      later(WORK_MS, finish);
    });
  }

  function finish() {
    var out = currentOut();
    hasResult = true;
    setPhase("done");
    if (!reduce) out.classList.add("flash");
    // Success is quiet: the pill sinks away and the text is the confirmation.
    pillTo("off");
    say("Typed into " + wins[state.app].getAttribute("data-name") + ": " + outText(out));
    running = false;
    if (auto.on) later(REST_MS, autoNext);
  }

  // Put the last finished result back on screen.
  function restore() {
    saidK.textContent = "Heard";
    showWords(true);
    setPhase(hasResult ? "done" : "idle");
  }

  function nothingHeard() {
    clearTimers();
    hold.classList.remove("down");
    restore();
    pillTo("note", "Nothing heard. Hold the keys while you talk.");
    say("Nothing heard. Hold the keys while you talk.");
    running = false;
    later(2600, function () {
      pillTo("off");
    });
  }

  function cancelRun() {
    clearTimers();
    hold.classList.remove("down");
    pillTo("off");
    running = false;
    holding = false;
  }

  /* ----- autoplay ----- */
  function autoSet(on) {
    auto.on = on;
    autobtn.textContent = on ? "Pause" : "Play demo";
    if (on) {
      auto.runs = 0;
      if (!running) later(350, autoNext);
    }
  }
  function autoNext() {
    if (!auto.on || !auto.visible || doc.hidden || running) return;
    if (auto.runs >= AUTO_RUNS) return autoSet(false);
    if (auto.runs > 0) {
      state.app = APPS[(APPS.indexOf(state.app) + 1) % APPS.length];
      syncChoice();
    }
    auto.runs++;
    startListen(false, true);
  }
  function autoInterrupt() {
    // Out of sight: stop an unattended run where it is and show the result.
    if (running && !byUser) {
      cancelRun();
      hasResult = true;
      restore();
    }
  }

  /* ----- the visitor's hold ----- */
  function startHold() {
    if (holding) return;
    cancelRun();
    autoSet(false);
    holding = true;
    startListen(true, false);
  }
  function endHold() {
    if (!holding) return;
    holding = false;
    lastHoldEnd = Date.now();
    if (performance.now() - t0 < TAP_MS) nothingHeard();
    else finishListen();
  }
  // Enter, or a click from assistive technology: nothing can be held, so play one through.
  function playOnce() {
    cancelRun();
    autoSet(false);
    startListen(true, true);
  }

  function isHoldKey(event) {
    if (event.key === " " || event.code === "Space") return true;
    return state.mode === "rewrite" && (event.key === "r" || event.key === "R");
  }

  hold.addEventListener("pointerdown", function (event) {
    if (event.button !== 0) return;
    hold.focus({ preventScroll: true });
    try {
      hold.setPointerCapture(event.pointerId);
    } catch (err) {
      /* a pointer that cannot be captured still gets pointerup on the button */
    }
    startHold();
  });
  ["pointerup", "pointercancel", "lostpointercapture"].forEach(function (type) {
    hold.addEventListener(type, endHold);
  });
  hold.addEventListener("contextmenu", function (event) {
    event.preventDefault();
  });
  hold.addEventListener("keydown", function (event) {
    if (!isHoldKey(event) || event.metaKey) return;
    event.preventDefault();
    if (!event.repeat) startHold();
  });
  hold.addEventListener("keyup", function (event) {
    if (!isHoldKey(event)) return;
    event.preventDefault();
    endHold();
  });
  hold.addEventListener("blur", endHold);
  hold.addEventListener("click", function () {
    // A click that ends a hold has already been dealt with.
    if (holding || Date.now() - lastHoldEnd < 600) return;
    playOnce();
  });

  // Clicking the stage puts focus on the keys, so Space works straight away.
  $("screen").addEventListener("click", function () {
    hold.focus({ preventScroll: true });
  });

  function choose(change) {
    cancelRun();
    autoSet(false);
    change();
    syncChoice();
    hasResult = true;
    byUser = true;
    restore();
    var win = wins[state.app];
    say(
      win.getAttribute("data-name") +
        ", " +
        (state.mode === "tidy" ? win.getAttribute("data-kind") + " style" : "rewritten") +
        ": " +
        outText(currentOut()),
    );
  }
  each(appButtons, function (button) {
    button.addEventListener("click", function () {
      choose(function () {
        state.app = button.getAttribute("data-app");
      });
    });
  });
  each(modeButtons, function (button) {
    button.addEventListener("click", function () {
      choose(function () {
        state.mode = button.getAttribute("data-mode");
      });
    });
  });

  autobtn.addEventListener("click", function () {
    if (auto.on) return autoSet(false);
    auto.visible = true;
    autoSet(true);
  });

  /* ----- start ----- */
  syncChoice();
  pill.style.transition = "none";
  if (reduce) {
    // No autoplay, and the finished frame stays as it is until asked.
    hasResult = true;
    setPhase("done");
    autobtn.textContent = "Play demo";
  } else {
    showWords(false);
    setPhase("idle");
  }
  pillTo("off");
  void pill.offsetWidth;
  pill.style.transition = "";

  window.addEventListener("resize", fitPill);

  // Autoplay only while the demo is on screen and the tab is in front.
  if ("IntersectionObserver" in window) {
    new IntersectionObserver(
      function (entries) {
        auto.visible = entries[entries.length - 1].isIntersecting;
        if (!auto.visible) return autoInterrupt();
        if (auto.on && !running) later(700, autoNext);
      },
      { threshold: 0.35 },
    ).observe(demo);
  } else {
    auto.visible = true;
    if (auto.on) later(700, autoNext);
  }
  doc.addEventListener("visibilitychange", function () {
    if (doc.hidden) return autoInterrupt();
    if (auto.on && !running) later(700, autoNext);
  });
})();
