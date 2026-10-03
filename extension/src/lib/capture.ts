import { APP_DEADLINE_MS } from "./ids";
import type { AddDownloadPayload, OdmResponse } from "./protocol";

export type BrowserItem = {
  url: string;
  finalUrl?: string;
  referrer?: string;
  filename?: string;
  totalBytes?: number;
  mime?: string;
};
export type Cookie = { name: string; value: string };

export function isHttp(url: string): boolean {
  return /^https?:/i.test(url);
}

export function basename(path: string | undefined): string | undefined {
  if (!path) return undefined;
  const name = path.slice(Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\")) + 1);
  return name === "" ? undefined : name;
}

export function cookieHeader(cookies: Cookie[]): string | undefined {
  if (cookies.length === 0) return undefined;
  return cookies.map((c) => `${c.name}=${c.value}`).join("; ");
}

export function captureUrl(item: BrowserItem): string {
  return item.finalUrl && isHttp(item.finalUrl) ? item.finalUrl : item.url;
}

export function shouldOffer(item: BrowserItem, altActive: boolean): boolean {
  return isHttp(captureUrl(item)) && !altActive;
}

export function toCapturePayload(
  item: BrowserItem,
  cookies: Cookie[],
  userAgent: string,
  now: number,
): AddDownloadPayload {
  return {
    url: captureUrl(item),
    filename: basename(item.filename),
    referrer: item.referrer || undefined,
    headers: { "User-Agent": userAgent },
    cookies: cookieHeader(cookies),
    filesize: item.totalBytes !== undefined && item.totalBytes > 0 ? item.totalBytes : undefined,
    mime: item.mime,
    source: "capture",
    expires_at_ms: now + APP_DEADLINE_MS,
  };
}

export function toMenuPayload(
  url: string,
  pageUrl: string | undefined,
  cookies: Cookie[],
  userAgent: string,
): AddDownloadPayload {
  return {
    url,
    referrer: pageUrl,
    headers: { "User-Agent": userAgent },
    cookies: cookieHeader(cookies),
    source: "menu",
  };
}

/** The browser download may be cancelled only when this is true. */
export function shouldCancelBrowserDownload(r: OdmResponse | null): boolean {
  return r?.ok === true;
}
