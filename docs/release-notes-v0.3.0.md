ODM 0.3.0 is a new look built to be easier to follow: every download shows its own next step. You can also choose what downloads next, and save any video ODM finds as MP3 or video.

## The new look

- **Home:** downloads are grouped into Needs you, Downloading, Up next and Finished, each with its own buttons. Click one for its details. The detailed table is one click away.
- **Paste a link:** press Ctrl+V anywhere. ODM reads the link and shows what it found. The other options are under More options.
- **Choose what's next:** drag a row in Up next, use the `…` menu, or press Alt+Up and Alt+Down. Sort by size puts the smallest first, and a second click puts the largest first.
- **How much is left:** the status bar adds up what's left to download.
- **Problems in plain words:** a failed download says what happened and offers the fix on its row.
- **Settings:** six sections, with the things most people change under Essentials. Nothing was removed, and search finds everything.

## Also new

- **MP3 or video:** choose for each video on the button over it, in the page's video list or in the extension's popup. Plain video files show their real type, such as WEBM. MP3 works for plain files, HLS streams and site videos, and needs FFmpeg from "Get video tools".
- **Move the video list:** drag it by its grip, or use the arrow keys. It stays where you put it.
- **Hide it:** the button over a video now has a ×. The × on the video list, and turning detection off, now really hide them.
- **A clearer warning:** the extension's icon shows when ODM doesn't answer a download from the browser.
- The video list no longer runs off the left edge on pages with a scrollbar.

## Install or update

**Update the app and the extension together.** With a new extension and an older ODM, choosing MP3 saves video instead.

- **Windows:** ODM offers the update itself. Your downloads pause and continue afterwards.
- **Linux:** install the new package from this page, as ODM can't update itself on Linux yet.
- **New install:** download **ODM-setup.exe** below and run it. The installer isn't code-signed yet, so Windows SmartScreen may show "Windows protected your PC". Choose **More info → Run anyway**.

After this update, an older ODM can't open your download list.

## Browser extension

- **Chrome, Edge, Brave, Vivaldi, Opera:** load `odm-extension-chromium.zip` unpacked (Developer mode → Load unpacked) until the store listing updates.
- **Firefox and Floorp:** use `odm-extension-firefox.zip` until the add-on update is approved.

Full list: [CHANGELOG.md](https://github.com/maysalim-sudo/ODM/blob/master/CHANGELOG.md)

## Files

| File | What it is |
|---|---|
| `ODM-setup.exe` | The Windows installer (the same file as `ODM_0.3.0_x64-setup.exe`) |
| `ODM_0.3.0_x64-setup.exe` and its `.sig` | The installer under its versioned name, and its update signature |
| `ODM-amd64.deb`, `ODM-x86_64.rpm`, `ODM-x86_64.AppImage` | The Linux packages |
| `latest.json` | Used by ODM's updater; you don't need it |
| `odm-extension-chromium.zip` | The extension for Chromium browsers (load unpacked) |
| `odm-extension-firefox.zip` | The extension for Firefox and Floorp |

Only download what you have the right to, and respect each site's terms.
