import { describe, expect, it } from "vitest";
import { applyEvent, bannerShown, bannerText, checkLine, fromStatus, initialView, primaryLabel, type UpdateView } from "./updates";
import type { UpdateStatus } from "./types";

const status = (over: Partial<UpdateStatus> = {}): UpdateStatus => ({
  supported: true,
  current: "0.1.0",
  last_checked_ms: null,
  available: null,
  dismissed: false,
  downloading: false,
  ready: false,
  ...over,
});
const info = { version: "0.2.0", notes: "- Settings page", date: "2026-10-20T12:00:00Z" };

describe("fromStatus", () => {
  it("starts idle without an update", () => {
    const v = fromStatus(status());
    expect(v.available).toBeNull();
    expect(v.phase).toBe("idle");
    expect(bannerShown(v)).toBe(false);
  });
  it("picks up a download in progress or a ready update", () => {
    expect(fromStatus(status({ available: info, downloading: true })).phase).toBe("downloading");
    expect(fromStatus(status({ available: info, ready: true })).phase).toBe("ready");
  });
});

describe("the banner", () => {
  const found = applyEvent(fromStatus(status()), { type: "update_available", ...info });
  it("announces a new version", () => {
    expect(bannerShown(found)).toBe(true);
    expect(bannerText(found, "ask")).toBe("ODM 0.2.0 is available.");
    expect(primaryLabel(found, "ask")).toBe("Update");
  });
  it("never shows in a build without updates", () => {
    const v = applyEvent(fromStatus(status({ supported: false })), { type: "update_available", ...info });
    expect(bannerShown(v)).toBe(false);
  });
  it("stays hidden after Later until another version", () => {
    expect(bannerShown(fromStatus(status({ available: info, dismissed: true })))).toBe(false);
    const later: UpdateView = { ...found, dismissed: true };
    expect(bannerShown(later)).toBe(false);
    const next = applyEvent(later, { type: "update_available", version: "0.3.0", notes: "", date: "" });
    expect(bannerShown(next)).toBe(true);
  });
  it("shows the download progress, then hides the button", () => {
    const v = applyEvent(found, { type: "update_progress", percent: 42 });
    expect(bannerText(v, "ask")).toBe("Downloading update… 42%");
    expect(primaryLabel(v, "ask")).toBeNull();
    const unknown = applyEvent(found, { type: "update_progress", percent: null });
    expect(bannerText(unknown, "ask")).toBe("Downloading update…");
  });
  it("says when an automatic update installs", () => {
    const v = applyEvent(found, { type: "update_ready", version: "0.2.0" });
    expect(v.phase).toBe("ready");
    expect(bannerText(v, "auto")).toBe("ODM 0.2.0 will be installed when you quit ODM.");
    expect(primaryLabel(v, "auto")).toBe("Install now");
    expect(primaryLabel(v, "ask")).toBe("Update");
  });
  it("shows a failure in plain words and lets the user try again", () => {
    const v = applyEvent(applyEvent(found, { type: "update_progress", percent: 10 }), {
      type: "update_failed",
      message: "The update couldn't be verified, so it wasn't installed.",
    });
    expect(v.phase).toBe("idle");
    expect(bannerText(v, "ask")).toBe("The update couldn't be verified, so it wasn't installed.");
    expect(primaryLabel(v, "ask")).toBe("Update");
  });
  it("ignores other events", () => {
    expect(applyEvent(found, { type: "resync" })).toBe(found);
  });
});

describe("checkLine", () => {
  const now = Date.UTC(2026, 9, 10, 12);
  it("says when ODM last checked", () => {
    expect(checkLine({ ...initialView(), supported: true }, now)).toBe("Not checked yet");
    expect(checkLine({ ...initialView(), supported: true, lastChecked: now - 2 * 3600_000 }, now)).toBe("Last checked 2 h ago");
    expect(checkLine({ ...initialView(), supported: true, lastChecked: now - 5_000 }, now)).toBe("Last checked just now");
  });
  it("says checking, up to date or what went wrong", () => {
    const base = { ...initialView(), supported: true, lastChecked: now };
    expect(checkLine({ ...base, checking: true }, now)).toBe("Checking…");
    expect(checkLine({ ...base, upToDate: true }, now)).toBe("ODM is up to date. Last checked just now");
    expect(checkLine({ ...base, error: "ODM couldn't check for updates." }, now)).toBe("ODM couldn't check for updates.");
  });
  it("explains a build without updates", () => {
    expect(checkLine(initialView(), now)).toBe("Updates are not available in this build.");
  });
});
