import { describe, expect, it } from "vitest";
import {
  ReloadGate,
  applyEvent,
  canCancel,
  canPause,
  canResume,
  counts,
  matches,
  mergeList,
  parseUrls,
  replaceAll,
  segments,
  toRow,
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
    // What the store does after a resync: the fresh list wins, live values survive.
    const stale = replaceAll([dl("a", { state: "downloading" }), dl("gone")]);
    const live = applyEvent(stale, {
      type: "progress_batch",
      items: [{ id: "a", bytes: 500, total: 1000, speed_bps: 7, eta_s: 3, chunks: [] }],
    }).rows;
    const fresh = mergeList(live, [dl("a", { state: "downloading", bytes: 400 }), dl("new")]);
    expect(live.has("gone")).toBe(true);
    expect([...fresh.keys()].sort()).toEqual(["a", "new"]);
    expect([fresh.get("a")!.bytes, fresh.get("a")!.speed_bps]).toEqual([500, 7]);
  });

  it("ignores progress for rows that are not active", () => {
    const rows = replaceAll([dl("c", { state: "completed", bytes: 1000 }), dl("p", { state: "paused", bytes: 10 })]);
    const r = applyEvent(rows, {
      type: "progress_batch",
      items: [
        { id: "c", bytes: 600, total: 1000, speed_bps: 5e6, eta_s: 1, chunks: [] },
        { id: "p", bytes: 99, total: 1000, speed_bps: 5, eta_s: 1, chunks: [] },
      ],
    });
    expect(r.rows).toBe(rows);
    expect(rows.get("c")!.bytes).toBe(1000);
    expect(rows.get("p")!.speed_bps).toBe(0);
  });
});

describe("mergeList", () => {
  it("keeps live values for running rows and drops them for rows that stopped", () => {
    const prev = applyEvent(replaceAll([dl("a", { state: "downloading" }), dl("b", { state: "downloading" })]), {
      type: "progress_batch",
      items: [
        { id: "a", bytes: 50, total: 1000, speed_bps: 9, eta_s: 4, chunks: [] },
        { id: "b", bytes: 60, total: 1000, speed_bps: 8, eta_s: 5, chunks: [] },
      ],
    }).rows;
    const m = mergeList(prev, [dl("a", { state: "downloading" }), dl("b", { state: "paused" }), dl("c")]);
    expect([m.get("a")!.speed_bps, m.get("a")!.eta_s, m.get("a")!.chunks]).toEqual([9, 4, []]);
    expect([m.get("b")!.speed_bps, m.get("b")!.eta_s, m.get("b")!.chunks]).toEqual([0, null, null]);
    expect(m.has("c")).toBe(true);
  });
});

describe("toRow", () => {
  it("does not carry live values from a row that was not active", () => {
    const prev = toRow(dl("a", { state: "failed", bytes: 700 }));
    const retried = toRow(dl("a", { state: "downloading", bytes: 0 }), prev);
    expect(retried.bytes).toBe(0);
  });
});

describe("ReloadGate", () => {
  it("drops the response of a superseded reload", () => {
    const gate = new ReloadGate();
    const t1 = gate.begin();
    const t2 = gate.begin();
    const prev = replaceAll([]);
    expect(gate.finish(t1, prev, [dl("old")])).toBeNull();
    expect([...gate.finish(t2, prev, [dl("new")])!.keys()]).toEqual(["new"]);
  });

  it("replays events recorded while the reload was in flight", () => {
    const gate = new ReloadGate();
    const prev = replaceAll([dl("a"), dl("b")]);
    const t = gate.begin();
    gate.record({ type: "upsert", download: dl("a", { state: "completed", bytes: 1000 }) });
    gate.record({ type: "removed", id: "b" });
    const out = gate.finish(t, prev, [dl("a", { state: "downloading" }), dl("b")])!;
    expect(out.get("a")!.state).toBe("completed");
    expect(out.has("b")).toBe(false);
  });

  it("does not buffer events when no reload is in flight", () => {
    const gate = new ReloadGate();
    gate.record({ type: "removed", id: "b" });
    const t = gate.begin();
    const out = gate.finish(t, replaceAll([]), [dl("b")])!;
    expect(out.has("b")).toBe(true);
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
