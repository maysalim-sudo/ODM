ODM 0.4.1 puts a download's details in a panel under the list, like uTorrent, and fixes the MP3 and stream problems found in 0.4.0.

## New

- **Details under the list:** click a download once and the panel below shows it, with tabs for Info, Speed (a graph of the last two minutes), Parts, and Files and peers for torrents. Drag the panel's top edge to resize it. Select several to see their combined size, progress and speed.
- **Select all:** Ctrl+A selects every download in the current tab.
- **Double-click** a finished download to open it.
- **Ask when a download finishes:** "When a download finishes" in Settings has a new choice, Ask. ODM comes forward and asks whether to open the file, its folder, or neither.

## Fixed

- **MP3 files go to your Audio folder** (Music), not Videos.
- **An MP3 is named .mp3 from the start**, so the list and the New download dialog show what you'll get.
- **HLS streams the browser plays itself** (in Chrome and Edge) are saved as video or MP3, not as a small `.m3u8` playlist file.
- **YouTube's home and search pages:** the button over a preview saves that video. It used to send the whole page, which made ODM download every video on it.
- The buttons on each download sit centred again.

## Install or update

**Update the app and the extension together.** The stream and YouTube fixes are in the extension.

- **Windows:** ODM offers the update itself. Your downloads pause and continue afterwards.
- **Linux:** install the new package from this page, as ODM can't update itself on Linux yet.
- **New install:** download **ODM-setup.exe** below and run it. The installer isn't code-signed yet, so Windows SmartScreen may show "Windows protected your PC". Choose **More info → Run anyway**.

If you're coming from a version before 0.4.0, an older ODM can't open your download list after this update.

## Browser extension

- **Chrome, Edge, Brave, Vivaldi, Opera:** load `odm-extension-chromium.zip` unpacked (Developer mode → Load unpacked) until the store listing updates.
- **Firefox and Floorp:** use `odm-extension-firefox.zip` until the add-on update is approved.

Full list: [CHANGELOG.md](https://github.com/maysalim-sudo/ODM/blob/master/CHANGELOG.md)

## Files

| File | What it is |
|---|---|
| `ODM-setup.exe` | The Windows installer (the same file as `ODM_0.4.1_x64-setup.exe`) |
| `ODM_0.4.1_x64-setup.exe` and its `.sig` | The installer under its versioned name, and its update signature |
| `ODM-amd64.deb`, `ODM-x86_64.rpm`, `ODM-x86_64.AppImage` | The Linux packages |
| `latest.json` | Used by ODM's updater; you don't need it |
| `odm-extension-chromium.zip` | The extension for Chromium browsers (load unpacked) |
| `odm-extension-firefox.zip` | The extension for Firefox and Floorp |

Only download what you have the right to, and respect each site's terms.
