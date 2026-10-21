// Download page (docs/download.md): moves the card for the visitor's device
// to the top and highlights it. Every card is a plain link that works without
// this script, and nothing is fetched.
(function () {
  "use strict";

  function detect(ua, touchPoints) {
    if (/Android/i.test(ua)) return "android";
    // iPadOS reports itself as a Mac, but a Mac has no touch screen.
    if (/iPhone|iPad|iPod/i.test(ua)) return "iphone";
    if (/Macintosh/i.test(ua) && touchPoints > 1) return "iphone";
    if (/Windows/i.test(ua)) return "windows";
    if (/Macintosh|Mac OS X/i.test(ua)) return "mac";
    // A Chromebook could use the Android or the Linux build: show both.
    if (/CrOS/i.test(ua)) return null;
    if (/Linux|X11/i.test(ua)) return "linux";
    return null;
  }

  function setup() {
    var list = document.getElementById("kd-systems");
    var pick = document.getElementById("kd-pick");
    var pickList = document.getElementById("kd-pick-list");
    if (!list || !pick || !pickList || pickList.children.length) return;

    var os = detect(navigator.userAgent || "", navigator.maxTouchPoints || 0);
    document.documentElement.setAttribute("data-kd-os", os || "unknown");
    if (!os) return;

    var card = list.querySelector('[data-os="' + os + '"]');
    if (!card) return;
    card.classList.add("kd-card--primary");
    pickList.appendChild(card);
    pick.hidden = false;

    var others = document.getElementById("kd-others");
    if (others) others.textContent = "Other systems";
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", setup);
  } else {
    setup();
  }
  // Material's instant navigation, if it is ever turned on.
  if (window.document$ && window.document$.subscribe) {
    window.document$.subscribe(setup);
  }
})();
