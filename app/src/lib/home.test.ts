import { describe, expect, it } from "vitest";
import { groupRows, lineLabel, queueOrder, remaining, remainingLabel, rowActions, rowSummary } from "./home";
import { formatDate } from "./format";
import { toRow, type Row } from "./model";
import type { Download } from "./types";

const GB = 1024 ** 3;

function row(id: string, patch: Partial<Download> = {}, live: Partial<Row> = {}): Row {
  const d: Download = {
    id,
    url: "https://e.com/a",
    filename: `${id}.iso`,
    save_dir: "D:/Downloads",
    size: 4 * GB,
    bytes: 0,
    mime: null,
    category: "general",
    state: "queued",
    error_code: null,
    error_message: null,
    max_connections: 4,
    kind: "http",
    page_url: null,
    page_title: null,
    created_at: 1,
    priority: 0,
    started_at: null,
    finished_at: null,
    pause_reason: null,
    speed_limit_kbps: null,
    ...patch,
  };
  return { ...toRow(d), ...live };
}

const keys = (rows: Row[], waiting: string[] = []) => groupRows(rows, waiting).map((g) => [g.key, g.rows.map((r) => r.id)]);

describe("groupRows", () => {
  it("orders the groups and hides empty ones", () => {
    const rows = [
      row("done", { state: "completed" }),
      row("next", { state: "queued" }),
      row("bad", { state: "failed" }),
      row("run", { state: "downloading" }),
    ];
    expect(keys(rows)).toEqual([["needs", ["bad"]], ["active", ["run"]], ["next", ["next"]], ["done", ["done"]]]);
    expect(keys([row("run", { state: "merging" })])).toEqual([["active", ["run"]]]);
    expect(keys([])).toEqual([]);
  });

  it("puts a torrent waiting for its files under Needs you", () => {
    const t = row("t", { state: "paused", kind: "torrent", has_meta: true });
    expect(keys([t], ["t"])).toEqual([["needs", ["t"]]]);
    expect(keys([t])).toEqual([["next", ["t"]]]);
  });

  it("leaves a plain download waiting for its dialog in Up next", () => {
    expect(keys([row("p", { state: "paused" })], ["p"])).toEqual([["next", ["p"]]]);
  });

  it("files cancelled downloads under Finished", () => {
    expect(keys([row("c", { state: "cancelled" })])).toEqual([["done", ["c"]]]);
  });

  it("keeps the list's sort inside a group, and the queue's order in Up next", () => {
    const rows = [
      row("d2", { state: "completed", created_at: 2 }),
      row("d1", { state: "completed", created_at: 1 }),
      row("low", { state: "queued", priority: 1 }),
      row("high", { state: "queued", priority: 2 }),
    ];
    expect(keys(rows)).toEqual([["next", ["high", "low"]], ["done", ["d2", "d1"]]]);
  });
});

describe("queueOrder", () => {
  it("follows the engine: priority first, then oldest", () => {
    const rows = [
      row("new", { created_at: 5 }),
      row("old", { created_at: 1 }),
      row("top", { created_at: 9, priority: 3, state: "paused" }),
    ];
    expect(queueOrder(rows).map((r) => r.id)).toEqual(["top", "old", "new"]);
  });

  it("holds only waiting rows", () => {
    expect(queueOrder([row("a", { state: "downloading" }), row("b", { state: "failed" })])).toEqual([]);
  });
});

describe("lineLabel", () => {
  it.each([
    [0, "1st in line"],
    [1, "2nd in line"],
    [2, "3rd in line"],
    [3, "4th in line"],
    [10, "11th in line"],
    [11, "12th in line"],
    [12, "13th in line"],
    [20, "21st in line"],
    [21, "22nd in line"],
  ])("%d is %s", (i, label) => expect(lineLabel(i)).toBe(label));
});

describe("remaining", () => {
  it("sums what is left of running and waiting rows", () => {
    const rows = [
      row("a", { state: "downloading", size: 4 * GB, bytes: GB }),
      row("b", { state: "queued", size: GB, bytes: 0 }),
      row("c", { state: "paused", size: 2 * GB, bytes: 2 * GB }),
    ];
    expect(remaining(rows)).toEqual({ bytes: 4 * GB, unknown: 0 });
  });

  it("skips failed, finished and cancelled rows", () => {
    const rows = [row("a", { state: "failed" }), row("b", { state: "completed" }), row("c", { state: "cancelled" })];
    expect(remaining(rows)).toEqual({ bytes: 0, unknown: 0 });
  });

  it("counts rows with no size", () => {
    expect(remaining([row("a", { size: null }), row("b", { state: "downloading", size: null, bytes: 50 })])).toEqual({ bytes: 0, unknown: 2 });
  });

  it("never goes below zero when a row has more bytes than its size", () => {
    expect(remaining([row("a", { state: "downloading", size: 10, bytes: 20 })])).toEqual({ bytes: 0, unknown: 0 });
  });
});

describe("remainingLabel", () => {
  const b = 3.7 * GB;
  it("says what is left", () => expect(remainingLabel({ bytes: b, unknown: 0 })).toBe("3.7 GB left"));
  it("adds unknown sizes", () => expect(remainingLabel({ bytes: b, unknown: 2 })).toBe("3.7 GB + 2 unknown left"));
  it("never says 0 B when only unknown sizes remain", () => expect(remainingLabel({ bytes: 0, unknown: 2 })).toBe("2 unknown left"));
  it("hides when nothing is left", () => expect(remainingLabel({ bytes: 0, unknown: 0 })).toBeNull());
  it("uses smaller units below a gigabyte", () => expect(remainingLabel({ bytes: 412 * 1024 ** 2, unknown: 0 })).toBe("412.0 MB left"));
});

describe("rowSummary", () => {
  it("shows size and time left while downloading", () => {
    const r = row("a", { state: "downloading", size: 5.9 * GB, bytes: 3.1 * GB }, { eta_s: 360 });
    expect(rowSummary(r, false, null)).toBe("3.1 GB of 5.9 GB · 6 min left");
  });

  it("drops what it doesn't know", () => {
    expect(rowSummary(row("a", { state: "downloading", size: 5.9 * GB, bytes: 3.1 * GB }), false, null)).toBe("3.1 GB of 5.9 GB");
    expect(rowSummary(row("a", { state: "downloading", size: null, bytes: 3.1 * GB }), false, null)).toBe("3.1 GB");
  });

  it("uses the status while nothing is transferring", () => {
    expect(rowSummary(row("a", { state: "probing" }), false, null)).toBe("Starting");
    expect(rowSummary(row("a", { state: "merging" }), false, null)).toBe("Merging");
  });

  it("says how far a paused download got", () => {
    expect(rowSummary(row("a", { state: "paused", size: 100, bytes: 38 }), false, null)).toBe("Paused at 38%");
    expect(rowSummary(row("a", { state: "paused", size: null, bytes: 38 }), false, null)).toBe("Paused");
  });

  it("gives the reason when ODM paused it", () => {
    const r = row("a", { state: "paused", size: 100, bytes: 38, pause_reason: "schedule" });
    expect(rowSummary(r, false, null)).toBe("Paused by schedule");
  });

  it("includes the queue schedule's wait", () => {
    expect(rowSummary(row("a"), false, "Waiting for the schedule (starts 01:00)")).toBe("Waiting for the schedule (starts 01:00)");
    expect(rowSummary(row("a"), false, null)).toBe("Queued");
  });

  it("says what a torrent waits for", () => {
    const t = row("t", { state: "paused", kind: "torrent", has_meta: true });
    expect(rowSummary(t, true, null)).toBe("Waiting for you");
  });

  it("shows size and finish time when finished", () => {
    const at = new Date(2026, 9, 5, 14, 30).getTime();
    const s = rowSummary(row("a", { state: "completed", size: GB * 2, finished_at: at }), false, null);
    expect(s).toBe(`2.0 GB · ${formatDate(at)}`);
  });

  it("says cancelled", () => expect(rowSummary(row("a", { state: "cancelled" }), false, null)).toBe("Cancelled"));
});

describe("rowActions", () => {
  const primary = (r: Row, closed = false, waiting = false) => rowActions(r, closed, waiting).primary;
  const menu = (r: Row, closed = false) => rowActions(r, closed).menu;

  it.each(["downloading", "probing", "merging"] as const)("%s can be paused", (state) => {
    expect(primary(row("a", { state }))).toBe("pause");
  });

  it("paused resumes", () => expect(primary(row("a", { state: "paused" }))).toBe("resume"));

  it("queued pauses while the queue may run, and starts now while it is closed", () => {
    expect(primary(row("a", { state: "queued" }))).toBe("pause");
    expect(primary(row("a", { state: "queued" }), true)).toBe("start-now");
  });

  it("start now never replaces resume on a paused row", () => {
    expect(primary(row("a", { state: "paused" }), true)).toBe("resume");
  });

  it("a torrent waiting for its files asks for them", () => {
    expect(primary(row("t", { state: "paused", kind: "torrent" }), false, true)).toBe("choose-files");
  });

  it("failed offers its fix", () => expect(primary(row("a", { state: "failed" }))).toBe("fix"));

  it("completed opens, with the folder beside it", () => {
    const a = rowActions(row("a", { state: "completed" }), false);
    expect([a.primary, a.secondary]).toEqual(["open", "open-folder"]);
  });

  it("cancelled can be removed", () => expect(primary(row("a", { state: "cancelled" }))).toBe("remove"));

  it("only completed rows have a second button", () => {
    expect(rowActions(row("a", { state: "downloading" }), false).secondary).toBeNull();
  });

  it("hides what doesn't apply from the menu", () => {
    expect(menu(row("a", { state: "downloading" }))).toEqual(["details", "open-folder", "copy-link", "cancel", "remove"]);
    expect(menu(row("a", { state: "paused" }))).toEqual(["details", "open-folder", "copy-link", "new-link", "cancel", "remove"]);
    expect(menu(row("a", { state: "failed" }))).toEqual(["details", "open-folder", "copy-link", "new-link", "remove"]);
    expect(menu(row("a", { state: "completed" }))).toEqual(["details", "open-folder", "copy-link", "remove"]);
    expect(menu(row("a", { state: "cancelled" }))).toEqual(["details", "open-folder", "copy-link", "remove"]);
  });
});
