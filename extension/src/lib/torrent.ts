import type { AddTorrentPayload } from "./protocol";

export function isMagnet(url: string): boolean {
  return /^magnet:\?/i.test(url);
}

const TORRENT_MIME = "application/x-bittorrent";

function endsTorrent(s: string | undefined): boolean {
  if (!s) return false;
  // A URL's query and fragment are not part of its file name.
  const path = s.split(/[?#]/)[0];
  return /\.torrent$/i.test(path);
}

/** A browser download of a `.torrent` file: by its name, its address or its type. */
export function isTorrentDownload(item: { url: string; finalUrl?: string; filename?: string; mime?: string }): boolean {
  if (item.mime?.toLowerCase().split(";")[0].trim() === TORRENT_MIME) return true;
  return endsTorrent(item.filename) || endsTorrent(item.finalUrl) || endsTorrent(item.url);
}

export function toTorrentPayload(
  link: string,
  kind: "magnet" | "url",
  referrer: string | undefined,
  cookies: string | undefined,
  userAgent: string,
  source: "capture" | "menu",
): AddTorrentPayload {
  return {
    link,
    kind,
    referrer: referrer || undefined,
    headers: { "User-Agent": userAgent },
    cookies,
    source,
  };
}
