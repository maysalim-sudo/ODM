import { describe, expect, it } from "vitest";
import { MIN_VIDEO_BYTES, SentHeaderCache, StreamStore, classify, responseSize, streamLabel, toStreamPayload, toVideoFilePayload } from "./streams";

describe("classify", () => {
  it("detects HLS by path, ignoring the query", () => {
    expect(classify("https://cdn.example.com/a/index.m3u8?token=1", undefined)).toBe("hls");
    expect(classify("https://cdn.example.com/a/INDEX.M3U8", undefined)).toBe("hls");
  });

  it("detects HLS by content type, ignoring parameters and case", () => {
    expect(classify("https://x/playlist", "application/vnd.apple.mpegurl; charset=utf-8")).toBe("hls");
    expect(classify("https://x/playlist", "application/x-mpegURL")).toBe("hls");
    expect(classify("https://x/playlist", "audio/mpegurl")).toBe("hls");
  });

  it("detects DASH", () => {
    expect(classify("https://x/manifest.mpd?a=b", undefined)).toBe("dash");
    expect(classify("https://x/manifest", "application/dash+xml")).toBe("dash");
  });

  it("ignores everything else", () => {
    expect(classify("https://x/clip.mp4", "video/mp4")).toBeNull();
    expect(classify("https://x/page.html", undefined)).toBeNull();
    expect(classify("not a url", undefined)).toBeNull();
  });
});

describe("streamLabel", () => {
  it("shows the last path segment and the host", () => {
    expect(streamLabel("https://cdn.example.com/a/b/index.m3u8?x=1")).toBe("index.m3u8 · cdn.example.com");
  });

  it("falls back to the host alone", () => {
    expect(streamLabel("https://cdn.example.com/")).toBe("cdn.example.com");
  });
});

describe("StreamStore", () => {
  it("stream_store_dedupes_and_clears", () => {
    const s = new StreamStore();
    const u = "https://cdn.example.com/a/index.m3u8";
    expect(s.add(1, u, "hls")).toBe(true);
    expect(s.add(1, u, "hls")).toBe(false);
    expect(s.add(1, `${u}#frag`, "hls")).toBe(false);
    expect(s.list(1)).toHaveLength(1);

    // Another tab is independent.
    expect(s.add(2, u, "hls")).toBe(true);

    // A query makes a different URL.
    expect(s.add(1, `${u}?v=2`, "hls")).toBe(true);
    expect(s.add(1, "https://cdn.example.com/m.mpd", "dash")).toBe(true);
    expect(s.list(1).map((x) => [x.kind, x.supported])).toEqual([
      ["hls", true],
      ["hls", true],
      ["dash", false],
    ]);
    expect(s.count(1)).toBe(2);

    s.clear(1);
    expect(s.list(1)).toEqual([]);
    expect(s.count(1)).toBe(0);
    expect(s.count(2)).toBe(1);
    expect(s.add(1, u, "hls")).toBe(true);
  });

  it("lists in insertion order with labels", () => {
    const s = new StreamStore();
    s.add(3, "https://a.example/one.m3u8", "hls");
    s.add(3, "https://b.example/two.m3u8", "hls");
    expect(s.list(3).map((x) => x.label)).toEqual(["one.m3u8 · a.example", "two.m3u8 · b.example"]);
  });
});

describe("toStreamPayload", () => {
  it("sets Referer to the page and sends cookies as a header string", () => {
    const stream = { url: "https://cdn.example.com/index.m3u8", kind: "hls" as const, label: "x", supported: true };
    const p = toStreamPayload(
      stream,
      "https://site.example/watch",
      "A film",
      [
        { name: "a", value: "1" },
        { name: "b", value: "2" },
      ],
      "UA/1",
    );
    expect(p).toEqual({
      page_url: "https://site.example/watch",
      page_title: "A film",
      playlist_url: "https://cdn.example.com/index.m3u8",
      headers: { "User-Agent": "UA/1", Referer: "https://site.example/watch" },
      cookies: "a=1; b=2",
    });
  });

  it("omits cookies and a blank title", () => {
    const stream = { url: "https://x/i.m3u8", kind: "hls" as const, label: "x", supported: true };
    const p = toStreamPayload(stream, "https://site/", "  ", [], "UA");
    expect(p.cookies).toBeUndefined();
    expect(p.page_title).toBeUndefined();
  });
});

const MB = 1024 * 1024;

describe("responseSize", () => {
  it("prefers the total from Content-Range", () => {
    expect(responseSize("100", undefined)).toBe(100);
    expect(responseSize(undefined, "bytes 0-1023/5000000")).toBe(5000000);
    expect(responseSize("1024", "bytes 0-1023/5000000")).toBe(5000000);
  });

  it("is undefined when no number is available", () => {
    expect(responseSize(undefined, "bytes 0-1/*")).toBeUndefined();
    expect(responseSize("abc", undefined)).toBeUndefined();
  });
});

describe("classify plain video files", () => {
  it("needs a video type and at least the minimum size", () => {
    expect(MIN_VIDEO_BYTES).toBe(2 * MB);
    expect(classify("https://x/a", "video/mp4", 3 * MB)).toBe("file");
    expect(classify("https://x/a", "video/mp4", 1 * MB)).toBeNull();
    expect(classify("https://x/a", "video/mp4")).toBeNull();
  });

  it("falls back to the extension when the type is missing or generic", () => {
    expect(classify("https://x/a.webm", "application/octet-stream", 5 * MB)).toBe("file");
    expect(classify("https://x/a.mkv", undefined, 5 * MB)).toBe("file");
    expect(classify("https://x/a.txt", "application/octet-stream", 5 * MB)).toBeNull();
  });

  it("never lists googlevideo.com", () => {
    expect(classify("https://rr1---sn.googlevideo.com/videoplayback?id=1", "video/mp4", 50 * MB)).toBeNull();
  });

  it("keeps HLS independent of size", () => {
    expect(classify("https://x/a.m3u8", undefined)).toBe("hls");
  });
});

describe("StreamStore files", () => {
  it("labels a file with name, size and host", () => {
    const s = new StreamStore();
    s.add(1, "https://cdn.example.com/v/clip.mp4", "file", 12.3 * MB);
    const [f] = s.list(1);
    expect(f.label).toBe("clip.mp4 · 12.3 MB · cdn.example.com");
    expect(f.supported).toBe(true);
  });

  it("fills a missing size on a repeat add and returns false", () => {
    const s = new StreamStore();
    const u = "https://cdn.example.com/clip.mp4";
    expect(s.add(1, u, "file")).toBe(true);
    expect(s.add(1, u, "file", 3 * MB)).toBe(false);
    expect(s.list(1)[0].size).toBe(3 * MB);
  });

  it("counts files", () => {
    const s = new StreamStore();
    s.add(1, "https://x/a.mp4", "file", 3 * MB);
    s.add(1, "https://x/a.m3u8", "hls");
    expect(s.count(1)).toBe(2);
  });
});

describe("toVideoFilePayload", () => {
  it("builds a menu download with the page as referrer", () => {
    const stream = { url: "https://cdn.x/v/clip.mp4?sig=1", kind: "file" as const, label: "x", supported: true };
    const p = toVideoFilePayload(stream, "https://site.example/watch", [{ name: "a", value: "1" }, { name: "b", value: "2" }], "UA/1");
    expect(p.source).toBe("menu");
    expect(p.referrer).toBe("https://site.example/watch");
    expect(p.filename).toBe("clip.mp4");
    expect(p.url).toBe("https://cdn.x/v/clip.mp4?sig=1");
    expect(p.headers["User-Agent"]).toBe("UA/1");
    expect(p.cookies).toBe("a=1; b=2");
  });
});

describe("SentHeaderCache", () => {
  it("remembers the Referer and Origin the browser sent, by URL without fragment", () => {
    const c = new SentHeaderCache();
    c.remember("https://cdn.x/p/master.m3u8#t=1", [
      { name: "referer", value: "https://player.embed.example/e/abc" },
      { name: "ORIGIN", value: "https://player.embed.example" },
      { name: "Accept", value: "*/*" },
    ]);
    expect(c.get("https://cdn.x/p/master.m3u8")).toEqual({
      referer: "https://player.embed.example/e/abc",
      origin: "https://player.embed.example",
    });
    expect(c.get("https://cdn.x/other.m3u8")).toBeUndefined();
  });
  it("ignores requests that sent neither header and forgets the oldest past its limit", () => {
    const c = new SentHeaderCache(2);
    c.remember("https://a/1", [{ name: "Accept", value: "*/*" }]);
    expect(c.get("https://a/1")).toBeUndefined();
    c.remember("https://a/1", [{ name: "Referer", value: "https://r/1" }]);
    c.remember("https://a/2", [{ name: "Referer", value: "https://r/2" }]);
    c.remember("https://a/3", [{ name: "Referer", value: "https://r/3" }]);
    expect(c.get("https://a/1")).toBeUndefined();
    expect(c.get("https://a/3")).toEqual({ referer: "https://r/3", origin: undefined });
  });
});

describe("payloads use the headers the player sent", () => {
  const sent = { referer: "https://player.embed.example/e/abc", origin: "https://player.embed.example" };
  it("stream: Referer and Origin come from the player, not the page", () => {
    const s = { url: "https://cdn.x/master.m3u8", kind: "hls" as const, label: "x", supported: true };
    const p = toStreamPayload(s, "https://anime.example/watch/1", "Ep 1", [], "UA", sent);
    expect(p.headers.Referer).toBe(sent.referer);
    expect(p.headers.Origin).toBe(sent.origin);
    expect(p.page_url).toBe("https://anime.example/watch/1");
  });
  it("stream: without recorded headers the page is the Referer and there is no Origin", () => {
    const s = { url: "https://cdn.x/master.m3u8", kind: "hls" as const, label: "x", supported: true };
    const p = toStreamPayload(s, "https://site/", "T", [], "UA");
    expect(p.headers.Referer).toBe("https://site/");
    expect(p.headers.Origin).toBeUndefined();
  });
  it("file: the player's Referer becomes the referrer, Origin is passed on", () => {
    const s = { url: "https://cdn.x/v/clip.mp4", kind: "file" as const, label: "x", supported: true, size: 5e6 };
    const p = toVideoFilePayload(s, "https://anime.example/watch/1", [], "UA", sent);
    expect(p.referrer).toBe(sent.referer);
    expect(p.headers.Origin).toBe(sent.origin);
  });
});

describe("stream pieces are not videos", () => {
  it("never lists HLS/DASH segments as video files", () => {
    expect(classify("https://cdn.x/hls/v.mp4/seg-3-v1-a1.m4s", "video/mp4", 5e6)).toBeNull();
    expect(classify("https://cdn.x/hls/seg_000004.ts", "video/mp2t", 5e6)).toBeNull();
    expect(classify("https://cdn.x/chunk.m4s?x=1", "application/octet-stream", 5e6)).toBeNull();
    expect(classify("https://cdn.x/full/movie.mp4", "video/mp4", 5e6)).toBe("file");
  });
});
