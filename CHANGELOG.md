# Changelog

All notable changes to ODM are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and ODM uses [semantic versioning](https://semver.org/).

The top versioned section becomes the "What's new" text of an update, so write it for users.

## [0.1.0] - 2026-10-11

First public release.

### Added

- Downloads files over several connections at once, resumes after a pause, a crash or a restart, and keeps a queue with speed and concurrency limits.
- Takes over downloads from Chrome, Edge, Brave, Vivaldi, Opera, Firefox and Floorp through the ODM extension.
- Finds the videos a page plays and downloads them: plain video files, HLS streams, and site videos (YouTube, Facebook, Instagram, X, TikTok, Vimeo and more) through yt-dlp.
- A one-click "Get video tools" download of FFmpeg, yt-dlp and Deno, checked against their published checksums.
- A welcome screen that helps you add the extension to your browser.
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
- More file types that run code (such as .url, .chm and .appinstaller) are flagged before you open them.

[0.1.0]: https://github.com/salimmay/ODM/releases/tag/v0.1.0
