# Linux build and more installer types — design

**Status:** drafted 2026-10-23 for review.
**Branch:** `master`.

## Goal

Ship ODM as every installer type we can build and support from this PC:

| Platform | Formats | Built on |
|---|---|---|
| Windows x64 | NSIS `.exe` (today), MSI | Windows |
| Windows ARM64 | NSIS `.exe`, MSI | Windows (cross-compiled, needs the MSVC ARM64 build tools) |
| Linux x64 | `.deb`, `.rpm`, `.AppImage` | WSL Ubuntu 22.04 |

Everything ODM does on Windows works on Linux, except what has no Linux equivalent (listed under Out of scope). Updates stay signed with the existing key.

### Out of scope

macOS (needs a Mac or CI and Apple signing), Linux ARM64, Flatpak/Snap, distro repositories, "Get video tools" on Linux (Linux users install FFmpeg and yt-dlp from their package manager; ODM already finds them on `PATH`), the Mark of the Web (Windows only), MSI per-user installs (Tauri's WiX template installs per machine).

## What changes on Linux

The engine, torrents, store, UI and settings already build anywhere. These parts are Windows-only today:

| Area | Windows today | Linux |
|---|---|---|
| App ↔ browser helper | named pipe `\\.\pipe\odm-<user>` (`odm-appcore::pipe`, `odm-host::relay`) | Unix socket `$XDG_RUNTIME_DIR/odm.sock`, else `/tmp/odm-<uid>.sock`, mode 0600; `ODM_PIPE_NAME` still overrides |
| Native-messaging registration | registry keys (`odm-host::register`, `odm-appcore::registration`) | manifest JSON files per browser (table below) |
| Integration status / repair | registry checks (`browsers.rs`, `integration.rs`) | the same checks against the manifest files |
| Data folder | `%APPDATA%\ODM`, tools in `%LOCALAPPDATA%\ODM\tools` | `$XDG_DATA_HOME/ODM` (else `~/.local/share/ODM`); tools folder unused |
| Prevent sleep | `SetThreadExecutionState` | `systemd-inhibit`-style inhibitor through D-Bus (`org.freedesktop.login1.Inhibit`, "sleep:idle"), held while anything downloads; no-op if unavailable |
| Start with Windows | autostart plugin (Run key) | the same plugin (`~/.config/autostart/ODM.desktop`); the label reads "Start ODM when you sign in" |
| Magnet / `.torrent` associations | installer question | `.desktop` file `MimeType=x-scheme-handler/magnet;application/x-bittorrent;` in deb/rpm; the AppImage registers nothing (documented) |
| "Get video tools" | downloads FFmpeg, yt-dlp, Deno | hidden; the Video tools card says to install `ffmpeg` and `yt-dlp` with the package manager, and shows what it found on `PATH` |
| Sparse torrent files | `FSCTL_SET_SPARSE` | nothing needed: Linux files are sparse when sized |

Native-messaging manifest locations (user level):

| Browser | Folder |
|---|---|
| Chrome | `~/.config/google-chrome/NativeMessagingHosts/` |
| Chromium | `~/.config/chromium/NativeMessagingHosts/` |
| Edge | `~/.config/microsoft-edge/NativeMessagingHosts/` |
| Brave | `~/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/` |
| Vivaldi | `~/.config/vivaldi/NativeMessagingHosts/` |
| Opera | `~/.config/opera/NativeMessagingHosts/` |
| Firefox | `~/.mozilla/native-messaging-hosts/` |
| Floorp | `~/.floorp/native-messaging-hosts/` |

The Chromium manifest lists `allowed_origins` (the extension IDs already in `extension/chromium-ids.json`); the Firefox manifest lists `allowed_extensions: ["odm@odm.local"]`. Both name the `odm-host` binary next to the app (`/usr/bin/odm-host` in deb/rpm; inside the AppImage the path changes per mount, so the app rewrites the manifests at each start, as it already re-checks registration on Windows).

### Code shape

- Platform code moves behind small modules with `#[cfg(windows)]` / `#[cfg(unix)]` twins and one shared interface:
  - `odm-core::ipc`: `transport` with `connect(name)` and `listen(name)` over a named pipe or a Unix socket; framing stays the same.
  - `odm-host::register`: `register()`, `unregister()`, `status()`; Windows uses the registry as today, Linux writes and removes the manifest files.
  - `odm-appcore::paths`: XDG on Linux.
  - `odm-appcore::power`: a Linux inhibitor.
- No Windows behaviour changes. The existing Windows tests keep passing; new tests run on Linux in WSL (`cargo test --workspace`), and the Windows-only tests get `#[cfg(windows)]`.

## Installers

### Windows MSI (x64 and ARM64)

- `tauri.conf.json` bundle targets become `["nsis", "msi"]`; WiX is fetched by the Tauri CLI.
- The MSI installs per machine (admin rights). It has no hooks: no association question (ODM's registration still runs on first start), and uninstall leaves the browser-helper registration until ODM's own "Repair" or the next NSIS uninstall. The download page calls it "MSI (for IT / admin installs)".
- Updates: `release.mjs --publish-ready` also signs the MSI and writes a `windows-x86_64-msi` (and `windows-aarch64-msi`) entry in `latest.json`, so an MSI install updates with the MSI, an NSIS install with the NSIS installer.

### Windows ARM64

- Rust target `aarch64-pc-windows-msvc`; the sidecar `odm-host` is built for that target too (`binaries/odm-host-aarch64-pc-windows-msvc.exe`).
- `latest.json` gains `windows-aarch64` (NSIS) and `windows-aarch64-msi`.
- Not tested on ARM hardware here; it is built and its tests run where possible. The download page marks it as such until someone confirms it on an ARM laptop.

### Linux (x64)

- Built in WSL with `node scripts/release.mjs` (it detects Linux) into `.deb`, `.rpm` and `.AppImage`.
- `odm-host` ships in the packages (`externalBin`) and lands next to the app.
- Updates: only the AppImage updates itself (Tauri's updater replaces the AppImage); `latest.json` gains `linux-x86_64` pointing at the AppImage. deb/rpm users update by installing the new package; the in-app banner says so instead of offering "Update".
- Signing: the same updater key signs the AppImage. The `.deb`/`.rpm` are not GPG-signed (out of scope).

### Release script

`scripts/release.mjs --publish-ready` builds what the current machine can build, signs it, and merges into one `target/release-assets/` with a `latest.json` covering every platform built so far (Windows run first, then the Linux run adds its entries). It also writes fresh extension zips and `SHA256SUMS.txt` for every file.

Release asset names (stable names for the website):

| File | What |
|---|---|
| `ODM-setup.exe` | Windows x64 installer (NSIS), as today |
| `ODM-setup-arm64.exe` | Windows ARM64 installer (NSIS) |
| `ODM-x64.msi`, `ODM-arm64.msi` | MSI installers |
| `ODM-amd64.deb`, `ODM-x86_64.rpm`, `ODM-x86_64.AppImage` | Linux packages |
| `odm-extension-chromium.zip`, `odm-extension-firefox.zip` | extension, rebuilt each release |
| `latest.json`, `*.sig`, `SHA256SUMS.txt` | updater files and checksums |

## Testing

- Linux: the full `cargo test --workspace` in WSL, plus the app and extension test suites; a WSLg run of the `.deb` build: add a magnet, download, and the browser connection with Chrome on Linux if available.
- Windows: every existing check, plus building the MSI and installing it in your session.
- ARM64: build only (no ARM hardware here).

## Order

1. Linux port (code), with tests in WSL.
2. Linux packages, then Windows MSI and ARM64 builds.
3. Release script: all formats, `latest.json` for all, extension zips, checksums.
4. Website brief (separate document for the website session).
