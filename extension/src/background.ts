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

const alt = new AltTracker();
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

ext.runtime.onMessage.addListener((msg: unknown) => {
  const m = msg as { type?: string; down?: boolean } | null;
  if (m?.type === "odm-alt") {
    if (m.down) alt.down(Date.now());
    else alt.up(Date.now());
  }
});

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
