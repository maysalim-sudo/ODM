# ODM (Open Downloader Manager)

A personal download manager for Windows. It downloads files over several connections at once, resumes after a pause or a crash, and keeps a queue with speed and concurrency limits.

It takes over downloads from Chrome, Edge, Brave, Vivaldi, Opera, Firefox and Floorp. It also finds the videos a page plays and downloads them: plain video files, HLS streams, and, through yt-dlp, videos on YouTube, Facebook, Instagram, X, TikTok, Vimeo and more.

ODM is a personal project, shared as is. It is unsigned and has no auto-update.

## Quick start (no building)

1. **Install the app.** Download `ODM_x.y.z_x64-setup.exe` from the [Releases](../../releases) page and run it. It installs for your user only and needs no admin rights. Because it is unsigned, Windows SmartScreen may warn you: choose **More info → Run anyway**.
2. **Start ODM once.** It registers its browser helper on first start.
3. **Add the extension.**
   - Chrome, Edge, Vivaldi and other Chromium browsers: unzip `odm-extension-chromium.zip` from the same release, open `chrome://extensions`, turn on **Developer mode**, choose **Load unpacked** and pick the unzipped folder. Keep that folder where it is.
   - Firefox and Floorp: see "Browser integration" below.
4. **Optional tools,** found automatically on `PATH`, or set in Settings:
   - [FFmpeg](https://ffmpeg.org/) for HLS streams and for site videos (`winget install Gyan.FFmpeg`).
   - [yt-dlp](https://github.com/yt-dlp/yt-dlp) for YouTube, Facebook and other sites (`winget install yt-dlp.yt-dlp`).
   - [Node.js](https://nodejs.org/), which yt-dlp needs to read YouTube.

Only download what you have the right to. Respect each site's terms.

## Building from source: requirements

- Windows 10 or 11 with the WebView2 runtime (included in Windows 11)
- Rust (stable, MSVC toolchain) and the Visual Studio C++ Build Tools
- Node 22.12 or newer and pnpm
- TypeScript is pinned to version 6 because the current `svelte-check` does not accept TypeScript 7.

## Building the installer

    node scripts/release.mjs

This builds the helper, the extension and the app, then prints the paths. Run the installer from `target\release\bundle\nsis\`. It installs for the current user, so it needs no admin rights, and it is unsigned, so Windows SmartScreen may ask you to confirm.

- ODM registers the browser helper (`odm-host.exe`, installed next to `ODM.exe`) the first time it starts.
- Uninstalling removes that registration.
- Load the extension from `extension\dist\chromium` or `extension\dist\firefox` as described under "Browser integration".

## Run the desktop app

    cd app
    pnpm install
    pnpm tauri dev

Build a standalone executable (written to `target/debug/odm-app.exe`):

    cd app
    pnpm tauri build --debug --no-bundle

The app bundles the built extension (`extension/dist`) and Tauri fails the build if it is missing, so build the extension first (see "Browser integration"), as `node scripts/release.mjs` does. This also applies to `cargo check` and `cargo clippy` of the whole workspace. In ODM, the sidebar's **Browser** chip opens a panel that shows what is registered, tests the connection, repairs the registration and opens the bundled extension folders.

## Browser integration

1. Build the browser helper next to the app: `cargo build -p odm-host` (or `--release` for a release build of the app). ODM registers it for Chrome, Edge, Brave, Vivaldi, Opera, Chromium, Firefox and Floorp the first time it starts, and again whenever it is moved.
2. Build the extension:

       cd extension
       pnpm install
       pnpm build

3. Chrome, Edge and other Chromium browsers: open `chrome://extensions`, turn on Developer mode, choose **Load unpacked** and pick `extension/dist/chromium`. Its ID is fixed (see `extension/chromium-id.txt`), which is what lets the helper talk to it.
4. Firefox and Floorp: open `about:debugging`, choose **This Firefox → Load Temporary Add-on** and pick `extension/dist/firefox/manifest.json`. A temporary add-on is removed when the browser restarts. To keep it, sign it as an unlisted add-on with your own addons.mozilla.org keys (`npx web-ext sign --channel=unlisted --source-dir dist/firefox`), or use a build that allows unsigned add-ons. In Firefox, open the ODM popup once and choose **Allow** so it can read download cookies.

Using it:
- Click a download link as usual. ODM takes it with the page's cookies and Referer, and the browser's own download disappears. If ODM doesn't answer within 5 seconds, the browser keeps the download.
- Hold **Alt** while clicking to let the browser download it instead.
- Small files (under the minimum capture size), sites on the skip list, and everything while capture is off stay in the browser. All three are in ODM's Settings.
- Right-click a link, video or audio element and choose **Download with ODM**.
- The toolbar popup shows whether ODM is running, the active and queued counts, the total speed, and Pause all / Resume all / Open ODM.
- If ODM is closed, a captured download starts it in the tray.

`odm-host --unregister` removes the registration.

## Video streams (HLS)

- When a page plays an HLS video, the extension's badge shows how many streams it found, and a small "ODM" button appears at the bottom right of the page. Pick a stream there, or in the toolbar popup, to send it to ODM. Nothing is sent unless you click.
- You can also paste an `.m3u8` link into **Add download**. For a playlist with several qualities, choose one under **Quality** (the highest is preselected).
- ODM downloads the segments over several connections, resumes after a pause or restart, decrypts AES-128 streams, and merges everything into an `.mp4` with FFmpeg.
- FFmpeg is required and not bundled. ODM uses the path set in **Settings → FFmpeg path**, or else `ffmpeg.exe` on your `PATH`.
- Not supported yet: live streams, SAMPLE-AES encryption, byte-range playlists and DASH (`.mpd`). DASH streams show up as "not supported yet".

## Command line

    cargo run -p odm-cli --release -- get <URL> [--dir DIR] [--name NAME] [-c CONNECTIONS] [--limit BYTES_PER_SECOND]

## Servers that limit connections

Some servers refuse extra connections (HTTP 429). ODM then keeps downloading with the connections that were accepted and lets the refused parts wait for a free one, so the download is slower but still finishes.

## Where ODM keeps its files

`%APPDATA%\ODM\` holds the database (`odm.db`), `settings.json` and `logs\`. Set the `ODM_DATA_DIR` environment variable to use another folder, for example to try a build without touching your real data.

## License

MIT. See [LICENSE](LICENSE).

## Tests

    cargo test --workspace -- --test-threads=4
    cd app && pnpm test && pnpm check
    cd extension && pnpm test && pnpm check

## Layout

| Path | What it is |
|---|---|
| `crates/odm-core` | Shared types: states, errors, file names, categories |
| `crates/odm-store` | SQLite storage |
| `crates/odm-engine` | The download engine and queue |
| `crates/odm-appcore` | App logic that doesn't depend on Tauri: settings, logging, UI data, event bridge |
| `crates/odm-cli` | Command-line tool |
| `crates/odm-host` | Browser helper (native messaging host) and its registration |
| `crates/odm-testserver` | Test-only HTTP server that can misbehave on purpose |
| `app/src-tauri` | The Tauri shell: commands, tray, window |
| `app/src` | The Svelte user interface |
| `extension` | The browser extension (Chromium and Firefox builds) |
| `docs/superpowers` | Design spec and implementation plans |
