// Render the concept SVGs to PNG with headless Chromium (Playwright).
// Run: NODE_PATH=<dir containing playwright> node branding/tools/render.mjs [concept-dir ...]
// ImageMagick's built-in SVG renderer (MSVG) drops gradients/masks, so Chromium is the renderer.
import { createRequire } from "node:module";
import { readdirSync, existsSync, readFileSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const require = createRequire(import.meta.url);
const { chromium } = require("playwright");

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..", "concepts");
const magick = "/opt/homebrew/bin/magick";

function size(file) {
  const s = readFileSync(file, "utf8");
  const w = parseFloat(s.match(/width="([\d.]+)"/)[1]);
  const h = parseFloat(s.match(/height="([\d.]+)"/)[1]);
  return { w, h };
}

// [svg, png, target width (px) or null for 1x, transparent?, opaque24?]
function jobsFor(dir) {
  const j = [];
  const p = (f) => join(dir, f);
  const has = (f) => existsSync(p(f));
  for (const f of ["symbol.svg", "symbol-dark.svg", "symbol-mono-ink.svg", "symbol-mono-white.svg", "symbol-mono-indigo.svg"])
    j.push([p(f), p(f.replace(".svg", "-512.png")), 512, true]);
  for (const s of [16, 32, 48, 64]) j.push([p("favicon.svg"), p(`favicon-${s}.png`), s, true]);
  for (const s of [16, 32]) j.push([p("favicon-dark.svg"), p(`favicon-dark-${s}.png`), s, true]);
  for (const f of readdirSync(dir).filter((f) => /^(lockup|wordmark).*\.svg$/.test(f)))
    j.push([p(f), p(f.replace(".svg", "@2x.png")), "2x", true]);
  j.push([p("icon-1024.svg"), p("icon-1024.png"), 1024, true]);
  j.push([p("icon-1024.svg"), p("icon-512.png"), 512, true]);
  j.push([p("icon-1024-fullbleed.svg"), p("icon-1024-fullbleed.png"), 1024, false, true]);
  // Google Play hi-res icon: 512x512, full square (Play applies its own mask)
  j.push([p("icon-1024-fullbleed.svg"), p("play-icon-512.png"), 512, false, true]);
  j.push([p("usage.svg"), p("usage.png"), null, false, true]);
  for (const f of ["ic_launcher_foreground", "ic_launcher_background", "ic_launcher_monochrome"]) {
    j.push([p(`android/${f}.svg`), p(`android/${f}.png`), 1024, f !== "ic_launcher_background"]);
    j.push([p(`android/${f}.svg`), p(`android/${f}-432.png`), 432, f !== "ic_launcher_background"]);
  }
  for (const f of ["social-preview", "banner-light", "banner-dark", "social-preview-ai", "banner-ai"])
    if (has(`${f}.svg`)) j.push([p(`${f}.svg`), p(`${f}.png`), null, false, true]);
  for (const f of ["play-feature", "play-feature-ai"])
    if (has(`${f}.svg`)) j.push([p(`${f}.svg`), p(`${f}.png`), null, false, true]);
  return j;
}

const dirs = process.argv.slice(2).length
  ? process.argv.slice(2).map((d) => resolve(root, d))
  : readdirSync(root).filter((d) => /^\d-/.test(d)).map((d) => join(root, d));

const browser = await chromium.launch();
const contexts = new Map();
async function pageAt(scale) {
  if (!contexts.has(scale)) contexts.set(scale, await browser.newContext({ deviceScaleFactor: scale }));
  return contexts.get(scale).newPage();
}

let count = 0;
for (const dir of dirs) {
  for (const [svgPath, png, target, transparent, opaque] of jobsFor(dir)) {
    const { w, h } = size(svgPath);
    const scale = target === null ? 1 : target === "2x" ? 2 : target / w;
    const page = await pageAt(scale);
    await page.setViewportSize({ width: Math.ceil(w), height: Math.ceil(h) });
    await page.goto("file://" + svgPath);
    await page.waitForLoadState("load");
    await page.screenshot({ path: png, omitBackground: !!transparent, clip: { x: 0, y: 0, width: w, height: h } });
    await page.close();
    if (opaque) execFileSync(magick, [png, "-background", "white", "-alpha", "remove", "-alpha", "off", "PNG24:" + png]);
    count++;
  }
  // multi-size favicon.ico from the pixel-grid renders
  execFileSync(magick, ["favicon-16.png", "favicon-32.png", "favicon-48.png", "favicon.ico"].map((f) => join(dir, f)));
}
await browser.close();
console.log(`rendered ${count} PNGs`);
