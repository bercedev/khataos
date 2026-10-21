/*
 * Download page: puts the visitor's system first, and fills in the version and
 * file sizes from the newest GitHub release. Without this script every card
 * still shows, in a fixed order, and the buttons still work: they use the
 * permanent releases/latest/download/<name> links.
 */
(function () {
  "use strict";

  var API = "https://api.github.com/repos/khataos/khataos/releases/latest";
  var CACHE = "khataos-release-v1";
  var MAX_AGE = 30 * 60 * 1000;

  function $all(sel, ctx) { return Array.prototype.slice.call((ctx || document).querySelectorAll(sel)); }

  /* ---------------------------------------------------- your device first */

  var os = window.KhataOS && window.KhataOS.os;
  var list = document.getElementById("dl-list");
  var card = os && document.getElementById("dl-" + os);
  if (list && card) {
    list.insertBefore(card, list.firstElementChild);
    card.classList.add("is-yours");
    var hint = document.getElementById("dl-hint");
    if (hint) hint.hidden = false;
  }

  /* ---------------------------------------------------- release details */

  function formatSize(bytes) {
    var mb = bytes / (1024 * 1024);
    return (mb >= 100 ? Math.round(mb) : mb.toFixed(1)) + " MB";
  }

  function formatDate(iso) {
    try {
      return new Date(iso).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
    } catch (e) { return ""; }
  }

  function readCache() {
    try {
      var raw = sessionStorage.getItem(CACHE);
      if (!raw) return null;
      var o = JSON.parse(raw);
      return Date.now() - o.t < MAX_AGE ? o.d : null;
    } catch (e) { return null; }
  }

  function writeCache(d) {
    try { sessionStorage.setItem(CACHE, JSON.stringify({ t: Date.now(), d: d })); } catch (e) { /* storage blocked */ }
  }

  function load() {
    var cached = readCache();
    if (cached) return Promise.resolve(cached);
    if (!window.fetch) return Promise.reject(new Error("no fetch"));
    return fetch(API, { headers: { Accept: "application/vnd.github+json" } }).then(function (r) {
      // 404: the repository has no published release yet.
      if (r.status === 404) return { none: true };
      if (!r.ok) throw new Error("HTTP " + r.status);
      return r.json().then(function (j) {
        return {
          tag: j.tag_name || "",
          url: j.html_url || "",
          date: j.published_at || "",
          assets: (j.assets || []).map(function (a) { return { n: a.name, s: a.size }; })
        };
      });
    }).then(function (d) { writeCache(d); return d; });
  }

  function render(d) {
    if (d.none) {
      var nr = document.getElementById("no-release");
      if (nr) nr.hidden = false;
      return;
    }
    var version = d.tag.replace(/^v/, "");
    var sizes = {};
    d.assets.forEach(function (a) { sizes[a.n] = a.s; });

    var line = document.getElementById("release-line");
    if (line && version) {
      line.querySelector("[data-version]").textContent = version;
      var date = formatDate(d.date);
      var dEl = line.querySelector("[data-date]");
      if (date) dEl.textContent = "Released " + date; else dEl.hidden = true;
      if (d.url) line.querySelector("[data-notes]").setAttribute("href", d.url);
      line.hidden = false;
    }

    // <p data-meta="KhataOS-Windows-Setup.exe">: "Version 0.1.0 · 84 MB"
    $all("[data-meta]").forEach(function (el) {
      var name = el.getAttribute("data-meta");
      var parts = [];
      if (version) parts.push("Version " + version);
      if (sizes[name] != null) parts.push(formatSize(sizes[name]));
      else if (d.assets.length) parts.push("Not in the newest release yet");
      if (parts.length) { el.textContent = parts.join(" · "); el.hidden = false; }
    });

    // <span data-size="KhataOS-Windows.msi">: "92 MB"
    $all("[data-size]").forEach(function (el) {
      var s = sizes[el.getAttribute("data-size")];
      if (s != null) { el.textContent = formatSize(s); el.hidden = false; }
    });
  }

  // If the request fails (offline, rate limited), the numbers simply stay hidden.
  load().then(render, function () {});
})();
