# Plan 3 smoke check: browser integration

## Automated or controller-run checks (2026-10-03)

**Host → pipe → app**, using `odm-host.exe` with real framing against the debug app (`--background`, throwaway `ODM_DATA_DIR` and pipe name):

```
{"id":"1","ok":true,"payload":{"version":"0.1.0"}}
{"id":"2","ok":true,"payload":{"download_id":"4ae18d79eb1b4f128c6ce07a0f7b8d22"}}
{"id":"3","ok":true,"payload":{"active":0,"queued":1,"total_speed_bps":0}}
{"id":"4","ok":false,"error":{"code":"expired","message":"ODM answered too late; the browser kept the download."}}
{"id":"5","ok":false,"error":{"code":"unknown_command","message":"Unknown command: NOPE"}}
```

**Background start:** the window stayed hidden after a `--background` start. It appeared when a reuse offer arrived.

**Reuse offer:**
- I paused a download, then sent `ADD_DOWNLOAD` for the same file name with a new URL.
- The dialog read "Continue the earlier download?" with the specified text and buttons.
- "Continue" resumed the paused download, and it finished at 25 165 824 bytes.

**Real Microsoft Edge** (throwaway profile, `--load-extension=extension/dist/chromium`):
- The extension loaded with the fixed ID `aijkcebmfjiicjdfchfbdehejghbpadb`.
- The popup showed "Connected to ODM 0.1.0" and "0 active · 0 queued · 0 B/s".
- Native messaging from the extension to the registered debug host worked.
- **Capture:** opening `http://127.0.0.1:8765/small.zip` in a new tab gave the download to ODM, which completed it. Edge's own download was cancelled and erased.
- **Bug found and fixed** (commit `bf9189f`): any window blur counted as an Alt release, so captures right after switching tabs fell back to the browser.
- **Not finished:** the Edge test was stopped early. The throwaway profile auto-signed in to the user's Microsoft account and synced their extensions, and on relaunch it didn't load the unpacked extension. The cookie and Referer check in a real browser therefore remains a manual item (5 below). The automated tests cover it: `add_download_capture_adds_with_cookies_and_referer`.

## Manual checklist (person at the desktop)

Load the extension as the README says, with the app built (`pnpm tauri build --debug --no-bundle`) and `cargo build -p odm-host`.

- [ ] 1. Chrome: click a large public file link (for example `https://proof.ovh.net/files/100Mb.dat`). ODM takes it, and the browser download disappears.
- [ ] 2. Chrome: the same with Alt held. The browser downloads it, and ODM does nothing.
- [ ] 3. Chrome: a small file (< 1 MB). The browser keeps it.
- [ ] 4. Chrome: right-click a link → "Download with ODM". It appears in ODM.
- [ ] 5. A cookie-gated download (a site you are logged in to). ODM finishes it, and the file is not an HTML login page.
- [ ] 6. Firefox/Floorp: items 1–4.
- [ ] 7. Quit ODM, then click a large link. ODM starts in the tray and takes the download within 5 s, or the browser keeps it. There is never a duplicate.
- [ ] 8. The popup shows "Connected to ODM 0.1.0", the counts and the speed. Pause all, Resume all and Open ODM work. With ODM quit, it shows "ODM is not running."
- [ ] 9. Turn capture off in ODM Settings. A large link stays in the browser.
- [ ] 10. Pause a captured download, then click the same file in the browser again. "Continue the earlier download?" appears, and "Continue" resumes from the partial data.
