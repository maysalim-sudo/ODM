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
