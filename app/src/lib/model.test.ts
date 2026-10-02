import { describe, expect, it } from "vitest";
import {
  applyEvent,
  canCancel,
  canPause,
  canResume,
  counts,
  matches,
  parseUrls,
  replaceAll,
  segments,
  visible,
} from "./model";
import type { Download } from "./types";

function dl(id: string, patch: Partial<Download> = {}): Download {
  return {
    id,
    url: `https://e.com/${id}`,
    filename: `${id}.bin`,
    save_dir: "C:/dl",
    size: 1000,
    bytes: 0,
    mime: null,
    category: "general",
    state: "queued",
    error_code: null,
    error_message: null,
    max_connections: 8,
    kind: "file",
    page_url: null,
    page_title: null,
    created_at: 1,
    started_at: null,
    finished_at: null,
    ...patch,
  };
}

describe("applyEvent", () => {
  it("upsert adds and replaces rows", () => {
    let rows = replaceAll([]);
    rows = applyEvent(rows, { type: "upsert", download: dl("a") }).rows;
    rows = applyEvent(rows, { type: "upsert", download: dl("a", { state: "downloading" }) }).rows;
    expect(rows.size).toBe(1);
    expect(rows.get("a")!.state).toBe("downloading");
  });

  it("progress updates bytes, speed, eta and chunks of known rows only", () => {
    let rows = replaceAll([dl("a", { state: "downloading", size: null })]);
    const chunk = { idx: 0, start: 0, end: 999, written: 400, state: "active" as const };
    rows = applyEvent(rows, {
      type: "progress_batch",
      items: [
        { id: "a", bytes: 400, total: 1000, speed_bps: 50, eta_s: 12, chunks: [chunk] },
        { id: "ghost", bytes: 1, total: 1, speed_bps: 1, eta_s: 1, chunks: [] },
      ],
    }).rows;
    const a = rows.get("a")!;
    expect([a.bytes, a.size, a.speed_bps, a.eta_s]).toEqual([400, 1000, 50, 12]);
    expect(a.chunks).toEqual([chunk]);
    expect(rows.has("ghost")).toBe(false);
  });

  it("a row that stops being active loses its live speed and chunks", () => {
    let rows = replaceAll([dl("a", { state: "downloading" })]);
    rows = applyEvent(rows, {
      type: "progress_batch",
      items: [{ id: "a", bytes: 10, total: 1000, speed_bps: 99, eta_s: 5, chunks: [] }],
    }).rows;
    rows = applyEvent(rows, { type: "upsert", download: dl("a", { state: "paused", bytes: 10 }) }).rows;
    const a = rows.get("a")!;
    expect([a.speed_bps, a.eta_s, a.chunks]).toEqual([0, null, null]);
  });

  it("removed deletes the row", () => {
    const rows = applyEvent(replaceAll([dl("a"), dl("b")]), { type: "removed", id: "a" }).rows;
    expect([...rows.keys()]).toEqual(["b"]);
  });

  it("resync asks for a reload and leaves rows alone", () => {
    const before = replaceAll([dl("a")]);
    const r = applyEvent(before, { type: "resync" });
    expect(r.resync).toBe(true);
    expect(r.rows).toBe(before);
  });

  it("resync_replaces_the_list", () => {
    // What the store does after a resync: the fresh list wins completely.
    const stale = replaceAll([dl("a", { state: "downloading" }), dl("gone")]);
    const fresh = replaceAll([dl("a", { state: "completed", bytes: 1000 }), dl("new")]);
    expect(stale.has("gone")).toBe(true);
    expect([...fresh.keys()].sort()).toEqual(["a", "new"]);
    expect(fresh.get("a")!.state).toBe("completed");
  });
});

describe("filters and counts", () => {
  const rows = replaceAll([
    dl("a", { state: "downloading", category: "video" }),
    dl("b", { state: "paused" }),
    dl("c", { state: "completed", category: "video" }),
    dl("d", { state: "failed" }),
    dl("e", { state: "probing" }),
  ]);

  it("matches by status group and category", () => {
    const ids = (f: Parameters<typeof matches>[1]) => [...rows.values()].filter((r) => matches(r, f)).map((r) => r.id);
    expect(ids("all")).toHaveLength(5);
    expect(ids("active")).toEqual(["a", "e"]);
    expect(ids("queued")).toEqual(["b"]);
    expect(ids("completed")).toEqual(["c"]);
    expect(ids("failed")).toEqual(["d"]);
    expect(ids("cat:video")).toEqual(["a", "c"]);
  });

  it("counts every filter", () => {
    const c = counts(rows);
    expect([c.all, c.active, c.queued, c.completed, c.failed]).toEqual([5, 2, 1, 1, 1]);
    expect(c["cat:video"]).toBe(2);
    expect(c["cat:audio"]).toBe(0);
  });

  it("sorts newest first by default and by name on request", () => {
    const r = replaceAll([dl("b", { created_at: 1 }), dl("a", { created_at: 2 })]);
    expect(visible(r, "all", "created_at", "desc").map((x) => x.id)).toEqual(["a", "b"]);
    expect(visible(r, "all", "filename", "asc").map((x) => x.id)).toEqual(["a", "b"]);
    expect(visible(r, "all", "filename", "desc").map((x) => x.id)).toEqual(["b", "a"]);
  });
});

describe("actions allowed per state", () => {
  it("follows the state machine", () => {
    const row = (state: Download["state"]) => replaceAll([dl("x", { state })]).get("x")!;
    expect(canPause(row("downloading")) && canPause(row("queued"))).toBe(true);
    expect(canPause(row("paused")) || canPause(row("completed"))).toBe(false);
    expect(canResume(row("paused")) && canResume(row("failed"))).toBe(true);
    expect(canResume(row("downloading"))).toBe(false);
    expect(canCancel(row("paused")) && canCancel(row("downloading"))).toBe(true);
    expect(canCancel(row("completed")) || canCancel(row("cancelled"))).toBe(false);
  });
});

describe("segments", () => {
  it("sizes each chunk by its share of the file", () => {
    const s = segments(
      [
        { idx: 0, start: 0, end: 249, written: 250, state: "done" },
        { idx: 1, start: 250, end: 999, written: 375, state: "active" },
      ],
      1000,
    )!;
    expect(s.map((x) => x.widthPct)).toEqual([25, 75]);
    expect(s.map((x) => x.fillPct)).toEqual([100, 50]);
  });

  it("is null when the size is unknown", () => {
    expect(segments([{ idx: 0, start: 0, end: null, written: 5, state: "active" }], null)).toBeNull();
    expect(segments([], 1000)).toBeNull();
  });
});

describe("parseUrls", () => {
  it("keeps http(s) lines once and reports the rest", () => {
    const p = parseUrls("https://a.com/1\n\n  http://b.com/2 \nftp://c\nhttps://a.com/1\nnope\nhttps://\n");
    expect(p.valid).toEqual(["https://a.com/1", "http://b.com/2"]);
    expect(p.rejected).toEqual(["ftp://c", "nope", "https://"]);
  });
});
