import { AltTracker } from "./lib/alt";
import { ext, cookiesFor, send, sendDetailed } from "./lib/browser";
import {
  ClaimSet,
  captureUrl,
  isHttp,
  shouldCancelBrowserDownload,
  shouldOffer,
  toCapturePayload,
  toMenuPayload,
  type BrowserItem,
} from "./lib/capture";
import { MENU_ID } from "./lib/ids";
import { errorText } from "./lib/protocol";
import { isSitePage, toPagePayload } from "./lib/sites";
import { BADGE, BADGE_ERROR } from "./lib/theme";
import { SentHeaderCache, StreamStore, classify, responseSize, toStreamPayload, toVideoFilePayload, type Stream } from "./lib/streams";

const alt = new AltTracker();
// Lost when the service worker restarts; the badge resets with it.
const streams = new StreamStore();
// Referer/Origin the browser sent for media requests, so ODM can repeat them (embedded players need this).
const sentHeaders = new SentHeaderCache(2000);
/** How long onCreated waits for onDeterminingFilename before taking the download itself. */
const CREATED_FALLBACK_MS = 500;
let badgeTimer: ReturnType<typeof setTimeout> | undefined;

async function flagError(message: string): Promise<void> {
  try {
    await ext.action.setBadgeText({ text: "!" });
    await ext.action.setBadgeBackgroundColor({ color: BADGE_ERROR.bg });
    await ext.action.setBadgeTextColor?.({ color: BADGE_ERROR.text });
    if (badgeTimer) clearTimeout(badgeTimer);
    badgeTimer = setTimeout(() => {
      void ext.action.setBadgeText({ text: "" });
    }, 5000);
    const area = ext.storage.session ?? ext.storage.local;
    await area.set({ lastError: { lastError: message, at: Date.now() } });
  } catch {
    // Reporting is best effort.
  }
}

function updateStreamBadge(tabId: number): void {
  const n = streams.count(tabId);
  void Promise.resolve(ext.action.setBadgeText({ tabId, text: n > 0 ? String(n) : "" })).catch(() => {});
  if (n > 0) {
    void Promise.resolve(ext.action.setBadgeBackgroundColor({ tabId, color: BADGE.bg })).catch(() => {});
    void Promise.resolve(ext.action.setBadgeTextColor?.({ tabId, color: BADGE.text })).catch(() => {});
  }
}

function clearStreams(tabId: number): void {
  streams.clear(tabId);
  updateStreamBadge(tabId);
}

type Reply = { ok: boolean; message: string };

async function pageOf(tabId: number, tab?: chrome.tabs.Tab): Promise<chrome.tabs.Tab | null> {
  if (tab) return tab;
  try {
    return await ext.tabs.get(tabId);
  } catch {
    return null;
  }
}

/** Sends one plain video file to ODM as a download. */
async function sendFile(stream: Stream, page: chrome.tabs.Tab): Promise<Reply> {
  const storeId = (page as { cookieStoreId?: string }).cookieStoreId;
  const cookies = await cookiesFor(stream.url, storeId);
  const payload = toVideoFilePayload(stream, page.url ?? "", cookies, navigator.userAgent, sentHeaders.get(stream.url));
  const r = await send("ADD_DOWNLOAD", payload, 15000);
  return r?.ok ? { ok: true, message: "Sent to ODM." } : { ok: false, message: errorText(r) };
}

/** Sends one listed stream to ODM. Only a click in the popup or the page button gets here. */
async function addStream(tabId: number | undefined, url: string | undefined, tab?: chrome.tabs.Tab): Promise<Reply> {
  const stream = tabId === undefined ? undefined : streams.list(tabId).find((s) => s.url === url && s.supported);
  if (tabId === undefined || !stream) return { ok: false, message: "That video is no longer listed. Reload the page." };
  const page = await pageOf(tabId, tab);
  if (!page) return { ok: false, message: "The tab is gone." };
  if (stream.kind === "file") return sendFile(stream, page);
  const pageUrl = page.url ?? "";
  const storeId = (page as { cookieStoreId?: string }).cookieStoreId;
  const cookies = await cookiesFor(stream.url, storeId);
  const payload = toStreamPayload(stream, pageUrl, page.title, cookies, navigator.userAgent, sentHeaders.get(stream.url));
  const r = await send("ADD_STREAM", payload, 15000);
  return r?.ok ? { ok: true, message: "Sent to ODM." } : { ok: false, message: errorText(r) };
}

/** Sends a site page (YouTube, Facebook…) to ODM, which downloads its video with yt-dlp. Only a click gets here. */
async function addPage(tabId: number | undefined, url: string | undefined, tab?: chrome.tabs.Tab): Promise<Reply> {
  if (!url || !isSitePage(url)) return { ok: false, message: "This page is not a supported video site." };
  const page = tabId === undefined ? tab : await pageOf(tabId, tab);
  const storeId = (page as { cookieStoreId?: string } | undefined)?.cookieStoreId;
  const cookies = await cookiesFor(url, storeId);
  const payload = toPagePayload(url, page?.title, cookies, navigator.userAgent);
  const r = await send("ADD_PAGE", payload, 15000);
  return r?.ok ? { ok: true, message: "Sent to ODM." } : { ok: false, message: errorText(r) };
}

const NOT_SEEN = "ODM hasn't seen this video's source yet. Play it for a few seconds and try again.";

/** The button on a video element: the video's own http(s) source, else the tab's best listed item. */
async function addVideo(tabId: number | undefined, src: string | undefined, tab?: chrome.tabs.Tab): Promise<Reply> {
  if (tabId === undefined) return { ok: false, message: NOT_SEEN };
  const page = await pageOf(tabId, tab);
  if (!page) return { ok: false, message: "The tab is gone." };
  if (src && isHttp(src)) {
    return sendFile({ url: src, kind: "file", label: src, supported: true }, page);
  }
  const supported = streams.list(tabId).filter((s) => s.supported);
  const hls = supported.find((s) => s.kind === "hls");
  const largest = supported
    .filter((s) => s.kind === "file")
    .reduce<Stream | undefined>((best, s) => (!best || (s.size ?? 0) > (best.size ?? 0) ? s : best), undefined);
  const pick = hls ?? largest;
  return pick ? addStream(tabId, pick.url, page) : { ok: false, message: NOT_SEEN };
}

ext.runtime.onMessage.addListener((msg: unknown, sender: chrome.runtime.MessageSender, sendResponse: (r: unknown) => void) => {
  const m = msg as { type?: string; down?: boolean; tabId?: number; url?: string; src?: string } | null;
  switch (m?.type) {
    case "odm-alt":
      if (m.down) alt.down(Date.now());
      else alt.up(Date.now());
      return false;
    case "odm-list-streams":
      sendResponse(streams.list(m.tabId ?? sender.tab?.id ?? -1));
      return false;
    case "odm-add-video":
      void addVideo(sender.tab?.id, m.src, sender.tab).then(sendResponse);
      return true;
    case "odm-add-page":
      // A content script is identified by its sender; the popup names the tab.
      void addPage(sender.tab?.id ?? m.tabId, m.url, sender.tab).then(sendResponse);
      return true;
    case "odm-add-stream":
      // A content script is identified by its sender; the popup names the tab.
      void addStream(sender.tab?.id ?? m.tabId, m.url, sender.tab).then(sendResponse);
      return true;
    default:
      return false;
  }
});

// Chromium only reveals Referer and Origin to extensions that ask for "extraHeaders"; Firefox rejects that flag.
ext.webRequest.onSendHeaders.addListener(
  (details) => {
    if (details.tabId >= 0 && details.requestHeaders) sentHeaders.remember(details.url, details.requestHeaders);
  },
  { urls: ["<all_urls>"], types: ["xmlhttprequest", "media", "other"] },
  (__TARGET__ === "chromium" ? ["requestHeaders", "extraHeaders"] : ["requestHeaders"]) as chrome.webRequest.OnSendHeadersOptions[],
);

ext.webRequest.onHeadersReceived.addListener(
  (details) => {
    if (details.tabId < 0) return;
    const type = details.responseHeaders?.find((h) => h.name.toLowerCase() === "content-type")?.value;
    const header = (name: string) => details.responseHeaders?.find((h) => h.name.toLowerCase() === name)?.value;
    const size = responseSize(header("content-length"), header("content-range"));
    const kind = classify(details.url, type, size);
    if (!kind || !streams.add(details.tabId, details.url, kind, size)) return;
    updateStreamBadge(details.tabId);
    void Promise.resolve(
      ext.tabs.sendMessage(details.tabId, { type: "odm-streams", streams: streams.list(details.tabId) }),
    ).catch(() => {});
  },
  { urls: ["<all_urls>"], types: ["xmlhttprequest", "media", "other"] },
  ["responseHeaders"],
);

// A new page starts a new list. A status-only update (loading finished) must not clear it.
ext.tabs.onUpdated.addListener((tabId, changeInfo) => {
  if (changeInfo.status === "loading" && changeInfo.url) clearStreams(tabId);
});
ext.tabs.onRemoved.addListener((tabId) => streams.clear(tabId));

type DownloadLike = chrome.downloads.DownloadItem & { cookieStoreId?: string };

async function handle(item: DownloadLike, storeId?: string): Promise<void> {
  const view: BrowserItem = item;
  if (!shouldOffer(view, alt.active(Date.now()))) return;
  const cookies = await cookiesFor(captureUrl(view), storeId);
  const payload = toCapturePayload(view, cookies, navigator.userAgent, Date.now());
  const { response: r, failure, detail } = await sendDetailed("ADD_DOWNLOAD", payload);
  if (!r) console.warn(`ODM did not answer (${failure}: ${detail}); the browser keeps the download.`);
  if (shouldCancelBrowserDownload(r)) {
    try {
      await ext.downloads.cancel(item.id);
    } catch {
      // The download may already be gone.
    }
    try {
      await ext.downloads.erase({ id: item.id });
    } catch {
      // Same.
    }
  } else if (r && !r.ok && r.error?.code !== "skipped") {
    await flagError(errorText(r));
  }
}

if (__TARGET__ === "chromium") {
  // Chrome normally reports a new download through onDeterminingFilename, with its final name and size.
  // Vivaldi (its own "Download?" dialog) and Chrome with "ask where to save" never fire it, so
  // onCreated is the fallback. It waits briefly so onDeterminingFilename wins when it does fire.
  const claims = new ClaimSet();
  ext.downloads.onDeterminingFilename.addListener((item, suggest) => {
    if (!claims.claim(item.id)) {
      suggest();
      return false;
    }
    void handle(item).finally(() => suggest());
    return true;
  });
  ext.downloads.onCreated.addListener((item: DownloadLike) => {
    setTimeout(() => {
      void (async () => {
        const [fresh] = await ext.downloads.search({ id: item.id });
        // A finished download can no longer be cancelled; handing it over would duplicate it.
        if (!fresh || fresh.state !== "in_progress" || !claims.claim(item.id)) return;
        await handle(fresh);
      })().catch(() => {});
    }, CREATED_FALLBACK_MS);
  });
} else {
  ext.downloads.onCreated.addListener((item: DownloadLike) => {
    void handle(item, item.cookieStoreId);
  });
}

ext.runtime.onInstalled.addListener(() => {
  ext.contextMenus.create({
    id: MENU_ID,
    title: "Download with ODM",
    contexts: ["link", "video", "audio"],
  });
});

ext.contextMenus.onClicked.addListener(async (info, tab) => {
  const url = info.linkUrl ?? info.srcUrl;
  if (!url || !isHttp(url)) return;
  const cookies = await cookiesFor(url);
  const payload = toMenuPayload(url, info.pageUrl ?? tab?.url, cookies, navigator.userAgent);
  // No browser download to fall back to, so a cold start of ODM may take a while.
  const r = await send("ADD_DOWNLOAD", payload, 15000);
  if (!r || !r.ok) await flagError(errorText(r));
});
