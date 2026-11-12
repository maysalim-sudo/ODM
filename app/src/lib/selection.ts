import { canCancel, canPause, canRefresh, canResume, type Row } from "./model";
import type { AddArgs } from "./types";

/** What the action bar and the right-click menu can do with the selected downloads. */
export type SelAction =
  | "open"
  | "open-folder"
  | "resume"
  | "start-now"
  | "pause"
  | "retry"
  | "move-top"
  | "move-up"
  | "move-down"
  | "move-bottom"
  | "low"
  | "normal"
  | "speed-unlimited"
  | "speed-low"
  | "speed-custom"
  | "copy-link"
  | "copy-name"
  | "copy-path"
  | "new-link"
  | "again"
  | "select-all"
  | "stop"
  | "remove"
  | "delete";

export const SEL_LABELS: Record<SelAction, string> = {
  open: "Open",
  "open-folder": "Open folder",
  resume: "Resume",
  "start-now": "Start now",
  pause: "Pause",
  retry: "Try again",
  "move-top": "Move to top",
  "move-up": "Move up",
  "move-down": "Move down",
  "move-bottom": "Move to bottom",
  low: "Set to low",
  normal: "Back to normal",
  "speed-unlimited": "Speed: unlimited",
  "speed-low": "Speed: low",
  "speed-custom": "Speed: custom…",
  "copy-link": "Copy link",
  "copy-name": "Copy file name",
  "copy-path": "Copy file location",
  "new-link": "Paste new link",
  again: "Download again",
  "select-all": "Select all",
  stop: "Stop",
  remove: "Remove from list",
  delete: "Delete with file",
};

/** "Set to low" and "Speed: low": slow enough to leave room for the rest. */
export const LOW_KBPS = 256;

const isWaiting = (r: Row) => r.state === "queued" || r.state === "paused";
const isDone = (r: Row) => r.state === "completed" || r.state === "cancelled";
const one = (rows: Row[], ok: (r: Row) => boolean) => (rows.length === 1 && ok(rows[0]) ? rows : []);

/**
 * The selected downloads `action` applies to; empty when it applies to none (or, for a
 * one-download action, when more than one is selected). `queueClosed`: the queue schedule keeps
 * queued downloads waiting, so Resume and Start now start them anyway.
 */
export function targets(rows: Row[], action: SelAction, queueClosed = false): Row[] {
  switch (action) {
    case "open":
      return one(rows, (r) => r.state === "completed");
    case "open-folder":
      return one(rows, () => true);
    case "new-link":
      return one(rows, canRefresh);
    case "resume":
      return rows.filter((r) => canResume(r, queueClosed));
    case "start-now":
      return queueClosed ? rows.filter((r) => r.state === "queued") : [];
    case "pause":
      return rows.filter(canPause);
    case "retry":
      return rows.filter((r) => r.state === "failed");
    case "move-top":
    case "move-up":
    case "move-down":
    case "move-bottom":
      return rows.filter(isWaiting);
    case "low":
    case "speed-unlimited":
    case "speed-low":
    case "speed-custom":
      return rows.filter((r) => !isDone(r));
    case "normal":
      return rows.filter((r) => !isDone(r) && r.speed_limit_kbps !== null);
    case "again":
      return rows.filter((r) => (r.state === "failed" || isDone(r)) && r.kind !== "torrent");
    case "stop":
      return rows.filter(canCancel);
    case "copy-link":
    case "copy-name":
    case "copy-path":
    case "select-all":
    case "remove":
    case "delete":
      return rows;
  }
}

/** The right-click menu's groups, top to bottom. */
const CONTEXT_GROUPS: SelAction[][] = [
  ["open", "open-folder"],
  ["resume", "start-now", "pause", "retry"],
  ["move-top", "move-bottom", "low", "normal"],
  ["speed-unlimited", "speed-low", "speed-custom"],
  ["copy-link", "copy-name", "copy-path"],
  ["new-link", "again"],
  ["select-all"],
  ["stop", "remove", "delete"],
];

/** The right-click menu for these downloads: what applies, in groups split by "sep". */
export function contextMenu(rows: Row[], queueClosed = false): (SelAction | "sep")[] {
  const out: (SelAction | "sep")[] = [];
  for (const group of CONTEXT_GROUPS) {
    const shown = group.filter((a) => targets(rows, a, queueClosed).length > 0 || a === "select-all");
    if (shown.length === 0) continue;
    if (out.length > 0) out.push("sep");
    out.push(...shown);
  }
  return out;
}

/** The action bar's dropdowns, by category. Resume, Pause and Stop are buttons of their own. */
export const BAR_MENUS: { label: string; items: SelAction[] }[] = [
  { label: "Queue", items: ["move-top", "move-up", "move-down", "move-bottom"] },
  { label: "Low", items: ["low", "normal"] },
  { label: "Speed", items: ["speed-unlimited", "speed-low", "speed-custom"] },
  { label: "More", items: ["retry", "open-folder", "copy-link", "new-link", "again", "remove", "delete"] },
];

/**
 * The queue with `selected` moved, keeping their order. Up and down move each one place without
 * passing another selected download, so a block moves together. Ids not in the queue are ignored.
 */
export function moveManyInQueue(queue: string[], selected: string[], to: "top" | "up" | "down" | "bottom"): string[] {
  const picked = new Set(selected.filter((id) => queue.includes(id)));
  if (picked.size === 0) return queue;
  if (to === "top" || to === "bottom") {
    const moved = queue.filter((id) => picked.has(id));
    const rest = queue.filter((id) => !picked.has(id));
    return to === "top" ? [...moved, ...rest] : [...rest, ...moved];
  }
  const out = [...queue];
  const step = to === "up" ? -1 : 1;
  const order = to === "up" ? out.map((_, i) => i) : out.map((_, i) => out.length - 1 - i);
  for (const i of order) {
    const j = i + step;
    if (!picked.has(out[i]) || j < 0 || j >= out.length || picked.has(out[j])) continue;
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

/** How to add `row` again as a new download; null for a torrent, which comes from a file or magnet. */
export function againArgs(row: Row): AddArgs | null {
  if (row.kind === "torrent") return null;
  if (row.kind === "site") return { url: row.page_url ?? row.url, page: true };
  if (row.kind === "hls") return { url: row.url, stream: true };
  return { url: row.url };
}
