import { ext, send, sendDetailed } from "./lib/browser";
import { statusLine } from "./lib/format";
import { errorText, type SendFailure } from "./lib/protocol";
import type { Stream } from "./lib/streams";

const FAILURE_TEXT: Record<SendFailure, string> = {
  missing_host: "ODM's browser helper is not installed. Open ODM once to set it up.",
  timeout: "ODM did not answer in time.",
  other: "Could not reach ODM.",
};
const TEN_MINUTES = 10 * 60 * 1000;

const $ = (id: string) => document.getElementById(id) as HTMLElement;

function setStatus(text: string): void {
  $("status").textContent = text;
}

async function refreshSummary(): Promise<void> {
  const r = await send<{ active: number; queued: number; total_speed_bps: number }>("GET_SUMMARY", {}, 2000);
  if (r?.ok && r.payload) $("summary").textContent = statusLine(r.payload);
  else $("summary").textContent = "";
}

async function init(): Promise<void> {
  const { response: r, failure } = await sendDetailed<{ version: string }>("PING", {}, 2000);
  if (r === null) {
    setStatus(FAILURE_TEXT[failure ?? "other"]);
  } else if (r.ok) {
    setStatus(`Connected to ODM ${r.payload?.version ?? ""}`.trim());
    await refreshSummary();
  } else if (r.error?.code === "app_not_running") {
    setStatus("ODM is not running.");
  } else {
    setStatus(errorText(r));
  }
}

async function showLastError(): Promise<void> {
  const area = ext.storage.session ?? ext.storage.local;
  const stored = (await area.get("lastError")) as { lastError?: { lastError: string; at: number } };
  const e = stored.lastError;
  if (e && Date.now() - e.at < TEN_MINUTES) {
    $("problem-text").textContent = `Last problem: ${e.lastError}`;
    $("problem").hidden = false;
  }
}

async function showPermissionPrompt(): Promise<void> {
  if (__TARGET__ !== "firefox") return;
  const granted = await ext.permissions.contains({ origins: ["<all_urls>"] });
  if (!granted) $("permission").hidden = false;
}

function el(parent: HTMLElement, tag: string, cls?: string, text?: string): HTMLElement {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  parent.appendChild(e);
  return e;
}

async function showVideos(): Promise<void> {
  const [tab] = await ext.tabs.query({ active: true, currentWindow: true });
  if (tab?.id === undefined) return;
  const tabId = tab.id;
  const list = (await ext.runtime.sendMessage({ type: "odm-list-streams", tabId })) as Stream[] | undefined;
  if (!Array.isArray(list) || list.length === 0) return;
  const ul = $("video-list");
  for (const s of list) {
    const li = el(ul, "li");
    const line = el(li, "div", "line");
    const name = el(line, "span", "name", s.label);
    name.title = s.url;
    if (!s.supported) {
      el(line, "span", "muted", "(DASH, not supported yet)");
      continue;
    }
    const note = el(li, "p", "note");
    note.hidden = true;
    const button = el(line, "button", undefined, "Download") as HTMLButtonElement;
    button.type = "button";
    button.addEventListener("click", async () => {
      button.disabled = true;
      note.textContent = "Sending...";
      note.hidden = false;
      try {
        const r = (await ext.runtime.sendMessage({ type: "odm-add-stream", url: s.url, tabId })) as
          | { ok: boolean; message: string }
          | undefined;
        note.textContent = r?.message ?? "ODM did not answer.";
      } catch {
        note.textContent = "ODM did not answer.";
      }
      button.disabled = false;
    });
  }
  $("videos").hidden = false;
}

$("pause").addEventListener("click", async () => {
  await send("PAUSE_ALL", {}, 2000);
  await refreshSummary();
});
$("resume").addEventListener("click", async () => {
  await send("RESUME_ALL", {}, 2000);
  await refreshSummary();
});
$("open").addEventListener("click", async () => {
  const { response: r, failure } = await sendDetailed("SHOW_WINDOW", {}, 15000);
  if (r?.ok) window.close();
  else setStatus(r === null ? FAILURE_TEXT[failure ?? "other"] : errorText(r));
});
$("dismiss").addEventListener("click", async () => {
  const area = ext.storage.session ?? ext.storage.local;
  await area.remove("lastError");
  await ext.action.setBadgeText({ text: "" });
  $("problem").hidden = true;
});
$("allow").addEventListener("click", async () => {
  const ok = await ext.permissions.request({ origins: ["<all_urls>"] });
  if (ok) $("permission").hidden = true;
});

void init();
void showLastError();
void showPermissionPrompt();
void showVideos().catch(() => {});
