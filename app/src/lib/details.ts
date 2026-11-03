import { isActive, type Row } from "./model";

/** What the bottom panel is about. */
export type PanelSubject = { kind: "none" } | { kind: "one"; row: Row } | { kind: "many"; rows: Row[] };

/** The selected rows that still exist: none, one, or several (shown as totals). */
export function panelSubject(selected: Set<string>, rows: Map<string, Row>): PanelSubject {
  const picked = [...selected].map((id) => rows.get(id)).filter((r): r is Row => r !== undefined);
  if (picked.length === 0) return { kind: "none" };
  if (picked.length === 1) return { kind: "one", row: picked[0] };
  return { kind: "many", rows: picked };
}

export interface Totals {
  count: number;
  /** Running right now. */
  active: number;
  /** Known sizes only; `unknown` counts the rest. */
  size: number;
  unknown: number;
  bytes: number;
  speed_bps: number;
}

export function combined(rows: Row[]): Totals {
  const t: Totals = { count: rows.length, active: 0, size: 0, unknown: 0, bytes: 0, speed_bps: 0 };
  for (const r of rows) {
    if (r.size === null) t.unknown += 1;
    else t.size += r.size;
    t.bytes += r.bytes;
    if (isActive(r.state)) {
      t.active += 1;
      t.speed_bps += r.speed_bps;
    }
  }
  return t;
}

/**
 * One more speed sample per download, taken once a second. Running downloads add their speed; one
 * that stopped adds a single zero so its graph drops, then waits. Downloads gone from the list are
 * forgotten. Each history keeps its newest `cap` samples.
 */
export function recordSpeeds(history: Map<string, number[]>, rows: Iterable<Row>, cap: number): Map<string, number[]> {
  const next = new Map<string, number[]>();
  for (const r of rows) {
    const before = history.get(r.id);
    if (isActive(r.state)) next.set(r.id, [...(before ?? []), r.speed_bps].slice(-cap));
    else if (before) next.set(r.id, before.at(-1) === 0 ? before : [...before, 0].slice(-cap));
  }
  return next;
}

/** Histories added up, lined up at their newest sample. */
export function sumSeries(series: number[][]): number[] {
  const len = Math.max(0, ...series.map((s) => s.length));
  const out = new Array<number>(len).fill(0);
  for (const s of series) s.forEach((v, i) => (out[len - s.length + i] += v));
  return out;
}

/**
 * SVG polyline points for the newest `slots` samples: the newest at the right edge, each slot
 * `width / (slots - 1)` apart, scaled so the peak touches the top. An empty history draws nothing.
 */
export function chartPoints(values: number[], width: number, height: number, slots: number): string {
  if (values.length === 0) return "";
  const shown = values.slice(-slots);
  const peak = Math.max(...shown);
  const step = width / Math.max(1, slots - 1);
  const start = slots - shown.length;
  return shown
    .map((v, i) => {
      const x = (start + i) * step;
      const y = peak > 0 ? height - (v / peak) * height : height;
      return `${round(x)},${round(y)}`;
    })
    .join(" ");
}

function round(n: number): number {
  return Math.round(n * 10) / 10;
}

export const PANEL_MIN = 140;
/** The list always keeps at least this much height above the panel. */
const LIST_MIN = 160;

export function clampHeight(h: number, available: number): number {
  if (!Number.isFinite(h)) return PANEL_MIN;
  return Math.max(PANEL_MIN, Math.min(h, available - LIST_MIN));
}
