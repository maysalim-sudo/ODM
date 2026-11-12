import { describe, expect, it } from "vitest";
import { expiryNote, linkExpiry, listDuplicate, namesInFolder, recentFolders, recentLinks, spaceProblem } from "./newDownload";
import { toRow, type Row } from "./model";
import type { Download } from "./types";

function row(id: string, patch: Partial<Download> = {}): Row {
  const d: Download = {
    id,
    url: `https://e.com/${id}.iso`,
    filename: `${id}.iso`,
    save_dir: "C:\\dl",
    size: 1000,
    bytes: 0,
    mime: null,
    category: "general",
    state: "completed",
    error_code: null,
    error_message: null,
    max_connections: 8,
    kind: "file",
    page_url: null,
    page_title: null,
    created_at: 0,
    priority: 0,
    started_at: null,
    finished_at: null,
    pause_reason: null,
    speed_limit_kbps: null,
    audio: false,
    ...patch,
  };
  return toRow(d);
}

describe("listDuplicate", () => {
  const fresh = row("new", { url: "https://e.com/a.iso", state: "paused" });

  it("finds another download of the same address and choice", () => {
    const old = row("old", { url: "https://e.com/a.iso" });
    expect(listDuplicate(fresh, [fresh, old])?.id).toBe("old");
  });

  it("MP3 and video of the same address are different files", () => {
    const mp3 = row("mp3", { url: "https://e.com/a.iso", audio: true });
    expect(listDuplicate(fresh, [fresh, mp3])).toBeUndefined();
  });

  it("failed and cancelled downloads, torrents and the download itself don't count", () => {
    const rows = [
      fresh,
      row("f", { url: "https://e.com/a.iso", state: "failed" }),
      row("c", { url: "https://e.com/a.iso", state: "cancelled" }),
    ];
    expect(listDuplicate(fresh, rows)).toBeUndefined();
    const t = row("t", { kind: "torrent", url: "magnet:?xt=1", state: "paused" });
    expect(listDuplicate(t, [t, row("t2", { kind: "torrent", url: "magnet:?xt=1" })])).toBeUndefined();
  });
});

describe("namesInFolder", () => {
  it("lists the other downloads' names going to the same folder, without case", () => {
    const me = row("me", { save_dir: "C:\\Videos", state: "paused" });
    const rows = [
      me,
      row("a", { filename: "x.mp4", save_dir: "c:\\videos" }),
      row("b", { filename: "y.mp4", save_dir: "C:\\Other" }),
      row("c", { filename: "z.mp4", save_dir: "C:\\Videos\\", state: "cancelled" }),
    ];
    expect(namesInFolder(me, rows)).toEqual(["x.mp4"]);
  });
});

describe("recentFolders", () => {
  it("lists the latest folders first, once each, leaving out the one shown", () => {
    const rows = [
      row("a", { save_dir: "D:\\Music", created_at: 3 }),
      row("b", { save_dir: "C:\\dl", created_at: 2 }),
      row("c", { save_dir: "d:\\music\\", created_at: 1 }),
      row("d", { save_dir: "E:\\ISO", created_at: 0 }),
    ];
    expect(recentFolders(rows, "E:\\ISO")).toEqual(["D:\\Music", "C:\\dl"]);
    expect(recentFolders(rows, null, 1)).toEqual(["D:\\Music"]);
  });
});

describe("recentLinks", () => {
  it("lists the latest links once each, with a site video's page, newest first", () => {
    const rows = [
      row("a", { url: "https://e.com/a.iso", filename: "a.iso", created_at: 3 }),
      row("s", { kind: "site", url: "https://www.youtube.com/watch?v=1", page_url: "https://www.youtube.com/watch?v=1", filename: "Song.mp3", created_at: 2 }),
      row("a2", { url: "https://e.com/a.iso", filename: "a (1).iso", created_at: 1 }),
      row("t", { kind: "torrent", url: "magnet:?xt=urn:btih:abc", filename: "Ubuntu", created_at: 0 }),
    ];
    expect(recentLinks(rows)).toEqual([
      { url: "https://e.com/a.iso", name: "a.iso" },
      { url: "https://www.youtube.com/watch?v=1", name: "Song.mp3" },
      { url: "magnet:?xt=urn:btih:abc", name: "Ubuntu" },
    ]);
    expect(recentLinks(rows, 1)).toHaveLength(1);
  });
});

describe("spaceProblem", () => {
  const GB = 1024 ** 3;

  it("says nothing when the file fits or a number is unknown", () => {
    expect(spaceProblem(2 * GB, 10 * GB)).toBeNull();
    expect(spaceProblem(null, 1)).toBeNull();
    expect(spaceProblem(5, null)).toBeNull();
  });

  it("warns when it won't fit", () => {
    expect(spaceProblem(12 * GB, 10 * GB)).toBe("Not enough space: it needs 12.00 GB and the drive has 10.00 GB free.");
  });
});

describe("linkExpiry", () => {
  const at = (iso: string) => new Date(iso).getTime();

  it("reads a Unix time from expires, Expires or exp", () => {
    expect(linkExpiry("https://cdn.e.com/a.mp4?expires=1794600000&sig=x")).toBe(1794600000 * 1000);
    expect(linkExpiry("https://cdn.e.com/a.mp4?Expires=1794600000")).toBe(1794600000 * 1000);
    expect(linkExpiry("https://cdn.e.com/a.mp4?exp=1794600000")).toBe(1794600000 * 1000);
  });

  it("reads an Amazon signed link: its date plus its lifetime", () => {
    const url = "https://b.s3.amazonaws.com/a.iso?X-Amz-Date=20261112T100000Z&X-Amz-Expires=3600&X-Amz-Signature=abc";
    expect(linkExpiry(url)).toBe(at("2026-11-12T11:00:00Z"));
  });

  it("knows nothing for an ordinary link or a nonsense value", () => {
    expect(linkExpiry("https://e.com/a.iso")).toBeNull();
    expect(linkExpiry("https://e.com/a.iso?expires=soon")).toBeNull();
    expect(linkExpiry("not a link")).toBeNull();
  });
});

describe("expiryNote", () => {
  const now = new Date("2026-11-12T10:00:00").getTime();

  it("says when the link stops working, or that it already has", () => {
    expect(expiryNote(new Date("2026-11-12T18:40:00").getTime(), now)).toBe("This link stops working around 18:40. Start it before then.");
    expect(expiryNote(new Date("2026-11-14T09:05:00").getTime(), now)).toMatch(/^This link stops working around .*09:05\. Start it before then\.$/);
    expect(expiryNote(now - 1000, now)).toBe("This link has probably expired. If it fails, paste a new link from the page.");
    expect(expiryNote(null, now)).toBeNull();
  });
});
