ODM 0.4.3 adds an action bar and a right-click menu for handling many downloads at once. It catches duplicates before they start, and lets Linux copies update themselves from the next version on.

## New

- **Action bar:** it sits above the list and is always there. Select one or several downloads and use Resume, Pause, Stop, or the **Queue**, **Low**, **Speed** and **More** menus. **Low** puts a download last in line and slows it down.
- **Right-click menu:** right-click any download for Open, Open folder, Pause or Resume, queue moves, speed limits, Copy link, Retry, Remove and more. It acts on everything you have selected.
- **Duplicates caught early:** the new download dialog tells you when a link is already in your list, with **Show it** or add it anyway. When the file name is taken, it suggests a free name such as "name (1).mp4".
- **Recent folders and links:** Save to offers the folders you used lately, and the empty paste bar offers the links you downloaded lately.
- **A safer Later:** when you choose Later, ODM finds the file's size, warns if the drive is too full, and shows when the link expires. An expired link offers **Open the page** to get a fresh one.
- **Old extension warning:** ODM and the extension's own popup tell you when the browser extension is older than ODM.
- **More video sites:** hover a thumbnail and click the ↓ to save it as Video or MP3, as on YouTube.
- **Linux updates in the app:** from the next version, AppImage, .deb and .rpm copies update from inside ODM. A .deb or .rpm copy asks for your system password.

## Changed

- ODM opens at 95% of your screen, centred.
- The download table and the details panel sit in rounded cards instead of touching the window's edges.

## Fixed

- A plain video file saved as MP3 is named .mp3 from the start.
- Pasting a YouTube channel or home address no longer downloads every video on it.
- At start-up, ODM repairs a browser that was left pointing at another copy of ODM.

## Install or update

**Update the app and the extension together.** Until you update the extension, ODM will say it is older.

- **Windows:** ODM offers the update itself. Your downloads pause and continue afterwards.
- **Linux:** install the new package from this page one last time. After that, ODM updates itself.
- **New install:** download **ODM-setup.exe** below and run it. The installer isn't code-signed yet, so Windows SmartScreen may show "Windows protected your PC". Choose **More info → Run anyway**.

If you're coming from a version before 0.4.0, an older ODM can't open your download list after this update.

## Browser extension

- **Chrome, Edge, Brave, Vivaldi, Opera:** load `odm-extension-chromium.zip` unpacked (Developer mode → Load unpacked) until the store listing updates.
- **Firefox and Floorp:** use `odm-extension-firefox.zip` until the add-on update is approved.

Full list: [CHANGELOG.md](https://github.com/maysalim-sudo/ODM/blob/master/CHANGELOG.md)

## Files

| File | What it is |
|---|---|
| `ODM-setup.exe` | The Windows installer (the same file as `ODM_0.4.3_x64-setup.exe`) |
| `ODM_0.4.3_x64-setup.exe` and its `.sig` | The installer under its versioned name, and its update signature |
| `ODM-amd64.deb`, `ODM-x86_64.rpm`, `ODM-x86_64.AppImage` | The Linux packages |
| `ODM_0.4.3_amd64.deb`, `ODM_0.4.3_x86_64.rpm`, `ODM_0.4.3_amd64.AppImage` and their `.sig` | The same Linux packages under versioned names, used by ODM's updater |
| `latest.json` | Used by ODM's updater; you don't need it |
| `odm-extension-chromium.zip` | The extension for Chromium browsers (load unpacked) |
| `odm-extension-firefox.zip` | The extension for Firefox and Floorp |

Only download what you have the right to, and respect each site's terms.
