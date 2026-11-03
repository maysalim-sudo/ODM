import { describe, expect, it } from "vitest";
import { PANEL_MIN, chartPoints, clampHeight, combined, panelSubject, recordSpeeds, sumSeries } from "./details";
import { toRow, type Row } from "./model";
import type { Download } from "./types";

function row(id: string, patch: Partial<Row> = {}): Row {
  const d: Download = {
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
    created_at: 0,
    priority: 0,
    started_at: null,
    finished_at: null,
    pause_reason: null,
    speed_limit_kbps: null,
  };
  return { ...toRow(d), ...patch };
}

const rows = (...list: Row[]) => new Map(list.map((r) => [r.id, r]));

describe("panelSubject", () => {
  it("is nothing, one download, or several", () => {
    const all = rows(row("a"), row("b"));
    expect(panelSubject(new Set(), all)).toEqual({ kind: "none" });
    expect(panelSubject(new Set(["a"]), all)).toMatchObject({ kind: "one", row: { id: "a" } });
    const many = panelSubject(new Set(["a", "b"]), all);
    expect(many.kind).toBe("many");
    expect(many.kind === "many" && many.rows.map((r) => r.id)).toEqual(["a", "b"]);
  });

  it("skips selected ids whose row has gone", () => {
    expect(panelSubject(new Set(["gone", "a"]), rows(row("a")))).toMatchObject({ kind: "one", row: { id: "a" } });
    expect(panelSubject(new Set(["gone"]), rows(row("a")))).toEqual({ kind: "none" });
  });
});

describe("combined", () => {
  it("adds up sizes, progress and speed, keeping unknown sizes apart", () => {
    const c = combined([
      row("a", { state: "downloading", size: 1000, bytes: 400, speed_bps: 50 }),
      row("b", { state: "completed", size: 2000, bytes: 2000 }),
      row("c", { state: "downloading", size: null, bytes: 300, speed_bps: 25 }),
    ]);
    expect(c).toEqual({ count: 3, active: 2, size: 3000, unknown: 1, bytes: 2700, speed_bps: 75 });
  });

  it("counts speed only for running downloads", () => {
    expect(combined([row("a", { state: "paused", speed_bps: 99 })]).speed_bps).toBe(0);
  });
});

describe("recordSpeeds", () => {
  it("adds one sample per running download and caps each history", () => {
    let h = new Map<string, number[]>();
    h = recordSpeeds(h, [row("a", { state: "downloading", speed_bps: 10 }), row("b")], 3);
    h = recordSpeeds(h, [row("a", { state: "downloading", speed_bps: 20 })], 3);
    h = recordSpeeds(h, [row("a", { state: "downloading", speed_bps: 30 })], 3);
    h = recordSpeeds(h, [row("a", { state: "downloading", speed_bps: 40 })], 3);
    expect(h.get("a")).toEqual([20, 30, 40]);
    expect(h.has("b")).toBe(false);
  });

  it("records a zero when a download stops, then nothing more", () => {
    let h = new Map([["a", [10]]]);
    h = recordSpeeds(h, [row("a", { state: "paused" })], 5);
    h = recordSpeeds(h, [row("a", { state: "paused" })], 5);
    expect(h.get("a")).toEqual([10, 0]);
  });

  it("forgets downloads that left the list", () => {
    const h = recordSpeeds(new Map([["gone", [5]]]), [], 5);
    expect(h.has("gone")).toBe(false);
  });
});

describe("sumSeries", () => {
  it("adds histories lined up at their newest sample", () => {
    expect(sumSeries([[1, 2, 3], [10, 20]])).toEqual([1, 12, 23]);
    expect(sumSeries([])).toEqual([]);
  });
});

describe("chartPoints", () => {
  it("spreads samples across the width, newest on the right, scaled to the peak", () => {
    expect(chartPoints([0, 50, 100], 200, 100, 3)).toBe("0,100 100,50 200,0");
  });

  it("pads a short history on the left and stays flat with no speed", () => {
    expect(chartPoints([0, 0], 100, 40, 3)).toBe("50,40 100,40");
    expect(chartPoints([], 100, 40, 3)).toBe("");
  });
});

describe("clampHeight", () => {
  it("keeps the panel usable and leaves room for the list", () => {
    expect(clampHeight(10, 800)).toBe(PANEL_MIN);
    expect(clampHeight(300, 800)).toBe(300);
    expect(clampHeight(780, 800)).toBe(800 - 160);
    expect(clampHeight(Number.NaN, 800)).toBe(PANEL_MIN);
  });
});
