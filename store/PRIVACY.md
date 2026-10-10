# ODM privacy policy

Last updated: 2026-10-10

ODM (Open Downloader Manager) is a Windows download manager with a browser extension. This policy covers both the ODM app and the ODM browser extension.

## Short version

ODM does not collect your data and does not send it to any server run by ODM or anyone else. There are no accounts, no analytics, no ads and no tracking.

## What the browser extension handles

When you start a download in the browser, or click a "Download with ODM" button, the extension sends the following to the ODM program on your own computer, and nowhere else:

- the download address (URL) and the page title;
- the cookies for that address, so downloads that need a login work;
- the Referer and Origin of the page.

This only happens after you start a download or click a Download button. The data travels from the extension to ODM on the same computer, through the browser's native messaging channel and a local pipe. It never leaves your computer through the extension.

Video detection reads response headers (for example, to spot `.m3u8` playlists and video files) locally in your browser, only to list the videos a page plays. Nothing is blocked or modified, and nothing is sent anywhere. You can turn video detection off for any site.

## What is stored

- Settings are stored locally on your computer (`%APPDATA%\ODM\settings.json`, and the extension's local storage).
- The download list is stored locally in `%APPDATA%\ODM\odm.db`.
- Logs are stored locally in `%APPDATA%\ODM\logs`, with query strings removed from addresses.

You can delete these at any time by removing the folder or uninstalling ODM.

## Downloads you start

ODM connects to the servers hosting the files you choose to download, as any download manager does. Those servers see your IP address and the request, as they would with any browser download.

## Update checks

ODM checks for a new version 10 seconds after it starts and then every 6 hours, by fetching a small file (`latest.json`) from ODM's GitHub releases. When you choose to update, it downloads the installer from the same place. GitHub sees these requests like any other download; ODM sends nothing about you or your downloads with them. You can turn the automatic checks off in Settings → Updates.

## Optional video tools

If you click to install the optional video tools, ODM downloads yt-dlp, FFmpeg (FFmpeg-Builds) and Deno from their GitHub releases. This happens only when you click, and GitHub sees that request like any other download.

## Contact

Questions or concerns: open an issue at https://github.com/salimmay/ODM/issues
