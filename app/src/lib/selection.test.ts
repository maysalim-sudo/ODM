import { describe, expect, it } from "vitest";
import { BAR_MENUS, LOW_KBPS, SEL_LABELS, againArgs, contextMenu, moveManyInQueue, targets, type SelAction } from "./selection";
import { toRow, type Row } from "./model";
import type { Download } from "./types";

function row(id: string, patch: Partial<Row> = {}): Row {
  const d: Download = {
    id,
    url: `https://e.com/${id}.iso`,
    filename: `${id}.iso`,
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

const ids = (rows: Row[]) => rows.map((r) => r.id);

describe("moveManyInQueue", () => {
  const q = ["a", "b", "c", "d", "e"];

  it("moves the selection to the top or bottom, keeping its order", () => {
    expect(moveManyInQueue(q, ["d", "b"], "top")).toEqual(["b", "d", "a", "c", "e"]);
    expect(moveManyInQueue(q, ["a", "c"], "bottom")).toEqual(["b", "d", "e", "a", "c"]);
  });

  it("moves each selected one a place up or down without passing another selected one", () => {
    expect(moveManyInQueue(q, ["b", "d"], "up")).toEqual(["b", "a", "d", "c", "e"]);
    expect(moveManyInQueue(q, ["a", "b"], "up")).toEqual(q);
    expect(moveManyInQueue(q, ["b", "d"], "down")).toEqual(["a", "c", "b", "e", "d"]);
    expect(moveManyInQueue(q, ["d", "e"], "down")).toEqual(q);
  });

  it("ignores ids that aren't waiting in the queue", () => {
    expect(moveManyInQueue(q, ["zz"], "top")).toEqual(q);
  });
});

describe("targets", () => {
  const running = row("run", { state: "downloading" });
  const waiting = row("wait", { state: "queued" });
  const paused = row("pause", { state: "paused" });
  const failed = row("fail", { state: "failed" });
  const done = row("done", { state: "completed" });
  const all = [running, waiting, paused, failed, done];

  it("each action picks the downloads it applies to", () => {
    expect(ids(targets(all, "pause"))).toEqual(["run", "wait"]);
    expect(ids(targets(all, "resume"))).toEqual(["pause", "fail"]);
    // A failed download has already stopped.
    expect(ids(targets(all, "stop"))).toEqual(["run", "wait", "pause"]);
    expect(ids(targets(all, "retry"))).toEqual(["fail"]);
    expect(ids(targets(all, "move-top"))).toEqual(["wait", "pause"]);
    expect(ids(targets(all, "low"))).toEqual(["run", "wait", "pause", "fail"]);
    expect(ids(targets(all, "again"))).toEqual(["fail", "done"]);
    expect(ids(targets(all, "remove"))).toEqual(ids(all));
  });

  it("Back to normal applies only to slowed downloads", () => {
    const slow = row("slow", { state: "queued", speed_limit_kbps: LOW_KBPS });
    expect(ids(targets([waiting, slow], "normal"))).toEqual(["slow"]);
  });

  it("one-download actions need exactly one", () => {
    expect(ids(targets([done], "open"))).toEqual(["done"]);
    expect(targets([done, row("d2", { state: "completed" })], "open")).toEqual([]);
    expect(targets([failed, paused], "new-link")).toEqual([]);
    expect(ids(targets([failed], "new-link"))).toEqual(["fail"]);
  });

  it("Start now is for queued downloads only while the queue is closed", () => {
    expect(targets([waiting], "start-now")).toEqual([]);
    expect(ids(targets([waiting], "start-now", true))).toEqual(["wait"]);
    expect(ids(targets([waiting], "resume", true))).toEqual(["wait"]);
  });

  it("torrents can't be downloaded again from a link", () => {
    expect(targets([row("t", { state: "completed", kind: "torrent" })], "again")).toEqual([]);
  });
});

describe("contextMenu", () => {
  const actions = (rows: Row[]) => contextMenu(rows).filter((e): e is SelAction => e !== "sep");

  it("offers what applies, in groups, with no separator at either end or twice", () => {
    const menu = contextMenu([row("done", { state: "completed" })]);
    expect(menu[0]).not.toBe("sep");
    expect(menu.at(-1)).not.toBe("sep");
    expect(menu.some((e, i) => e === "sep" && menu[i - 1] === "sep")).toBe(false);
    expect(actions([row("done", { state: "completed" })])).toEqual([
      "open",
      "open-folder",
      "copy-link",
      "copy-name",
      "copy-path",
      "again",
      "select-all",
      "remove",
      "delete",
    ]);
  });

  it("for a waiting download it offers the queue, low and speed choices", () => {
    const got = actions([row("w", { state: "paused" })]);
    for (const a of ["resume", "move-top", "move-bottom", "low", "speed-unlimited", "speed-low", "speed-custom", "new-link", "stop"] as const) {
      expect(got, a).toContain(a);
    }
    expect(got).not.toContain("open");
  });

  it("every entry has a label", () => {
    for (const a of actions([row("w"), row("d", { state: "completed" }), row("f", { state: "failed" })])) expect(SEL_LABELS[a], a).toBeTruthy();
  });
});

describe("the bar's menus", () => {
  it("group the actions by category", () => {
    expect(BAR_MENUS.map((m) => m.label)).toEqual(["Queue", "Low", "Speed", "More"]);
    expect(BAR_MENUS[0].items).toEqual(["move-top", "move-up", "move-down", "move-bottom"]);
    expect(BAR_MENUS[1].items).toEqual(["low", "normal"]);
  });
});

describe("againArgs", () => {
  it("adds the same kind of download again", () => {
    expect(againArgs(row("f", { state: "completed" }))).toEqual({ url: "https://e.com/f.iso" });
    expect(againArgs(row("s", { kind: "site", url: "https://www.youtube.com/watch?v=x", page_url: "https://www.youtube.com/watch?v=x" }))).toEqual({
      url: "https://www.youtube.com/watch?v=x",
      page: true,
    });
    expect(againArgs(row("h", { kind: "hls", url: "https://e.com/a.m3u8" }))).toEqual({ url: "https://e.com/a.m3u8", stream: true });
    expect(againArgs(row("t", { kind: "torrent" }))).toBeNull();
  });
});
