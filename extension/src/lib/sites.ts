import { cookieHeader, type Cookie } from "./capture";
import type { AddPagePayload } from "./protocol";

/** Sites whose pages ODM hands to yt-dlp. Keep in step with `SITE_HOSTS` in the app's engine. */
export const SITE_HOSTS: string[] = [
  "youtube.com",
  "youtu.be",
  "facebook.com",
  "fb.watch",
  "instagram.com",
  "x.com",
  "twitter.com",
  "tiktok.com",
  "vimeo.com",
  "dailymotion.com",
  "twitch.tv",
  "reddit.com",
  // Adult video sites with yt-dlp extractors; their direct video links are signed per player session.
  "pornhub.com",
  "xvideos.com",
  "xhamster.com",
  "xnxx.com",
  "redtube.com",
  "youporn.com",
  "spankbang.com",
  "eporner.com",
];

function hostIs(url: URL, host: string): boolean {
  const h = url.hostname.toLowerCase();
  return h === host || h.endsWith(`.${host}`);
}

function parse(url: string): URL | null {
  try {
    const u = new URL(url);
    return u.protocol === "http:" || u.protocol === "https:" ? u : null;
  } catch {
    return null;
  }
}

/** True for an http(s) page whose host is a listed site or a subdomain of one. */
export function isSitePage(url: string): boolean {
  const u = parse(url);
  return u !== null && SITE_HOSTS.some((h) => hostIs(u, h));
}

const FACEBOOK_VIDEO_PATHS = ["/videos/", "/reel/", "/watch/?v="];

/**
 * The best page address for a video element on a site. On facebook.com a feed shows many videos on
 * one page, so the first `/videos/`, `/reel/` or `/watch/?v=` link of the video's post wins (made
 * absolute); everywhere else, and when there is no such link, it is the page address itself.
 * Pure: takes the candidate hrefs of the post and the page URL.
 */
export function pageAddress(pageUrl: string, candidateHrefs: string[]): string {
  const page = parse(pageUrl);
  if (!page || !hostIs(page, "facebook.com")) return pageUrl;
  for (const href of candidateHrefs) {
    let absolute: URL;
    try {
      absolute = new URL(href, page);
    } catch {
      continue;
    }
    if (!hostIs(absolute, "facebook.com")) continue;
    const pathAndQuery = absolute.pathname + absolute.search;
    if (FACEBOOK_VIDEO_PATHS.some((p) => pathAndQuery.includes(p))) return absolute.href;
  }
  return pageUrl;
}

export function toPagePayload(pageUrl: string, title: string | undefined, cookies: Cookie[], userAgent: string): AddPagePayload {
  return {
    page_url: pageUrl,
    page_title: title?.trim() ? title : undefined,
    headers: { "User-Agent": userAgent, Referer: pageUrl },
    cookies: cookieHeader(cookies),
  };
}

/**
 * On a yt-dlp site the links the extension sees are signed for the page's own player and fail when
 * fetched separately (HTTP 403/410), so only the page download is offered there.
 */
export function visibleStreams<T>(pageUrl: string | undefined, streams: T[]): T[] {
  return pageUrl && isSitePage(pageUrl) ? [] : streams;
}
