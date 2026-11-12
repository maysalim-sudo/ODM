import { cookieHeader, type Cookie } from "./capture";
import type { AddPagePayload, VideoFormat } from "./protocol";

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

/** One YouTube video: `/watch?v=`, `/shorts/…`, `/live/…`, or a youtu.be link. */
function isYoutubeVideo(u: URL): boolean {
  if (hostIs(u, "youtu.be")) return u.pathname.length > 1;
  if (!hostIs(u, "youtube.com")) return false;
  if (u.pathname === "/watch") return !!u.searchParams.get("v");
  return /^\/(shorts|live)\/[^/]+/.test(u.pathname);
}

/**
 * YouTube's home, search and channel pages play previews of many videos, and yt-dlp given such a
 * page downloads every video on it. So the preview's own link wins, then the page when it is one
 * video; otherwise there is nothing to send.
 */
function youtubeAddress(page: URL, candidateHrefs: string[]): string | null {
  for (const href of candidateHrefs) {
    const video = youtubeVideoLink(href, page.href);
    if (video) return video;
  }
  return isYoutubeVideo(page) ? page.href : null;
}

/**
 * A YouTube link (a thumbnail's, say) as its video's own address: `/watch?v=` with only the id, or
 * `/shorts/…`, `/live/…`, youtu.be. Null for anything that isn't one video, including a playlist or
 * Mix card. `base` resolves relative links.
 */
export function youtubeVideoLink(href: string, base: string): string | null {
  let u: URL;
  try {
    u = new URL(href, base);
  } catch {
    return null;
  }
  if (!href || !isYoutubeVideo(u)) return null;
  // A playlist or Mix card links to its first video with `list=`; a video inside a list has `index=`.
  if (u.searchParams.has("list") && !u.searchParams.has("index")) return null;
  if (u.pathname === "/watch") return `${u.origin}/watch?v=${u.searchParams.get("v")}`;
  return `${u.origin}${u.pathname}`;
}

/**
 * A thumbnail's link as the address of its one video, on the sites that get a ↓ on thumbnails:
 * YouTube (see `youtubeVideoLink`) and pornhub.com, whose video pages are
 * `/view_video.php?viewkey=…` (kept with only the key). Null for anything else.
 */
export function thumbnailVideoLink(href: string, base: string): string | null {
  const yt = youtubeVideoLink(href, base);
  if (yt) return yt;
  let u: URL;
  try {
    u = new URL(href, base);
  } catch {
    return null;
  }
  const key = u.searchParams.get("viewkey");
  if (!hostIs(u, "pornhub.com") || u.pathname !== "/view_video.php" || !key) return null;
  return `${u.origin}/view_video.php?viewkey=${encodeURIComponent(key)}`;
}

/**
 * The best page address for a video element on a site. On facebook.com a feed shows many videos on
 * one page, so the first `/videos/`, `/reel/` or `/watch/?v=` link of the video's post wins (made
 * absolute); on YouTube see `youtubeAddress`, which may give null; everywhere else, and when there
 * is no such link, it is the page address itself.
 * Pure: takes the candidate hrefs of the post and the page URL.
 */
export function pageAddress(pageUrl: string, candidateHrefs: string[]): string | null {
  const page = parse(pageUrl);
  if (page && (hostIs(page, "youtube.com") || hostIs(page, "youtu.be"))) return youtubeAddress(page, candidateHrefs);
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

export function toPagePayload(
  pageUrl: string,
  title: string | undefined,
  cookies: Cookie[],
  userAgent: string,
  format?: VideoFormat,
): AddPagePayload {
  return {
    page_url: pageUrl,
    page_title: title?.trim() ? title : undefined,
    headers: { "User-Agent": userAgent, Referer: pageUrl },
    cookies: cookieHeader(cookies),
    ...(format ? { format } : {}),
  };
}

/**
 * On a yt-dlp site the links the extension sees are signed for the page's own player and fail when
 * fetched separately (HTTP 403/410), so only the page download is offered there.
 */
export function visibleStreams<T>(pageUrl: string | undefined, streams: T[]): T[] {
  return pageUrl && isSitePage(pageUrl) ? [] : streams;
}
