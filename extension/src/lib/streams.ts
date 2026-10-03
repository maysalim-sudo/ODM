import { cookieHeader, type Cookie } from "./capture";
import type { AddStreamPayload } from "./protocol";

export type StreamKind = "hls" | "dash";
export type Stream = { url: string; kind: StreamKind; label: string; supported: boolean };

const HLS_TYPES = ["application/vnd.apple.mpegurl", "application/x-mpegurl", "audio/mpegurl"];

function pathOf(url: string): string | null {
  try {
    return new URL(url).pathname.toLowerCase();
  } catch {
    return null;
  }
}

/** HLS or DASH by the URL's path (the query is ignored) or by the response content type. */
export function classify(url: string, contentType: string | undefined): StreamKind | null {
  const path = pathOf(url);
  if (path === null) return null;
  const type = (contentType ?? "").split(";")[0].trim().toLowerCase();
  if (path.endsWith(".m3u8") || HLS_TYPES.includes(type)) return "hls";
  if (path.endsWith(".mpd") || type === "application/dash+xml") return "dash";
  return null;
}

/** `<last path segment> · <host>`, for telling playlists apart in a list. */
export function streamLabel(url: string): string {
  try {
    const u = new URL(url);
    const last = decodeURIComponent(u.pathname.split("/").filter(Boolean).pop() ?? "");
    return last ? `${last} · ${u.host}` : u.host;
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
  add(tabId: number, url: string, kind: StreamKind): boolean {
    const key = withoutFragment(url);
    let tab = this.tabs.get(tabId);
    if (!tab) {
      tab = new Map();
      this.tabs.set(tabId, tab);
    }
    if (tab.has(key)) return false;
    tab.set(key, { url: key, kind, label: streamLabel(key), supported: kind === "hls" });
    return true;
  }

  list(tabId: number): Stream[] {
    return [...(this.tabs.get(tabId)?.values() ?? [])];
  }

  clear(tabId: number): void {
    this.tabs.delete(tabId);
  }

  /** Supported streams only; this is the badge number. */
  count(tabId: number): number {
    return this.list(tabId).filter((s) => s.supported).length;
  }
}

export function toStreamPayload(
  s: Stream,
  pageUrl: string,
  pageTitle: string | undefined,
  cookies: Cookie[],
  userAgent: string,
): AddStreamPayload {
  return {
    page_url: pageUrl,
    page_title: pageTitle?.trim() ? pageTitle : undefined,
    playlist_url: s.url,
    headers: { "User-Agent": userAgent, Referer: pageUrl },
    cookies: cookieHeader(cookies),
  };
}
