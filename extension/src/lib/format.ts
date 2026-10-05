export function formatSpeed(bps: number): string {
  if (bps < 1024) return `${Math.round(bps)} B/s`;
  const units = ["KB/s", "MB/s", "GB/s"];
  let v = bps / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(1)} ${units[i]}`;
}

/** "12.3 MB" style, 1024 base, one decimal. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(1)} ${units[i]}`;
}

export function statusLine(s: { active: number; queued: number; total_speed_bps: number }): string {
  return `${s.active} active · ${s.queued} queued · ${formatSpeed(s.total_speed_bps)}`;
}
