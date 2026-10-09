import type { ChunkState, ChunkView, Download, DownloadState, IntegrationStatus, Offer, SegmentProgress, Settings, ToolInfo, ToolsStatus, UiEvent } from "./types";

/** A list row: the stored download plus live values from progress events. */
export interface Row extends Download {
  speed_bps: number;
  eta_s: number | null;
  chunks: ChunkView[] | null;
  /** HLS only: from the last progress event. */
  segments: SegmentProgress | null;
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

/** Download progress as 0–100: segments done over total for HLS, else bytes over size. */
export function progressPct(row: Row): number {
  let pct = 0;
  if (row.segments) {
    pct = row.segments.total > 0 ? (row.segments.done / row.segments.total) * 100 : 0;
  } else if (row.size) {
    pct = (row.bytes / row.size) * 100;
  }
  return Math.min(100, Math.max(0, pct));
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
    segments: live ? live.segments : null,
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
          segments: p.segments ?? null,
        });
      }
      return { rows: next ?? rows, resync: false };
    }
    case "resync":
      return { rows, resync: true };
    case "reuse_offer":
    case "confirm_download":
    case "tools_progress":
      // Offers and confirmations are not rows; the store handles them before this function.
      return { rows, resync: false };
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
      return row.segments || row.size ? progressPct(row) : -1;
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

export function canRefresh(row: Row): boolean {
  return row.state === "paused" || row.state === "failed";
}

const RISKY_EXTENSIONS = new Set(
  "exe msi bat cmd com scr ps1 vbs vbe js jse wsf wsh hta lnk reg cpl msc jar appx msix".split(" "),
);

/** True for file types that run code when opened, judged by extension only. */
export function isRiskyFile(name: string): boolean {
  const dot = name.lastIndexOf(".");
  return dot !== -1 && RISKY_EXTENSIONS.has(name.slice(dot + 1).toLowerCase());
}

/** The list with `o` added, or the same list when its offer_id is already there. */
export function addOffer(list: Offer[], o: Offer): Offer[] {
  return list.some((x) => x.offer_id === o.offer_id) ? list : [...list, o];
}

/** The first pending id whose row is paused, i.e. the next "New download" dialog to show. */
export function nextConfirm(ids: string[], rows: Row[]): Row | undefined {
  const byId = new Map(rows.map((r) => [r.id, r]));
  for (const id of ids) {
    const row = byId.get(id);
    if (row?.state === "paused") return row;
  }
  return undefined;
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

/** How long ago `ms` was, in plain words. A clock slightly ahead counts as "just now". */
export function relativeTime(ms: number, now: number): string {
  const sec = Math.max(0, (now - ms) / 1000);
  if (sec < 45) return "just now";
  const min = sec / 60;
  if (min < 90) return `${Math.max(1, Math.round(min))} min ago`;
  const h = min / 60;
  if (h < 36) return `${Math.round(h)} h ago`;
  return `${Math.round(h / 24)} days ago`;
}

/** The first thing wrong with the browser connection, in a sentence; null when all is ready. */
export function integrationProblem(s: IntegrationStatus): string | null {
  if (!s.host_found) return "The browser helper is missing next to ODM.";
  if (!s.pipe_running) return "ODM couldn't open its connection for browsers. Another copy of ODM may be running.";
  const bad = s.browsers.find((b) => b.status !== "ok");
  if (!bad) return null;
  if (bad.status === "missing") return `${bad.browser} is not registered.`;
  if (bad.status === "wrong_path") return `${bad.browser} is registered for another copy of ODM.`;
  return `The registration file for ${bad.browser} is damaged.`;
}

const PART_WORDS: Record<ChunkState, string> = {
  pending: "waiting",
  active: "downloading",
  retrying: "retrying",
  done: "finished",
  failed: "failed",
};

/** The chunk map's legend: finished parts, then any parts downloading, retrying or failed, so state never relies on colour. */
export function chunkLegend(chunks: ChunkView[]): string {
  const n = (state: ChunkState) => chunks.filter((c) => c.state === state).length;
  const extra = (["active", "retrying", "failed"] as const)
    .filter((s) => n(s) > 0)
    .map((s) => `${n(s)} ${PART_WORDS[s]}`);
  return [`${n("done")} of ${chunks.length} parts finished`, ...extra].join(" · ");
}

/** Tooltip for one chunk-map segment, e.g. "Part 3: retrying, 40%". */
export function partTitle(idx: number, state: ChunkState, fillPct: number): string {
  return `Part ${idx + 1}: ${PART_WORDS[state]}, ${fillPct.toFixed(0)}%`;
}

/** One line per video tool ("yt-dlp 2026.08.19 · from PATH"), plus whether any, all, or ODM's own copies exist. */
export function toolsSummary(s: ToolsStatus): { lines: string[]; installed: boolean; complete: boolean; own: boolean } {
  const tools: [string, ToolInfo | null][] = [
    ["FFmpeg", s.ffmpeg],
    ["yt-dlp", s.ytdlp],
    ["Deno", s.deno],
  ];
  return {
    lines: tools.map(([name, t]) => (t ? `${name} ${t.version} · from ${t.source}` : `${name}: not installed`)),
    installed: tools.some(([, t]) => t),
    complete: tools.every(([, t]) => t),
    own: tools.some(([, t]) => t?.source === "ODM"),
  };
}

/** What the video-tools download is doing, e.g. "FFmpeg: 12.0 MB of 150 MB". */
export function toolsProgressText(p: { stage: string; done: number; total: number }): string {
  if (p.stage === "Installing") return "Installing…";
  const mb = (n: number) => n / (1024 * 1024);
  const done = `${mb(p.done).toFixed(1)} MB`;
  return p.total > 0 ? `${p.stage}: ${done} of ${Math.round(mb(p.total))} MB` : `${p.stage}: ${done}`;
}

/** Whether a failed download's message says FFmpeg or yt-dlp is missing, so "Get video tools" can fix it. */
export function needsVideoTools(message: string | null): boolean {
  return message !== null && /^(yt-dlp|FFmpeg) not found/.test(message);
}
