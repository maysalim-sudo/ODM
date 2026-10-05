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

// In-page "ODM · N videos" button. Nothing is shown unless the background reports a supported stream,
// and nothing is sent until the user clicks Download.
type PageStream = { url: string; label: string; supported: boolean };
type AddReply = { ok: boolean; message: string };

const SHADOW_CSS = `
  :host { all: initial; }
  .box { position: fixed; right: 16px; bottom: 16px; z-index: 2147483647; display: flex; flex-direction: column;
    align-items: flex-end; gap: 8px; font: 13px/1.4 system-ui, sans-serif; color: #e0e0e6; }
  .bar { display: flex; border: 1px solid #6366f1; border-radius: 8px; background: #14141c; overflow: hidden; }
  button { font: inherit; color: inherit; background: none; border: 0; padding: 7px 10px; cursor: pointer; }
  button:hover { background: #1f1f2b; }
  .main { font-weight: 600; }
  .close { border-left: 1px solid #2a2a3a; color: #6b6b80; }
  .list { width: 300px; max-height: 260px; overflow: auto; padding: 8px; border: 1px solid #2a2a3a; border-radius: 8px;
    background: #0f0f14; display: flex; flex-direction: column; gap: 8px; }
  .item { display: flex; flex-direction: column; gap: 4px; }
  .row { display: flex; align-items: center; gap: 8px; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dl { border: 1px solid #6366f1; border-radius: 6px; background: #6366f1; color: #fff; }
  .dl:hover { background: #7578f3; }
  .msg { color: #6b6b80; font-size: 12px; overflow-wrap: anywhere; }
  [hidden] { display: none !important; }
`;

// Only the top frame shows the button, although every frame receives the message.
if (window.top === window) {
  let streams: PageStream[] = [];
  let dismissed = false;
  let open = false;
  const messages = new Map<string, string>();
  let ui: { host: HTMLElement; main: HTMLButtonElement; list: HTMLElement } | null = null;

  const add = (parent: Node, tag: string, cls?: string, text?: string): HTMLElement => {
    const el = document.createElement(tag);
    if (cls) el.className = cls;
    if (text !== undefined) el.textContent = text;
    parent.appendChild(el);
    return el;
  };

  const askToAdd = async (url: string): Promise<AddReply> => {
    try {
      const r = (await chrome.runtime.sendMessage({ type: "odm-add-stream", url })) as AddReply | undefined;
      return r ?? { ok: false, message: "ODM did not answer." };
    } catch {
      return { ok: false, message: "ODM did not answer." };
    }
  };

  const build = () => {
    const host = document.createElement("div");
    const root = host.attachShadow({ mode: "closed" });
    add(root, "style", undefined, SHADOW_CSS);
    const box = add(root, "div", "box");
    const list = add(box, "div", "list");
    list.hidden = true;
    const bar = add(box, "div", "bar");
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
    ui = { host, main, list };
  };

  const render = () => {
    const supported = streams.filter((s) => s.supported);
    if (dismissed || supported.length === 0) {
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
      const dl = add(row, "button", "dl", "Download") as HTMLButtonElement;
      dl.addEventListener("click", async () => {
        dl.disabled = true;
        messages.set(s.url, "Sending...");
        render();
        const r = await askToAdd(s.url);
        messages.set(s.url, r.message);
        render();
      });
      const text = messages.get(s.url);
      if (text) add(item, "div", "msg", text);
    }
  };

  try {
    chrome.runtime.onMessage.addListener((msg: unknown) => {
      const m = msg as { type?: string; streams?: PageStream[] } | null;
      if (m?.type === "odm-streams" && Array.isArray(m.streams)) {
        streams = m.streams;
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
  .wrap { position: fixed; z-index: 2147483647; display: flex; flex-direction: column; align-items: flex-end; gap: 4px;
    font: 13px/1.4 system-ui, sans-serif; color: #e0e0e6; }
  button { font: inherit; color: inherit; background: #14141c; border: 1px solid #6366f1; border-radius: 8px;
    padding: 7px 10px; cursor: pointer; font-weight: 600; }
  button:hover { background: #1f1f2b; }
  button:disabled { cursor: default; opacity: 0.7; }
  .msg { max-width: 260px; padding: 6px 8px; border: 1px solid #2a2a3a; border-radius: 8px; background: #0f0f14;
    color: #e0e0e6; font-size: 12px; overflow-wrap: anywhere; }
  [hidden] { display: none !important; }
`;
const MIN_VIDEO_WIDTH = 200;
const MIN_VIDEO_HEIGHT = 120;
const HIDE_DELAY_MS = 1500;
const MESSAGE_MS = 4000;

{
  const watched = new WeakSet<HTMLVideoElement>();
  let current: HTMLVideoElement | null = null;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  let messageTimer: ReturnType<typeof setTimeout> | undefined;
  let sending = false;
  let ui: { host: HTMLElement; wrap: HTMLElement; button: HTMLButtonElement; msg: HTMLElement } | null = null;

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
    button.textContent = "Download with ODM";
    const msg = document.createElement("div");
    msg.className = "msg";
    msg.hidden = true;
    wrap.append(button, msg);
    root.appendChild(wrap);
    host.addEventListener("mouseenter", () => clearTimeout(hideTimer));
    host.addEventListener("mouseleave", scheduleHide);
    button.addEventListener("click", async () => {
      const video = current;
      if (!video || sending) return;
      sending = true;
      button.disabled = true;
      clearTimeout(messageTimer);
      msg.textContent = "Sending…";
      msg.hidden = false;
      let reply: AddReply;
      try {
        const r = (await chrome.runtime.sendMessage({ type: "odm-add-video", src: video.currentSrc })) as AddReply | undefined;
        reply = r ?? { ok: false, message: "ODM did not answer." };
      } catch {
        reply = { ok: false, message: "ODM did not answer." };
      }
      sending = false;
      button.disabled = false;
      msg.textContent = reply.message;
      messageTimer = setTimeout(() => {
        msg.hidden = true;
      }, MESSAGE_MS);
    });
    document.documentElement.appendChild(host);
    ui = { host, wrap, button, msg };
  };

  const show = (video: HTMLVideoElement) => {
    clearTimeout(hideTimer);
    if (document.fullscreenElement || !bigEnough(video)) return;
    if (!ui) build();
    if (!ui) return;
    current = video;
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
}
