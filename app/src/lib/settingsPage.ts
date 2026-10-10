import type { Settings } from "./types";

/** The Settings page: its sections, rows, search, restore and inline checks. */

export type SectionId = "general" | "downloads" | "speed" | "browser" | "video" | "network" | "updates" | "advanced";

/** Most-used first; advanced last. */
export const SECTIONS: { id: SectionId; title: string }[] = [
  { id: "general", title: "General" },
  { id: "downloads", title: "Downloads" },
  { id: "speed", title: "Speed" },
  { id: "browser", title: "Browser" },
  { id: "video", title: "Video" },
  { id: "network", title: "Network" },
  { id: "updates", title: "Updates" },
  { id: "advanced", title: "Advanced" },
];

export interface RowDef {
  id: string;
  section: SectionId;
  label: string;
  description: string;
  /** The settings this row edits, reset by the section's "Restore defaults". */
  fields: (keyof Settings)[];
}

const row = (section: SectionId, id: string, label: string, description: string, fields: (keyof Settings)[] = []): RowDef => ({
  id,
  section,
  label,
  description,
  fields,
});

export const ROWS: RowDef[] = [
  row("general", "start_with_windows", "Start ODM with Windows", "ODM starts in the tray when you sign in.", ["start_with_windows"]),
  row("general", "start_minimized", "Start minimized to the tray", "The window stays hidden until you open it from the tray.", ["start_minimized"]),
  row("general", "close_to_tray", "Keep running in the tray when closed", "Closing the window keeps downloads going.", ["close_to_tray"]),
  row("general", "notify_complete", "Notify when a download finishes", "Shows a Windows notification with the file name.", ["notify_complete"]),
  row("general", "notify_failed", "Notify when a download fails", "So you can retry it or see what went wrong.", ["notify_failed"]),
  row("general", "prevent_sleep", "Prevent sleep while downloading", "The PC stays awake until every download is done.", ["prevent_sleep"]),

  row("downloads", "download_dir", "Download folder", "Where new downloads are saved.", ["download_dir"]),
  row("downloads", "sort_by_type", "Sort into folders by type", "Each type gets its own folder inside the download folder.", ["sort_by_type", "category_folders"]),
  row("downloads", "on_exists", "When a file already exists", "Rename keeps both files by adding a number.", ["on_exists"]),
  row("downloads", "on_complete", "When a download finishes", "Open its folder or the file itself.", ["on_complete"]),
  row("downloads", "start_mode", "Start new downloads", "Paused downloads wait until you press Resume.", ["start_mode"]),
  row("downloads", "max_concurrent", "Downloads at once", "The rest wait in the queue.", ["max_concurrent"]),
  row("downloads", "history_days", "Remove finished downloads from the list", "Only the list entry goes. The file stays.", ["history_days"]),

  row("speed", "speed_limit", "Speed limit", "Caps the total speed of all downloads.", ["speed_limit_kbps"]),
  row("speed", "limit_schedule", "Limit only at certain times", "Outside these hours downloads run at full speed.", ["limit_schedule"]),
  row("speed", "max_connections", "Connections per download", "More connections can be faster. Some servers allow only a few.", ["max_connections"]),
  row("speed", "queue_schedule", "Run the queue at certain times", "Downloads start at the start time and pause at the stop time.", ["queue_schedule"]),
  row("speed", "after_queue", "When the queue is done", "Runs once the queue has nothing left to download.", ["after_queue"]),

  row("browser", "capture_enabled", "Take over downloads from the browser", "Needs the ODM browser extension.", ["capture_enabled"]),
  row("browser", "confirm_browser_downloads", "Ask where to save each one", "Shows the new download window before it starts.", ["confirm_browser_downloads"]),
  row("browser", "min_capture", "Only take files larger than", "Smaller files stay in the browser.", ["min_capture_bytes"]),
  row("browser", "skip_types", "Leave these file types to the browser", "For example pdf or jpg.", ["skip_types"]),
  row("browser", "skip_hosts", "Sites to leave to the browser", "Downloads from these sites stay in the browser.", ["skip_hosts"]),
  row("browser", "browser_connection", "Browser connection", "Check or repair the link between ODM and your browsers."),

  row("video", "video_tools", "Video tools", "FFmpeg and yt-dlp download videos from streaming sites."),
  row("video", "video_quality", "Preferred quality", "ODM picks the closest quality the site offers.", ["video_quality"]),
  row("video", "video_container", "Format", "MP4 plays everywhere. MKV keeps more audio and subtitle tracks.", ["video_container"]),
  row("video", "video_subtitles", "Download subtitles when available", "Subtitles are embedded in the video file.", ["video_subtitles"]),
  row("video", "ytdlp_auto_update", "Keep yt-dlp up to date automatically", "Video sites change often. ODM checks once a week.", ["ytdlp_auto_update"]),

  row("network", "proxy", "Proxy", "System uses the Windows proxy settings.", ["proxy_mode", "proxy_url"]),
  row("network", "connect_timeout_s", "Connect timeout", "How long to wait for a server to answer.", ["connect_timeout_s"]),
  row("network", "read_timeout_s", "Read timeout", "How long a stalled connection waits before it retries.", ["read_timeout_s"]),
  row("network", "max_attempts", "Retries per part", "How often each part is retried before the download fails.", ["max_attempts"]),

  row("updates", "update_check", "Check for updates automatically", "ODM tells you when a new version is ready.", ["update_check"]),
  row("updates", "update_install", "When an update is ready", "Installing when you quit saves a step.", ["update_install"]),
  row("updates", "check_now", "Check for updates now", "Looks for a new version right away."),
  row("updates", "app_version", "Version", "The version of ODM you are running."),

  row("advanced", "user_agent", "Custom User-Agent", "Leave empty to use ODM's own. Browser downloads keep the browser's.", ["user_agent"]),
  row("advanced", "ffmpeg_path", "Path to FFmpeg", "Leave empty to use the video tools or find FFmpeg automatically.", ["ffmpeg_path"]),
  row("advanced", "ytdlp_path", "Path to yt-dlp", "Leave empty to use the video tools or find yt-dlp automatically.", ["ytdlp_path"]),
  row("advanced", "app_folders", "ODM folders", "The logs help when something goes wrong."),
  row("advanced", "reset_all", "Reset all settings", "Every setting goes back to its default."),
];

/**
 * Rows whose engine work has not landed yet stay hidden, so the page never shows a
 * control that does nothing. Later tasks flip their entries to true. Rows not listed are ready.
 */
export const READY: Partial<Record<string, boolean>> = {
  // Task 4
  start_with_windows: false,
  video_quality: false,
  video_container: false,
  video_subtitles: false,
  ytdlp_auto_update: false,
  proxy: false,
  user_agent: false,
  // Task 1
  update_check: false,
  update_install: false,
  check_now: false,
};

const BY_ID = new Map(ROWS.map((r) => [r.id, r]));

export function rowDef(id: string): RowDef {
  const r = BY_ID.get(id);
  if (!r) throw new Error(`unknown settings row ${id}`);
  return r;
}

const ready = (id: string) => READY[id] !== false;

/** Every word of the query appears in the label or the description, ignoring case. */
export function matchesQuery(r: RowDef, query: string): boolean {
  const hay = `${r.label} ${r.description}`.toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w));
}

export function isShown(id: string, query: string): boolean {
  return ready(id) && matchesQuery(rowDef(id), query);
}

/** Ids of the ready rows that match; a null section searches them all. */
export function visibleRows(section: SectionId | null, query: string): string[] {
  return ROWS.filter((r) => (section === null || r.section === section) && ready(r.id) && matchesQuery(r, query)).map((r) => r.id);
}

const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;

/** Settings a section's "Restore defaults" never touches: users keep their download folder. */
const KEPT_ON_RESTORE: (keyof Settings)[] = ["download_dir"];

function restorable(section: SectionId): (keyof Settings)[] {
  return ROWS.filter((r) => r.section === section && ready(r.id))
    .flatMap((r) => r.fields)
    .filter((f) => !KEPT_ON_RESTORE.includes(f));
}

export function sectionHasRestore(section: SectionId): boolean {
  return restorable(section).length > 0;
}

/** `current` with the visible settings of one section back at their defaults. */
export function restoreSection(current: Settings, defaults: Settings, section: SectionId): Settings {
  const out = clone(current);
  const d = clone(defaults);
  for (const f of restorable(section)) (out as unknown as Record<string, unknown>)[f] = d[f];
  return out;
}

/**
 * Undo for "Restore defaults" and "Reset all settings": the fields that went from `before` to
 * `after` get their `before` values back, on top of `current`, so edits made since then stay.
 */
export function undoRestore(before: Settings, after: Settings, current: Settings): Settings {
  const out = clone(current) as unknown as Record<string, unknown>;
  const b = before as unknown as Record<string, unknown>;
  const a = after as unknown as Record<string, unknown>;
  for (const k of Object.keys(b)) {
    if (JSON.stringify(b[k]) !== JSON.stringify(a[k])) out[k] = clone(b[k]);
  }
  return out as unknown as Settings;
}

/** Every setting at its default, except that the welcome screen stays done. */
export function resetAll(current: Settings, defaults: Settings): Settings {
  return { ...clone(defaults), onboarding_done: current.onboarding_done };
}

/** null when `v` is a whole number in lo..hi, else the message to show under the field. */
export function checkInt(v: number | null | undefined, lo: number, hi: number): string | null {
  return typeof v === "number" && Number.isInteger(v) && v >= lo && v <= hi ? null : `Enter a whole number from ${lo} to ${hi}.`;
}

/** The same rule as the Rust side: one plain folder name, or empty for the download folder itself. */
export function checkFolderName(name: string): string | null {
  if (name === "") return null;
  if (/[\\/:]/.test(name) || name === "." || name === ".." || name.trim() !== name) {
    return "Use a plain folder name, without \\ / or :.";
  }
  return null;
}

export function checkProxyUrl(url: string): string | null {
  const m = /^(http|https|socks5):\/\/([^\s/:@]+)(?::(\d{1,5}))?\/?$/i.exec(url.trim());
  if (!m) return "Enter an address like http://host:8080 or socks5://host:1080.";
  if (m[3] !== undefined) {
    const port = Number(m[3]);
    if (port < 1 || port > 65535) return "The port must be from 1 to 65535.";
  }
  return null;
}

export type SpeedUnit = "kb" | "mb";

export const MAX_KBPS = 10_000_000;

export function toKbps(value: number, unit: SpeedUnit): number {
  return Math.round(unit === "mb" ? value * 1024 : value);
}

/** MB/s when the speed is at least 1 MB/s and a tenth of a MB/s fits it exactly, else KB/s. */
export function fromKbps(kbps: number): { value: number; unit: SpeedUnit } {
  const mb = kbps / 1024;
  return kbps >= 1024 && Number.isInteger(mb * 10) ? { value: mb, unit: "mb" } : { value: kbps, unit: "kb" };
}

export function checkSpeed(value: number | null | undefined, unit: SpeedUnit): string | null {
  if (typeof value !== "number" || !Number.isFinite(value)) return "Enter a speed.";
  const kbps = toKbps(value, unit);
  if (kbps < 1) return "Enter a speed of at least 1 KB/s.";
  if (kbps > MAX_KBPS) return "That is faster than any connection. Turn the limit off instead.";
  return null;
}

/** A site for the skip list, read the way the Rust side's normalize_host reads it. */
export function checkHost(entry: string): string | null {
  let s = entry.trim();
  const scheme = s.indexOf("://");
  if (scheme !== -1) s = s.slice(scheme + 3);
  s = s.split(/[/?#]/)[0];
  s = s.slice(s.lastIndexOf("@") + 1).split(":")[0].toLowerCase().replace(/^\.+|\.+$/g, "");
  if (s.startsWith("www.")) s = s.slice(4);
  return s !== "" && /^[a-z0-9.-]+$/.test(s) ? null : `"${entry}" is not a site name like example.com.`;
}

/** A file type for the skip list: letters and digits, with or without a leading dot. */
export function checkType(t: string): string | null {
  return /^\.?[a-z0-9]{1,16}$/i.test(t.trim()) ? null : `"${t}" is not a file type like pdf.`;
}

/** Entries typed into a tag field: split on commas, spaces and new lines, without repeats. */
export function splitTags(text: string): string[] {
  const out: string[] = [];
  for (const t of text.split(/[\s,;]+/)) if (t && !out.includes(t)) out.push(t);
  return out;
}

const OMEGA = 8;
const END = 1 - (1 + OMEGA) * Math.exp(-OMEGA);

/** A critically damped spring (no overshoot) as an easing from 0 to 1. */
export function springEase(t: number): number {
  if (t <= 0) return 0;
  if (t >= 1) return 1;
  return Math.min(1, (1 - (1 + OMEGA * t) * Math.exp(-OMEGA * t)) / END);
}

/** How long the page takes to slide in or out. */
export const PAGE_MS = 350;

export const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] as const;
