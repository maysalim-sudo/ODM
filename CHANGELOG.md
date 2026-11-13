# Changelog

All notable changes to ODM are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and ODM uses [semantic versioning](https://semver.org/).

The top versioned section becomes the "What's new" text of an update, so write it for users.

## [Unreleased]

## [0.4.3] - 2026-11-13

An action bar and a right-click menu for many downloads at once, duplicates caught before they start, and Linux copies that update themselves from the next version on. Update the ODM browser extension along with this version.

### Added

- An action bar above the list: Resume, Pause and Stop, plus Queue, Low, Speed and More menus for the selected downloads. It replaces the floating "N selected" bar.
- A right-click menu on downloads, acting on the whole selection.
- The new download dialog warns when a link is already in your list (Show it, or add it anyway) and suggests a free name when the file name is taken.
- Recent save folders under Save to, and recent links under the empty paste bar.
- Choosing Later shows the file's size, warns when the drive is too full, and says when the link expires, with Open the page to get a fresh one.
- ODM says when the browser extension is older than ODM.
- The ↓ on thumbnails on more video sites, as on YouTube.
- Linux: AppImage, .deb and .rpm copies update from inside ODM, starting with the next version.

### Changed

- ODM opens at 95% of the screen, centred.
- The download table and the details panel sit in rounded cards, off the window's edges.

### Fixed

- A plain video file saved as MP3 is named .mp3 from the start.
- Pasting a YouTube channel or home address no longer downloads every video on it.
- At start-up, ODM repairs a browser still pointing at another copy of ODM.

## [0.4.2] - 2026-11-09

Download straight from YouTube thumbnails, and no more update offer for video tools that are already current. Update the ODM browser extension along with this version: the thumbnail button is part of the extension.

### Added

- A ↓ on every YouTube thumbnail (home, search, channels, playlists, the list beside a video, Shorts) that saves the video as Video or MP3. Playlist and Mix cards don't get one.

### Changed

- Settings → Video offers "Update video tools" only when a newer yt-dlp or Deno is out, or FFmpeg is over four months old; otherwise it says "Up to date".

## [0.4.1] - 2026-11-08

A download's details now sit in a panel under the list, and the MP3 and stream problems found in 0.4.0 are fixed. Update the ODM browser extension along with this version: the stream and YouTube fixes are in the extension.

### Added

- Details in a resizable panel under the list, with tabs for Info, Speed (the last two minutes), Parts, and Files and peers for torrents. One click shows a download there; several show their combined totals.
- Ctrl+A selects every download in the current tab.
- Double-click a finished download to open it.
- "When a download finishes" can be set to Ask: ODM comes forward and asks whether to open the file, its folder, or neither.

### Changed

- The side drawer is gone; the panel under the list replaces it.
- The floating selection bar appears only when two or more downloads are selected.

### Fixed

- MP3 downloads are saved in the Audio folder, not Videos.
- A stream saved as MP3 is named .mp3 from the start.
- HLS streams that the browser plays itself are saved as streams, not as the small playlist file.
- On YouTube's home and search pages, the button over a preview saves that video instead of every video on the page.
- The buttons on cards sit centred again.

## [0.4.0] - 2026-11-02

A new look built to be easier to follow: each download now shows its own next step. Update the ODM browser extension along with this version: the new video buttons and controls are part of the extension.

### Added

- A new home screen. Downloads are grouped into Needs you, Downloading, Up next and Finished, and each one has its own buttons.
- Choose what downloads next: drag a row in Up next, use Move to top, Move up and Move down, or press Alt+Up and Alt+Down. Sort by size puts the smallest first, and a second click puts the largest first.
- See how much is left to download in the status bar, with its own figure on each group.
- Paste a link anywhere with Ctrl+V. ODM reads it, shows what it found, and asks only for what it needs. The other options are under More options.
- Plain explanations when a download fails, with the fix on the row. Technical details are in the details panel.
- Choose the video quality and subtitles for one download when you add it.
- The detailed table is one click away with the view switch.
- Inter is now ODM's typeface.
- Choose MP3 or video for each video ODM finds: on the button over a video, in the page's video list and in the extension's popup. Plain video files show their real type, such as WEBM.
- MP3 works for plain video files, HLS streams and site videos. It needs FFmpeg from "Get video tools".
- Move the page's video list: drag it by its grip, or use the arrow keys. It stays where you put it.
- A × on the button over a video hides it.
- The extension's icon shows a warning when ODM doesn't answer a download from the browser.

### Changed

- Cancelled downloads are now under Finished, not Problems.
- Settings has six sections, with the settings most people change under Essentials. Every setting is still there, and search still finds them all.
- The welcome screen shows one step at a time.
- Ctrl+N now goes to the paste bar.

### Fixed

- The × on the page's video list, and turning video detection off, now really hide them.
- The page's video list no longer runs off the left edge on pages with a scrollbar.

## [0.2.0] - 2026-10-23

### Added

- Torrent downloads. Add a magnet link or a .torrent file from the Add window, by dropping it on the window, from the browser extension, or by opening it in Windows (an installer option).
- Choose which files of a torrent to download before it starts, and change them while it runs. A magnet shows "Fetching file list…" until peers send its list.
- Follow a torrent in the details panel: peers, upload speed, per-file progress and a piece map.
- A Torrents section in Settings: listen port, router port opening (UPnP), upload while downloading, and peers per torrent.
- ODM is a downloader, not a seeding client: a torrent stops sharing once its files are complete.

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

[0.4.0]: https://github.com/maysalim-sudo/ODM/releases/tag/v0.4.0
[0.2.0]: https://github.com/maysalim-sudo/ODM/releases/tag/v0.2.0
[0.1.0]: https://github.com/maysalim-sudo/ODM/releases/tag/v0.1.0
