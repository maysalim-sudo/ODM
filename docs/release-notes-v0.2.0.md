ODM 0.2.0 adds torrent downloads. ODM stays a downloader: a torrent stops sharing once its files are complete.

## Torrents

- **Add a torrent** from the Add window (paste a magnet link, or pick a `.torrent` file), by dropping a `.torrent` on the window, from the browser extension, or by opening one in Windows.
- **Choose the files** before it starts, and change them while it runs. A magnet shows "Fetching file list…" until peers send its list; **Download all** doesn't wait for it. Files you leave out take no disk space.
- **Follow it** in the details panel: peers, upload speed, per-file progress and a piece map.
- **Settings → Torrents:** listen port, router port opening (UPnP), upload while downloading, and peers per torrent.
- After a pause, a restart or a crash, a torrent continues from where it was.

## Install or update

- **Already have ODM 0.1.0?** Install this version by hand once: download **ODM-setup.exe** below and run it; your downloads and settings stay. ODM's update signing key changed with this release, so 0.1.0 can't install it by itself. From 0.2.0 on, updates arrive in the app again.
- **New install:** download **ODM-setup.exe** below and run it. It installs for your user only, with no admin rights needed. The installer isn't code-signed yet, so Windows SmartScreen may show "Windows protected your PC". Choose **More info → Run anyway**.
- The installer asks whether to open magnet links and `.torrent` files with ODM. The answer is No unless you choose Yes; you can change it later in Windows Settings → Apps → Default apps.

## Browser extension

The extension now sends magnet links you click, and `.torrent` files you download, to ODM. It follows **Take over downloads from the browser**; **Download with ODM** on a magnet link always works. Hold Alt while clicking to leave a link to the browser.

- **Chrome, Edge, Brave, Vivaldi, Opera:** load `odm-extension-chromium.zip` unpacked (Developer mode → Load unpacked) until the store listing updates.
- **Firefox and Floorp:** use `odm-extension-firefox.zip` until the add-on update is approved.

Full list: [CHANGELOG.md](https://github.com/salimmay/ODM/blob/master/CHANGELOG.md)

## Files

| File | What it is |
|---|---|
| `ODM-setup.exe` | The installer (the same file as `ODM_0.2.0_x64-setup.exe`) |
| `ODM_0.2.0_x64-setup.exe` and its `.sig` | The installer under its versioned name, and its update signature |
| `latest.json` | Used by ODM's updater; you don't need it |
| `odm-extension-chromium.zip` | The extension for Chromium browsers (load unpacked) |
| `odm-extension-firefox.zip` | The extension for Firefox and Floorp |

Only download what you have the right to, and respect each site's terms.
