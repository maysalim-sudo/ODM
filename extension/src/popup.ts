import { ext, send } from "./lib/browser";
import { statusLine } from "./lib/format";
import { errorText } from "./lib/protocol";

const MISSING_HELPER = "ODM's browser helper is not installed. Open ODM once to set it up.";
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
  const r = await send<{ version: string }>("PING", {}, 2000);
  if (r === null) {
    setStatus(MISSING_HELPER);
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

$("pause").addEventListener("click", async () => {
  await send("PAUSE_ALL", {}, 2000);
  await refreshSummary();
});
$("resume").addEventListener("click", async () => {
  await send("RESUME_ALL", {}, 2000);
  await refreshSummary();
});
$("open").addEventListener("click", async () => {
  const r = await send("SHOW_WINDOW", {}, 15000);
  if (r?.ok) window.close();
  else setStatus(r === null ? MISSING_HELPER : errorText(r));
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
