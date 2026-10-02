import type { ChunkState, ChunkView, Download, DownloadState, UiEvent } from "./types";

/** A list row: the stored download plus live values from progress events. */
export interface Row extends Download {
  speed_bps: number;
  eta_s: number | null;
  chunks: ChunkView[] | null;
}

export const CATEGORIES = ["video", "audio", "document", "compressed", "image", "program", "general"] as const;

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
