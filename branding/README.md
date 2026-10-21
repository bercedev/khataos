# KhataOS brand

**Status: Spine (`concepts/1-spine`) was chosen on 2026-10-21.** Applied: desktop, Android and iOS app icons (`tauri icon` from `icon-manifest.json`; the iOS set regenerated from `icon-1024-fullbleed.png`), the favicon in `apps/desktop/public/`, and the README banner. The in-app mark in `style.css` is applied with the theme system. Still to do by hand: the GitHub social preview upload and the Play Store listing (steps 5 and 6 below). Only Spine is published in this repository; the two other concepts, the comparison sheets, the theme screenshots and the AI log stay with the maintainers.

| # | Concept | Idea |
|---|---|---|
| 1 | [Spine](concepts/1-spine/) | A bound ledger reduced to two shapes: a marigold spine standing proud of an indigo cover, with the accountant's double rule under a total. |
| 2 | Ruled K (not published) | The K of Khata set from ledger rules: one marigold margin rule and five indigo rules stepping out like keystrokes. |
| 3 | Balance Key (not published) | A keycap with an equals sign on its face: debits equal credits, and every action is one key away. |

## Contents

```
branding/
  README.md            this file: palette, rules, swap steps
  tools/
    build.py           hand-built geometry -> every SVG (python3.11 with fontTools)
    textpath.py        outlines Inter into SVG paths, applying the font's kerning
    render.mjs         SVG -> PNG with headless Chromium (Playwright)
  concepts/
    index.html         side-by-side comparison sheet
    naming-study.svg   "KhataOS" vs "Khata OS"
    <n>-<name>/
      symbol.svg                    master symbol, 64-unit grid, full colour for light backgrounds
      symbol-dark.svg               for dark backgrounds and the indigo top bar
      symbol-mono-{ink,white,indigo}.svg   one colour
      favicon.svg, favicon-dark.svg, favicon-mono.svg   16-px pixel-grid versions
      favicon-auto.svg              switches with prefers-color-scheme
      favicon-{16,32,48,64}.png, favicon.ico
      wordmark*.svg                 "KhataOS" outlined, light, dark and mono
      lockup-horizontal*.svg, lockup-stacked*.svg  (+ @2x PNGs)
      icon-1024.svg/.png            macOS master: squircle, 100 px platform padding, transparent
      icon-512.png
      icon-1024-fullbleed.svg/.png  square and opaque, for iOS, the App Store and Windows tiles
      play-icon-512.png             Google Play hi-res icon (full square, opaque)
      icon-manifest.json            tauri icon manifest with Android adaptive layers
      android/ic_launcher_{foreground,background,monochrome}.svg/.png (+ -432.png)
      social-preview.svg/.png       GitHub, 1280x640
      play-feature.svg/.png         Google Play feature graphic, 1024x500, 24-bit, no alpha
      banner-light.svg/.png, banner-dark.svg/.png   README, 1600x400
      *-ai.svg/.png                 optional variants on an AI photo background
      usage.svg/.png                do and don't
      ai/                           AI exploration sheet and hero background (reference only)
```

To rebuild after a change:

```sh
/opt/homebrew/opt/python@3.11/bin/python3.11 branding/tools/build.py
NODE_PATH=<a node_modules folder containing playwright> node branding/tools/render.mjs
```

`build.py` looks for Inter at `$INTER_TTF`, with a local default path. Any copy of Inter 3.x or 4.x works, for example from [rsms/inter](https://github.com/rsms/inter/releases). ImageMagick 7's built-in SVG renderer (MSVG) can't draw these files: it drops fills, masks and gradients, and `rsvg-convert` isn't installed. So Chromium renders the PNGs and `magick` only builds `favicon.ico` and strips alpha from the store graphics.

## Typography

- **Wordmark:** Inter 3.019 by The Inter Project Authors, under the **SIL Open Font License 1.1**. The letters are converted to SVG outlines, applying the font's own GPOS kerning, so no file depends on a font.
  - The OFL lets you use a font in artwork, including outlined logos. A logo is not "Font Software", so no licence file has to travel with the brand files.
  - The weights differ by concept:

    | Concept | "Khata" | "OS" | Tracking |
    |---|---|---|---|
    | Spine | SemiBold 600 | SemiBold 600 | -1.2% |
    | Ruled K | Bold 700 | Bold 700 | -2% |
    | Balance Key | Medium 500 | Bold 700 | -1% |
- **Name styling:** "KhataOS" is one word with "OS" in the accent colour, which matches the app's `Khata<b>OS</b>`. Where colour isn't available, as in one-colour versions, the name keeps one colour and stays one word. Never "KHATAOS" or "Khataos" in the logo.
- **Taglines and UI text** use Inter, as the app does. Taglines are English only. Any Arabic or Devanagari version of the name or tagline must come from a native speaker. These files contain none.

## Palette

The brand colours are the app's own tokens, so the product and the brand stay one system. They are the default **Ledger** theme in `apps/desktop/src/themes.css`, and `node apps/desktop/scripts/theme-check.mjs` checks every pair to WCAG 2.2 AA. Contrast ratios are WCAG 2.x. AA needs 4.5:1 for normal text, and 3:1 for large text (24 px, or 18.7 px bold) and for graphics.

| Token | Hex | Role | Contrast | AA text? |
|---|---|---|---|---|
| Ledger indigo (`--brand`) | `#2b2f77` | Primary: symbol, "Khata", top bar | 11.8:1 on white, 10.7:1 on paper | Yes |
| Indigo 600 (`--brand-2`) | `#3f45a8` | Keycap face, hover | 8.0:1 on white | Yes |
| Marigold (`--accent`) | `#c76a12` | Accent on light: spine, margin rule, "OS", the cursor row's edge | 3.8:1 on white, **3.4:1 on paper**, 3.1:1 on the cursor row | Large text and graphics only |
| Marigold text (`--accent-text`) | `#a5580e` | Small marigold text on paper, white and the bars: key letters | 5.3:1 on white, 4.7:1 on paper, 4.5:1 on surface-2 | Yes |
| Marigold on the cursor row (`--sel-accent-text`) | `#954f0b` | Key letters on the highlighted row, where `#a5580e` is only 4.25:1 | 5.0:1 on the cursor row | Yes |
| Marigold light (`--topbar-key`) | `#f2a65a` | Accent on dark and on indigo: keys, "OS" | 5.8:1 on indigo, 9.0:1 on night | Yes |
| Paper (`--bg`) | `#f5f3ee` | Light background; cover on dark | Ink on paper 14.9:1 | Yes |
| Paper light | `#faf7f0` | Book cover in the mark on dark and on indigo (`symbol-dark.svg`); not a window colour | 11.1:1 against indigo | Yes |
| Surface 2 (`--surface-2`) | `#f0eee8` | Title strip, F-key panel, table heads | Ink 14.3:1, muted 5.2:1 | Yes |
| Cursor row (`--sel`) | `#e3e6fc` | The keyboard cursor's row | Ink 13.4:1, indigo 9.6:1, muted 4.9:1 | Yes |
| Ink (`--ink`) | `#1c1e2a` | Body text; one-colour mark | 14.9:1 on paper | Yes |
| Muted (`--muted`) | `#5e6276` | Secondary text on light | 5.4:1 on paper, 6.0:1 on white, 4.9:1 on the cursor row | Yes |
| Warning (`--warn`) | `#9c5104` | Warning text | 5.9:1 on white, 5.3:1 on paper | Yes |
| Profit (`--pos`) | `#1b6a8a` | Profit and positive results, blue-teal for colour-blind safety (loss stays red `#b4232a`) | 6.0:1 on white, 5.4:1 on paper | Yes |
| Field border (`--field-line`) | `#87857f` | Text fields and other control boundaries (WCAG 1.4.11) | 3.7:1 on white, 3.3:1 on paper | Graphics |
| Top-bar muted | `#c3c5dc` | Secondary text on indigo | 7.0:1 on indigo | Yes |
| Night (`--bg` dark) | `#14151c` | Dark background | Paper on night 16.4:1 | Yes |
| Muted dark | `#9b9eb1` | Secondary text on night | 6.9:1 on night, 4.5:1 on the dark cursor row | Yes |
| Cursor row dark | `#2c3263` | The keyboard cursor's row on night | Ink 10.8:1 | Yes |
| Periwinkle (`--brand` dark) | `#8f96ff` | Indigo's stand-in on dark (keycap skirt) | 6.9:1 on night | Yes |

Marigold as text: use `#a5580e` for small text on paper, white and surface-2, and `#954f0b` for text on the cursor row. `#c76a12` is for the mark, rules and large text only. The window paper stays `#f5f3ee`; `#faf7f0` is the book cover in the mark only.

Pairs to avoid as text:
- marigold `#c76a12` on paper at body size (3.4:1);
- marigold light `#f2a65a` on Indigo 600 (3.95:1);
- indigo on marigold (3.1:1).

The logo is exempt from the contrast rules. It still clears 3:1 against every background it is supplied for.

The blue-and-marigold pair is deliberately not Tally's palette, or QuickBooks green, Xero sky blue or Zoho's four colours. It is the indigo rules and red-orange margin of real ledger paper.

## Clear space and minimum size

- **Clear space:** keep a margin of **one quarter of the symbol's height** free on every side of the symbol or lockup. Text, edges and other logos stay outside it. `usage.png` shows the box.
- **Lockup proportions:**
  - Horizontal: the wordmark's cap height is 41% of the symbol box, and the gap between them is 20% of the box. The wordmark's cap height is centred on the symbol.
  - Stacked: the wordmark is centred under the symbol with a gap of 18% of the box.
  - Don't rebuild the lockups by hand. Use the files.
- **Minimum sizes:**

  | Asset | Screen | Print |
  |---|---|---|
  | Symbol | 16 px | 6 mm |
  | Horizontal lockup | 96 px wide | 25 mm wide |
  | Stacked lockup | 64 px wide | 18 mm wide |
- **Which symbol file to use at each size:**
  - Below 48 px, use `favicon.svg`. It is drawn on a 16-pixel grid, so edges land on whole pixels at 16 and 32 px.
  - From 48 px up, use `symbol.svg`.
  - Below 24 px, drop the wordmark and use the symbol alone.

## Do and don't

The examples are in `concepts/<n>-<name>/usage.png`.

**Do:**
- Use the supplied files: full colour on light, the dark colourway on dark or on the indigo top bar, and one colour where only one ink is available.
- Keep the clear space.
- Put photos behind a solid or gradient scrim. See the `*-ai` variants.
- Use the pixel-grid favicon below 48 px.

**Don't:**
- Stretch, squash, rotate or skew the symbol.
- Recolour it outside the palette.
- Add shadows, glows, bevels or gradients. The only exception is the macOS icon's platform shadow and sheen.
- Use the light colourway on a dark background, or the reverse.
- Place it on busy patterns or low-contrast photos.
- Outline it.
- Change the space between the symbol and the wordmark, or move the wordmark to the left of the symbol.
- Set "KhataOS" in another typeface.
- Add a tagline inside the lockup.

**Right-to-left:** the logo does not mirror in Arabic layouts, because a logo is a fixed picture. The app's small in-product Spine mark may keep mirroring its spine to the inline-start side, as it does today, but don't mirror Ruled K or Balance Key.

## Originality and clean-room check

The concepts were drawn from first principles: ledger book, ledger ruling, double-entry "=", keycap. The AI images were used only as a sounding board (the log of AI jobs is kept by the maintainers), and nothing was traced.

Each concept was checked by eye against the marks of the brands below. This was a check of well-known marks from general knowledge, not a trademark search.

| Brand | Its mark | KhataOS concepts |
|---|---|---|
| Tally / TallyPrime | A wordmark-led logo | No "T", no Tally type or colours. None of the three uses a wordmark-only approach or Tally's letterform. |
| Zoho / Zoho Books | Four coloured tiles | No multicolour tiles |
| QuickBooks | Green circle with "qb" | No circles, no green |
| Xero | Blue circle with a lowercase script wordmark | No circle, no script |
| Busy, Marg ERP, Vyapar | Red-led wordmarks and marks | Red is not a brand colour here |
| Stock icons | Book icons with page edges, ribbons or open spreads; calculator "=" buttons; striped letters | Spine has a proud spine and a double rule. Balance Key has a keycap skirt. Ruled K has offset rules rather than a striped solid letter. |

**Before launch:**
1. Run a proper trademark search on the name and the chosen mark: India TMR classes 9 and 42, UAE, KSA, Singapore, the UK, and WIPO Global Brand Database.
2. Note that **Khatabook**, an Indian ledger app, shares the word "khata". Generic as the word is, it is the nearest name in the market. The search should cover it, and the mark should stay clearly unlike a book-plus-K, which is why no concept combines the two.

## Applying the chosen concept

Set `CHOSEN` to the chosen concept's folder, then follow the steps below. Nothing here has been run yet.

```sh
CHOSEN=concepts/1-spine
```

1. **Desktop, Android and iOS icons.** Run this from `apps/desktop`, the package with the `tauri` script:

   ```sh
   cd apps/desktop
   pnpm tauri icon ../../branding/$CHOSEN/icon-1024.png
   ```

   - The command writes the desktop set into `src-tauri/icons/`: `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico` and Windows Store sizes. The names in `tauri.conf.json` → `bundle.icon` already match.
   - It also writes mobile icon sets (`icons/android`, `icons/ios`).
   - To get proper **Android adaptive layers** (foreground, background and Android 13 themed monochrome), use the manifest instead of the PNG:

     ```sh
     pnpm tauri icon ../../branding/$CHOSEN/icon-manifest.json
     ```

   - **iOS:** iOS icons must be full-bleed and opaque. `tauri icon` flattens the padded macOS master onto the manifest's `bg_color`, which is set to the tile colour. Check `src-tauri/gen/apple/Assets.xcassets/AppIcon.appiconset/` afterwards.
     - If the iOS icon shows a squircle inside a square, regenerate the iOS set from the full-bleed master into a scratch folder:

       ```sh
       pnpm tauri icon ../../branding/$CHOSEN/icon-1024-fullbleed.png -o /tmp/khataos-icons
       ```

     - Then copy `/tmp/khataos-icons/ios/*` over the files in `AppIcon.appiconset`. If `tauri icon` already refreshed `src-tauri/gen/android` or `gen/apple` itself, compare with `git status` before you commit.
   - **Store listings:**
     - App Store: `icon-1024-fullbleed.png`, 1024×1024, opaque.
     - Google Play hi-res icon: `play-icon-512.png`.

2. **The in-app mark.** Copy the symbols next to the stylesheet, then replace the two placeholder rules in `apps/desktop/src/style.css`:

   ```sh
   mkdir -p apps/desktop/src/brand
   cp branding/$CHOSEN/symbol.svg branding/$CHOSEN/symbol-dark.svg apps/desktop/src/brand/
   ```

   ```css
   /* top bar: always indigo, so always the dark colourway */
   .brand-mark {
     width: 22px;
     height: 22px;
     background: url("./brand/symbol-dark.svg") center / contain no-repeat;
   }

   .startup-mark {
     width: 72px;
     height: 72px;
     background: url("./brand/symbol.svg") center / contain no-repeat;
   }

   @media (prefers-color-scheme: dark) {
     .startup-mark { background-image: url("./brand/symbol-dark.svg"); }
   }
   ```

   - Remove the old `linear-gradient(to var(--spine) …)` backgrounds, and the startup mark's `border-radius` and `box-shadow`.
   - **Spine only:** to keep today's right-to-left behaviour, add `[dir="rtl"] .brand-mark, [dir="rtl"] .startup-mark { transform: scaleX(-1); }`. After that, `--spine` is no longer used and can go.
   - Ruled K and Balance Key must not mirror.
   - The brand text in `shell.ts` (`Khata<b>OS</b>`) can stay as live text, because it already matches the wordmark's colours and Inter.

3. **Favicon** (for any web build, the docs site or the Vite dev page):

   ```sh
   mkdir -p apps/desktop/public
   cp branding/$CHOSEN/favicon-auto.svg apps/desktop/public/favicon.svg
   cp branding/$CHOSEN/favicon.ico apps/desktop/public/favicon.ico
   ```

   Then add this to the `<head>` of `apps/desktop/index.html`:

   ```html
   <link rel="icon" href="/favicon.svg" type="image/svg+xml">
   <link rel="icon" href="/favicon.ico" sizes="16x16 32x32 48x48">
   ```

4. **README banner.** Put this at the top of the root `README.md`, above `# KhataOS`:

   ```html
   <picture>
     <source media="(prefers-color-scheme: dark)" srcset="branding/concepts/1-spine/banner-dark.png">
     <img alt="KhataOS: open-source, local-first accounting" src="branding/concepts/1-spine/banner-light.png" width="800">
   </picture>
   ```

   Replace `concepts/1-spine` with the chosen folder. The PNGs are used rather than the SVGs so the outlined text renders the same everywhere.

5. **GitHub social preview.** Upload `branding/$CHOSEN/social-preview.png` under repository **Settings → General → Social preview**. There is no API for this. It is 1280×640 and keeps the content inside a 40 px border. `social-preview-ai.png` is an optional alternative.

6. **Google Play listing.** In the Play Console, under **Grow → Store presence → Main store listing**:
   - Feature graphic: `play-feature.png`, 1024×500, 24-bit PNG with no alpha. `play-feature-ai.png` is an alternative.
   - App icon: `play-icon-512.png`.

7. **Check the result.**
   - Run `pnpm app:dev` and look at the top bar, the startup screen, dark mode and Arabic (right-to-left).
   - Build once for each platform to see the dock, taskbar and launcher icons.

8. **Tidy up.**
   - Once the choice is settled, move the chosen folder to `branding/<name>/` if you prefer shorter paths, and update the paths above and in the README banner.
   - Delete the other concepts, or keep them for the record.
