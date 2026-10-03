import { AltTracker } from "./lib/alt";
import { ext, cookiesFor, send } from "./lib/browser";
import {
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
import { StreamStore, classify, toStreamPayload } from "./lib/streams";

const alt = new AltTracker();
// Lost when the service worker restarts; the badge resets with it.
const streams = new StreamStore();
const BADGE_COLOR = "#6366f1";
let badgeTimer: ReturnType<typeof setTimeout> | undefined;

async function flagError(message: string): Promise<void> {
  try {
    await ext.action.setBadgeText({ text: "!" });
    await ext.action.setBadgeBackgroundColor({ color: "#ef4444" });
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
    void Promise.resolve(ext.action.setBadgeBackgroundColor({ tabId, color: BADGE_COLOR })).catch(() => {});
  }
}

function clearStreams(tabId: number): void {
  streams.clear(tabId);
  updateStreamBadge(tabId);
}

type Reply = { ok: boolean; message: string };

/** Sends one listed stream to ODM. Only a click in the popup or the page button gets here. */
async function addStream(tabId: number | undefined, url: string | undefined, tab?: chrome.tabs.Tab): Promise<Reply> {
  const stream = tabId === undefined ? undefined : streams.list(tabId).find((s) => s.url === url && s.supported);
  if (tabId === undefined || !stream) return { ok: false, message: "That video is no longer listed. Reload the page." };
  let page = tab;
  if (!page) {
    try {
      page = await ext.tabs.get(tabId);
    } catch {
      return { ok: false, message: "The tab is gone." };
    }
  }
  const pageUrl = page.url ?? "";
  const storeId = (page as { cookieStoreId?: string }).cookieStoreId;
  const cookies = await cookiesFor(stream.url, storeId);
  const payload = toStreamPayload(stream, pageUrl, page.title, cookies, navigator.userAgent);
  const r = await send("ADD_STREAM", payload, 15000);
  return r?.ok ? { ok: true, message: "Sent to ODM." } : { ok: false, message: errorText(r) };
}

ext.runtime.onMessage.addListener((msg: unknown, sender: chrome.runtime.MessageSender, sendResponse: (r: unknown) => void) => {
  const m = msg as { type?: string; down?: boolean; tabId?: number; url?: string } | null;
  switch (m?.type) {
    case "odm-alt":
      if (m.down) alt.down(Date.now());
      else alt.up(Date.now());
      return false;
    case "odm-list-streams":
      sendResponse(streams.list(m.tabId ?? sender.tab?.id ?? -1));
      return false;
    case "odm-add-stream":
      // A content script is identified by its sender; the popup names the tab.
      void addStream(sender.tab?.id ?? m.tabId, m.url, sender.tab).then(sendResponse);
      return true;
    default:
      return false;
  }
});

ext.webRequest.onHeadersReceived.addListener(
  (details) => {
    if (details.tabId < 0) return;
    const type = details.responseHeaders?.find((h) => h.name.toLowerCase() === "content-type")?.value;
    const kind = classify(details.url, type);
    if (!kind || !streams.add(details.tabId, details.url, kind)) return;
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
  const r = await send("ADD_DOWNLOAD", payload);
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
  ext.downloads.onDeterminingFilename.addListener((item, suggest) => {
    void handle(item).finally(() => suggest());
    return true;
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
