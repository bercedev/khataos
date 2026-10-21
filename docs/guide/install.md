# Install

**What you'll do:** download KhataOS for Windows, macOS, Linux or Android and open it for the first time. If you'd rather build it yourself, or want it on an iPhone, see [Build from source](#build-from-source).

> KhataOS is in alpha. Use a test company first, and [back up your company files](backup.md) before you update.

## Download

The easiest way is the **[download page](https://khataos.github.io/khataos/download/)**. Open it on the phone or computer where you want KhataOS. It picks the right file for that device and shows the install steps.

The first release is an alpha, an early test version.

You can also use these direct links. They always get the newest release:

| Your system | Download |
|---|---|
| Windows 10 or 11 | [`KhataOS-Windows-Setup.exe`](https://github.com/khataos/khataos/releases/latest/download/KhataOS-Windows-Setup.exe), or [`KhataOS-Windows.msi`](https://github.com/khataos/khataos/releases/latest/download/KhataOS-Windows.msi) if your IT team prefers it |
| macOS, on Apple silicon or Intel | [`KhataOS-macOS.dmg`](https://github.com/khataos/khataos/releases/latest/download/KhataOS-macOS.dmg) |
| Linux | [`KhataOS-Linux.AppImage`](https://github.com/khataos/khataos/releases/latest/download/KhataOS-Linux.AppImage) (any distribution), [`KhataOS-Linux.deb`](https://github.com/khataos/khataos/releases/latest/download/KhataOS-Linux.deb) (Debian, Ubuntu, Mint) or [`KhataOS-Linux.rpm`](https://github.com/khataos/khataos/releases/latest/download/KhataOS-Linux.rpm) (Fedora, openSUSE) |
| Android 7.0 or later | [`KhataOS-Android.apk`](https://github.com/khataos/khataos/releases/latest/download/KhataOS-Android.apk) |
| iPhone and iPad | Not available yet: [build from source](#android-and-ios) |

Every release, including older ones, is on the [releases page](https://github.com/khataos/khataos/releases). There the same files also have the version in their names, for example `KhataOS_0.1.0-alpha.1_x64-setup.exe`. If a direct link says **Not Found**, that file isn't in the newest release yet.

The first releases are **not signed** yet. Signing is how Windows, macOS and Android know who made an app, so for now your system warns you before it opens KhataOS. The steps below show what to click. Only do this for files you downloaded from the KhataOS download page or releases page, and [check the download](#check-your-download) if you want to be sure it arrived intact.

### Windows

1. Download `KhataOS-Windows-Setup.exe` and open it. If Edge says the file isn't commonly downloaded, click **...** next to it, then **Keep**, and if asked, **Show more**, then **Keep anyway**.
2. While the installer is unsigned, Windows shows **Windows protected your PC**. Click **More info**, then **Run anyway**.
3. Follow the installer. KhataOS appears in the Start menu.

KhataOS needs Microsoft WebView2. Windows 10 and 11 usually have it, and the installer downloads it if yours doesn't.

### macOS

1. Download `KhataOS-macOS.dmg` and open it.
2. Drag **KhataOS** onto the **Applications** folder in the window that opens.
3. Open KhataOS from Applications.

While the app isn't notarised by Apple, macOS won't open it the first time. It shows a message that Apple can't check KhataOS for malicious software. To open it anyway:

1. Click **Done** in that message.
2. Open **System Settings**, then **Privacy & Security**.
3. Scroll down to the **Security** section. Next to the note about KhataOS, click **Open Anyway**.
4. Confirm that you want to open KhataOS, and enter your Mac password if asked.

You only need to do this once. After that, KhataOS opens normally.

On macOS 14 Sonoma and older there's a shorter way: in Applications, Control-click (or right-click) **KhataOS**, choose **Open**, then click **Open** in the message.

### Linux

**AppImage** (works on most distributions, nothing to install):

```sh
chmod +x KhataOS-Linux.AppImage
./KhataOS-Linux.AppImage
```

If it doesn't start and mentions FUSE, install it: `sudo apt install libfuse2` on Ubuntu 22.04, or `sudo apt install libfuse2t64` on Ubuntu 24.04 and newer.

**Debian, Ubuntu or Mint** (`.deb`): installing it also installs the libraries KhataOS needs.

```sh
sudo apt install ./KhataOS-Linux.deb
```

**Fedora** (`.rpm`):

```sh
sudo dnf install ./KhataOS-Linux.rpm
```

On openSUSE, use `sudo zypper install` with the same file.

KhataOS then appears in your applications menu.

### Android

KhataOS isn't in the Play Store yet, so you install the APK file yourself (this is called sideloading).

1. On the phone, open the [download page](https://khataos.github.io/khataos/download/) and tap **Download for Android**. This downloads `KhataOS-Android.apk`. If Chrome warns that this type of file can harm your device, tap **Download anyway**.
2. Open the downloaded file. Android asks you to allow installs from this source (your browser or Files app). Tap **Settings**, turn on **Allow from this source**, go back, and tap **Install**.
3. If Google Play Protect says **Unsafe app blocked**, or warns about an unknown developer, tap **More details**, then **Install anyway**.

On **Xiaomi, Redmi and POCO** phones (HyperOS or MIUI), the phone's security app also scans the app first and may warn that it is unknown or not verified. It may ask you to tick a box to say you understand the risk, and wait a few seconds, before you can continue.

The exact wording differs between phone makers and Android versions.

If the Android link says **Not Found**, the newest release has no Android app you can install yet. A release may have only a file ending in `-unsigned.apk`: Android can't install that, because it installs only signed apps. That file is for people who sign APKs themselves. Use a later release, or [build from source](#android-and-ios).

Your company files live in the app's own storage on the phone. **Uninstalling KhataOS deletes them**, so [make a backup](backup.md) and copy it off the phone first. Installing a newer release over the old one keeps them.

### iPhone and iPad

There's no iOS download yet. Apps on an iPhone must come from the App Store or be signed with an Apple developer account, so for now you [build it from source](#android-and-ios) on a Mac.

### Check your download

Each release has a file called `SHA256SUMS.txt`. It lists a checksum (a long fingerprint) for every file. If your file's checksum matches the one in the list, the download is complete and unchanged.

- **Windows** (PowerShell, in your Downloads folder):

  ```powershell
  Get-FileHash .\KhataOS-Windows-Setup.exe
  ```

- **macOS:**

  ```sh
  shasum -a 256 KhataOS-macOS.dmg
  ```

- **Linux:** put `SHA256SUMS.txt` in the same folder as your download, then run

  ```sh
  sha256sum --check --ignore-missing SHA256SUMS.txt
  ```

  It prints `OK` next to each file that matches.

Compare the result with the line for that file in `SHA256SUMS.txt`. If they differ, delete the file and download it again.

### Update

Download the newest release from the [download page](https://khataos.github.io/khataos/download/) and install it the same way, over the old one. Your company files are kept outside the app, so updating doesn't touch them. Still, [back them up](backup.md) first: KhataOS is in alpha.

## Build from source

Build KhataOS yourself if you want the newest code, an iPhone build, or to help develop it.

### Desktop (Windows, macOS, Linux)

#### 1. Install the tools

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

#### 2. Download and run

```sh
git clone https://github.com/khataos/khataos.git
cd khataos/apps/desktop
pnpm install
pnpm app:dev
```

The first run compiles everything, so it takes a while. Then the KhataOS window opens on the company list. Next: [Create your first company](first-company.md).

#### 3. Build an installer (optional)

```sh
pnpm app:build
```

This makes an installer for your system (for example a `.dmg` on macOS, `.msi` or `.exe` on Windows, `.deb` or `.AppImage` on Linux) under `target/release/bundle/` in the repository. These builds aren't signed, so your system may warn when you open one.

#### Update

```sh
cd khataos
git pull
cd apps/desktop
pnpm install
pnpm app:dev
```

Your company files are kept outside the code folder, so updating doesn't touch them. Still, [back them up](backup.md) first: KhataOS is in alpha.

### Android and iOS

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
