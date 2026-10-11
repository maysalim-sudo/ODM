import type { DownloadState, Schedule } from "./types";

const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(n: number): string {
  let v = n;
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i += 1;
  }
  if (i === 0) return `${Math.round(v)} B`;
  return `${v.toFixed(i >= 3 ? 2 : 1)} ${UNITS[i]}`;
}

export function formatSpeed(bps: number): string {
  return bps > 0 ? `${formatBytes(bps)}/s` : "—";
}

export function formatEta(seconds: number | null): string {
  if (seconds === null) return "—";
  const s = Math.max(0, Math.round(seconds));
  const parts: [number, string][] = [
    [Math.floor(s / 86400), "d"],
    [Math.floor((s % 86400) / 3600), "h"],
    [Math.floor((s % 3600) / 60), "min"],
    [s % 60, "s"],
  ];
  const first = parts.findIndex(([v]) => v > 0);
  if (first === -1) return "0 s";
  return parts
    .slice(first, first + 2)
    .filter(([v]) => v > 0)
    .map(([v, u]) => `${v} ${u}`)
    .join(" ");
}

/** 0–100, or null when the total size is unknown. */
export function percent(bytes: number, total: number | null): number | null {
  if (!total || total <= 0) return null;
  return Math.min(100, (bytes / total) * 100);
}

export function formatDate(ms: number): string {
  const d = new Date(ms);
  const sameDay = d.toDateString() === new Date().toDateString();
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return sameDay ? time : `${d.toLocaleDateString()} ${time}`;
}

const LABELS: Record<DownloadState, string> = {
  queued: "Queued",
  probing: "Starting",
  downloading: "Downloading",
  paused: "Paused",
  merging: "Merging",
  completed: "Completed",
  failed: "Failed",
  cancelled: "Cancelled",
};

export function stateLabel(state: DownloadState): string {
  return LABELS[state];
}

const PAUSE_REASONS: Record<string, string> = {
  schedule: "Paused by schedule",
  exists: "Waiting for your choice",
};

/**
 * The status of a row, saying why a paused download is paused. `queueWait` is
 * `queueWaitLabel(...)`: set while the queue schedule keeps queued rows waiting.
 */
export function statusLabel(state: DownloadState, pauseReason: string | null, queueWait: string | null = null): string {
  if (state === "paused" && pauseReason) return PAUSE_REASONS[pauseReason] ?? LABELS.paused;
  if (state === "queued" && queueWait) return queueWait;
  return LABELS[state];
}

const DAY_NAMES = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const DAY_MIN = 24 * 60;

function minutesOf(hhmm: string): number | null {
  const m = /^(\d{2}):(\d{2})$/.exec(hhmm);
  if (!m) return null;
  const h = Number(m[1]);
  const min = Number(m[2]);
  return h < 24 && min < 60 ? h * 60 + min : null;
}

/**
 * "Waiting for the schedule (starts HH:MM)" while the queue schedule is closed at `now`,
 * with the weekday when the next start is not today; null when queued downloads may run.
 * Follows the engine: a window that crosses midnight belongs to the day it starts on, and
 * the same from and to means the whole day.
 */
export function queueWaitLabel(s: Schedule, now: Date): string | null {
  const from = minutesOf(s.from);
  const to = minutesOf(s.to);
  if (!s.enabled || !s.days.some(Boolean) || from === null || to === null) return null;
  const today = (now.getDay() + 6) % 7;
  const minute = now.getHours() * 60 + now.getMinutes();
  const ticked = (d: number) => s.days[((d % 7) + 7) % 7];
  const open =
    from === to
      ? ticked(today)
      : from < to
        ? ticked(today) && minute >= from && minute < to
        : (ticked(today) && minute >= from) || (ticked(today - 1) && minute < to);
  if (open) return null;
  const start = from === to ? 0 : from;
  for (let offset = 0; offset <= 7; offset++) {
    if (!ticked(today + offset) || offset * DAY_MIN + start <= minute) continue;
    const time = `${String(Math.floor(start / 60)).padStart(2, "0")}:${String(start % 60).padStart(2, "0")}`;
    const when = offset === 0 ? time : `${DAY_NAMES[(today + offset) % 7]} ${time}`;
    return `Waiting for the schedule (starts ${when})`;
  }
  return null;
}

/** Join a folder and a file name with the separator the folder already uses. */
export function joinPath(dir: string, name: string): string {
  const sep = dir.includes("\\") ? "\\" : "/";
  return dir.endsWith("\\") || dir.endsWith("/") ? `${dir}${name}` : `${dir}${sep}${name}`;
}
