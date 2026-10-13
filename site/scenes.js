/* The small stages further down the page, and the way sections arrive.
   Plain JavaScript, no dependencies, no network requests, nothing stored.

   Each stage ([data-scene]) is an illustration with example text. Its markup
   shows the finished frame. While a stage is on screen this file steps its
   data-step attribute from 0 upwards, holding each step for the time given in
   data-steps, and styles.css draws each step. With reduced motion, or without
   IntersectionObserver, nothing here runs and the finished frames stay put. */
(function () {
  "use strict";

  var doc = document;
  function each(list, fn) {
    Array.prototype.forEach.call(list, fn);
  }

  /* ---------- the header gets a hairline once the page has moved under it ---------- */
  var top = doc.querySelector(".top");
  function markStuck() {
    if (top) top.classList.toggle("stuck", window.scrollY > 8);
  }
  window.addEventListener("scroll", markStuck, { passive: true });
  markStuck();

  var reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (reduce || !("IntersectionObserver" in window)) return;

  /* ---------- arriving ---------- */
  doc.documentElement.classList.add("rv-on");
  var arrive = new IntersectionObserver(
    function (entries) {
      each(entries, function (entry) {
        if (!entry.isIntersecting) return;
        entry.target.classList.add("in");
        arrive.unobserve(entry.target);
      });
    },
    { rootMargin: "0px 0px -8% 0px", threshold: 0.08 },
  );
  each(doc.querySelectorAll(".rv"), function (el) {
    arrive.observe(el);
  });

  /* ---------- a soft light follows the pointer across a panel ---------- */
  each(doc.querySelectorAll(".panel"), function (panel) {
    panel.addEventListener("pointermove", function (event) {
      if (event.pointerType !== "mouse") return;
      var box = panel.getBoundingClientRect();
      panel.style.setProperty("--mx", event.clientX - box.left + "px");
      panel.style.setProperty("--my", event.clientY - box.top + "px");
      panel.classList.add("lit");
    });
    panel.addEventListener("pointerleave", function () {
      panel.classList.remove("lit");
    });
  });

  /* ---------- the stages ---------- */
  var HF_SECONDS = 150; // what the hands-free timer counts up to: a long dictation, sped up
  var hfTime = doc.getElementById("hf-time");

  function clock(seconds) {
    var s = Math.floor(seconds);
    return Math.floor(s / 60) + ":" + (s % 60 < 10 ? "0" : "") + (s % 60);
  }

  var scenes = [];
  each(doc.querySelectorAll("[data-scene]"), function (el) {
    var steps = (el.getAttribute("data-steps") || "").split(",").map(Number);
    if (!steps.length || steps.some(isNaN)) return;
    scenes.push({
      el: el,
      steps: steps,
      last: el.getAttribute("data-step"),
      timer: 0,
      frame: 0,
      playing: false,
      visible: false,
      handsFree: el.classList.contains("s-free"),
    });
  });

  function show(scene, step) {
    scene.el.setAttribute("data-step", step);
    if (!scene.handsFree || !hfTime) return;
    cancelAnimationFrame(scene.frame);
    if (step < 2) {
      hfTime.textContent = "0:00";
    } else if (step === 2) {
      // The timer runs fast, so a few seconds on the page stand for minutes of talking.
      var began = performance.now();
      var length = scene.steps[2];
      var tick = function (now) {
        var done = Math.min(1, (now - began) / length);
        hfTime.textContent = clock(done * HF_SECONDS);
        if (done < 1 && scene.playing) scene.frame = requestAnimationFrame(tick);
      };
      scene.frame = requestAnimationFrame(tick);
    } else {
      hfTime.textContent = clock(HF_SECONDS);
    }
  }

  function play(scene, step) {
    show(scene, step);
    scene.timer = setTimeout(function () {
      play(scene, (step + 1) % scene.steps.length);
    }, scene.steps[step]);
  }

  function start(scene) {
    if (scene.playing || !scene.visible || doc.hidden) return;
    scene.playing = true;
    play(scene, 0);
  }

  function stop(scene) {
    if (!scene.playing) return;
    scene.playing = false;
    clearTimeout(scene.timer);
    // Left on its finished frame, so it reads correctly if glimpsed while paused.
    show(scene, scene.last);
  }

  var watch = new IntersectionObserver(
    function (entries) {
      each(entries, function (entry) {
        each(scenes, function (scene) {
          if (scene.el !== entry.target) return;
          scene.visible = entry.isIntersecting;
          if (scene.visible) start(scene);
          else stop(scene);
        });
      });
    },
    { threshold: 0.45 },
  );
  each(scenes, function (scene) {
    watch.observe(scene.el);
  });

  doc.addEventListener("visibilitychange", function () {
    each(scenes, doc.hidden ? stop : start);
  });
})();
