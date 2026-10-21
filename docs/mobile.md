# KhataOS on phones and tablets (Android, iOS)

The Android and iOS apps are the desktop app: the same Rust backend
(`apps/desktop/src-tauri`) and the same TypeScript frontend (`apps/desktop/src`),
built with Tauri 2's mobile support. There is no second codebase to keep in step.
Books, reports and tax rules behave exactly as on the desktop, because the code is
the same.

KhataOS is keyboard-first. A phone keeps every keyboard shortcut (a Bluetooth
keyboard works as on a PC) and adds touch on top.

## What works on a phone

| Area | On a phone |
|---|---|
| Companies | Create, open, close. Company files live in the app's private data folder (`<app data>/companies`). |
| Vouchers, masters, inventory, banking | Everything. Amount fields open the decimal keypad. |
| Reports | Everything. Wide tables scroll sideways inside the screen. |
| Tax (GST, VAT, TDS, e-invoice JSON) | Everything that is computed in the app. |
| Sharing: **Client** mode | Connect to an office PC running KhataOS in Server mode, on the same Wi-Fi. |
| Sharing: **relay** | End-to-end encrypted sync over the internet, invites, moving relays. |
| Sharing: **accountant files** (`.khatapkg`) | Send and receive. |
| Sync | The background sync loop runs while the app is open. |
| Languages | English and Arabic (right-to-left). |

### Touch layout

`src/platform.ts` adds the `mobile` class to `<html>` on Android and iOS only, and
`src/mobile.css` styles the phone layout. Every mobile rule sits under `html.mobile`,
so the desktop layout does not change.

- **Bottom bar**: the action keys (Ctrl+A Accept, and so on) are buttons at least
  44 px tall. A long bar scrolls sideways.
- **F1–F12 button** (title strip): opens the function-key panel as a drawer. Choosing
  a key closes it, and so does tapping outside it.
- **Back button** (title strip) sends Esc, which is the app's own "go back": it closes
  a screen, asks before discarding a voucher, and so on. The Android back gesture
  does the same.
- **List pickers** (ledgers, groups, items) open as a full-width bottom sheet. The
  field you type into stays visible above the sheet. Picker rows are 44 px tall on a
  phone: `mobile.css` sets the CSS variable `--row` on `.picker`, and `picker.ts`
  reads `--row` to work out how many rows fit on a page, so the two stay in step.
  `--row` must be a px value.
- **Top bar**: the menu (Company, Go To, Import…) takes the room left after the
  brand mark and the search box, and scrolls sideways inside it.
- **Shortcut hints**: on a touch screen, buttons drop their key chord
  (`Ctrl+A: Accept` reads `Accept`), in the top-bar menu, the bottom bar and the
  picker's pinned rows (`Alt+C`, `Enter`), and the search box shows "Find"
  instead of `Alt+F`. The keys keep working. The first key
  that only a hardware keyboard sends (Ctrl, Alt, Cmd, Esc or an F-key) adds the
  `hwkeys` class to `<html>`, and the hints come back. The F-key drawer keeps
  its F1–F12 names, because Tally users know the features by those names.
- **Find / Go To** open full screen.
- **Forms** use one column: each label sits above its field.
- **Safe areas**: the layout stays clear of the notch, the status bar and the home
  indicator (`env(safe-area-inset-*)`).
- **Amounts**: `input.amount` fields get `inputmode="decimal"`. A MutationObserver
  sets this, so screens need no changes.
- To preview the phone layout in a desktop browser, open the dev server with
  `?mobile` in the URL.

## Turned off on phones (desktop only)

| Feature | Why | On a phone |
|---|---|---|
| **Office server** ("KhataOS acts as" Server or Both) | A phone is not an always-on office machine, and the OS stops background servers. | Server is saved as None, and Both as Client. `set_sharing_config` refuses Server and Both. **Host company** reports that the server isn't running. |
| **Print / Save as PDF** | Tauri can't open a second window on mobile, and the mobile webviews have no print dialog. | Print commands return an error that points to Export (CSV) or the desktop app. |
| **Quit** | iOS apps never quit themselves. | iOS: Quit does nothing. Android: Quit closes the activity. |

All the gating is in the Rust code, behind `#[cfg(mobile)]`, `#[cfg(desktop)]` and
`#[cfg(target_os = "ios")]`:

- `src/lib.rs`: `#[cfg_attr(mobile, tauri::mobile_entry_point)]` on `run()`.
- `src/commands.rs`: `quit_app`, plus `open_print_window` and `print_webview`
  (the print window).
- `src/lan.rs`: `DeviceConfig::on_phone`, `LanState::apply` and `set_sharing_config`.

Desktop builds compile exactly the same code as before.

### Files on a phone

On a phone the open and save dialogs (dialog plugin) return URIs, not paths:
`content://…` on Android (the Storage Access Framework) and a security-scoped
`file://…` URL on iOS. `std::fs` can't open either. `src/phone_files.rs` opens
them through `tauri-plugin-fs`, which asks the OS for a file descriptor:

- `write_csv` (report exports), `write_gst_json` (GST, e-invoice, e-way bill JSON
  and ZATCA, XRechnung and CFDI XML) and `read_text_file` (bank statements) check
  for a URI first, behind `#[cfg(mobile)]`. A plain path, and every desktop build,
  takes the same code as before.
- The GSTR-2B JSON is read the same way (`phone_files::read_text`), and binary
  PDF files (Factur-X, invoice PDFs) are written through a URI too
  (`peppol::write_binary_uri`).
- Files that need a real path go through `phone_files::local_input` (copy a picked
  file in) and `local_output` / `finish_output` (write locally, then copy out):
  Tally XML import (`import_tally`, up to 200 MB a file), accountant files
  (`.khatapkg` send, inspect and receive in `package.rs`), and the invoice logo and
  signature images (`invoice_design.rs`).
- The extension checks stay. A `file://` URL must end in `.csv` (or `.json`/`.xml`,
  or `.csv`/`.txt` for bank statements), as a desktop path must. An Android
  `content://` URI often has no file name in it
  (`…/document/msf%3A1000000123`). If it shows one, the extension must match. If it
  doesn't, the write is allowed, because Android only opens a document that the
  user picked in its own dialog.
- The 20 MB limit on bank statements stays.
- The fs plugin is registered only on Android and iOS (`src/lib.rs`), and only Rust
  calls it. No capability grants its `fs:` commands to the page, so the webview can
  touch no more files than before.

Known rough edges, to fix later:

- Some list screens move by arrow keys only. Tapping a row may select it without
  opening it. Enter is on the bottom bar wherever a screen offers it.

## Building

All commands run from `apps/desktop`. The Tauri CLI comes from the
`@tauri-apps/cli` dev dependency (`pnpm tauri …`).

### One-time setup

```sh
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
```

- Android: the Android SDK with an NDK (built here with NDK 28.2.13676358), a JDK
  (17 or newer), and these environment variables:
  `ANDROID_HOME=~/Library/Android/sdk`, `NDK_HOME=$ANDROID_HOME/ndk/<version>`,
  `JAVA_HOME=…`.
- iOS: Xcode, plus `xcodegen` and CocoaPods. `tauri ios init` installs or updates
  them through Homebrew when they are missing.

The generated projects are in `src-tauri/gen/android` and `src-tauri/gen/apple`.
To regenerate them, run `pnpm tauri android init` or `pnpm tauri ios init`. After
regenerating, put the KhataOS icons back. Write them to a temporary folder first,
so the desktop icons stay as they are:

```sh
npx tauri icon src-tauri/icons/icon.png -o /tmp/khata-icons --ios-color "#2b2f77"
cp -R /tmp/khata-icons/android/mipmap-* src-tauri/gen/android/app/src/main/res/
cp /tmp/khata-icons/android/values/ic_launcher_background.xml src-tauri/gen/android/app/src/main/res/values/
cp /tmp/khata-icons/ios/*.png src-tauri/gen/apple/Assets.xcassets/AppIcon.appiconset/
```

### Identifiers

The desktop app keeps `org.khataos.desktop`, and its data folder depends on that
identifier. The phone apps use **`org.khataos.mobile`** (an identifier ending in `.app` clashes with the app-bundle extension, and the Tauri CLI warns about it), set in
`src-tauri/tauri.android.conf.json` and `src-tauri/tauri.ios.conf.json`. Tauri merges
those files only into Android or iOS builds. The minimum versions are Android 7.0
(API 24) and iOS 15.

### Android

```sh
pnpm tauri android build --debug --apk --target aarch64      # phones
pnpm tauri android build --debug --apk                       # all four ABIs
pnpm tauri android dev                                       # live reload on a device or emulator
```

The debug APK is written to
`src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`.
To install it and take a screenshot:

```sh
adb install -r <apk>
adb shell am start -n org.khataos.mobile/.MainActivity
adb exec-out screencap -p > khataos-android.png
```

The emulator needs emulator 31.2.7 or newer for the API 31 arm64 image. Install
it with Android Studio → SDK Manager → Android Emulator.

#### Release APKs

The release workflow (`.github/workflows/release.yml`) builds one APK per phone
type, then one universal APK with all three:

```sh
pnpm tauri android build --apk --split-per-abi --target aarch64 armv7 x86_64
pnpm tauri android build --apk --target aarch64 armv7 x86_64
```

The second command reuses the compiled dependencies from the first, but the
Tauri CLI makes cargo build and link the app crate again for each ABI, which
takes a few minutes with LTO. The APKs are written to
`src-tauri/gen/android/app/build/outputs/apk/<flavour>/release/app-<flavour>-release.apk`
(`-release-unsigned.apk` without the signing key, see "Android: Google Play"):

| Flavour | ABI | For | Stable download name |
|---|---|---|---|
| `arm64` | arm64-v8a | almost every phone since 2017 | `KhataOS-Android.apk` |
| `arm` | armeabi-v7a | older 32 bit phones | `KhataOS-Android-32bit.apk` |
| `x86_64` | x86_64 | emulators and some Chromebooks | `KhataOS-Android-x86_64.apk` |
| `universal` | all three | any phone, in one larger file | `KhataOS-Android-universal.apk` |

All four have the same package name, version code and signing key, so any of
them installs over another.

#### Download size

Measured in October 2026 on an Apple silicon Mac:

| Build | APK | Rust library (arm64) |
|---|---|---|
| Universal APK of v0.1.0 alpha 1 (all three ABIs) | 101 MB | 37.9 MB, stored (28 to 38.5 MB for each ABI) |
| Split per ABI, old profile | 39.6 MB | 37.9 MB, stored |
| Plus `[profile.release]` in the root `Cargo.toml` (strip, LTO, one codegen unit) | 24.6 MB | 22.9 MB, stored |
| Plus compressed native libraries (`useLegacyPackaging`) | 12.5 MB | 10.9 MB compressed |

- **Symbols.** Without `strip = true` the library kept its symbol table
  (about 11.7 MB of the 37.9 MB): the Android Gradle plugin's own stripping
  doesn't remove it.
- **Compressed libraries.** `app/build.gradle.kts` sets
  `packaging.jniLibs.useLegacyPackaging = true`. For apps that support only
  Android 6 and later, the Android Gradle plugin otherwise stores native
  libraries uncompressed, so they can run straight from the APK. Compressed, the
  download is half the size. Android unpacks the library once at install, so the
  app takes about 35 MB on the phone instead of 25 MB. Google Play
  makes its own split APKs from the `.aab`, and compresses them for download
  either way.
- **opt-level "s"** would save another 2 MB, but made opening a 50,000 voucher
  company about 25% slower, so the profile keeps `opt-level = 3`.
- **Fonts.** The PDF fonts in `crates/khata-print/assets` are deflated into the
  library at build time: about 0.9 MB in all (DejaVu Sans 0.38 MB, the ten Noto
  fonts 0.5 MB). Trimming them would save little and risk missing characters in
  PDFs, so they stay as they are.

### iOS

A Simulator build needs no signing team:

```sh
pnpm tauri ios build --debug --target aarch64-sim
xcrun simctl boot "iPhone 17"
xcrun simctl install booted <path to KhataOS.app>
xcrun simctl launch booted org.khataos.mobile
xcrun simctl io booted screenshot khataos-ios.png
```

`pnpm tauri ios dev` runs the app with live reload in a Simulator.

The Tauri CLI refuses to build when no Simulator runtime matches the Xcode SDK
version. For example, Xcode 27 with only the iOS 26.5 runtime fails with "Xcode
Simulator SDK 27.0 is not installed". Install the matching runtime from Xcode →
Settings → Components. As a stopgap, put a small `xcrun` wrapper first on `PATH`.
For `xcrun simctl list runtimes --json`, the wrapper adds a copy of the installed
runtime labelled with the SDK version; every other call goes to `/usr/bin/xcrun`
unchanged. The app built this way runs on the older runtime, because its
deployment target is iOS 15.

## Releasing to the stores

No secret is in the repository. The signing keys and their passwords come from
environment variables, or from the developer's own Apple account in Xcode. A
template for a local, untracked shell file (for example `~/.khataos-signing.env`,
loaded with `source` before a release build):

```sh
# Android: the Play upload key (never commit the .jks file or this file)
export KHATAOS_ANDROID_KEYSTORE="$HOME/keys/khataos-upload.jks"
export KHATAOS_ANDROID_KEYSTORE_PASSWORD="…"
export KHATAOS_ANDROID_KEY_ALIAS="upload"         # optional, default "upload"
export KHATAOS_ANDROID_KEY_PASSWORD="…"           # optional, default: the keystore password
# iOS: your Apple Developer team id (Membership details, 10 characters)
export APPLE_DEVELOPMENT_TEAM="ABCDE12345"
```

### Android: Google Play

`gen/android/app/build.gradle.kts` has a `release` signing config. It is used only
when `KHATAOS_ANDROID_KEYSTORE` is set. Without that variable a release build is
left unsigned, as before. If the variable is set but the password isn't, the build
stops with an error. `gen/android/.gitignore` ignores `*.jks`, `*.keystore` and
`keystore.properties`.

1. Create the upload key once. Keep it, and a backup, outside the repository:

   ```sh
   keytool -genkeypair -v -keystore ~/keys/khataos-upload.jks -alias upload \
     -keyalg RSA -keysize 2048 -validity 10000
   ```

2. Build the bundle: `source ~/.khataos-signing.env && pnpm tauri android build --aab`.
   The signed bundle is written to
   `src-tauri/gen/android/app/build/outputs/bundle/universalRelease/app-universal-release.aab`.
   Raise `version` in `tauri.conf.json` for each upload: Tauri turns it into the
   `versionCode`.
3. In [Google Play Console](https://play.google.com/console): **Create app**, then
   choose a name, the default language, App and Free.
4. **Test and release → Setup → App signing**: keep Play App Signing (Google holds
   the app-signing key, and you upload with the upload key from step 1).
5. Fill in **App content**:
   - **Privacy policy**: a public URL with the privacy text below.
   - **Data safety**: no data collected or shared by the developer. Data is
     encrypted in transit (relay sync is end-to-end encrypted). Users can delete
     their data by deleting the company file or uninstalling the app.
   - **Ads**: none. **Target audience**: 18 and over (business software).
     **Content rating**: fill in the questionnaire (a utility with no user-generated
     content).
6. **Testing → Internal testing**: create a release and upload the `.aab`. Add
   testers, then promote the release to Closed testing and Production. A new personal
   developer account must run a closed test with testers before Production.

### iOS: App Store Connect

Simulator builds need no team. A device or App Store build needs one: set
`APPLE_DEVELOPMENT_TEAM`, which the Tauri CLI reads. You can instead put
`"developmentTeam": "ABCDE12345"` under `bundle.iOS` in `tauri.ios.conf.json`. A
team id isn't secret, but leaving it out keeps the repository usable by forks.

1. Join the Apple Developer Program. In
   [Certificates, Identifiers & Profiles](https://developer.apple.com/account/resources),
   register the App ID `org.khataos.mobile`. It needs no extra capabilities.
2. In [App Store Connect](https://appstoreconnect.apple.com): **Apps → + → New App**,
   iOS, name KhataOS, bundle id `org.khataos.mobile`, and a SKU.
3. Build and export:
   `source ~/.khataos-signing.env && pnpm tauri ios build --export-method app-store-connect`.
   Xcode's automatic signing creates the distribution certificate and profile when
   you are signed in to the team in Xcode → Settings → Accounts.
4. Upload the `.ipa` with Transporter, or with
   `xcrun altool --upload-app -t ios -f <ipa> --apiKey … --apiIssuer …` (an App
   Store Connect API key, kept outside the repository).
5. **App Privacy**: "Data Not Collected". **Export compliance**: the app uses
   standard encryption (HTTPS and the end-to-end encrypted relay), so answer the
   encryption questions accordingly (normally the exemption for standard
   algorithms applies). Then add `ITSAppUsesNonExemptEncryption` to
   `src-tauri/Info.ios.plist` to skip the question on later uploads.
6. **App Review**: no sign-in is needed. In the review notes, explain that a
   reviewer can create a company on the first screen in a few seconds,
   and that Client mode needs a KhataOS PC on the same network, so it is optional.
7. Add screenshots (6.9-inch and 13-inch iPad), then submit through TestFlight
   first.

#### Info.plist: local network

Client mode connects to a KhataOS PC on the office Wi-Fi, so iOS asks the user for
permission first and shows `NSLocalNetworkUsageDescription`. That text lives in
`src-tauri/Info.ios.plist`, which the Tauri CLI merges into the generated
`gen/apple/khataos-desktop_iOS/Info.plist` on every iOS build. Edit
`Info.ios.plist`, not the generated file. Desktop builds don't read it. The app
sets no `NSBonjourServices`, because it doesn't browse for services: you type the
server's address, and a `.local` name resolves without it.

### Privacy text (store listing and privacy policy)

> **KhataOS keeps your books on your device.** KhataOS is local-first accounting
> software. Your companies, vouchers and reports are stored in files on this
> device, in the app's private storage, and nowhere else unless you choose to share
> them.
>
> **No accounts, no analytics, no ads.** KhataOS has no sign-up, no tracking, no
> analytics or crash-reporting services, and no advertising. The developers
> receive no data from the app.
>
> **Sharing is your choice.** If you connect to a KhataOS server on your office
> network, your data goes to that computer. If you use internet sync, your data is
> end-to-end encrypted on this device before it leaves, so the relay server you
> choose can't read it. If you send an accountant file, it goes only where you send
> it. Filing returns (e.g. e-invoices to the GST portal or HMRC) sends the return to
> that tax authority when you ask for it.
>
> **Permissions.** Local network: only to reach the KhataOS server on your office
> network. Files: only the files you pick to import or export.
>
> **Deleting your data.** Uninstall the app, or clear its storage in the system
> settings. There is no command to delete a company inside the app yet.

## Future work

- **Cleartext HTTP (Android)**: the Rust networking (ureq) uses its own sockets, so
  Android's `usesCleartextTraffic=false` does not affect plain-HTTP connections to an
  office server. Check this on a release build before shipping.
- **Printing**: invoices, notes and vouchers already have a pure-Rust PDF writer
  (`crates/khata-print/src/pdf`), reached from a saved voucher's Export menu
  (Ctrl+E → PDF), which works on phones. Report printing (Ctrl+P) is still off on
  phones: render reports to PDF the same way, or hand the HTML to the platform print
  services (`UIPrintInteractionController`, Android `PrintManager`) through a small
  Tauri plugin, then offer Share.
- **Share sheet**: exports already save through the system file dialog. A
  "Share…" action (mail, WhatsApp) would need a small share plugin.
- **Tablets**: an iPad or Android tablet in landscape could keep the desktop layout
  with the F-key column. That only needs a width check in `platform.ts`.
