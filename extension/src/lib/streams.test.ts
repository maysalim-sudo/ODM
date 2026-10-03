import { describe, expect, it } from "vitest";
import { StreamStore, classify, streamLabel, toStreamPayload } from "./streams";

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
