import { formatBytes, formatDate, formatEta } from "./format";
import { canCancel, canRefresh, canResume, isActive, problemInfo, progressPct, rowStatus, type ProblemFix, type Row } from "./model";

export type GroupKey = "needs" | "active" | "next" | "done";

export type RowAction =
  | "pause"
  | "resume"
  | "start-now"
  | "choose-files"
  | "fix"
  | "open"
  | "open-folder"
  | "details"
  | "copy-link"
  | "new-link"
  | "open-page"
  | "cancel"
  | "remove"
  | "move-top"
  | "move-up"
  | "move-down";

export const ACTION_LABELS: Record<RowAction, string> = {
  pause: "Pause",
  resume: "Resume",
  "start-now": "Start now",
  "choose-files": "Choose files",
  fix: "Try again",
  open: "Open",
  "open-folder": "Open folder",
  details: "Details",
  "copy-link": "Copy link",
  "new-link": "Paste new link",
  "open-page": "Open the page",
  cancel: "Cancel download",
  remove: "Remove from list",
  "move-top": "Move to top",
  "move-up": "Move up",
  "move-down": "Move down",
};

export const FIX_LABELS: Record<ProblemFix, string> = {
  "new-link": "Paste new link",
  tools: "Install",
  retry: "Try again",
};

const GIB = 1024 ** 3;

/** One decimal from a gigabyte up to a terabyte, so "3.7 GB of 5.9 GB" stays short. */
function shortBytes(n: number): string {
  return n >= GIB && n < GIB * 1024 ? `${(n / GIB).toFixed(1)} GB` : formatBytes(n);
}

const isWaiting = (r: Row) => r.state === "queued" || r.state === "paused";

/** The waiting rows in the order the engine starts them: priority first, then oldest. */
export function queueOrder(rows: Row[]): Row[] {
  return rows
    .filter(isWaiting)
    .sort((a, b) => b.priority - a.priority || a.created_at - b.created_at || (a.id < b.id ? -1 : 1));
}

/**
 * The All tab's groups, empty ones left out. `confirms` are the rows waiting for the "New download"
 * dialog: a torrent among them needs the user's file choice. Up next is always in queue order.
 */
export function groupRows(rows: Row[], confirms: string[] = []): { key: GroupKey; rows: Row[] }[] {
  const needsChoice = (r: Row) => r.kind === "torrent" && r.state === "paused" && confirms.includes(r.id);
  const needs = rows.filter((r) => r.state === "failed" || needsChoice(r));
  const groups: { key: GroupKey; rows: Row[] }[] = [
    { key: "needs", rows: needs },
    { key: "active", rows: rows.filter((r) => isActive(r.state)) },
    { key: "next", rows: queueOrder(rows.filter((r) => !needs.includes(r))) },
    { key: "done", rows: rows.filter((r) => r.state === "completed" || r.state === "cancelled") },
  ];
  return groups.filter((g) => g.rows.length > 0);
}

function ordinal(n: number): string {
  const tens = n % 100;
  if (tens >= 11 && tens <= 13) return `${n}th`;
  return `${n}${["th", "st", "nd", "rd"][n % 10] ?? "th"}`;
}

/** "1st in line" for the row at `index` of the queue. */
export function lineLabel(index: number): string {
  return `${ordinal(index + 1)} in line`;
}

/** What is left to download: running and waiting rows only. Rows with no size are counted apart. */
export function remaining(rows: Row[]): { bytes: number; unknown: number } {
  const out = { bytes: 0, unknown: 0 };
  for (const r of rows) {
    if (!isActive(r.state) && !isWaiting(r)) continue;
    if (r.size === null) out.unknown += 1;
    else out.bytes += Math.max(0, r.size - r.bytes);
  }
  return out;
}

/** "3.7 GB left", "3.7 GB + 2 unknown left" or "2 unknown left"; null when nothing is left. */
export function remainingLabel(r: { bytes: number; unknown: number }): string | null {
  const known = r.bytes > 0 ? shortBytes(r.bytes) : null;
  const unknown = r.unknown > 0 ? `${r.unknown} unknown` : null;
  const parts = [known, unknown].filter(Boolean);
  return parts.length > 0 ? `${parts.join(" + ")} left` : null;
}

/** "stopped at 38% of 2.3 GB" for a failed row with a known size. */
export function stoppedAt(row: Row): string | null {
  return row.size === null ? null : `stopped at ${Math.floor(progressPct(row))}% of ${shortBytes(row.size)}`;
}

/** The list with `id` moved. `to` is a place, or a final index for a drop. Unknown ids change nothing. */
export function moveInQueue(ids: string[], id: string, to: "top" | "up" | "down" | number): string[] {
  const from = ids.indexOf(id);
  if (from === -1) return ids;
  const place = to === "top" ? 0 : to === "up" ? from - 1 : to === "down" ? from + 1 : to;
  const rest = ids.filter((x) => x !== id);
  rest.splice(Math.min(rest.length, Math.max(0, place)), 0, id);
  return rest;
}

/** The queue by size, smallest or largest first. Rows with no known size stay last, in their order. */
export function sortQueueBySize(rows: Row[], dir: "asc" | "desc"): string[] {
  const sign = dir === "asc" ? 1 : -1;
  const known = rows.filter((r) => r.size !== null).sort((a, b) => sign * ((a.size as number) - (b.size as number)));
  return [...known, ...rows.filter((r) => r.size === null)].map((r) => r.id);
}

/** The line under a card's name. `line` is its place in the queue, such as "2nd in line". */
export function rowSummary(row: Row, waiting: boolean, queueWait: string | null, line: string | null = null): string {
  if (line !== null && isWaiting(row)) {
    const size = row.state === "queued" && !queueWait ? (row.size === null ? null : shortBytes(row.size)) : rowSummary(row, waiting, queueWait);
    return [size, line].filter(Boolean).join(" · ");
  }
  const status = rowStatus(row, waiting, queueWait);
  switch (row.state) {
    case "downloading": {
      const done = row.size === null ? shortBytes(row.bytes) : `${shortBytes(row.bytes)} of ${shortBytes(row.size)}`;
      return row.eta_s === null ? done : `${done} · ${formatEta(row.eta_s)} left`;
    }
    case "paused":
      if (status !== "Paused") return status;
      return row.size || row.segments ? `Paused at ${Math.floor(progressPct(row))}%` : status;
    case "failed":
      return problemInfo(row).summary;
    case "completed":
      return [row.size === null ? null : shortBytes(row.size), row.finished_at === null ? null : formatDate(row.finished_at)]
        .filter(Boolean)
        .join(" · ");
    default:
      return status;
  }
}

function primaryAction(row: Row, queueClosed: boolean, waiting: boolean): RowAction | null {
  if (isActive(row.state)) return "pause";
  switch (row.state) {
    case "paused":
      return waiting && row.kind === "torrent" ? "choose-files" : "resume";
    case "queued":
      return canResume(row, queueClosed) ? "start-now" : "pause";
    case "failed":
      return "fix";
    case "completed":
      return "open";
    case "cancelled":
      return "remove";
  }
  return null;
}

/** A row's buttons and its `…` menu. Items that don't apply are left out, not greyed. */
export function rowActions(
  row: Row,
  queueClosed: boolean,
  waiting = false,
): { primary: RowAction | null; secondary: RowAction | null; menu: RowAction[] } {
  const menu: RowAction[] = ["details", "open-folder", "copy-link"];
  if (canRefresh(row)) menu.push("new-link");
  if (canCancel(row)) menu.push("cancel");
  menu.push("remove");
  return {
    primary: primaryAction(row, queueClosed, waiting),
    secondary: row.state === "completed" ? "open-folder" : null,
    menu,
  };
}
