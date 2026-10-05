import { basename, cookieHeader, type Cookie } from "./capture";
import { formatBytes } from "./format";
import type { AddDownloadPayload, AddStreamPayload } from "./protocol";

export type StreamKind = "hls" | "dash" | "file";
export type Stream = { url: string; kind: StreamKind; label: string; supported: boolean; size?: number };

/** Smaller responses are ads, thumbnails and previews, not videos worth listing. */
export const MIN_VIDEO_BYTES = 2 * 1024 * 1024;
const VIDEO_TYPES = ["video/mp4", "video/webm", "video/quicktime", "video/x-matroska", "video/ogg"];
const VIDEO_EXTENSIONS = [".mp4", ".webm", ".mov", ".mkv", ".m4v"];

const HLS_TYPES = ["application/vnd.apple.mpegurl", "application/x-mpegurl", "audio/mpegurl"];

function pathOf(url: string): string | null {
  try {
    return new URL(url).pathname.toLowerCase();
  } catch {
    return null;
  }
}

function numberOf(text: string | undefined): number | undefined {
  if (text === undefined || !/^\s*\d+\s*$/.test(text)) return undefined;
  return Number(text);
}

/** Total size from Content-Range "bytes a-b/TOTAL" when present, else Content-Length; undefined when neither is a number. */
export function responseSize(contentLength: string | undefined, contentRange: string | undefined): number | undefined {
  const total = numberOf(contentRange?.match(/\/\s*(\d+)\s*$/)?.[1]);
  return total ?? numberOf(contentLength);
}

/** HLS or DASH by the URL's path (the query is ignored) or by the response content type; a plain video file by type or extension and size. */
export function classify(url: string, contentType: string | undefined, size?: number): StreamKind | null {
  const path = pathOf(url);
  if (path === null) return null;
  if (new URL(url).hostname.toLowerCase().endsWith("googlevideo.com")) return null;
  const type = (contentType ?? "").split(";")[0].trim().toLowerCase();
  if (path.endsWith(".m3u8") || HLS_TYPES.includes(type)) return "hls";
  if (path.endsWith(".mpd") || type === "application/dash+xml") return "dash";
  const byType = VIDEO_TYPES.includes(type);
  const byExtension = (type === "" || type === "application/octet-stream") && VIDEO_EXTENSIONS.some((e) => path.endsWith(e));
  if ((byType || byExtension) && size !== undefined && size >= MIN_VIDEO_BYTES) return "file";
  return null;
}

/** `<last path segment> · <host>`, with the size between them when known, for telling items apart in a list. */
export function streamLabel(url: string, size?: number): string {
  try {
    const u = new URL(url);
    const last = decodeURIComponent(u.pathname.split("/").filter(Boolean).pop() ?? "");
    const parts = [last, size === undefined ? "" : formatBytes(size), u.host].filter(Boolean);
    return parts.join(" · ");
  } catch {
    return url;
  }
}

function withoutFragment(url: string): string {
  const i = url.indexOf("#");
  return i === -1 ? url : url.slice(0, i);
}

/** The playlists seen per tab, in the order they were seen. Nothing here is ever sent anywhere. */
export class StreamStore {
  private tabs = new Map<number, Map<string, Stream>>();

  /** True when the URL (ignoring its fragment) was not listed for this tab yet. */
  add(tabId: number, url: string, kind: StreamKind, size?: number): boolean {
    const key = withoutFragment(url);
    let tab = this.tabs.get(tabId);
    if (!tab) {
      tab = new Map();
      this.tabs.set(tabId, tab);
    }
    const known = tab.get(key);
    if (known) {
      if (known.size === undefined && size !== undefined) {
        known.size = size;
        if (known.kind === "file") known.label = streamLabel(key, size);
      }
      return false;
    }
    const isFile = kind === "file";
    tab.set(key, {
      url: key,
      kind,
      label: streamLabel(key, isFile ? size : undefined),
      supported: kind === "hls" || isFile,
      size,
    });
    return true;
  }

  list(tabId: number): Stream[] {
    return [...(this.tabs.get(tabId)?.values() ?? [])];
  }

  clear(tabId: number): void {
    this.tabs.delete(tabId);
  }

  /** Supported items only (HLS and files); this is the badge number. */
  count(tabId: number): number {
    return this.list(tabId).filter((s) => s.supported).length;
  }
}

/** The Referer and Origin the browser itself sent when it fetched a URL. */
export type SentHeaders = { referer?: string; origin?: string };

/**
 * Players embedded from another site (common on video and anime sites) fetch their streams with the
 * embed's address as Referer and Origin, and the video server refuses anything else. Remembering what
 * the browser sent lets ODM repeat it exactly.
 */
export class SentHeaderCache {
  private map = new Map<string, SentHeaders>();

  constructor(private readonly limit = 500) {}

  remember(url: string, headers: { name: string; value?: string }[]): void {
    const find = (n: string) => headers.find((h) => h.name.toLowerCase() === n)?.value || undefined;
    const sent = { referer: find("referer"), origin: find("origin") };
    if (!sent.referer && !sent.origin) return;
    const key = withoutFragment(url);
    this.map.delete(key);
    this.map.set(key, sent);
    if (this.map.size > this.limit) this.map.delete(this.map.keys().next().value as string);
  }

  get(url: string): SentHeaders | undefined {
    return this.map.get(withoutFragment(url));
  }
}

function withOrigin(headers: Record<string, string>, sent?: SentHeaders): Record<string, string> {
  return sent?.origin ? { ...headers, Origin: sent.origin } : headers;
}

export function toStreamPayload(
  s: Stream,
  pageUrl: string,
  pageTitle: string | undefined,
  cookies: Cookie[],
  userAgent: string,
  sent?: SentHeaders,
): AddStreamPayload {
  return {
    page_url: pageUrl,
    page_title: pageTitle?.trim() ? pageTitle : undefined,
    playlist_url: s.url,
    headers: withOrigin({ "User-Agent": userAgent, Referer: sent?.referer ?? pageUrl }, sent),
    cookies: cookieHeader(cookies),
  };
}

/** A plain video file as a menu-style download; the page is the referrer. */
export function toVideoFilePayload(
  s: Stream,
  pageUrl: string,
  cookies: Cookie[],
  userAgent: string,
  sent?: SentHeaders,
): AddDownloadPayload {
  let filename: string | undefined;
  try {
    filename = basename(decodeURIComponent(new URL(s.url).pathname));
  } catch {
    filename = undefined;
  }
  return {
    url: s.url,
    filename,
    referrer: sent?.referer ?? (pageUrl || undefined),
    headers: withOrigin({ "User-Agent": userAgent }, sent),
    cookies: cookieHeader(cookies),
    source: "menu",
  };
}
