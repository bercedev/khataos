# Install

**What you'll do:** build KhataOS from its source code and run it on Windows, macOS or Linux. You can also build it for an Android phone or an iPhone.

> **There are no ready-made downloads yet.** Until the first release, you build KhataOS yourself.

## Desktop (Windows, macOS, Linux)

### 1. Install the tools

You need:

- **Rust** 1.85 or newer. Install it from [rustup.rs](https://rustup.rs).
- **Node.js 22** and **pnpm**. Install Node from [nodejs.org](https://nodejs.org), then pnpm from [pnpm.io](https://pnpm.io).
- **Git**, to download the code.
- The system libraries Tauri needs (see [Tauri's prerequisites](https://tauri.app/start/prerequisites/)):
  - **macOS:** the Xcode command-line tools (`xcode-select --install`).
  - **Windows:** the Microsoft C++ Build Tools and WebView2 (WebView2 comes with Windows 10 and 11).
  - **Linux:** WebKitGTK. On Ubuntu or Debian:

    ```sh
    sudo apt-get install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
    ```

### 2. Download and run

```sh
git clone https://github.com/khataos/khataos.git
cd khataos/apps/desktop
pnpm install
pnpm app:dev
```

The first run compiles everything, so it takes a while. Then the KhataOS window opens on the company list. Next: [Create your first company](first-company.md).

### 3. Build an installer (optional)

```sh
pnpm app:build
```

This makes an installer for your system (for example a `.dmg` on macOS, `.msi` or `.exe` on Windows, `.deb` or `.AppImage` on Linux) under `target/release/bundle/` in the repository. These builds aren't signed, so your system may warn when you open one.

### Update

```sh
cd khataos
git pull
cd apps/desktop
pnpm install
pnpm app:dev
```

Your company files are kept outside the code folder, so updating doesn't touch them. Still, [back them up](backup.md) first: KhataOS is in alpha.

## Android and iOS

The phone apps are the same app, built with Tauri's mobile support. There are no store releases yet.

**Android** (from a Mac, Windows or Linux computer):

1. Install Android Studio with the Android SDK and NDK, and JDK 17 or newer. Set `ANDROID_HOME`, `NDK_HOME` and `JAVA_HOME`.
2. Add the Rust targets:

   ```sh
   rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
   ```

3. In `apps/desktop`, build a test APK for most modern phones:

   ```sh
   pnpm tauri android build --debug --apk --target aarch64
   ```

4. The APK is under `src-tauri/gen/android/app/build/outputs/apk/`. Copy it to the phone and install it (allow installs from unknown sources).

Or run it on a connected phone or emulator with `pnpm tauri android dev`.

**iOS** (needs a Mac with Xcode):

1. Add the Rust targets:

   ```sh
   rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
   ```

2. In `apps/desktop`, run it in the simulator with `pnpm tauri ios dev`.

Installing on a real iPhone needs an Apple developer account. The full steps, including release builds and signing, are in [Android and iOS](../mobile.md).

## Run your own sync server (optional)

Only if you want a server for your office or firm. Most people don't need one: see [Sharing](sharing.md).

```sh
KHATA_ADMIN_KEY="$(openssl rand -hex 32)" KHATA_DATA_DIR=./khata-data cargo run -p khata-server
```

To host a relay for sharing over the internet, see [Hosting a relay](../deploy-relay.md).

## Problems?

See [Troubleshooting](troubleshooting.md), or ask in the [issues](https://github.com/khataos/khataos/issues).
