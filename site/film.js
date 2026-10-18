/* The player for the launch film. Plain JavaScript, no dependencies.

   The page ships the film with the browser's own controls, so it plays
   without script. This file takes them off and runs the player drawn in
   index.html instead: one big button over the poster, then a slim bar that
   fades out while the film plays and the pointer is still.

   The frame's data-state is the one thing the CSS reads:
     ready    poster, nothing loaded yet
     playing  film running
     paused   stopped part-way
     ended    finished; the big button offers to play it again
   and two flags beside it: data-waiting (buffering) and data-idle (hide the bar). */
(function () {
  "use strict";

  var frame = document.getElementById("film-player");
  if (!frame) return;
  var video = frame.querySelector("video");
  var ui = frame.querySelector(".fp-ui");
  if (!video || !ui) return;

  function part(name) {
    return frame.querySelector('[data-fp="' + name + '"]');
  }
  var big = frame.querySelector(".fp-big");
  var bigLabel = frame.querySelector(".fp-label-t");
  var barToggle = frame.querySelector(".fp-bar [data-fp=toggle]");
  var seek = part("seek");
  var vol = part("vol");
  var mute = part("mute");
  var full = part("full");
  var now = part("now");
  var dur = part("dur");
  var done = frame.querySelector(".fp-done");
  var buf = frame.querySelector(".fp-buf");
  var tip = frame.querySelector(".fp-tip");

  video.controls = false;
  var length = 44; // until the file says otherwise
  var IDLE_MS = 2200; // pointer still this long while playing: the bar fades
  var IDLE_TOUCH_MS = 3500; // longer on a touch screen, with no hover to bring it back
  var SKIP_S = 5;

  function clock(seconds) {
    var s = Math.max(0, Math.floor(seconds || 0));
    return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0");
  }
  function setState(state) {
    frame.setAttribute("data-state", state);
    var playing = state === "playing";
    barToggle.setAttribute("aria-label", playing ? "Pause" : "Play");
    big.setAttribute("aria-label", state === "ended" ? "Play the film again" : "Play the film");
    bigLabel.textContent = state === "ended" ? "Watch again" : "Play the film";
    if (!playing) wake();
  }

  function play() {
    // The film is short; once someone asks for it, fetch it all.
    if (video.preload === "none") video.preload = "auto";
    var started = video.play();
    if (started && started.catch) {
      started.catch(function () {
        setState(video.currentTime > 0 ? "paused" : "ready");
      });
    }
  }
  function toggle() {
    if (video.paused || video.ended) play();
    else video.pause();
  }

  /* ---------- time and the scrubber ---------- */
  function paint() {
    var t = video.currentTime || 0;
    now.textContent = clock(t);
    if (!scrubbing) seek.value = t;
    var p = length ? (t / length) * 100 : 0;
    done.style.width = p + "%";
    seek.setAttribute("aria-valuetext", clock(t) + " of " + clock(length));
  }
  function paintBuffer() {
    var b = video.buffered;
    if (!b.length || !length) return;
    buf.style.width = (b.end(b.length - 1) / length) * 100 + "%";
  }
  video.addEventListener("loadedmetadata", function () {
    if (isFinite(video.duration) && video.duration > 0) length = video.duration;
    seek.max = length;
    dur.textContent = clock(length);
    paint();
  });
  video.addEventListener("timeupdate", paint);
  video.addEventListener("progress", paintBuffer);

  // Smooth progress while playing; timeupdate alone moves in steps.
  var raf = 0;
  function tick() {
    paint();
    raf = video.paused ? 0 : requestAnimationFrame(tick);
  }

  var scrubbing = false;
  var resumeAfter = false;
  seek.addEventListener("pointerdown", function () {
    scrubbing = true;
    resumeAfter = !video.paused;
    video.pause();
  });
  seek.addEventListener("input", function () {
    video.currentTime = +seek.value;
    done.style.width = (+seek.value / length) * 100 + "%";
    now.textContent = clock(+seek.value);
  });
  function endScrub() {
    if (!scrubbing) return;
    scrubbing = false;
    if (resumeAfter) play();
  }
  seek.addEventListener("pointerup", endScrub);
  seek.addEventListener("pointercancel", endScrub);
  seek.addEventListener("change", endScrub);

  // The time under the pointer, above the scrubber.
  seek.addEventListener("pointermove", function (e) {
    var box = seek.getBoundingClientRect();
    var x = Math.min(Math.max(e.clientX - box.left, 0), box.width);
    tip.textContent = clock((x / box.width) * length);
    tip.style.left = x + "px";
  });

  /* ---------- playback state ---------- */
  video.addEventListener("play", function () {
    setState("playing");
    if (!raf) raf = requestAnimationFrame(tick);
    nap();
  });
  video.addEventListener("pause", function () {
    if (!video.ended) setState(video.currentTime > 0 ? "paused" : "ready");
  });
  video.addEventListener("ended", function () {
    setState("ended");
  });
  video.addEventListener("waiting", function () {
    frame.setAttribute("data-waiting", "");
  });
  ["playing", "canplay", "pause"].forEach(function (type) {
    video.addEventListener(type, function () {
      frame.removeAttribute("data-waiting");
    });
  });

  /* ---------- sound ---------- */
  function paintSound() {
    var silent = video.muted || video.volume === 0;
    frame.toggleAttribute("data-muted", silent);
    mute.setAttribute("aria-label", silent ? "Unmute" : "Mute");
    vol.value = silent ? 0 : video.volume;
    vol.style.setProperty("--p", (silent ? 0 : video.volume) * 100 + "%");
  }
  mute.addEventListener("click", function () {
    if (video.muted || video.volume === 0) {
      video.muted = false;
      if (video.volume === 0) video.volume = 1;
    } else {
      video.muted = true;
    }
  });
  vol.addEventListener("input", function () {
    video.volume = +vol.value;
    video.muted = +vol.value === 0;
  });
  video.addEventListener("volumechange", paintSound);
  paintSound();

  /* ---------- full screen ---------- */
  function isFull() {
    return (document.fullscreenElement || document.webkitFullscreenElement) === frame;
  }
  function toggleFull() {
    if (isFull()) {
      (document.exitFullscreen || document.webkitExitFullscreen).call(document);
    } else if (frame.requestFullscreen) {
      frame.requestFullscreen();
    } else if (frame.webkitRequestFullscreen) {
      frame.webkitRequestFullscreen();
    } else if (video.webkitEnterFullscreen) {
      // iPhone: only the video element itself can go full screen.
      video.webkitEnterFullscreen();
    }
  }
  function paintFull() {
    frame.toggleAttribute("data-full", isFull());
    full.setAttribute("aria-label", isFull() ? "Exit full screen" : "Full screen");
  }
  document.addEventListener("fullscreenchange", paintFull);
  document.addEventListener("webkitfullscreenchange", paintFull);
  full.addEventListener("click", toggleFull);

  /* ---------- clicks and keys ---------- */
  each(frame.querySelectorAll("[data-fp=toggle]"), function (button) {
    button.addEventListener("click", toggle);
  });
  // With a mouse, a click on the picture plays or pauses and a double click goes
  // full screen. On a touch screen a tap on the picture shows the controls, or
  // hides them if they are showing, as in other players; the bar's own button
  // pauses. A double tap does nothing there: on an iPhone it would drop the
  // page into the system player.
  var touch = false; // the latest press on the player was a finger
  var hiddenAtPress = false; // and the bar was hidden when it came down
  frame.addEventListener(
    "pointerdown",
    function (e) {
      touch = e.pointerType === "touch";
      hiddenAtPress = frame.hasAttribute("data-idle");
    },
    true,
  );
  video.addEventListener("click", function () {
    if (touch && !video.paused) {
      if (hiddenAtPress) wake();
      else frame.setAttribute("data-idle", "");
      return;
    }
    toggle();
  });
  video.addEventListener("dblclick", function () {
    if (!touch) toggleFull();
  });

  frame.addEventListener("keydown", function (e) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    var onSlider = e.target.type === "range";
    var key = e.key.toLowerCase();
    if (key === "k" || (key === " " && e.target.tagName !== "BUTTON")) toggle();
    else if (key === "m") mute.click();
    else if (key === "f") toggleFull();
    else if (!onSlider && key === "arrowleft") video.currentTime = Math.max(0, video.currentTime - SKIP_S);
    else if (!onSlider && key === "arrowright") video.currentTime = Math.min(length, video.currentTime + SKIP_S);
    else return wake();
    e.preventDefault();
    wake();
  });

  /* ---------- the bar fades while you watch ---------- */
  var idleTimer = 0;
  function wake() {
    frame.removeAttribute("data-idle");
    nap();
  }
  // Someone tabbing through the controls needs to see them; a mouse click
  // that leaves focus on a button does not.
  function keyboardFocusInside() {
    var el = document.activeElement;
    if (!frame.contains(el)) return false;
    try {
      return el.matches(":focus-visible");
    } catch (e) {
      return true;
    }
  }
  function nap() {
    clearTimeout(idleTimer);
    idleTimer = setTimeout(
      function () {
        if (!video.paused && !scrubbing && !keyboardFocusInside()) frame.setAttribute("data-idle", "");
      },
      touch ? IDLE_TOUCH_MS : IDLE_MS,
    );
  }
  frame.addEventListener("pointermove", wake);
  frame.addEventListener("pointerdown", wake);
  frame.addEventListener("focusin", wake);
  frame.addEventListener("focusout", nap);
  // A finger "leaves" the moment it lifts; only a mouse leaving means look away.
  frame.addEventListener("pointerleave", function (e) {
    if (e.pointerType !== "touch" && !video.paused) frame.setAttribute("data-idle", "");
  });

  // A film with sound should not keep talking from somewhere off screen.
  if ("IntersectionObserver" in window) {
    new IntersectionObserver(
      function (entries) {
        if (!entries[entries.length - 1].isIntersecting && !video.paused && !isFull()) video.pause();
      },
      { threshold: 0.15 },
    ).observe(frame);
  }

  function each(list, fn) {
    Array.prototype.forEach.call(list, fn);
  }

  frame.classList.add("fp-on");
  setState("ready");
})();
