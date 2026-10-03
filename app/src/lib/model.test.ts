import { describe, expect, it } from "vitest";
import {
  CATEGORIES,
  CATEGORY_LABELS,
  ReloadGate,
  coerceSettings,
  nextFilename,
  shouldCloseOnBackdrop,
  toInt,
  applyEvent,
  canCancel,
  canPause,
  canResume,
  canRefresh,
  isRiskyFile,
  addOffer,
  counts,
  COL,
  NAME_MIN,
  keepVisible,
  matches,
  mergeList,
  parseUrls,
  progressPct,
  replaceAll,
  segments,
  toRow,
  visible,
  visibleColumns,
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

describe("keepVisible", () => {
  const rows = [toRow(dl("a")), toRow(dl("b"))];

  it("returns the same set when nothing is hidden", () => {
    const sel = new Set(["a", "b"]);
    expect(keepVisible(sel, rows)).toBe(sel);
    const empty = new Set<string>();
    expect(keepVisible(empty, rows)).toBe(empty);
  });

  it("drops ids that are not visible", () => {
    const sel = new Set(["a", "gone"]);
    const out = keepVisible(sel, rows);
    expect(out).not.toBe(sel);
    expect([...out]).toEqual(["a"]);
  });
});

describe("visibleColumns", () => {
  const nameWidth = (w: number) => {
    const c = visibleColumns(w);
    return w - COL.size - COL.progress - COL.status - (c.speed ? COL.speed : 0) - (c.eta ? COL.eta : 0) - (c.added ? COL.added : 0);
  };

  it("keeps the name column at least NAME_MIN wide at every width from the table minimum up", () => {
    for (let w = 616; w <= 1400; w++) expect(nameWidth(w), `width ${w}`).toBeGreaterThanOrEqual(NAME_MIN);
  });

  it("switches columns on at exactly 712, 808 and 940", () => {
    expect(visibleColumns(711)).toEqual({ added: false, eta: false, speed: false });
    expect(visibleColumns(712)).toEqual({ added: false, eta: false, speed: true });
    expect(visibleColumns(807)).toEqual({ added: false, eta: false, speed: true });
    expect(visibleColumns(808)).toEqual({ added: false, eta: true, speed: true });
    expect(visibleColumns(939)).toEqual({ added: false, eta: true, speed: true });
    expect(visibleColumns(940)).toEqual({ added: true, eta: true, speed: true });
  });
});

describe("toInt", () => {
  it("keeps whole numbers and rounds fractions", () => {
    expect(toInt(7, 3)).toBe(7);
    expect(toInt(7.6, 3)).toBe(8);
  });
  it("falls back for null, NaN, undefined, strings and infinity", () => {
    expect(toInt(null, 3)).toBe(3);
    expect(toInt(Number.NaN, 3)).toBe(3);
    expect(toInt(undefined, 3)).toBe(3);
    expect(toInt("12" as unknown, 3)).toBe(3);
    expect(toInt(Number.POSITIVE_INFINITY, 3)).toBe(3);
  });
});

describe("coerceSettings", () => {
  const base = {
    download_dir: "C:\\dl",
    category_folders: { video: "Video" },
    max_concurrent: 3,
    max_connections: 8,
    max_attempts: 5,
    speed_limit_kbps: 0,
    connect_timeout_s: 20,
    read_timeout_s: 60,
    ffmpeg_path: "",
    capture_enabled: true,
    min_capture_bytes: 1048576,
    skip_hosts: [],
    close_to_tray: true,
    start_minimized: false,
    notifications: true,
  };
  it("replaces emptied number fields with the previously loaded value", () => {
    const form = { ...base, max_connections: null, speed_limit_kbps: Number.NaN, max_concurrent: 4.4 };
    const out = coerceSettings(form as unknown as typeof base, { ...base, speed_limit_kbps: 500 });
    expect(out.max_connections).toBe(8);
    expect(out.speed_limit_kbps).toBe(500);
    expect(out.max_concurrent).toBe(4);
    expect(out.download_dir).toBe("C:\\dl");
  });
});

describe("shouldCloseOnBackdrop", () => {
  it("closes only when the press and the release were both on the backdrop", () => {
    expect(shouldCloseOnBackdrop(true, true)).toBe(true);
    expect(shouldCloseOnBackdrop(false, true)).toBe(false);
    expect(shouldCloseOnBackdrop(true, false)).toBe(false);
    expect(shouldCloseOnBackdrop(false, false)).toBe(false);
  });
});

describe("nextFilename", () => {
  it("fills an empty field", () => {
    expect(nextFilename("", null, "a.zip")).toBe("a.zip");
  });
  it("replaces an earlier auto-filled name", () => {
    expect(nextFilename("a.zip", "a.zip", "b.zip")).toBe("b.zip");
  });
  it("never overwrites a name the user typed", () => {
    expect(nextFilename("mine.zip", "a.zip", "b.zip")).toBe("mine.zip");
    expect(nextFilename("mine.zip", null, "b.zip")).toBe("mine.zip");
  });
});

describe("CATEGORY_LABELS", () => {
  it("names every category in plain words", () => {
    expect(CATEGORIES.map((c) => CATEGORY_LABELS[c])).toEqual([
      "Video",
      "Audio",
      "Documents",
      "Compressed",
      "Images",
      "Programs",
      "Other",
    ]);
  });
});

describe("isRiskyFile", () => {
  it("flags programs by extension, case-insensitively", () => {
    expect(isRiskyFile("setup.EXE")).toBe(true);
    expect(isRiskyFile("script.ps1")).toBe(true);
    expect(isRiskyFile(".exe")).toBe(true);
    expect(isRiskyFile("a.tar.gz")).toBe(false);
    expect(isRiskyFile("noext")).toBe(false);
    expect(isRiskyFile("movie.mp4")).toBe(false);
  });
});

describe("canRefresh", () => {
  it("is true only for paused and failed rows", () => {
    expect(canRefresh(toRow(dl("a", { state: "paused" })))).toBe(true);
    expect(canRefresh(toRow(dl("a", { state: "failed" })))).toBe(true);
    expect(canRefresh(toRow(dl("a", { state: "downloading" })))).toBe(false);
    expect(canRefresh(toRow(dl("a", { state: "completed" })))).toBe(false);
  });
});

describe("offers", () => {
  it("addOffer ignores a duplicate offer_id", () => {
    const o = { offer_id: "o1", existing_id: "e", filename: "f.zip" };
    const once = addOffer([], o);
    expect(once).toEqual([o]);
    expect(addOffer(once, { ...o, filename: "other.zip" })).toBe(once);
  });

  it("applyEvent leaves rows alone for reuse_offer", () => {
    const rows = replaceAll([dl("a")]);
    const r = applyEvent(rows, { type: "reuse_offer", offer: { offer_id: "o", existing_id: "a", filename: "f" } });
    expect(r.rows).toBe(rows);
    expect(r.resync).toBe(false);
  });
});

describe("progressPct", () => {
  const row = (patch: Partial<Download>, segments: { done: number; total: number } | null = null) => ({
    ...toRow(dl("p", patch)),
    segments,
  });

  it("uses segments when the progress has them", () => {
    expect(progressPct(row({ size: 5, bytes: 1 }, { done: 3, total: 12 }))).toBe(25);
  });

  it("falls back to bytes over size", () => {
    expect(progressPct(row({ size: 200, bytes: 50 }))).toBe(25);
  });

  it("is 0 without a size and never passes 100", () => {
    expect(progressPct(row({ size: null, bytes: 50 }))).toBe(0);
    expect(progressPct(row({ size: 100, bytes: 500 }))).toBe(100);
    expect(progressPct(row({}, { done: 13, total: 12 }))).toBe(100);
    expect(progressPct(row({}, { done: 0, total: 0 }))).toBe(0);
  });

  it("progress events carry segments onto the row", () => {
    let rows = replaceAll([dl("a", { state: "downloading", kind: "hls", size: null })]);
    rows = applyEvent(rows, {
      type: "progress_batch",
      items: [
        {
          id: "a",
          bytes: 300,
          total: 1200,
          speed_bps: 1,
          eta_s: 1,
          chunks: [],
          segments: { done: 3, total: 12 },
        },
      ],
    }).rows;
    expect(rows.get("a")!.segments).toEqual({ done: 3, total: 12 });
    expect(progressPct(rows.get("a")!)).toBe(25);
  });
});
