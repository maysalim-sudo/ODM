# ODM (Open Downloader Manager)

A personal download manager for Windows. It downloads files over several connections at once, resumes after a pause or a crash, and keeps a queue with speed and concurrency limits.

## Status

- Done: the download engine, the command-line tool and the desktop app.
- Not yet built: the browser extension, video streams (HLS) and the installer.

## Requirements

- Windows 10 or 11 with the WebView2 runtime (included in Windows 11)
- Rust (stable, MSVC toolchain) and the Visual Studio C++ Build Tools
- Node 22.12 or newer and pnpm
- TypeScript is pinned to version 6 because the current `svelte-check` does not accept TypeScript 7.

## Run the desktop app

    cd app
    pnpm install
    pnpm tauri dev

Build a standalone executable (written to `target/debug/odm-app.exe`):

    cd app
    pnpm tauri build --debug --no-bundle

## Command line

    cargo run -p odm-cli --release -- get <URL> [--dir DIR] [--name NAME] [-c CONNECTIONS] [--limit BYTES_PER_SECOND]

## Servers that limit connections

Some servers refuse extra connections (HTTP 429). ODM then keeps downloading with the connections that were accepted and lets the refused parts wait for a free one, so the download is slower but still finishes.

## Where ODM keeps its files

`%APPDATA%\ODM\` holds the database (`odm.db`), `settings.json` and `logs\`. Set the `ODM_DATA_DIR` environment variable to use another folder, for example to try a build without touching your real data.

## Tests

    cargo test --workspace -- --test-threads=4
    cd app && pnpm test && pnpm check

## Layout

| Path | What it is |
|---|---|
| `crates/odm-core` | Shared types: states, errors, file names, categories |
| `crates/odm-store` | SQLite storage |
| `crates/odm-engine` | The download engine and queue |
| `crates/odm-appcore` | App logic that doesn't depend on Tauri: settings, logging, UI data, event bridge |
| `crates/odm-cli` | Command-line tool |
| `crates/odm-testserver` | Test-only HTTP server that can misbehave on purpose |
| `app/src-tauri` | The Tauri shell: commands, tray, window |
| `app/src` | The Svelte user interface |
| `docs/superpowers` | Design spec and implementation plans |
