# ODM Linux port — plan

Implements step 1 ("Linux port (code), with tests") of
[2026-10-23-odm-linux-and-installers-design.md](../specs/2026-10-23-odm-linux-and-installers-design.md).
Packages, MSI/ARM64 and the release script come later.

Rule: no Windows behaviour changes. Windows code stays as it is behind `#[cfg(windows)]`; Linux
code sits next to it behind `#[cfg(unix)]`. Line endings: commit with `git -c core.autocrlf=input`.

## Tasks

1. **Socket name** — `odm-core::ids`: `pipe_name_for` returns `$XDG_RUNTIME_DIR/odm.sock`, else
   `/tmp/odm-<uid>.sock` on Unix; `ODM_PIPE_NAME` still overrides. Unit tests.
2. **Linux manifest locations** — `odm-core::ids::LINUX_HOST_DIRS`: the eight browser folders from
   the design, relative to `$HOME`, each Chromium-type or Mozilla-type. One table shared by
   odm-host (writes) and odm-appcore (checks). Also `registration_file(home)`:
   `$XDG_CONFIG_HOME/ODM/registration.json` (else `~/.config/ODM/…`), the Linux stand-in for
   `HKCU\Software\ODM` (`HostPath`, `AppPath`).
3. **odm-host register** — Unix `register`/`unregister` write/remove `com.odm.host.json` in each
   folder and the registration file. `RegisterOptions` has `home` on Unix in place of
   `hkcu_prefix`. Round-trip test in a temp home.
4. **odm-host relay** — Unix: connect a `UnixStream` (not-found / refused ⇒ app not running),
   app path from `ODM_APP_PATH` else the registration file, start the app in its own process
   group. Relay tests run against a Unix-socket echo server.
5. **odm-appcore pipe** — Unix: `serve` binds a `UnixListener` (0600). A live socket at the path is
   an immediate Err (another ODM); a stale file is removed. The socket file is removed on stop.
   `client_loop` becomes generic so both platforms share it. Pipe tests run on both.
6. **odm-appcore registration/integration** — Unix `ensure_registered` reads the registration
   file; `integration::status` checks each manifest file (eight rows on Linux: Firefox and Floorp
   are separate). Path comparison is exact on Unix (case-sensitive).
7. **odm-appcore paths** — `$XDG_DATA_HOME/ODM`, else `~/.local/share/ODM`.
8. **odm-appcore power** — keep awake: a `systemd-inhibit --what=sleep:idle … sleep infinity`
   child held while downloading (no-op if missing); sleep: `systemctl suspend`; shutdown:
   `shutdown -P +1` / `shutdown -c`.
9. **browsers** — Linux launch specs: executable names looked up on `PATH`.
10. **Tauri shell** — serve the socket and run registration on Unix too; hide "Get video tools" on
    Linux (Video tools card says to install `ffmpeg` and `yt-dlp` from the package manager);
    "Start ODM when you sign in" label.
11. **Verify** — `cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo test
    --workspace` on Linux; app and extension test suites; run the app here.
