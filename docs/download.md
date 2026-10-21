---
title: Download KhataOS
description: "Download KhataOS, free open source accounting software that works offline, for Android phones, Windows, Mac and Linux. Simple install steps for each."
hide:
  - navigation
---

<!--
  The download links use stable file names on the newest GitHub release
  (releases/latest/download/<name>), made by .github/workflows/release.yml.
  Nothing here is fetched at runtime. javascripts/download.js only moves the
  card for the visitor's device to the top; with scripts off, all cards show.
  The QR code (images/download-qr.svg) opens this page. To make it again:
    python -m pip install segno
    python -c "import segno; segno.make('https://khataos.github.io/khataos/download/', error='m').save('docs/images/download-qr.svg', border=4, dark='#14151c', light='#ffffff', xmldecl=False, omitsize=True)"
-->

<div class="kd-logo" markdown>

![KhataOS](images/brand/lockup-horizontal.svg#gh-light-mode-only){ width="262" height="72" }
![KhataOS](images/brand/lockup-horizontal-dark.svg#gh-dark-mode-only){ width="262" height="72" }

</div>

# Download KhataOS

<p class="kd-lead">Free, open source accounting that works offline. This is an early test version (alpha).</p>

<div class="kd-pick" id="kd-pick" hidden>
<p class="kd-pick__label">For your device</p>
<ul class="kd-grid kd-grid--pick" id="kd-pick-list"></ul>
</div>

<h2 class="kd-others" id="kd-others">Choose your system</h2>

<ul class="kd-grid" id="kd-systems">
<li class="kd-card" data-os="android">
<h3 class="kd-card__title">Android</h3>
<p class="kd-card__note">Phones and tablets with Android 7 or later: Xiaomi, Redmi, POCO, Samsung, Vivo, Oppo, Realme and others.</p>
<a class="kd-btn" href="https://github.com/khataos/khataos/releases/latest/download/KhataOS-Android.apk">Download for Android</a>
<p class="kd-card__more"><code>KhataOS-Android.apk</code> · <a href="#android">How to install</a></p>
</li>
<li class="kd-card" data-os="windows">
<h3 class="kd-card__title">Windows</h3>
<p class="kd-card__note">Windows 10 or 11, 64 bit.</p>
<a class="kd-btn" href="https://github.com/khataos/khataos/releases/latest/download/KhataOS-Windows-Setup.exe">Download for Windows</a>
<p class="kd-card__more"><code>KhataOS-Windows-Setup.exe</code> · <a href="#windows">How to install</a></p>
<p class="kd-card__alt">For IT teams: <a href="https://github.com/khataos/khataos/releases/latest/download/KhataOS-Windows.msi">the MSI installer</a></p>
</li>
<li class="kd-card" data-os="mac">
<h3 class="kd-card__title">Mac</h3>
<p class="kd-card__note">Any recent Mac, with Apple silicon or Intel.</p>
<a class="kd-btn" href="https://github.com/khataos/khataos/releases/latest/download/KhataOS-macOS.dmg">Download for Mac</a>
<p class="kd-card__more"><code>KhataOS-macOS.dmg</code> · <a href="#mac">How to install</a></p>
</li>
<li class="kd-card" data-os="linux">
<h3 class="kd-card__title">Linux</h3>
<p class="kd-card__note">64 bit. The AppImage runs on most distributions.</p>
<a class="kd-btn" href="https://github.com/khataos/khataos/releases/latest/download/KhataOS-Linux.AppImage">Download for Linux</a>
<p class="kd-card__more"><code>KhataOS-Linux.AppImage</code> · <a href="#linux">How to install</a></p>
<p class="kd-card__alt">Or a package: <a href="https://github.com/khataos/khataos/releases/latest/download/KhataOS-Linux.deb">.deb</a> for Ubuntu, Debian and Mint, <a href="https://github.com/khataos/khataos/releases/latest/download/KhataOS-Linux.rpm">.rpm</a> for Fedora and openSUSE</p>
</li>
<li class="kd-card kd-card--soon" data-os="iphone">
<h3 class="kd-card__title">iPhone and iPad</h3>
<p class="kd-card__note">Not available yet. Use KhataOS on a computer or an Android phone for now.</p>
<p class="kd-soon">Coming later</p>
<p class="kd-card__more"><a href="#iphone">Why not yet?</a></p>
</li>
</ul>

!!! note "KhataOS is in alpha"

    This is an early test version. Try it with a test company first, and keep your current accounting software for your real books for now. Your books stay on your own phone or computer.

<p class="kd-small">If a link opens a GitHub page that says <strong>Not Found</strong>, that file isn't in the newest release yet. Every file, and older versions, are on the <a href="https://github.com/khataos/khataos/releases">releases page</a>.</p>

<div class="kd-qr" markdown>

![QR code that opens this download page](images/download-qr.svg){ .kd-qr__img width="164" height="164" }

<div class="kd-qr__text" markdown>

**Want it on your phone?**

Point your phone's camera at this code. It opens this page on your phone, with the right download at the top.

</div>
</div>

## Install on Android { #android }

KhataOS isn't in the Play Store yet, so your phone treats it as an app from an unknown source and asks you to confirm a few times. This is normal for apps from outside the Play Store. These steps use Chrome; other browsers ask the same things.

1. Tap **Download for Android**. Chrome may warn that this type of file can harm your device. Tap **Download anyway**.
2. When the download finishes, tap **Open**. You can also find `KhataOS-Android.apk` later in your **Downloads** or in the **File Manager** app.
3. The phone says it isn't allowed to install unknown apps from this source. Tap **Settings**, turn on **Allow from this source**, then go back.
4. Tap **Install**.
5. If **Google Play Protect** says **Unsafe app blocked**, or that it doesn't recognise the developer, tap **More details**, then **Install anyway**. If it offers to scan the app first, you can let it.
6. Tap **Open**. KhataOS is now in your app list.

**Xiaomi, Redmi and POCO phones (HyperOS or MIUI)** add their own check. The phone's security app scans KhataOS before installing it and may warn that the app is unknown or not verified. It may ask you to tick a box to say you understand the risk, and wait a few seconds, before the button to continue works. Then carry on with the install. The exact words differ between phones and versions.

When KhataOS is installed, you can turn **Allow from this source** off again for your browser. KhataOS keeps working.

!!! warning "Uninstalling deletes your books"

    On Android, your company files live inside the app. If you uninstall KhataOS, they are deleted. [Make a backup](guide/backup.md) and copy it off the phone first. To update, install the new version over the old one: that keeps your books.

## Install on Windows { #windows }

1. Click **Download for Windows** and open `KhataOS-Windows-Setup.exe` when it has downloaded.
2. Edge may say the file isn't commonly downloaded. Click the **...** button next to it, then **Keep**. If it asks again, click **Show more**, then **Keep anyway**.
3. Windows shows **Windows protected your PC**. Click **More info**, then **Run anyway**. This appears because the installer isn't signed yet.
4. Follow the installer. KhataOS appears in the Start menu.

## Install on a Mac { #mac }

1. Click **Download for Mac** and open `KhataOS-macOS.dmg`.
2. Drag **KhataOS** onto the **Applications** folder in the window that opens.
3. Open KhataOS from Applications. The first time, macOS says it can't check KhataOS for malicious software, because Apple hasn't checked it yet. Click **Done**.
4. Open **System Settings**, then **Privacy & Security**. Scroll down, and next to the note about KhataOS click **Open Anyway**. Confirm, and enter your Mac password if asked.

You only do this once. After that, KhataOS opens like any other app.

## Install on Linux { #linux }

**AppImage**, for most distributions: download `KhataOS-Linux.AppImage`, allow it to run as a program (in your file manager's Properties, or with the command below), then open it.

```sh
chmod +x KhataOS-Linux.AppImage
./KhataOS-Linux.AppImage
```

If it mentions FUSE, install it: `sudo apt install libfuse2` on Ubuntu 22.04, or `sudo apt install libfuse2t64` on Ubuntu 24.04 and newer.

**Ubuntu, Debian or Mint:** download `KhataOS-Linux.deb` and run `sudo apt install ./KhataOS-Linux.deb`.

**Fedora or openSUSE:** download `KhataOS-Linux.rpm` and run `sudo dnf install ./KhataOS-Linux.rpm` (on openSUSE, `sudo zypper install ./KhataOS-Linux.rpm`).

KhataOS then appears in your applications menu.

## iPhone and iPad { #iphone }

There's no iPhone or iPad app yet. Apple only lets you install apps from the App Store, or apps signed with a paid Apple developer account, and KhataOS isn't in the App Store yet. For now, use KhataOS on a Windows, Mac or Linux computer, or on an Android phone.

Developers with a Mac can build it themselves: see [Android and iOS](mobile.md).

## Is it safe? { #safe }

- **It's open source.** All of the code is [public on GitHub](https://github.com/khataos/khataos). Anyone can read it and check what it does.
- **It's built in public.** GitHub's servers build every installer from that public code, and anyone can see the build logs.
- **Your books stay on your device.** KhataOS works offline. Each company is a file on your own phone or computer. Nothing is sent anywhere unless you choose to share your books or send a report.
- **Why the warnings?** Windows, Mac and Android warn about any app that isn't signed with a paid certificate or isn't from their store. KhataOS isn't signed yet. The warning means your system doesn't know who made the app. It doesn't mean it found a problem.
- **You can check your file.** Each release has a list of fingerprints (SHA-256 checksums) for every file: [`SHA256SUMS.txt`](https://github.com/khataos/khataos/releases/latest/download/SHA256SUMS.txt). The [install guide](guide/install.md#check-your-download) shows how to compare.

Only download KhataOS from this page or from the [KhataOS releases page](https://github.com/khataos/khataos/releases).

## More help

- The [install guide](guide/install.md) has more detail, and how to build KhataOS from source.
- Next: [create your first company](guide/first-company.md).
- Stuck? See [Troubleshooting](guide/troubleshooting.md), or ask in the [issues](https://github.com/khataos/khataos/issues).
