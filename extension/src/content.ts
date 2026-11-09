import { BAR_DEFAULT, BAR_KEY, clampBar, nudgeBar, readBar } from "./lib/barPosition";
import type { VideoFormat } from "./lib/protocol";
import { isSitePage, pageAddress, visibleStreams, youtubeVideoLink } from "./lib/sites";
import { videoLabel, type StreamKind } from "./lib/streams";
import { THEME } from "./lib/theme";
import { isMagnet } from "./lib/torrent";

function notify(down: boolean): void {
  try {
    const p = chrome.runtime.sendMessage({ type: "odm-alt", down });
    // The promise rejects when the extension was reloaded or no listener answers.
    if (p && typeof p.catch === "function") p.catch(() => {});
  } catch {
    // Extension context invalidated after a reload.
  }
}

window.addEventListener(
  "keydown",
  (e) => {
    if (e.key === "Alt") notify(true);
  },
  true,
);
window.addEventListener(
  "keyup",
  (e) => {
    if (e.key === "Alt") notify(false);
  },
  true,
);
window.addEventListener("blur", () => notify(false), true);

// Magnet links open in ODM. With Alt held, or when ODM does not take it, the browser
// handles the link as it would without the extension.
document.addEventListener(
  "click",
  (e) => {
    if (e.defaultPrevented || e.button !== 0 || e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
    const a = (e.target as Element | null)?.closest?.("a[href]") as HTMLAnchorElement | null;
    if (!a || !isMagnet(a.href)) return;
    e.preventDefault();
    const href = a.href;
    const fallBack = () => {
      location.href = href;
    };
    try {
      const p = chrome.runtime.sendMessage({ type: "odm-magnet", url: href }) as Promise<{ ok?: boolean } | undefined>;
      void p.then((r) => (r?.ok ? undefined : fallBack())).catch(fallBack);
    } catch {
      fallBack();
    }
  },
  true,
);

// Video detection can be turned off per site or everywhere (popup). Off hides both on-page buttons.
// No answer from the background counts as on. Each block below registers how it reacts.
let detecting = true;
const detectionHooks: Array<(on: boolean) => void> = [];

// In-page "ODM · N videos" button. Nothing is shown unless the background reports a supported stream,
// and nothing is sent until the user clicks Download.
type PageStream = { url: string; kind: StreamKind; label: string; supported: boolean };
type AddReply = { ok: boolean; message: string };

const SHADOW_CSS = `
  :host { all: initial; }
  :host([hidden]) { display: none !important; }
  .box { position: fixed; right: 16px; bottom: 16px; z-index: 2147483647; display: flex; flex-direction: column;
    align-items: flex-end; gap: 8px; font: 13px/1.4 system-ui, sans-serif; color: ${THEME.text}; }
  .bar { display: flex; border: 1px solid ${THEME.accent}; border-radius: 8px; background: ${THEME.surface}; overflow: hidden; }
  button { font: inherit; color: inherit; background: none; border: 0; padding: 7px 10px; cursor: pointer; }
  button:hover { background: ${THEME.surface2}; }
  .main { font-weight: 700; }
  .move { padding: 7px 8px; border-right: 1px solid ${THEME.border}; color: ${THEME.muted}; cursor: grab; touch-action: none; }
  .move:active { cursor: grabbing; }
  .close { border-left: 1px solid ${THEME.border}; color: ${THEME.muted}; }
  .list { width: 300px; max-height: 260px; overflow: auto; padding: 8px; border: 1px solid ${THEME.border}; border-radius: 8px;
    background: ${THEME.bg}; display: flex; flex-direction: column; gap: 8px; }
  .item { display: flex; flex-direction: column; gap: 4px; }
  .row { display: flex; align-items: center; gap: 8px; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dl { border: 1px solid ${THEME.accent}; border-radius: 4px; background: ${THEME.accent}; color: ${THEME.onAccent}; font-weight: 700; }
  .dl:hover { background: ${THEME.accentLight}; border-color: ${THEME.accentLight}; }
  .dl.alt { border-color: ${THEME.border}; background: ${THEME.surface2}; color: ${THEME.text}; font-weight: 600; }
  .dl.alt:hover { background: ${THEME.surface}; border-color: ${THEME.accent}; }
  .msg { color: ${THEME.muted}; font-size: 12px; overflow-wrap: anywhere; }
  [hidden] { display: none !important; }
`;

// Only the top frame shows the button, although every frame receives the message.
if (window.top === window) {
  let streams: PageStream[] = [];
  let dismissed = false;
  let open = false;
  const messages = new Map<string, string>();
  let ui: { host: HTMLElement; box: HTMLElement; main: HTMLButtonElement; list: HTMLElement } | null = null;
  let spot = BAR_DEFAULT;

  const add = (parent: Node, tag: string, cls?: string, text?: string): HTMLElement => {
    const el = document.createElement(tag);
    if (cls) el.className = cls;
    if (text !== undefined) el.textContent = text;
    parent.appendChild(el);
    return el;
  };

  const askToAdd = async (url: string, format: VideoFormat): Promise<AddReply> => {
    try {
      const r = (await chrome.runtime.sendMessage({ type: "odm-add-stream", url, format })) as AddReply | undefined;
      return r ?? { ok: false, message: "ODM did not answer." };
    } catch {
      return { ok: false, message: "ODM did not answer." };
    }
  };

  const windowSize = () => ({ width: document.documentElement.clientWidth, height: document.documentElement.clientHeight });

  /** Shows the bar at its place, kept in view; the saved place stays as the user chose it. */
  const placeBar = () => {
    if (!ui) return;
    const shown = clampBar(spot, ui.box.getBoundingClientRect(), windowSize());
    ui.box.style.right = `${shown.right}px`;
    ui.box.style.bottom = `${shown.bottom}px`;
  };

  const saveSpot = () => {
    try {
      void Promise.resolve(chrome.storage.local.set({ [BAR_KEY]: spot })).catch(() => {});
    } catch {
      // Extension context invalidated after a reload.
    }
  };

  const makeMovable = (handle: HTMLButtonElement, box: HTMLElement) => {
    let start: { x: number; y: number; right: number; bottom: number } | null = null;
    handle.addEventListener("pointerdown", (e) => {
      handle.setPointerCapture(e.pointerId);
      start = { x: e.clientX, y: e.clientY, ...spot };
    });
    handle.addEventListener("pointermove", (e) => {
      if (!start) return;
      spot = clampBar({ right: start.right - (e.clientX - start.x), bottom: start.bottom - (e.clientY - start.y) }, box.getBoundingClientRect(), windowSize());
      placeBar();
    });
    const drop = () => {
      if (start) saveSpot();
      start = null;
    };
    handle.addEventListener("pointerup", drop);
    handle.addEventListener("pointercancel", drop);
    handle.addEventListener("keydown", (e) => {
      const next = nudgeBar(spot, e.key);
      if (!next) return;
      e.preventDefault();
      spot = clampBar(next, box.getBoundingClientRect(), windowSize());
      placeBar();
      saveSpot();
    });
  };

  const build = () => {
    const host = document.createElement("div");
    const root = host.attachShadow({ mode: "closed" });
    add(root, "style", undefined, SHADOW_CSS);
    const box = add(root, "div", "box");
    const list = add(box, "div", "list");
    list.hidden = true;
    const bar = add(box, "div", "bar");
    const move = add(bar, "button", "move", "⋮⋮") as HTMLButtonElement;
    move.title = "Drag to move";
    move.setAttribute("aria-label", "Move the ODM bar. Use the arrow keys.");
    makeMovable(move, box);
    const main = add(bar, "button", "main") as HTMLButtonElement;
    const close = add(bar, "button", "close", "×") as HTMLButtonElement;
    close.title = "Hide for this page";
    close.setAttribute("aria-label", "Hide for this page");
    main.addEventListener("click", () => {
      open = !open;
      render();
    });
    close.addEventListener("click", () => {
      dismissed = true;
      render();
    });
    document.documentElement.appendChild(host);
    ui = { host, box, main, list };
  };

  const render = () => {
    const supported = streams.filter((s) => s.supported);
    if (!detecting || dismissed || supported.length === 0) {
      if (ui) ui.host.hidden = true;
      return;
    }
    if (!ui) build();
    if (!ui) return;
    ui.host.hidden = false;
    ui.main.textContent = `ODM · ${supported.length} ${supported.length === 1 ? "video" : "videos"}`;
    ui.list.hidden = !open;
    ui.list.textContent = "";
    for (const s of supported) {
      const item = add(ui.list, "div", "item");
      const row = add(item, "div", "row");
      const name = add(row, "span", "name", s.label);
      name.title = s.url;
      const choices: [VideoFormat, string, string][] = [
        ["video", "dl", videoLabel(s.kind, s.url)],
        ["mp3", "dl alt", "MP3"],
      ];
      for (const [format, cls, text] of choices) {
        const dl = add(row, "button", cls, text) as HTMLButtonElement;
        dl.title = `Save with ODM as ${text}`;
        dl.disabled = messages.get(s.url) === "Sending...";
        dl.addEventListener("click", async () => {
          messages.set(s.url, "Sending...");
          render();
          const r = await askToAdd(s.url, format);
          messages.set(s.url, r.message);
          render();
        });
      }
      const text = messages.get(s.url);
      if (text) add(item, "div", "msg", text);
    }
    placeBar();
  };

  window.addEventListener("resize", placeBar);
  try {
    void Promise.resolve(chrome.storage.local.get(BAR_KEY))
      .then((stored) => {
        spot = readBar((stored as Record<string, unknown>)[BAR_KEY]);
        placeBar();
      })
      .catch(() => {});
  } catch {
    // Extension context invalidated after a reload.
  }

  detectionHooks.push((on) => {
    if (!on) streams = [];
    render();
  });

  try {
    chrome.runtime.onMessage.addListener((msg: unknown) => {
      const m = msg as { type?: string; streams?: PageStream[] } | null;
      if (m?.type === "odm-streams" && Array.isArray(m.streams)) {
        streams = visibleStreams(location.href, m.streams);
        render();
      }
    });
  } catch {
    // Extension context invalidated after a reload.
  }
}

// Hover button on <video> elements, in every frame because embedded players often live in iframes.
// Nothing is sent until the user clicks it.
const VIDEO_CSS = `
  :host { all: initial; }
  :host([hidden]) { display: none !important; }
  .wrap { position: fixed; z-index: 2147483647; display: flex; flex-direction: column; align-items: flex-end; gap: 4px;
    font: 13px/1.4 system-ui, sans-serif; color: ${THEME.text}; }
  button { font: inherit; color: inherit; background: ${THEME.surface}; border: 1px solid ${THEME.accent}; border-radius: 8px;
    padding: 7px 10px; cursor: pointer; font-weight: 700; }
  button:hover { background: ${THEME.surface2}; }
  button:disabled { cursor: default; opacity: 0.7; }
  .row { display: flex; gap: 4px; }
  .alt { border-color: ${THEME.border}; }
  .x { color: ${THEME.muted}; font-weight: 400; }
  .msg { max-width: 260px; padding: 6px 8px; border: 1px solid ${THEME.border}; border-radius: 8px; background: ${THEME.bg};
    color: ${THEME.text}; font-size: 12px; overflow-wrap: anywhere; }
  [hidden] { display: none !important; }
`;
const MIN_VIDEO_WIDTH = 200;
const MIN_VIDEO_HEIGHT = 120;
const HIDE_DELAY_MS = 1500;
const MESSAGE_MS = 4000;
const MIN_THUMB_WIDTH = 120;

const onYoutube = /(^|\.)(youtube\.com|youtu\.be)$/i.test(location.hostname);

/**
 * On YouTube, the link of the thumbnail (or hover preview) `el` sits in, when that link is one
 * video. Found by what it is, not by YouTube's card names, which change: a link to one video
 * that holds an image or a playing preview.
 */
function thumbnailLink(el: Element | null): HTMLAnchorElement | null {
  if (!onYoutube || !el) return null;
  const a = el.closest("a[href]") as HTMLAnchorElement | null;
  if (!a || !youtubeVideoLink(a.href, location.href)) return null;
  if (!a.querySelector("img, video, yt-image, yt-thumbnail-view-model")) return null;
  return a.getBoundingClientRect().width >= MIN_THUMB_WIDTH ? a : null;
}

/** A hover preview's <video> belongs to a thumbnail; its link is in the preview's own box. */
function previewLink(v: HTMLVideoElement): HTMLAnchorElement | null {
  if (!onYoutube) return null;
  const box = v.closest("ytd-video-preview");
  const a = box?.querySelector('a[href*="/watch?v="], a[href*="/shorts/"]') ?? null;
  return thumbnailLink(a ?? v);
}

{
  const watched = new WeakSet<HTMLVideoElement>();
  let current: HTMLVideoElement | null = null;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  let messageTimer: ReturnType<typeof setTimeout> | undefined;
  let sending = false;
  let hiddenHere = false;
  let ui: { host: HTMLElement; wrap: HTMLElement; button: HTMLButtonElement; mp3: HTMLButtonElement; msg: HTMLElement } | null = null;

  /** The links of the post (feed item) that holds the video, for finding the video's own page. */
  // A Facebook post, or the YouTube card or hover preview the video plays in. YouTube's main
  // player is in none of these, so the watch page itself is used there.
  const postLinks = (v: HTMLVideoElement): string[] => {
    const post = v.closest(
      '[role="article"], ytd-video-preview, ytd-rich-item-renderer, ytd-video-renderer, ytd-compact-video-renderer, ytd-reel-item-renderer',
    );
    return post ? Array.from(post.querySelectorAll("a[href]"), (a) => (a as HTMLAnchorElement).href) : [];
  };

  const bigEnough = (v: HTMLVideoElement): boolean => {
    const r = v.getBoundingClientRect();
    return r.width >= MIN_VIDEO_WIDTH && r.height >= MIN_VIDEO_HEIGHT;
  };

  const place = () => {
    if (!ui || !current) return;
    const r = current.getBoundingClientRect();
    ui.wrap.style.top = `${Math.max(8, r.top + 8)}px`;
    ui.wrap.style.right = `${Math.max(8, window.innerWidth - Math.min(r.right, window.innerWidth) + 8)}px`;
  };

  const hide = () => {
    clearTimeout(hideTimer);
    if (sending || !ui) return;
    ui.host.hidden = true;
    ui.msg.hidden = true;
    current = null;
  };
  const scheduleHide = () => {
    clearTimeout(hideTimer);
    hideTimer = setTimeout(hide, HIDE_DELAY_MS);
  };

  const build = () => {
    const host = document.createElement("div");
    const root = host.attachShadow({ mode: "closed" });
    const style = document.createElement("style");
    style.textContent = VIDEO_CSS;
    root.appendChild(style);
    const wrap = document.createElement("div");
    wrap.className = "wrap";
    const button = document.createElement("button");
    button.type = "button";
    const mp3 = document.createElement("button");
    mp3.type = "button";
    mp3.className = "alt";
    mp3.textContent = "MP3";
    mp3.title = "Save with ODM as MP3";
    const msg = document.createElement("div");
    msg.className = "msg";
    msg.hidden = true;
    const close = document.createElement("button");
    close.type = "button";
    close.className = "x";
    close.textContent = "×";
    close.title = "Hide for this page";
    close.setAttribute("aria-label", "Hide for this page");
    close.addEventListener("click", () => {
      hiddenHere = true;
      hide();
    });
    const row = document.createElement("div");
    row.className = "row";
    row.append(button, mp3, close);
    wrap.append(row, msg);
    root.appendChild(wrap);
    host.addEventListener("mouseenter", () => clearTimeout(hideTimer));
    host.addEventListener("mouseleave", scheduleHide);
    const send = async (format: VideoFormat) => {
      const video = current;
      if (!video || sending) return;
      sending = true;
      button.disabled = true;
      mp3.disabled = true;
      clearTimeout(messageTimer);
      msg.textContent = "Sending…";
      msg.hidden = false;
      let reply: AddReply;
      try {
        // On a video site the page is the thing to download (yt-dlp finds the media); elsewhere the file or stream.
        const page = isSitePage(location.href) ? pageAddress(location.href, postLinks(video)) : undefined;
        if (page === null) {
          reply = { ok: false, message: "Open the video first, then try again." };
        } else {
          const message = page
            ? { type: "odm-add-page", url: page, format }
            : { type: "odm-add-video", src: video.currentSrc, format };
          const r = (await chrome.runtime.sendMessage(message)) as AddReply | undefined;
          reply = r ?? { ok: false, message: "ODM did not answer." };
        }
      } catch {
        reply = { ok: false, message: "ODM did not answer." };
      }
      sending = false;
      button.disabled = false;
      mp3.disabled = false;
      msg.textContent = reply.message;
      messageTimer = setTimeout(() => {
        msg.hidden = true;
      }, MESSAGE_MS);
    };
    button.addEventListener("click", () => void send("video"));
    mp3.addEventListener("click", () => void send("mp3"));
    document.documentElement.appendChild(host);
    ui = { host, wrap, button, mp3, msg };
  };

  const show = (video: HTMLVideoElement) => {
    if (!detecting || hiddenHere) return;
    clearTimeout(hideTimer);
    if (document.fullscreenElement || !bigEnough(video)) return;
    // A YouTube thumbnail's preview gets the thumbnail's ↓ instead (below).
    if (previewLink(video)) return;
    if (!ui) build();
    if (!ui) return;
    current = video;
    const kind = isSitePage(location.href) ? "page" : /^https?:/i.test(video.currentSrc) ? "file" : "hls";
    const label = videoLabel(kind, video.currentSrc);
    ui.button.textContent = `↓ ${label}`;
    ui.button.title = `Save with ODM as ${label}`;
    ui.host.hidden = false;
    place();
  };

  const watch = (video: HTMLVideoElement) => {
    if (watched.has(video)) return;
    watched.add(video);
    video.addEventListener("mouseenter", () => show(video));
    video.addEventListener("mousemove", () => show(video));
    video.addEventListener("mouseleave", scheduleHide);
  };

  const scan = (root: ParentNode) => {
    if (root instanceof HTMLVideoElement) watch(root);
    root.querySelectorAll("video").forEach(watch);
  };

  scan(document);
  new MutationObserver((records) => {
    for (const rec of records) {
      rec.addedNodes.forEach((n) => {
        if (n instanceof Element) scan(n);
      });
    }
  }).observe(document.documentElement, { childList: true, subtree: true });

  window.addEventListener(
    "scroll",
    () => {
      if (!sending) hide();
    },
    true,
  );
  document.addEventListener("fullscreenchange", () => {
    if (document.fullscreenElement) hide();
  });
  detectionHooks.push((on) => {
    if (!on) hide();
  });
}

// ↓ on YouTube thumbnails (home, search, channels, playlists, the list beside a video, Shorts):
// one small button that opens Video or MP3. Nothing is sent until one is picked.
const THUMB_CSS = `
  :host { all: initial; }
  :host([hidden]) { display: none !important; }
  .wrap { position: fixed; z-index: 2147483647; display: flex; flex-direction: column; align-items: flex-start; gap: 4px;
    font: 13px/1.4 system-ui, sans-serif; color: ${THEME.text}; }
  button { font: inherit; color: inherit; background: ${THEME.surface}; border: 1px solid ${THEME.border}; border-radius: 8px;
    cursor: pointer; font-weight: 700; }
  button:hover { background: ${THEME.surface2}; }
  button:disabled { cursor: default; opacity: 0.7; }
  .down { width: 32px; height: 32px; padding: 0; border-color: ${THEME.accent}; font-size: 16px; line-height: 1; }
  .menu { display: flex; flex-direction: column; gap: 2px; padding: 4px; border: 1px solid ${THEME.border}; border-radius: 10px;
    background: ${THEME.bg}; }
  .menu button { padding: 6px 12px; border-color: transparent; background: transparent; text-align: left; font-weight: 600; }
  .menu button:hover { background: ${THEME.surface2}; }
  .msg { max-width: 220px; padding: 6px 8px; border: 1px solid ${THEME.border}; border-radius: 8px; background: ${THEME.bg};
    font-size: 12px; overflow-wrap: anywhere; }
  [hidden] { display: none !important; }
`;

if (onYoutube) {
  let current: HTMLAnchorElement | null = null;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  let messageTimer: ReturnType<typeof setTimeout> | undefined;
  let sending = false;
  let ui: { host: HTMLElement; wrap: HTMLElement; down: HTMLButtonElement; menu: HTMLElement; msg: HTMLElement } | null = null;

  // Top-left: YouTube puts Watch later and Add to queue at the top-right.
  const place = () => {
    if (!ui || !current) return;
    const r = current.getBoundingClientRect();
    ui.wrap.style.top = `${Math.max(8, r.top + 6)}px`;
    ui.wrap.style.left = `${Math.max(8, r.left + 6)}px`;
  };

  const hide = () => {
    clearTimeout(hideTimer);
    if (sending || !ui) return;
    ui.host.hidden = true;
    ui.menu.hidden = true;
    ui.msg.hidden = true;
    current = null;
  };
  const scheduleHide = () => {
    clearTimeout(hideTimer);
    hideTimer = setTimeout(hide, HIDE_DELAY_MS);
  };

  const build = () => {
    const host = document.createElement("div");
    const root = host.attachShadow({ mode: "closed" });
    const style = document.createElement("style");
    style.textContent = THUMB_CSS;
    root.appendChild(style);
    const wrap = document.createElement("div");
    wrap.className = "wrap";
    const down = document.createElement("button");
    down.type = "button";
    down.className = "down";
    down.textContent = "↓";
    down.title = "Save with ODM";
    down.setAttribute("aria-label", "Save with ODM");
    down.setAttribute("aria-haspopup", "menu");
    const menu = document.createElement("div");
    menu.className = "menu";
    menu.setAttribute("role", "menu");
    menu.hidden = true;
    const msg = document.createElement("div");
    msg.className = "msg";
    msg.hidden = true;
    const send = async (format: VideoFormat) => {
      const url = current ? youtubeVideoLink(current.href, location.href) : null;
      if (!url || sending) return;
      sending = true;
      menu.hidden = true;
      down.disabled = true;
      clearTimeout(messageTimer);
      msg.textContent = "Sending…";
      msg.hidden = false;
      let reply: AddReply;
      try {
        const r = (await chrome.runtime.sendMessage({ type: "odm-add-page", url, format })) as AddReply | undefined;
        reply = r ?? { ok: false, message: "ODM did not answer." };
      } catch {
        reply = { ok: false, message: "ODM did not answer." };
      }
      sending = false;
      down.disabled = false;
      msg.textContent = reply.message;
      messageTimer = setTimeout(() => {
        msg.hidden = true;
      }, MESSAGE_MS);
    };
    for (const [label, format] of [
      ["Video", "video"],
      ["MP3", "mp3"],
    ] as const) {
      const b = document.createElement("button");
      b.type = "button";
      b.setAttribute("role", "menuitem");
      b.textContent = label;
      b.addEventListener("click", () => void send(format));
      menu.appendChild(b);
    }
    down.addEventListener("click", () => {
      menu.hidden = !menu.hidden;
      msg.hidden = true;
    });
    wrap.append(down, menu, msg);
    root.appendChild(wrap);
    host.addEventListener("mouseenter", () => clearTimeout(hideTimer));
    host.addEventListener("mouseleave", scheduleHide);
    document.documentElement.appendChild(host);
    ui = { host, wrap, down, menu, msg };
  };

  const show = (a: HTMLAnchorElement) => {
    if (!detecting || document.fullscreenElement) return;
    clearTimeout(hideTimer);
    if (!ui) build();
    if (!ui) return;
    if (current !== a && !sending) {
      ui.menu.hidden = true;
      ui.msg.hidden = true;
    }
    if (!sending) current = a;
    ui.host.hidden = false;
    place();
  };

  document.addEventListener(
    "mouseover",
    (e) => {
      const a = thumbnailLink(e.target as Element | null);
      if (a) show(a);
    },
    true,
  );
  document.addEventListener(
    "mouseout",
    (e) => {
      if (current && thumbnailLink(e.target as Element | null) === current) scheduleHide();
    },
    true,
  );
  window.addEventListener(
    "scroll",
    () => {
      if (!sending) hide();
    },
    true,
  );
  detectionHooks.push((on) => {
    if (!on) hide();
  });
}

const setDetecting = (on: boolean): void => {
  detecting = on;
  for (const hook of detectionHooks) hook(on);
};

try {
  chrome.runtime.onMessage.addListener((msg: unknown) => {
    const m = msg as { type?: string; on?: boolean } | null;
    if (m?.type === "odm-detection") setDetecting(m.on !== false);
  });
  const asked = chrome.runtime.sendMessage({ type: "odm-detection-state" }) as Promise<{ on?: boolean } | undefined> | undefined;
  asked?.then((r) => setDetecting(r?.on !== false), () => {});
} catch {
  // Extension context invalidated after a reload.
}
