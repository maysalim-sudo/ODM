# Vivaldi takeover on Linux: findings

**Status:** cause not confirmed. It has not been reproduced.

## Why it has not been reproduced

The WSL `Ubuntu-22.04` install needs a sudo password for the desktop libraries and Vivaldi, so the running-browser test in Task 2 could not be done unattended.

## What the code shows

- Vivaldi skips `onDeterminingFilename`, so takeover depends on the `onCreated` fallback, which waits 500 ms (`extension/src/background.ts`).
- With no answer from ODM, the browser kept the download and only the console said so. Task 1 of this plan now shows the error badge, so the next failure names itself.
- On Linux, ODM writes `~/.config/vivaldi/NativeMessagingHosts/com.odm.host.json`, and only when `~/.config/vivaldi` exists (`crates/odm-host/src/register.rs`).

## Likely causes, most likely first

1. **Stale manifest:** the file points at a build that is gone. The 2026-10-27 registration plan found this kind of failure on Kubuntu.
2. **Packaged Vivaldi:** Flatpak or Snap Vivaldi cannot read that file or start the host, and `vivaldi-snapshot` uses its own folder, which ODM does not write to.
3. **Timing:** a small file that finishes within the 500 ms wait is left to the browser, as designed.

## To settle it

From a Linux install with the new extension: note the badge's message, then run:

```bash
cat ~/.config/vivaldi/NativeMessagingHosts/com.odm.host.json
```

```bash
ls -d ~/.config/vivaldi* ~/.var/app/com.vivaldi.Vivaldi 2>/dev/null
```
