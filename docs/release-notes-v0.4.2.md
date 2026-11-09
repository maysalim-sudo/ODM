ODM 0.4.2 adds a download button to every YouTube thumbnail, and stops asking you to update video tools that are already current.

## New

- **Download from YouTube thumbnails:** hover any thumbnail on YouTube (home, search, a channel's Videos or Shorts, playlists, or the list beside a video) and a small ↓ appears at its top-left. Click it and choose **Video** or **MP3**. You no longer have to open the video or wait for its preview. Playlist and Mix cards don't get one, because they only lead to their first video.

## Changed

- **"Update video tools" only when there's an update:** ODM checks for newer yt-dlp and Deno releases, and offers FFmpeg again once your copy is over four months old. When everything is current, Settings → Video says "Up to date" instead of showing the button.

## Install or update

**Update the app and the extension together.** The thumbnail button is in the extension.

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
| `ODM-setup.exe` | The Windows installer (the same file as `ODM_0.4.2_x64-setup.exe`) |
| `ODM_0.4.2_x64-setup.exe` and its `.sig` | The installer under its versioned name, and its update signature |
| `ODM-amd64.deb`, `ODM-x86_64.rpm`, `ODM-x86_64.AppImage` | The Linux packages |
| `latest.json` | Used by ODM's updater; you don't need it |
| `odm-extension-chromium.zip` | The extension for Chromium browsers (load unpacked) |
| `odm-extension-firefox.zip` | The extension for Firefox and Floorp |

Only download what you have the right to, and respect each site's terms.
