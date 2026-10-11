ODM is a free download manager for Windows. It downloads over several connections at once, resumes after a pause or a crash, takes over downloads from your browser, and downloads the videos a page plays.

## Install

1. Download **ODM-setup.exe** below and run it. It installs for your user only, with no admin rights needed.
2. The installer isn't code-signed yet, so Windows SmartScreen may show "Windows protected your PC". Choose **More info → Run anyway**.
3. Start ODM. The welcome screen helps you add the browser extension and, if you want site videos, get the video tools in one click.

ODM checks for updates on its own and installs them when you say so. Every update is signed and verified before it installs.

## Browser extension

- **Chrome, Edge, Brave, Vivaldi, Opera:** the welcome screen opens the extension folder and your browser's extensions page. Turn on Developer mode, choose Load unpacked, and pick that folder. Store listings (Chrome Web Store, Edge Add-ons) are on the way.
- **Firefox and Floorp:** the add-on is in review on addons.mozilla.org. Until then, use `odm-extension-firefox.zip` through `about:debugging` (see the README).

## Highlights

- Multi-connection downloads with resume, a queue, and speed and concurrency limits
- A speed chip and tray menu for quick limits, plus schedules for the speed limit and the queue
- Videos: plain files, HLS streams, and YouTube, Facebook, Instagram, X, TikTok, Vimeo and more through yt-dlp, with quality, format and subtitle choices
- When a file exists, when a download finishes, when the queue is done (sleep, shut down or quit, after a 60-second countdown)
- Proxy, custom User-Agent, start with Windows, prevent sleep while downloading
- Downloads go to `Downloads\ODM` by default; you can change it in Settings

Full list: [CHANGELOG.md](https://github.com/salimmay/ODM/blob/master/CHANGELOG.md)

## Files

| File | What it is |
|---|---|
| `ODM-setup.exe` | The installer (the same file as `ODM_0.1.0_x64-setup.exe`) |
| `latest.json` | Used by ODM's updater; you don't need it |
| `odm-extension-chromium.zip` | The extension for Chromium browsers (load unpacked) |
| `odm-extension-firefox.zip` | The extension for Firefox and Floorp |
| `odm-extension-source.zip` | The extension's source, for reviewers |
| `SHA256SUMS.txt` | Checksums of every file |

Only download what you have the right to, and respect each site's terms.
