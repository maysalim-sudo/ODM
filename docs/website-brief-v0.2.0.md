# Brief for the website session: ODM 0.2.0 on salimos.cc/odm

**From:** the ODM app session, 2026-10-23.
**Page:** https://salimos.cc/odm (and its "How it was built" page, `/me/odm`).

## What changed

ODM **0.2.0** adds **torrent downloads**. It is published as the GitHub release `v0.2.0`; the stable download URLs the page already uses keep working and now serve 0.2.0. Installed 0.1.0 copies are offered the update automatically.

The GitHub repository moved from `salimmay/ODM` to **`maysalim-sudo/ODM`**. Old `salimmay/ODM` links redirect, but use the new address for new links.

## Changes to make on the page

### 1. Version banner

- Was: "v0.1.0 is out · What's new"
- Now: **"v0.2.0 is out: torrents · What's new"** → `https://github.com/maysalim-sudo/ODM/blob/master/CHANGELOG.md`

### 2. New feature block in "What it does"

Add a seventh card next to the six existing ones (same style). Suggested copy:

> **Torrents, without the seeding**
> Paste a magnet link or open a `.torrent` file. Pick the files you want before it starts, or change your mind while it runs; the ones you skip take no disk space. Peers, speed and a piece map are right in the details panel. When your files are done, ODM stops sharing; it's a downloader, not a seeding client.

A short line could also go in the hero subtitle, for example "…and torrents, too".

### 3. Hero and spec line

- The spec line "Windows 10 & 11, 64-bit · 5.7 MB · No admin rights · MIT licence": change the size to the real size of the new installer (check the `ODM-setup.exe` asset on the v0.2.0 release; the build is about **7.5 MB**).
- Download buttons stay as they are (`…/releases/latest/download/ODM-setup.exe`), but use `maysalim-sudo/ODM` in the URL.

### 4. Install section

- After "Run the installer": the installer now asks **"Open magnet links and .torrent files with ODM?"** (default No). Add one line: "Say Yes if you want magnet links in your browser to open in ODM. You can change it later in Windows Settings → Apps → Default apps."

### 5. Browser extension section

- The extension is now **0.2.0**: it also sends **magnet links you click** and **`.torrent` files you download** to ODM (it follows "Take over downloads"; Alt+click leaves a link to the browser).
- The two download boxes keep their links (`odm-extension-chromium.zip`, `odm-extension-firefox.zip`), which now serve the 0.2.0 extension. Store badges stay "Coming soon".

### 6. FAQ

Add:

- **"Does ODM seed torrents?"** No. It uploads a little while it downloads (peers share faster with those who share back; you can cap it or turn it off in Settings → Torrents), and stops sharing as soon as your files are complete.
- **"Can I download only some files from a torrent?"** Yes. ODM shows the file list before it starts (for a magnet, as soon as peers send it), and you can tick or untick files later in the details panel.

Update **"How do updates work?"** to mention that 0.1.0 users get 0.2.0 offered automatically.

### 7. Footer links

Replace `github.com/salimmay/ODM` with `github.com/maysalim-sudo/ODM` everywhere (Source code, Release notes, Changelog, Privacy, Report a problem).

### 8. Screenshots (optional)

A screenshot of the **New torrent** window (file tree with tick boxes) and of the details panel with the piece map would show the feature best. The app session can take them on request.

## Coming next (don't add yet)

More installer types are being built and will arrive in a later release (0.2.x or 0.3.0). When they ship, the download area should become a platform picker. Planned asset names (stable, under `…/releases/latest/download/`):

| Platform | File |
|---|---|
| Windows x64 (recommended) | `ODM-setup.exe` |
| Windows ARM64 | `ODM-setup-arm64.exe` |
| Windows MSI (IT / admin installs, x64 / ARM64) | `ODM-x64.msi`, `ODM-arm64.msi` |
| Linux `.deb` (Ubuntu, Debian, Mint) | `ODM-amd64.deb` |
| Linux `.rpm` (Fedora, openSUSE) | `ODM-x86_64.rpm` |
| Linux AppImage (any distro) | `ODM-x86_64.AppImage` |

Notes for that version: the spec line becomes "Windows 10 & 11 · Linux"; only the Windows installers and the AppImage update themselves (deb/rpm users install the new package); the MSI needs admin rights. The app session will send an updated brief with real sizes when those builds exist.
