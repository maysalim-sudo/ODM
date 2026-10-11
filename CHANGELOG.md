# Changelog

All notable changes to ODM are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and ODM uses [semantic versioning](https://semver.org/).

The top versioned section becomes the "What's new" text of an update, so write it for users.

## [Unreleased]

### Added

- A Settings page with sections, search and "Restore defaults". Changes apply at once.
- New downloads go to Downloads\ODM by default.
- Choose what happens when a file already exists, what to do when a download finishes, whether new downloads start at once or paused (the New download window can still start one now), and when finished downloads leave the list.
- Schedules: limit the speed only at certain times, and run the queue at certain times. While the queue waits for its start time, the list says when it starts; Resume on a download, or Start now when you add one, starts it anyway.
- When the queue is done: sleep, shut down or quit ODM. ODM counts down 60 seconds first, with a notification and a Cancel button, and a download that starts meanwhile cancels it.
- Prevent sleep while downloading.
- A speed limit for a single download.
- Leave chosen file types to the browser.
- Proxy settings: the Windows proxy, no proxy, or your own.
- A custom User-Agent.
- Video quality, format (MP4 or MKV) and subtitles.
- Start ODM with Windows, in the tray.
- A speed chip in the status bar and a speed limit menu in the tray, for quick limits.
- Automatic updates: ODM checks for a new version, shows what's new and installs it. Updates are signed, and ODM refuses one that does not verify.

### Fixed

- More file types that run code (such as .url, .chm and .appinstaller) are flagged before you open them.
- Uninstalling removes "Start ODM with Windows"; updating keeps it and the video tools.

## [0.1.0]

First public release.

[Unreleased]: https://github.com/salimmay/ODM/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/salimmay/ODM/releases/tag/v0.1.0
