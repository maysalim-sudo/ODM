import { describe, expect, test } from "vitest";
import { isMagnet, isTorrentDownload, toTorrentPayload } from "./torrent";

describe("torrent links", () => {
  test("is_magnet_variants", () => {
    expect(isMagnet("magnet:?xt=urn:btih:abc")).toBe(true);
    expect(isMagnet("MAGNET:?xt=urn:btih:abc")).toBe(true);
    expect(isMagnet("magnet:abc")).toBe(false);
    expect(isMagnet("https://e.com/magnet:?x")).toBe(false);
    expect(isMagnet("")).toBe(false);
  });

  test("torrent_by_extension_and_mime", () => {
    expect(isTorrentDownload({ url: "https://e.com/a.torrent" })).toBe(true);
    expect(isTorrentDownload({ url: "https://e.com/dl?id=4", filename: "C:\\Users\\me\\Downloads\\Show.S01.TORRENT" })).toBe(true);
    expect(isTorrentDownload({ url: "https://e.com/dl?id=4", mime: "application/x-bittorrent" })).toBe(true);
    expect(isTorrentDownload({ url: "https://e.com/get", finalUrl: "https://cdn.e.com/x.torrent?sig=1" })).toBe(true);
    expect(isTorrentDownload({ url: "https://e.com/a.zip", mime: "application/zip" })).toBe(false);
    expect(isTorrentDownload({ url: "https://e.com/a.torrent.zip" })).toBe(false);
  });

  test("payload_shape", () => {
    const p = toTorrentPayload("https://e.com/a.torrent", "url", "https://e.com/page", "k=v", "UA/1", "capture");
    expect(p).toEqual({
      link: "https://e.com/a.torrent",
      kind: "url",
      referrer: "https://e.com/page",
      headers: { "User-Agent": "UA/1" },
      cookies: "k=v",
      source: "capture",
    });
    expect(Object.keys(toTorrentPayload("magnet:?xt=urn:btih:a", "magnet", undefined, undefined, "UA", "menu")).sort()).toEqual([
      "cookies",
      "headers",
      "kind",
      "link",
      "referrer",
      "source",
    ]);
  });
});
