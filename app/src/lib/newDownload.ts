import { formatBytes } from "./format";
import type { Row } from "./model";

/** A folder path compared the way Windows does: no case, no trailing slash. */
const folderKey = (dir: string) => dir.replace(/[\\/]+$/, "").toLowerCase();

const counts = (r: Row) => r.state !== "failed" && r.state !== "cancelled";

/**
 * Another download of the same address with the same MP3 or video choice, newest first. Failed
 * and cancelled ones don't count (adding again is how they're retried); torrents are matched by
 * ODM when they're added.
 */
export function listDuplicate(row: Row, rows: Row[]): Row | undefined {
  if (row.kind === "torrent") return undefined;
  return rows
    .filter((r) => r.id !== row.id && r.url === row.url && !!r.audio === !!row.audio && counts(r))
    .sort((a, b) => b.created_at - a.created_at)[0];
}

/** The names of the other downloads going to `row`'s folder, which its name mustn't clash with. */
export function namesInFolder(row: Row, rows: Row[]): string[] {
  const dir = folderKey(row.save_dir);
  return rows.filter((r) => r.id !== row.id && r.state !== "cancelled" && folderKey(r.save_dir) === dir).map((r) => r.filename);
}

/** The folders downloads went to lately, newest first, once each, without `current`. */
export function recentFolders(rows: Row[], current: string | null, limit = 5): string[] {
  const seen = new Set(current ? [folderKey(current)] : []);
  const out: string[] = [];
  for (const r of [...rows].sort((a, b) => b.created_at - a.created_at)) {
    const key = folderKey(r.save_dir);
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(r.save_dir.replace(/[\\/]+$/, ""));
    if (out.length >= limit) break;
  }
  return out;
}

/** The links downloaded lately, newest first, once each: a site video's page, else the link itself. */
export function recentLinks(rows: Row[], limit = 8): { url: string; name: string }[] {
  const seen = new Set<string>();
  const out: { url: string; name: string }[] = [];
  for (const r of [...rows].sort((a, b) => b.created_at - a.created_at)) {
    const url = r.kind === "site" ? (r.page_url ?? r.url) : r.url;
    if (seen.has(url)) continue;
    seen.add(url);
    out.push({ url, name: r.filename });
    if (out.length >= limit) break;
  }
  return out;
}

/** What to say when a file of `size` bytes won't fit in `free` bytes; null when it fits or isn't known. */
export function spaceProblem(size: number | null, free: number | null): string | null {
  if (size === null || free === null || size <= free) return null;
  return `Not enough space: it needs ${formatBytes(size)} and the drive has ${formatBytes(free)} free.`;
}

/** Unix seconds a plausible expiry lies between: 2001 to 2286. Anything else isn't a time. */
const PLAUSIBLE = (s: number) => s > 1_000_000_000 && s < 10_000_000_000;

/**
 * When a signed link stops working, in milliseconds since 1970, from what the link itself says:
 * `expires`, `Expires` or `exp` as a Unix time, or Amazon's `X-Amz-Date` plus `X-Amz-Expires`.
 * Null when the link doesn't say.
 */
export function linkExpiry(url: string): number | null {
  let q: URLSearchParams;
  try {
    q = new URL(url).searchParams;
  } catch {
    return null;
  }
  const amzDate = q.get("X-Amz-Date");
  const amzFor = Number(q.get("X-Amz-Expires"));
  if (amzDate && Number.isFinite(amzFor) && amzFor > 0) {
    const m = /^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z$/.exec(amzDate);
    if (m) return Date.UTC(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +m[6]) + amzFor * 1000;
  }
  for (const key of ["expires", "Expires", "exp"]) {
    const v = q.get(key);
    if (v !== null && /^\d+$/.test(v) && PLAUSIBLE(Number(v))) return Number(v) * 1000;
  }
  return null;
}

/** The dialog's line about a link that expires: the time today, or the day and time later on. */
export function expiryNote(expiry: number | null, now = Date.now()): string | null {
  if (expiry === null) return null;
  if (expiry <= now) return "This link has probably expired. If it fails, paste a new link from the page.";
  const at = new Date(expiry);
  const time = at.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", hour12: false });
  const sameDay = at.toDateString() === new Date(now).toDateString();
  const when = sameDay ? time : `${at.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" })} ${time}`;
  return `This link stops working around ${when}. Start it before then.`;
}
