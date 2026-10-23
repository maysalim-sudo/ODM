# Torrent downloads: smoke checklist

Plan: `2026-10-23-odm-torrents.md`. Spec: `../specs/2026-10-23-odm-torrents-design.md`.

The automated tests cover the engine against a local seeder and a fake client, the app core, the UI logic and the extension. These checks need a real swarm, a real browser or an installed build. Run them yourself, from your own session: anything Claude starts runs inside its app container, so Windows registrations it makes are not visible to your browsers.

**Before you start:** close any running ODM (tray icon → Quit), so the build you test is the one that opens.

## 1. A real magnet, end to end

Use a legal public torrent with a few files, for example a Linux distribution or a Creative Commons film pack.

- [ ] Paste the magnet into **Add**. The button reads **Add torrent**. The **New torrent** window opens with "Fetching file list…".
- [ ] Within a minute the file tree appears with every file ticked and the selected total shown.
- [ ] Untick a file and press **Download**. The row shows "Checking files…" or "Downloading · N peers".
- [ ] The detail panel shows the peers ("N connected (M found)"), the upload speed, the info hash, the piece map filling in, and per-file bars.
- [ ] Tick the skipped file in the detail panel while it downloads. The size grows, and the file downloads too.
- [ ] Untick every file: the panel says "Keep at least one file selected."
- [ ] Pause, quit ODM from the tray, start it again, and resume. It continues from where it was and does not start over.
- [ ] When it finishes, the row says **Completed** and the files are in `Downloads\ODM\Torrents\<name>\`. In Task Manager → Performance → Ethernet/Wi-Fi, ODM no longer uploads after a minute or so.
- [ ] Tick one more file on the completed row. It goes back to the queue and downloads only that file.

## 2. Dead magnet and duplicates

- [ ] Add a magnet with a made-up hash. It stays at "Fetching file list…". **Cancel** removes it and no stray row is left.
- [ ] Add the same real magnet twice. No second row appears; the existing one is selected.

## 3. Settings

- [ ] **Settings → Torrents** shows a listen port between 49152 and 65535. **Random** changes it.
- [ ] Upload: **Don't upload**, then download something. The row's upload speed stays at 0.
- [ ] Network → Proxy → Manual `http://…`: the Torrents section says torrents connect directly.
- [ ] The first download that listens may bring up the Windows Firewall prompt once. Allow ODM on private networks.

## 4. Browser extension

Reload the unpacked extension (or install the updated store build) in Chrome and in Firefox.

- [ ] Click a magnet link on a page. ODM comes up with the **New torrent** window, and the browser shows no "open with" prompt.
- [ ] Alt+click the same link. The browser handles it as before.
- [ ] Right-click a magnet link → **Download with ODM**. It is added.
- [ ] Download a `.torrent` file from a site that needs you signed in. The browser download is cancelled, and ODM opens it with its file list.
- [ ] Turn off **Take over downloads from the browser**. Magnet clicks and `.torrent` downloads stay in the browser; the context menu still works.

## 5. Installer and Windows

Build the installer with `node scripts/release.mjs` (or use the debug NSIS bundle) and run it yourself.

- [ ] A fresh install asks "Open magnet links and .torrent files with ODM?" with **No** as the default. Answer **Yes**.
- [ ] Double-click a `.torrent` file in Explorer: ODM opens it (whether ODM was running or not).
- [ ] Click a magnet link in a browser **without** the extension: Windows offers ODM, and ODM opens it.
- [ ] Install an update over it (the in-app updater or `/UPDATE`): you are not asked again, and the associations still work.
- [ ] Uninstall: the associations are gone. A `.torrent` association you set to another app beforehand is left alone.
