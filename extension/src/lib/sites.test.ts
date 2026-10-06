import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { SITE_HOSTS, isSitePage, pageAddress, toPagePayload, visibleStreams } from "./sites";

describe("SITE_HOSTS", () => {
  it("lists the same sites as the app", () => {
    // Read the engine's list from its source, so the two can never drift apart.
    const rust = readFileSync(new URL("../../../crates/odm-engine/src/site/mod.rs", import.meta.url), "utf8");
    const block = rust.slice(rust.indexOf("pub const SITE_HOSTS"), rust.indexOf("];", rust.indexOf("pub const SITE_HOSTS")));
    const hosts = [...block.matchAll(/"([^"]+)"/g)].map((m) => m[1]);
    expect(hosts.length).toBeGreaterThan(10);
    expect(SITE_HOSTS).toEqual(hosts);
  });
});

describe("isSitePage", () => {
  it("accepts the hosts and their subdomains", () => {
    expect(isSitePage("https://www.youtube.com/watch?v=jNQXAC9IVRw")).toBe(true);
    expect(isSitePage("https://www.youtube.com/shorts/abc")).toBe(true);
    expect(isSitePage("https://m.youtube.com/watch?v=abc")).toBe(true);
    expect(isSitePage("https://www.facebook.com/someone/videos/1")).toBe(true);
    expect(isSitePage("https://youtu.be/abc")).toBe(true);
  });

  it("rejects other hosts, look-alikes and non-web addresses", () => {
    expect(isSitePage("https://example.com/watch?v=abc")).toBe(false);
    expect(isSitePage("https://notyoutube.com/")).toBe(false);
    expect(isSitePage("https://youtube.com.evil.example/")).toBe(false);
    expect(isSitePage("ftp://www.youtube.com/")).toBe(false);
    expect(isSitePage("not a url")).toBe(false);
  });
});

describe("pageAddress", () => {
  const fb = "https://www.facebook.com/";

  it("picks the first video or reel link and makes it absolute", () => {
    expect(pageAddress(fb, ["/someone", "/someone/videos/123", "/reel/9"])).toBe(
      "https://www.facebook.com/someone/videos/123",
    );
    expect(pageAddress(fb, ["/profile", "/reel/9?s=1", "/x/videos/1"])).toBe("https://www.facebook.com/reel/9?s=1");
    expect(pageAddress(fb, ["/watch/?v=42&x=1"])).toBe("https://www.facebook.com/watch/?v=42&x=1");
    expect(pageAddress(fb, ["https://www.facebook.com/a/videos/7"])).toBe("https://www.facebook.com/a/videos/7");
  });

  it("falls back to the page address", () => {
    expect(pageAddress(fb, [])).toBe(fb);
    expect(pageAddress(fb, ["/someone", "/photos/1"])).toBe(fb);
  });

  it("ignores candidates on other sites", () => {
    const yt = "https://www.youtube.com/watch?v=abc";
    expect(pageAddress(yt, ["/other/videos/1"])).toBe(yt);
  });
});

describe("toPagePayload", () => {
  it("sets Referer to the page and sends cookies as a header string", () => {
    const p = toPagePayload(
      "https://www.youtube.com/watch?v=abc",
      "A film - YouTube",
      [
        { name: "a", value: "1" },
        { name: "b", value: "2" },
      ],
      "UA/1",
    );
    expect(p).toEqual({
      page_url: "https://www.youtube.com/watch?v=abc",
      page_title: "A film - YouTube",
      headers: { "User-Agent": "UA/1", Referer: "https://www.youtube.com/watch?v=abc" },
      cookies: "a=1; b=2",
    });
  });

  it("omits cookies and a blank title", () => {
    const p = toPagePayload("https://youtu.be/abc", "  ", [], "UA");
    expect(p.cookies).toBeUndefined();
    expect(p.page_title).toBeUndefined();
  });
});

describe("adult video sites go to yt-dlp", () => {
  it("accepts the big ones yt-dlp supports, with their regional subdomains", () => {
    for (const u of [
      "https://www.pornhub.com/view_video.php?viewkey=abc",
      "https://de.pornhub.com/view_video.php?viewkey=abc",
      "https://www.xvideos.com/video123/x",
      "https://xhamster.com/videos/x-1",
      "https://www.xnxx.com/video-1/x",
      "https://www.redtube.com/123",
      "https://www.youporn.com/watch/1/x/",
      "https://spankbang.com/abc/video/x",
      "https://www.eporner.com/video-abc/x/",
    ]) expect(isSitePage(u)).toBe(true);
    expect(isSitePage("https://notpornhub.com/x")).toBe(false);
  });
});

describe("visibleStreams", () => {
  const list = [{ url: "https://cdn.x/master.m3u8", kind: "hls" as const, label: "x", supported: true }];
  it("hides detected stream links on yt-dlp sites, where only the page download works", () => {
    expect(visibleStreams("https://www.pornhub.com/view_video.php?viewkey=a", list)).toEqual([]);
    expect(visibleStreams("https://www.youtube.com/watch?v=a", list)).toEqual([]);
  });
  it("keeps them everywhere else", () => {
    expect(visibleStreams("https://anime.example/watch/1", list)).toEqual(list);
    expect(visibleStreams(undefined, list)).toEqual(list);
  });
});
