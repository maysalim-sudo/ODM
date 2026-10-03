import type { ChunkState, ChunkView, Download, DownloadState, Settings, UiEvent } from "./types";

/** A list row: the stored download plus live values from progress events. */
export interface Row extends Download {
  speed_bps: number;
  eta_s: number | null;
  chunks: ChunkView[] | null;
}

export const CATEGORIES = ["video", "audio", "document", "compressed", "image", "program", "general"] as const;

/** Display names for the category ids, used by the sidebar and the settings. */
export const CATEGORY_LABELS: Record<(typeof CATEGORIES)[number], string> = {
  video: "Video",
  audio: "Audio",
  document: "Documents",
  compressed: "Compressed",
  image: "Images",
  program: "Programs",
  general: "Other",
};

export type StatusFilter = "all" | "active" | "queued" | "completed" | "failed";
export type Filter = StatusFilter | `cat:${string}`;
export type SortKey = "filename" | "size" | "progress" | "speed_bps" | "state" | "created_at";
export type SortDir = "asc" | "desc";

const ACTIVE: DownloadState[] = ["probing", "downloading", "merging"];
const DONE: DownloadState[] = ["completed", "failed", "cancelled"];

export function isActive(state: DownloadState): boolean {
  return ACTIVE.includes(state);
}

export function toRow(d: Download, prev?: Row): Row {
  // Live values only carry over while the download is still running.
  const live = isActive(d.state) && prev && isActive(prev.state) ? prev : undefined;
  return {
    ...d,
    // While running, the last progress event is newer than the stored byte count.
    bytes: live ? Math.max(d.bytes, live.bytes) : d.bytes,
    speed_bps: live ? live.speed_bps : 0,
    eta_s: live ? live.eta_s : null,
    chunks: live ? live.chunks : null,
  };
}

export function replaceAll(list: Download[]): Map<string, Row> {
  return new Map(list.map((d) => [d.id, toRow(d)]));
}

/** Like replaceAll, but running rows keep their live speed, ETA and chunk map. */
export function mergeList(prev: Map<string, Row>, list: Download[]): Map<string, Row> {
  return new Map(list.map((d) => [d.id, toRow(d, prev.get(d.id))]));
}

export function applyEvent(rows: Map<string, Row>, ev: UiEvent): { rows: Map<string, Row>; resync: boolean } {
  switch (ev.type) {
    case "upsert": {
      const next = new Map(rows);
      next.set(ev.download.id, toRow(ev.download, rows.get(ev.download.id)));
      return { rows: next, resync: false };
    }
    case "removed": {
      if (!rows.has(ev.id)) return { rows, resync: false };
      const next = new Map(rows);
      next.delete(ev.id);
      return { rows: next, resync: false };
    }
    case "progress_batch": {
      let next: Map<string, Row> | null = null;
      for (const p of ev.items) {
        const row = rows.get(p.id);
        // A late item must not revive a row that already paused, finished or failed.
        if (!row || !isActive(row.state)) continue;
        next ??= new Map(rows);
        next.set(p.id, {
          ...row,
          bytes: p.bytes,
          size: p.total ?? row.size,
          speed_bps: p.speed_bps,
          eta_s: p.eta_s,
          chunks: p.chunks,
        });
      }
      return { rows: next ?? rows, resync: false };
    }
    case "resync":
      return { rows, resync: true };
  }
}

export function matches(row: Row, filter: Filter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "active":
      return isActive(row.state);
    case "queued":
      return row.state === "queued" || row.state === "paused";
    case "completed":
      return row.state === "completed";
    case "failed":
      return row.state === "failed" || row.state === "cancelled";
    default:
      return row.category === filter.slice(4);
  }
}

export function counts(rows: Map<string, Row>): Record<string, number> {
  const out: Record<string, number> = { all: 0, active: 0, queued: 0, completed: 0, failed: 0 };
  for (const c of CATEGORIES) out[`cat:${c}`] = 0;
  const keys = Object.keys(out) as Filter[];
  for (const row of rows.values()) {
    for (const k of keys) if (matches(row, k)) out[k] += 1;
  }
  return out;
}

function sortValue(row: Row, key: SortKey): string | number {
  switch (key) {
    case "filename":
      return row.filename.toLowerCase();
    case "size":
      return row.size ?? -1;
    case "progress":
      return row.size ? row.bytes / row.size : -1;
    case "speed_bps":
      return row.speed_bps;
    case "state":
      return row.state;
    case "created_at":
      return row.created_at;
  }
}

export function visible(rows: Map<string, Row>, filter: Filter, key: SortKey, dir: SortDir): Row[] {
  const sign = dir === "asc" ? 1 : -1;
  return [...rows.values()]
    .filter((r) => matches(r, filter))
    .sort((a, b) => {
      const x = sortValue(a, key);
      const y = sortValue(b, key);
      if (x < y) return -sign;
      if (x > y) return sign;
      return a.id < b.id ? -1 : 1;
    });
}

export function canPause(row: Row): boolean {
  return isActive(row.state) || row.state === "queued";
}

export function canResume(row: Row): boolean {
  return row.state === "paused" || row.state === "failed";
}

export function canCancel(row: Row): boolean {
  return !DONE.includes(row.state);
}

export interface Segment {
  idx: number;
  widthPct: number;
  fillPct: number;
  state: ChunkState;
}

/** Chunk-map geometry, or null when it can't be drawn (unknown size, no chunks). */
export function segments(chunks: ChunkView[], total: number | null): Segment[] | null {
  if (!total || chunks.length === 0 || chunks.some((c) => c.end === null)) return null;
  return [...chunks]
    .sort((a, b) => a.start - b.start)
    .map((c) => {
      const len = (c.end as number) - c.start + 1;
      return {
        idx: c.idx,
        widthPct: (len / total) * 100,
        fillPct: len > 0 ? Math.min(100, (c.written / len) * 100) : 100,
        state: c.state,
      };
    });
}

export interface ParsedUrls {
  valid: string[];
  rejected: string[];
}

/** Same rule as the Rust side: one http(s) URL per line, duplicates once. */
export function parseUrls(text: string): ParsedUrls {
  const out: ParsedUrls = { valid: [], rejected: [] };
  for (const line of text.split(/\r?\n/).map((l) => l.trim()).filter(Boolean)) {
    const ok = /^https?:\/\/[^/]/i.test(line);
    const bucket = ok ? out.valid : out.rejected;
    if (!bucket.includes(line)) bucket.push(line);
  }
  return out;
}

/**
 * Makes list reloads safe: a superseded response is dropped, and upsert/removed events
 * that arrived while the request was in flight are replayed on top of the fresh list.
 */
export class ReloadGate {
  private latest = 0;
  private inFlight = false;
  private buffer: UiEvent[] = [];

  begin(): number {
    this.latest += 1;
    this.inFlight = true;
    // The new list is fetched after everything seen so far, so earlier events are in it.
    this.buffer = [];
    return this.latest;
  }

  record(ev: UiEvent): void {
    if (this.inFlight && (ev.type === "upsert" || ev.type === "removed")) this.buffer.push(ev);
  }

  finish(ticket: number, prev: Map<string, Row>, list: Download[]): Map<string, Row> | null {
    if (ticket !== this.latest) return null;
    let rows = mergeList(prev, list);
    for (const ev of this.buffer) rows = applyEvent(rows, ev).rows;
    this.buffer = [];
    this.inFlight = false;
    return rows;
  }
}

/** The selection restricted to visible rows; the same Set instance when nothing changes. */
export function keepVisible(selected: Set<string>, visible: Row[]): Set<string> {
  const shown = new Set(visible.map((r) => r.id));
  const kept = [...selected].filter((id) => shown.has(id));
  return kept.length === selected.size ? selected : new Set(kept);
}

/**
 * Table column widths in px. Keep in sync with DownloadTable.svelte (CSS repeats these numbers).
 * Always shown: size + progress + status = 396, so the table minimum is 396 + NAME_MIN = 616.
 * Speed shows from 616 + 96 = 712, time left from 712 + 96 = 808, added from 808 + 132 = 940.
 */
export const NAME_MIN = 220;
export const COL = { size: 96, progress: 190, status: 110, speed: 96, eta: 96, added: 132 } as const;
const BASE = COL.size + COL.progress + COL.status;
export const TABLE_MIN = BASE + NAME_MIN;
const SPEED_AT = TABLE_MIN + COL.speed;
const ETA_AT = SPEED_AT + COL.eta;
const ADDED_AT = ETA_AT + COL.added;

/** Which optional columns fit at this table width while the name keeps NAME_MIN. */
export function visibleColumns(width: number): { added: boolean; eta: boolean; speed: boolean } {
  return { added: width >= ADDED_AT, eta: width >= ETA_AT, speed: width >= SPEED_AT };
}

/** A finite whole number from a number input's value; `fallback` when it is empty or not a number. */
export function toInt(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? Math.round(value) : fallback;
}

const NUMERIC_SETTINGS = [
  "max_concurrent",
  "max_connections",
  "max_attempts",
  "speed_limit_kbps",
  "connect_timeout_s",
  "read_timeout_s",
  "min_capture_bytes",
] as const;

/** The settings form with every number field made a finite integer, so the backend never sees null. */
export function coerceSettings(form: Settings, loaded: Settings): Settings {
  const out: Settings = { ...form };
  for (const k of NUMERIC_SETTINGS) out[k] = toInt(form[k], loaded[k]);
  return out;
}

/** A backdrop click closes a dialog only when the press and the release both happened on the backdrop. */
export function shouldCloseOnBackdrop(downOnBackdrop: boolean, upOnBackdrop: boolean): boolean {
  return downOnBackdrop && upOnBackdrop;
}

/** The file name after a probe: auto-fill unless the user typed their own. */
export function nextFilename(current: string, lastAuto: string | null, suggested: string): string {
  return current === "" || current === lastAuto ? suggested : current;
}
