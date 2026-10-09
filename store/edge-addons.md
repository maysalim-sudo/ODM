# Microsoft Edge Add-ons listing

Ready-to-paste fields for the Microsoft Edge Add-ons (Partner Center) submission.

## Basics

- **Name:** ODM – Open Downloader Manager
- **Short description (max 132 characters):**
  ```
  Send downloads to ODM, a fast resumable download manager for Windows. Finds videos on pages too.
  ```
- **Category:** Productivity
- **Language:** English
- **Support URL:** https://github.com/salimmay/ODM/issues
- **Privacy policy URL:** https://github.com/salimmay/ODM/blob/master/store/PRIVACY.md
- **Website / homepage:** https://github.com/salimmay/ODM

## Detailed description

```
ODM sends your browser's downloads to the ODM desktop app, a free download manager for Windows.

Features
- Hands downloads from the browser to the ODM desktop app. If ODM cannot take a download, the browser keeps it, so nothing is lost.
- Multi-connection, resumable downloads: files are split into parts, can be paused and resumed, and finish even if the connection drops.
- Video finder: lists the videos on a page, including plain video files, HLS streams (.m3u8) and sites supported by yt-dlp, with a "Download with ODM" button on the video.
- Video detection can be switched off per site from the toolbar popup.
- Hold Alt while clicking a link to skip ODM and download in the browser as usual.
- Right-click menu: "Download with ODM" on links and media.

Requires the free ODM desktop app for Windows. Install it first: https://github.com/salimmay/ODM/releases/latest

Privacy: ODM collects nothing and sends nothing to any server. Download addresses, page titles, cookies and Referer/Origin go only to the ODM program on your own computer, and only after you start a download or click a Download button.

Open source (MIT): https://github.com/salimmay/ODM
```

## Single purpose

ODM sends downloads and videos from the browser to the ODM download manager installed on the same computer.

## Permission justifications

Permissions in `extension/manifests/chromium.json`: `downloads`, `cookies`, `nativeMessaging`, `contextMenus`, `storage`, `webRequest`, and host permission `<all_urls>`.

- **downloads:** When the user starts a download in the browser, the extension offers it to ODM. Only if ODM accepts does it cancel the browser's own copy; otherwise the browser keeps it.
- **cookies:** Reads the cookies for the download's own URL and passes them to ODM so downloads that need a login work. They go only to the ODM program on the same computer.
- **nativeMessaging:** The extension's only channel to the outside. It talks to `com.odm.host`, a small program installed with the ODM desktop app, which passes messages to ODM over a local pipe. Nothing is sent to any server.
- **contextMenus:** Adds the "Download with ODM" item for links and media.
- **storage:** Remembers the last error to show in the popup, and per-site settings such as the video-detection switch.
- **webRequest:** Read-only. Watches response headers for video playlists (.m3u8, .mpd) and video files so the extension can list the videos a page plays. Nothing is blocked or modified.
- **Host permissions (`<all_urls>`):** Needed because downloads and videos can be on any site. It lets the extension read the cookies for a download's URL, track the Alt key, and add a "Download with ODM" button on video elements and a list of detected videos, on any page.

## Remote code

**Are you using remote code?** No. All code is bundled in the extension package.

## Data usage

- Does not collect or use user data.
- Does not sell or transfer user data to third parties.
- Does not use or transfer user data for purposes unrelated to the extension's single purpose.
- Does not use or transfer user data to determine creditworthiness or for lending.
- Cookies, download addresses and page titles are handled only on the user's computer: they are passed to the ODM program on the same computer after a user action, and never to a server.

## Edge-specific notes

- **Notes for certification (optional):** The extension needs the ODM desktop app (free, open source) to do anything. Install it from https://github.com/salimmay/ODM/releases/latest, then start a download in the browser. Source: https://github.com/salimmay/ODM (see `extension/AMO-NOTES.md` for build steps).
- Partner Center asks whether the extension accesses personal information: ODM collects no data, as above.

## Images to upload

Files in `store/images/`:

- [ ] Screenshots, 1280x800, 1 to 5: `1-list.png` (main list), `2-chunks.png` (chunk map), `3-new-download.png` (New download dialog), `4-video-button.png` (video button on a page), `5-popup.png` (popup with Video detection)
- [ ] Small promo tile, 440x280: `promo-440x280.png`
- [ ] Icon, 128x128: `icon-128.png`

Regenerate the promo tile and icon with `python store/make-images.py`.

## Package

Upload `odm-extension-chromium-store.zip` as a draft first; submit for review after the store ID is filled into `chromium-ids.json` and the package is rebuilt.
