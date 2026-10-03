import { describe, expect, it } from "vitest";
import {
  basename,
  captureUrl,
  cookieHeader,
  isHttp,
  shouldCancelBrowserDownload,
  shouldOffer,
  toCapturePayload,
  toMenuPayload,
} from "./capture";

describe("isHttp", () => {
  it("accepts http(s) case-insensitively", () => {
    expect(isHttp("HTTPS://x")).toBe(true);
    expect(isHttp("http://x/y")).toBe(true);
  });
  it("rejects other schemes", () => {
    for (const u of ["blob:https://x/1", "data:text/plain,a", "ftp://x/a", "file:///a"]) {
      expect(isHttp(u)).toBe(false);
    }
  });
});

describe("basename", () => {
  it("handles both separators", () => {
    expect(basename("C:\\Users\\a\\Downloads\\movie.mp4")).toBe("movie.mp4");
    expect(basename("/home/a/x.zip")).toBe("x.zip");
  });
  it("is undefined when empty", () => {
    expect(basename("")).toBeUndefined();
    expect(basename(undefined)).toBeUndefined();
  });
});

describe("cookieHeader", () => {
  it("joins cookies", () => {
    expect(cookieHeader([{ name: "a", value: "1" }, { name: "b", value: "2" }])).toBe("a=1; b=2");
  });
  it("is undefined when none", () => {
    expect(cookieHeader([])).toBeUndefined();
  });
});

describe("captureUrl and shouldOffer", () => {
  it("prefers an http finalUrl", () => {
    expect(captureUrl({ url: "https://a/x", finalUrl: "https://b/y" })).toBe("https://b/y");
  });
  it("falls back to url when finalUrl is blob", () => {
    expect(captureUrl({ url: "https://a/x", finalUrl: "blob:https://a/1" })).toBe("https://a/x");
  });
  it("does not offer blob URLs or when Alt is active", () => {
    expect(shouldOffer({ url: "blob:https://a/1" }, false)).toBe(false);
    expect(shouldOffer({ url: "https://a/x" }, true)).toBe(false);
    expect(shouldOffer({ url: "https://a/x" }, false)).toBe(true);
  });
});

describe("payloads", () => {
  it("builds a capture payload", () => {
    const item = { url: "https://a/x", referrer: "https://r/", filename: "C:\\d\\f.zip", totalBytes: 10, mime: "application/zip" };
    const p = toCapturePayload(item, [{ name: "a", value: "1" }], "UA", 1000);
    expect(p.expires_at_ms).toBe(5500);
    expect(p.source).toBe("capture");
    expect(p.filename).toBe("f.zip");
    expect(p.referrer).toBe("https://r/");
    expect(p.cookies).toBe("a=1");
    expect(p.filesize).toBe(10);
    expect(p.headers["User-Agent"]).toBe("UA");
  });
  it("leaves filesize undefined for unknown sizes", () => {
    expect(toCapturePayload({ url: "https://a/x", totalBytes: -1 }, [], "UA", 0).filesize).toBeUndefined();
    expect(toCapturePayload({ url: "https://a/x", totalBytes: 0 }, [], "UA", 0).filesize).toBeUndefined();
  });
  it("builds a menu payload", () => {
    const p = toMenuPayload("https://a/x", "https://page/", [], "UA");
    expect(p.source).toBe("menu");
    expect(p.expires_at_ms).toBeUndefined();
    expect(p.referrer).toBe("https://page/");
  });
});

describe("shouldCancelBrowserDownload", () => {
  it("only_ok_true_cancels_the_browser_download", () => {
    expect(shouldCancelBrowserDownload({ id: "1", ok: true })).toBe(true);
    expect(shouldCancelBrowserDownload(null)).toBe(false);
    expect(shouldCancelBrowserDownload({ id: "1", ok: false })).toBe(false);
    expect(shouldCancelBrowserDownload({} as never)).toBe(false);
  });
});
