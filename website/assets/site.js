/*
 * KhataOS website: small progressive enhancements. Every page works without
 * this file: the theme follows the system, links go to the download page and
 * the gallery opens images directly.
 */
(function () {
  "use strict";

  var root = document.documentElement;
  var KEY = "khataos-theme";
  var mql = window.matchMedia ? window.matchMedia("(prefers-color-scheme: dark)") : null;

  function $all(sel, ctx) { return Array.prototype.slice.call((ctx || document).querySelectorAll(sel)); }

  function systemMode() { return mql && mql.matches ? "dark" : "light"; }
  function effectiveMode() {
    var t = root.getAttribute("data-theme");
    return t === "light" || t === "dark" ? t : systemMode();
  }

  /* ---------------------------------------------------- site theme */

  // <source data-scheme="dark" media="(prefers-color-scheme: dark)"> follows the
  // system; when the visitor picks a theme by hand, force the media query.
  function syncPictures() {
    var forced = root.getAttribute("data-theme");
    $all('source[data-scheme="dark"]').forEach(function (s) {
      var m = forced === "dark" ? "all" : forced === "light" ? "not all" : "(prefers-color-scheme: dark)";
      if (s.media !== m) s.media = m;
    });
  }

  function syncToggle() {
    var mode = effectiveMode();
    $all(".theme-toggle").forEach(function (b) {
      b.setAttribute("aria-label", mode === "dark" ? "Switch to light theme" : "Switch to dark theme");
      b.setAttribute("title", mode === "dark" ? "Light theme" : "Dark theme");
    });
    var meta = document.querySelector('meta[name="theme-color"]:not([media])');
    if (meta) meta.setAttribute("content", mode === "dark" ? "#14151c" : "#f5f3ee");
  }

  function setTheme(mode) {
    // Choosing the same mode as the system clears the override.
    if (mode === systemMode()) {
      root.removeAttribute("data-theme");
      try { localStorage.removeItem(KEY); } catch (e) { /* storage blocked */ }
    } else {
      root.setAttribute("data-theme", mode);
      try { localStorage.setItem(KEY, mode); } catch (e) { /* storage blocked */ }
    }
    syncPictures();
    syncToggle();
    document.dispatchEvent(new CustomEvent("sitetheme", { detail: effectiveMode() }));
  }

  $all(".theme-toggle").forEach(function (b) {
    b.addEventListener("click", function () { setTheme(effectiveMode() === "dark" ? "light" : "dark"); });
  });
  if (mql) {
    var onSystem = function () {
      syncToggle();
      if (!root.getAttribute("data-theme")) document.dispatchEvent(new CustomEvent("sitetheme", { detail: effectiveMode() }));
    };
    if (mql.addEventListener) mql.addEventListener("change", onSystem);
    else if (mql.addListener) mql.addListener(onSystem);
  }
  syncPictures();
  syncToggle();

  // Close the phone menu on Escape and when a link in it is used.
  $all("details.menu").forEach(function (d) {
    d.addEventListener("keydown", function (e) {
      if (e.key === "Escape" && d.open) { d.open = false; d.querySelector("summary").focus(); }
    });
    document.addEventListener("click", function (e) { if (d.open && !d.contains(e.target)) d.open = false; });
  });

  /* ---------------------------------------------------- device */

  function detectOS() {
    var ua = navigator.userAgent || "";
    var plat = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
    if (/android/i.test(ua)) return "android";
    if (/iphone|ipad|ipod/i.test(ua) || (/mac/i.test(plat) && navigator.maxTouchPoints > 1)) return "iphone";
    if (/win/i.test(plat) || /windows/i.test(ua)) return "windows";
    if (/mac/i.test(plat) || /macintosh/i.test(ua)) return "mac";
    if (/cros/i.test(ua)) return null;
    if (/linux|x11/i.test(plat + " " + ua)) return "linux";
    return null;
  }
  var os = detectOS();
  window.KhataOS = { os: os, effectiveMode: effectiveMode };

  var LABELS = { windows: "Download for Windows", mac: "Download for Mac", linux: "Download for Linux", android: "Download for Android" };
  $all("[data-os-label]").forEach(function (el) {
    if (os && LABELS[os]) el.textContent = LABELS[os];
  });

  /* ---------------------------------------------------- mode-aware images */

  // An <img data-light="…" data-dark="…"> shows one of two files. A link that
  // wraps it (for the lightbox) and a caption with data-light/data-dark text
  // follow along.
  function applyMode(scope, mode) {
    $all("img[data-light][data-dark]", scope).forEach(function (img) {
      var pic = img.parentNode;
      if (pic && pic.tagName === "PICTURE") {
        $all("source", pic).forEach(function (s) { s.parentNode.removeChild(s); });
      }
      var src = img.getAttribute("data-" + mode);
      if (img.getAttribute("src") !== src) img.setAttribute("src", src);
      var alt = img.getAttribute("data-alt-" + mode);
      if (alt) img.setAttribute("alt", alt);
      var link = img.closest("a.shot-link");
      if (link) link.setAttribute("href", src);
    });
    $all("[data-text-light][data-text-dark]", scope).forEach(function (el) {
      el.textContent = el.getAttribute("data-text-" + mode);
    });
    $all(".seg [data-mode]", scope).forEach(function (b) {
      b.setAttribute("aria-pressed", String(b.getAttribute("data-mode") === mode));
    });
  }

  // Each block with [data-modes] has its own Light / Dark control. Until the
  // visitor uses it, the block follows the site theme.
  $all("[data-modes]").forEach(function (block) {
    var touched = false;
    applyMode(block, effectiveMode());
    $all(".seg [data-mode]", block).forEach(function (b) {
      b.addEventListener("click", function () {
        touched = true;
        applyMode(block, b.getAttribute("data-mode"));
      });
    });
    document.addEventListener("sitetheme", function (e) { if (!touched) applyMode(block, e.detail); });
  });

  /* ---------------------------------------------------- home: theme switcher */

  var sw = document.querySelector("[data-switcher]");
  if (sw) {
    var img = sw.querySelector("[data-tv-img]");
    var nameEl = sw.querySelector("[data-tv-name]");
    var audEl = sw.querySelector("[data-tv-aud]");
    var whyEl = sw.querySelector("[data-tv-why]");
    var linkEl = sw.querySelector("[data-tv-link]");
    var current = "ledger";

    // The theme links work without scripts (they open the themes page). With
    // scripts they become toggle buttons that swap the picture here.
    $all("a.tbtn", sw).forEach(function (a) {
      var b = document.createElement("button");
      b.type = "button";
      b.className = a.className;
      b.innerHTML = a.innerHTML;
      ["t", "name", "aud", "why", "file"].forEach(function (k) {
        b.setAttribute("data-" + k, a.getAttribute("data-" + k));
      });
      b.setAttribute("aria-pressed", String(a.getAttribute("data-t") === current));
      a.parentNode.replaceChild(b, a);
    });

    var pick = function (t) {
      var btn = sw.querySelector('.tbtn[data-t="' + t + '"]');
      if (!btn) return;
      current = t;
      $all(".tbtn", sw).forEach(function (x) { x.setAttribute("aria-pressed", String(x === btn)); });
      var file = btn.getAttribute("data-file");
      var name = btn.getAttribute("data-name");
      img.setAttribute("data-light", "images/shots/" + file + "-light-home.webp");
      img.setAttribute("data-dark", "images/shots/" + file + "-dark-home.webp");
      img.setAttribute("data-alt-light", "KhataOS home screen in the " + name + " theme, light mode, with the company summary and the keyboard menu");
      img.setAttribute("data-alt-dark", "KhataOS home screen in the " + name + " theme, dark mode, with the company summary and the keyboard menu");
      nameEl.textContent = name;
      audEl.textContent = btn.getAttribute("data-aud");
      whyEl.textContent = btn.getAttribute("data-why");
      if (linkEl) linkEl.setAttribute("href", "themes/#" + t);
      var mode = sw.getAttribute("data-mode-now") || effectiveMode();
      img.classList.add("is-swapping");
      applyMode(sw, mode);
      var done = function () { img.classList.remove("is-swapping"); };
      if (img.decode) img.decode().then(done, done); else done();
    };

    sw.addEventListener("click", function (e) {
      var b = e.target.closest(".tbtn");
      if (b) pick(b.getAttribute("data-t"));
      var m = e.target.closest(".seg [data-mode]");
      if (m) sw.setAttribute("data-mode-now", m.getAttribute("data-mode"));
    });

    // Warm the cache for the other themes once the page is idle.
    var warm = function () {
      $all(".tbtn", sw).forEach(function (b) {
        var f = b.getAttribute("data-file");
        var i = new Image();
        i.src = "images/shots/" + f + "-" + effectiveMode() + "-home.webp";
      });
    };
    if ("IntersectionObserver" in window) {
      var io = new IntersectionObserver(function (es) {
        if (es.some(function (x) { return x.isIntersecting; })) { io.disconnect(); warm(); }
      }, { rootMargin: "400px" });
      io.observe(sw);
    }
  }

  /* ---------------------------------------------------- lightbox */

  var dlg = document.getElementById("lightbox");
  if (dlg && typeof dlg.showModal === "function") {
    var lbImg = dlg.querySelector("img");
    var lbCap = dlg.querySelector("[data-lb-cap]");
    var lbCount = dlg.querySelector("[data-lb-count]");
    var links = [];
    var idx = 0;
    var opener = null;

    var captionFor = function (a) {
      var fig = a.closest("figure");
      var cap = fig && fig.querySelector("figcaption");
      var t = cap ? cap.textContent : "";
      var img = a.querySelector("img");
      var sec = a.closest("[data-name]");
      var text = (t || (img && img.alt) || "").replace(/\s+/g, " ").trim();
      return sec && text.indexOf(sec.getAttribute("data-name")) !== 0 ? sec.getAttribute("data-name") + " · " + text : text;
    };

    var show = function (i) {
      idx = (i + links.length) % links.length;
      var a = links[idx];
      var img = a.querySelector("img");
      lbImg.src = a.getAttribute("href");
      lbImg.alt = img ? img.alt : "";
      lbImg.setAttribute("width", (img && img.getAttribute("width")) || "1360");
      lbImg.setAttribute("height", (img && img.getAttribute("height")) || "820");
      lbCap.textContent = captionFor(a);
      lbCount.textContent = (idx + 1) + " of " + links.length;
    };

    document.addEventListener("click", function (e) {
      var a = e.target.closest("a.shot-link");
      if (!a || e.ctrlKey || e.metaKey || e.shiftKey || e.button !== 0) return;
      e.preventDefault();
      links = $all("a.shot-link");
      opener = a;
      show(links.indexOf(a));
      dlg.showModal();
      dlg.querySelector("[data-lb-close]").focus();
    });

    dlg.addEventListener("keydown", function (e) {
      if (e.key === "ArrowRight") { e.preventDefault(); show(idx + 1); }
      else if (e.key === "ArrowLeft") { e.preventDefault(); show(idx - 1); }
      else if (e.key === "Home") { e.preventDefault(); show(0); }
      else if (e.key === "End") { e.preventDefault(); show(links.length - 1); }
    });
    dlg.querySelector("[data-lb-prev]").addEventListener("click", function () { show(idx - 1); });
    dlg.querySelector("[data-lb-next]").addEventListener("click", function () { show(idx + 1); });
    dlg.querySelector("[data-lb-close]").addEventListener("click", function () { dlg.close(); });
    // A click on the dimmed backdrop closes it.
    dlg.addEventListener("click", function (e) { if (e.target === dlg) dlg.close(); });
    dlg.addEventListener("close", function () {
      lbImg.removeAttribute("src");
      if (opener) opener.focus();
    });
  }
})();
