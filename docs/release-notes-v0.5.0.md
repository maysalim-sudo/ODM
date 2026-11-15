ODM 0.5.0 gives ODM a new look: a new logo, a sunset whose reflection narrows into a download arrow, and a new welcome screen. ODM also opens at its earlier size again.

## Changed

- **A new logo**, in the app, the taskbar, the tray, the browser extension and the installer.
- **A new welcome screen:** its steps sit in a clean card over a rising sunset.
- **ODM opens at its earlier size again**, not 95% of your screen.

## Fixed

- A video sent from the browser that fails with your browser's login is tried once more without it. This helps on Facebook when its login page gets in the way.

## Install or update

**Update the app and the extension together.** Until you update the extension, ODM will say it is older.

- **Windows:** ODM offers the update itself. Your downloads pause and continue afterwards.
- **Linux:** a 0.4.3 copy offers the update itself: an AppImage replaces itself, and a .deb or .rpm copy asks for your system password. From an older version, install the new package from this page once.
- **New install:** download **ODM-setup.exe** below and run it. The installer isn't code-signed yet, so Windows SmartScreen may show "Windows protected your PC". Choose **More info → Run anyway**.

## Browser extension

- **Chrome, Edge, Brave, Vivaldi, Opera:** load `odm-extension-chromium.zip` unpacked (Developer mode → Load unpacked) until the store listing updates.
- **Firefox and Floorp:** use `odm-extension-firefox.zip` until the add-on update is approved.

Full list: [CHANGELOG.md](https://github.com/maysalim-sudo/ODM/blob/master/CHANGELOG.md)

## Files

| File | What it is |
|---|---|
| `ODM-setup.exe` | The Windows installer (the same file as `ODM_0.5.0_x64-setup.exe`) |
| `ODM_0.5.0_x64-setup.exe` and its `.sig` | The installer under its versioned name, and its update signature |
| `ODM-amd64.deb`, `ODM-x86_64.rpm`, `ODM-x86_64.AppImage` | The Linux packages |
| `ODM_0.5.0_amd64.deb`, `ODM_0.5.0_x86_64.rpm`, `ODM_0.5.0_amd64.AppImage` and their `.sig` | The same Linux packages under versioned names, used by ODM's updater |
| `latest.json` | Used by ODM's updater; you don't need it |
| `odm-extension-chromium.zip` | The extension for Chromium browsers (load unpacked) |
| `odm-extension-firefox.zip` | The extension for Firefox and Floorp |
| `odm-extension-source.zip` | The extension's source code |
| `SHA256SUMS.txt` | Checksums of the files above |

Only download what you have the right to, and respect each site's terms.
