# Notes for addons.mozilla.org reviewers

ODM is the browser half of ODM (Open Downloader Manager), a desktop download manager for Windows. It is open source under the MIT licence: https://github.com/salimmay/ODM

## Build from source

Requirements: Node 22.12 or newer and pnpm 9 or newer.

```
cd extension
pnpm install --frozen-lockfile
pnpm build
```

The Firefox build is written to `extension/dist/firefox`. The build:
- uses Vite to bundle each entry (`src/background.ts`, `src/content.ts`, `src/popup.ts`) into one unminified IIFE file;
- copies `src/popup.html`, `src/popup.css` and the two icons;
- writes the manifest from `manifests/firefox.json`.

The script is `scripts/build.mjs`. Tests: `pnpm test`.

## What the extension does, and why it needs each permission

- **nativeMessaging:** this is the extension's only channel to the outside. It talks to `com.odm.host`, a small program installed with the ODM desktop app, which passes messages to ODM over a local named pipe. Nothing is sent to any server.
- **downloads:** when the user starts a download in the browser, the extension offers it to ODM. Only if ODM accepts does it cancel the browser's copy; otherwise the browser keeps it. Holding Alt while clicking skips ODM.
- **cookies + `<all_urls>` host access:**
  - The cookies for the download's own URL are read and passed to ODM, so downloads that need a login work.
  - The content script tracks the Alt key, and adds a "Download with ODM" button on video elements and a small list of detected videos.
- **webRequest:** read-only. It watches response headers for video playlists (`.m3u8`, `.mpd`) and video files, so it can list the videos a page plays. Nothing is blocked or modified.
- **contextMenus:** the "Download with ODM" item for links and media.
- **storage:** remembers the last error to show in the popup.

**Data collection:** none (`data_collection_permissions: none`). Cookies, URLs and page titles go only to the ODM program on the same computer, and only after a user action (a download the user started, or a click on a Download button).
