import type { DownloadState } from "./types";

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

/** Join a folder and a file name with the separator the folder already uses. */
export function joinPath(dir: string, name: string): string {
  const sep = dir.includes("\\") ? "\\" : "/";
  return dir.endsWith("\\") || dir.endsWith("/") ? `${dir}${name}` : `${dir}${sep}${name}`;
}
