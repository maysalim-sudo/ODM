import { describe, expect, it } from "vitest";
import {
  READY,
  ROWS,
  SECTIONS,
  checkFolderName,
  checkHost,
  checkType,
  checkInt,
  checkProxyUrl,
  checkUserAgent,
  PROXY_BAD_PORT,
  PROXY_FORMAT,
  PROXY_NO_PORT,
  UA_CHARS,
  UA_LONG,
  checkSpeed,
  fromKbps,
  isShown,
  matchesQuery,
  resetAll,
  restoreSection,
  undoRestore,
  sectionHasRestore,
  splitTags,
  springEase,
  toKbps,
  visibleRows,
} from "./settingsPage";
import type { Settings } from "./types";

const DEFAULTS: Settings = {
  download_dir: "C:\\Users\\me\\Downloads\\ODM",
  category_folders: { video: "Videos", general: "" },
  max_concurrent: 3,
  max_connections: 8,
  max_attempts: 5,
  speed_limit_kbps: 0,
  connect_timeout_s: 30,
  read_timeout_s: 60,
  ffmpeg_path: "",
  ytdlp_path: "",
  capture_enabled: true,
  min_capture_bytes: 1048576,
  skip_hosts: [],
  close_to_tray: true,
  start_minimized: false,
  confirm_browser_downloads: true,
  onboarding_done: false,
  start_with_windows: false,
  notify_complete: true,
  notify_failed: true,
  prevent_sleep: true,
  sort_by_type: true,
  on_exists: "rename",
  on_complete: "nothing",
  start_mode: "now",
  history_days: 0,
  limit_schedule: { enabled: false, days: [true, true, true, true, true, true, true], from: "09:00", to: "18:00" },
  queue_schedule: { enabled: false, days: [true, true, true, true, true, true, true], from: "01:00", to: "07:00" },
  after_queue: "nothing",
  skip_types: [],
  video_quality: "best",
  video_container: "mp4",
  video_subtitles: false,
  ytdlp_auto_update: true,
  proxy_mode: "system",
  proxy_url: "",
  user_agent: "",
  update_check: true,
  update_install: "ask",
};

describe("sections", () => {
  it("come in the most-used-first order", () => {
    expect(SECTIONS.map((s) => s.title)).toEqual([
      "General",
      "Downloads",
      "Speed",
      "Browser",
      "Video",
      "Network",
      "Updates",
      "Advanced",
    ]);
  });
  it("every row belongs to a section and has a one-line description in plain words", () => {
    const ids = new Set(SECTIONS.map((s) => s.id));
    for (const r of ROWS) {
      expect(ids.has(r.section), r.id).toBe(true);
      expect(r.label[0], r.id).toBe(r.label[0].toUpperCase());
      expect(r.label + r.description, r.id).not.toContain("!");
      expect(r.description.includes("\n"), r.id).toBe(false);
    }
    expect(new Set(ROWS.map((r) => r.id)).size).toBe(ROWS.length);
  });
});

describe("READY", () => {
  it("shows every row now that all engine work has landed", () => {
    expect(Object.values(READY).every((r) => r !== false)).toBe(true);
    expect(isShown("download_dir", "")).toBe(true);
    expect(isShown("sort_by_type", "")).toBe(true);
    expect(isShown("notify_failed", "")).toBe(true);
  });
  it("shows the download behaviour rows", () => {
    for (const id of [
      "prevent_sleep",
      "on_exists",
      "on_complete",
      "start_mode",
      "history_days",
      "limit_schedule",
      "queue_schedule",
      "after_queue",
      "skip_types",
    ]) {
      expect(isShown(id, ""), id).toBe(true);
    }
  });
  it("shows the update rows", () => {
    for (const id of ["update_check", "update_install", "check_now", "whats_new", "app_version"]) {
      expect(isShown(id, ""), id).toBe(true);
    }
  });
  it("shows the network and video rows", () => {
    for (const id of ["start_with_windows", "video_quality", "video_container", "video_subtitles", "ytdlp_auto_update", "proxy", "user_agent"]) {
      expect(isShown(id, ""), id).toBe(true);
    }
  });
  it("only names rows that exist", () => {
    const ids = new Set(ROWS.map((r) => r.id));
    for (const id of Object.keys(READY)) expect(ids.has(id as never), id).toBe(true);
  });
});

describe("search", () => {
  const row = ROWS.find((r) => r.id === "max_connections")!;
  it("matches label and description, ignoring case and word order", () => {
    expect(matchesQuery(row, "")).toBe(true);
    expect(matchesQuery(row, "CONNECTIONS")).toBe(true);
    expect(matchesQuery(row, "download connections")).toBe(true);
    expect(matchesQuery(row, "proxy")).toBe(false);
  });
  it("never shows a hidden row, even when it matches", () => {
    expect(visibleRows("updates", "")).toContain("update_check");
    expect(visibleRows(null, "windows").every((id) => READY[id] !== false)).toBe(true);
  });
  it("searches every section when a query is given", () => {
    const found = visibleRows(null, "timeout");
    expect(found).toContain("connect_timeout_s");
    expect(visibleRows("general", "")).toContain("close_to_tray");
  });
});

describe("restore defaults", () => {
  const changed: Settings = {
    ...DEFAULTS,
    close_to_tray: false,
    notify_failed: false,
    max_connections: 16,
    download_dir: "D:\\dl",
    sort_by_type: false,
    onboarding_done: true,
  };
  it("resets only the fields of that section", () => {
    const out = restoreSection(changed, DEFAULTS, "general");
    expect(out.close_to_tray).toBe(true);
    expect(out.notify_failed).toBe(true);
    expect(out.max_connections).toBe(16);
    expect(out.download_dir).toBe("D:\\dl");
    const dl = restoreSection(changed, DEFAULTS, "downloads");
    // Users keep their folder; only "Reset all settings" puts it back.
    expect(dl.download_dir).toBe("D:\\dl");
    expect(dl.sort_by_type).toBe(true);
    expect(dl.close_to_tray).toBe(false);
  });
  it("copies nested values instead of sharing them", () => {
    const out = restoreSection(changed, DEFAULTS, "downloads");
    expect(out.category_folders).toEqual(DEFAULTS.category_folders);
    expect(out.category_folders).not.toBe(DEFAULTS.category_folders);
  });
  it("is offered only where a section has visible settings", () => {
    expect(sectionHasRestore("general")).toBe(true);
    expect(sectionHasRestore("updates")).toBe(true);
    expect(sectionHasRestore("advanced")).toBe(true);
  });
  it("reset all keeps the finished welcome screen", () => {
    const out = resetAll(changed, DEFAULTS);
    expect(out).toEqual({ ...DEFAULTS, onboarding_done: true });
  });
  it("reset all still resets the download folder", () => {
    expect(resetAll(changed, DEFAULTS).download_dir).toBe(DEFAULTS.download_dir);
  });
  it("undo puts back only what the restore changed, keeping later edits", () => {
    const before = changed;
    const after = restoreSection(before, DEFAULTS, "general");
    // Within the 5 s window the user changes a setting in another section.
    const current: Settings = { ...after, max_connections: 4 };
    const out = undoRestore(before, after, current);
    expect(out.close_to_tray).toBe(false);
    expect(out.notify_failed).toBe(false);
    expect(out.max_connections).toBe(4);
    expect(out.download_dir).toBe("D:\\dl");
  });
  it("undo of reset all keeps later edits to fields the reset did not change", () => {
    const before: Settings = { ...DEFAULTS, close_to_tray: false };
    const after = resetAll(before, DEFAULTS);
    const current: Settings = { ...after, max_attempts: 9 };
    const out = undoRestore(before, after, current);
    expect(out.close_to_tray).toBe(false);
    expect(out.max_attempts).toBe(9);
  });
  it("undo copies nested values", () => {
    const before: Settings = { ...DEFAULTS, skip_hosts: ["a.com"] };
    const after = restoreSection(before, DEFAULTS, "browser");
    const out = undoRestore(before, after, after);
    expect(out.skip_hosts).toEqual(["a.com"]);
    expect(out.skip_hosts).not.toBe(before.skip_hosts);
  });
});

describe("inline checks", () => {
  it("whole numbers in range", () => {
    expect(checkInt(3, 1, 10)).toBeNull();
    expect(checkInt(1, 1, 10)).toBeNull();
    expect(checkInt(10, 1, 10)).toBeNull();
    expect(checkInt(0, 1, 10)).toBe("Enter a whole number from 1 to 10.");
    expect(checkInt(11, 1, 10)).toBe("Enter a whole number from 1 to 10.");
    expect(checkInt(2.5, 1, 10)).toBe("Enter a whole number from 1 to 10.");
    expect(checkInt(null, 1, 10)).toBe("Enter a whole number from 1 to 10.");
    expect(checkInt(Number.NaN, 1, 10)).toBe("Enter a whole number from 1 to 10.");
  });
  it("folder names", () => {
    expect(checkFolderName("Videos")).toBeNull();
    expect(checkFolderName("")).toBeNull();
    for (const bad of ["a\\b", "a/b", "C:", "..", ".", " lead", "trail "]) expect(checkFolderName(bad), bad).not.toBeNull();
  });
  // The same cases as PROXY_CASES in crates/odm-appcore/tests/settings.rs, so both sides agree.
  const PROXY_CASES: [string, string | null][] = [
    ["http://127.0.0.1:8080", null],
    ["https://proxy.example.com:443", null],
    ["socks5://proxy.local:1080", null],
    ["socks5h://proxy.local:1080/", null],
    ["HTTP://Proxy-1.Local:3128", null],
    ["  http://host:8080  ", null],
    ["http://host:65535", null],
    ["", PROXY_FORMAT],
    ["proxy:8080", PROXY_FORMAT],
    ["127.0.0.1:8080", PROXY_FORMAT],
    ["ftp://x:1", PROXY_FORMAT],
    ["socks4://host:1080", PROXY_FORMAT],
    ["http://", PROXY_FORMAT],
    ["http://:8080", PROXY_FORMAT],
    ["http://user:pw@host:8080", PROXY_FORMAT],
    ["http://host:8080/path", PROXY_FORMAT],
    ["http://ho st:8080", PROXY_FORMAT],
    ["http://-host:8080", PROXY_FORMAT],
    ["http://host..local:8080", PROXY_FORMAT],
    ["http://host:", PROXY_FORMAT],
    ["http://host:abc", PROXY_FORMAT],
    ["http://[::1]:8080", PROXY_FORMAT],
    ["http://host", PROXY_NO_PORT],
    ["socks5://host/", PROXY_NO_PORT],
    ["http://host:99999", PROXY_BAD_PORT],
    ["http://host:0", PROXY_BAD_PORT],
    ["http://host:123456", PROXY_BAD_PORT],
  ];
  it("proxy URLs, with the same rule as the backend", () => {
    for (const [url, want] of PROXY_CASES) expect(checkProxyUrl(url), JSON.stringify(url)).toBe(want);
    expect(PROXY_FORMAT).toBe("Enter an address like http://host:8080 or socks5://host:1080.");
    expect(PROXY_NO_PORT).toBe("Add the port, like http://host:8080.");
    expect(PROXY_BAD_PORT).toBe("The port must be from 1 to 65535.");
  });
  it("user agents, with the same rule as the backend", () => {
    expect(checkUserAgent("")).toBeNull();
    expect(checkUserAgent("Mozilla/5.0 (Windows NT 10.0) Gecko/1")).toBeNull();
    expect(checkUserAgent("a\r\nX-Evil: 1")).toBe(UA_CHARS);
    expect(checkUserAgent("tab\there")).toBe(UA_CHARS);
    expect(checkUserAgent("caf\u00e9")).toBe(UA_CHARS);
    expect(checkUserAgent("a".repeat(512))).toBeNull();
    expect(checkUserAgent("a".repeat(513))).toBe(UA_LONG);
    expect(UA_CHARS).toBe("Use only standard keyboard characters, on one line.");
    expect(UA_LONG).toBe("Use at most 512 characters.");
  });
  it("speed limit", () => {
    expect(checkSpeed(500, "kb")).toBeNull();
    expect(checkSpeed(0.5, "mb")).toBeNull();
    expect(checkSpeed(0, "kb")).not.toBeNull();
    expect(checkSpeed(null, "mb")).not.toBeNull();
    expect(checkSpeed(0.4, "kb")).not.toBeNull();
  });
});

describe("speed units", () => {
  it("converts to KB/s", () => {
    expect(toKbps(500, "kb")).toBe(500);
    expect(toKbps(2, "mb")).toBe(2048);
    expect(toKbps(1.5, "mb")).toBe(1536);
  });
  it("shows whole MB/s in MB/s, everything else in KB/s", () => {
    expect(fromKbps(2048)).toEqual({ value: 2, unit: "mb" });
    expect(fromKbps(1536)).toEqual({ value: 1.5, unit: "mb" });
    expect(fromKbps(500)).toEqual({ value: 500, unit: "kb" });
    expect(fromKbps(1000)).toEqual({ value: 1000, unit: "kb" });
  });
});

describe("splitTags", () => {
  it("splits on commas, spaces and new lines and drops empties and repeats", () => {
    expect(splitTags("pdf, jpg  .PNG\npdf,,")).toEqual(["pdf", "jpg", ".PNG"]);
    expect(splitTags("  ")).toEqual([]);
  });
});

describe("springEase", () => {
  it("starts at 0, ends at 1 and never overshoots", () => {
    expect(springEase(0)).toBe(0);
    expect(springEase(1)).toBe(1);
    let prev = 0;
    for (let i = 1; i <= 100; i++) {
      const v = springEase(i / 100);
      expect(v).toBeGreaterThanOrEqual(prev);
      expect(v).toBeLessThanOrEqual(1);
      prev = v;
    }
  });
  it("is mostly there by the middle, like a critically damped spring", () => {
    expect(springEase(0.5)).toBeGreaterThan(0.8);
  });
});

describe("tag checks", () => {
  it("hosts follow the Rust normalisation", () => {
    expect(checkHost("example.com")).toBeNull();
    expect(checkHost("https://www.Example.com/path")).toBeNull();
    expect(checkHost("bad host!")).not.toBeNull();
    expect(checkHost("")).not.toBeNull();
  });
  it("file types are plain extensions", () => {
    expect(checkType("pdf")).toBeNull();
    expect(checkType(".JPG")).toBeNull();
    expect(checkType("tar.gz")).not.toBeNull();
    expect(checkType("a b")).not.toBeNull();
  });
});

describe("schedule editor", () => {
  it("asks for at least one day, also when a schedule arrives with none", async () => {
    const ed = (await import("../components/ScheduleEditor.svelte?raw")).default;
    expect(ed).toContain('"Pick at least one day."');
    expect(ed).toMatch(/schedule\.days\.some\(Boolean\)/);
  });
});

describe("page source", () => {
  it("slides with the spring, fades for reduced motion, and moves only transform and opacity", async () => {
    const page = (await import("../components/SettingsPage.svelte?raw")).default;
    expect(page).toMatch(/easing: springEase/);
    expect(page).toMatch(/prefers-reduced-motion: reduce/);
    for (const css of page.matchAll(/css: \(t: number\) =>([^\n]*)/g)) {
      const props = [...css[1].matchAll(/([a-z-]+):\s/g)].map((m) => m[1]);
      expect(props.every((p) => p === "opacity" || p === "transform"), css[1]).toBe(true);
    }
  });
  it("the switch keeps the extension popup's colours", async () => {
    const sw = (await import("../components/Switch.svelte?raw")).default;
    expect(sw).toMatch(/\.track\s*\{[^}]*background:\s*var\(--surface-2\)/);
    expect(sw).toMatch(/input:checked \+ \.track\s*\{[^}]*background:\s*var\(--accent\)/);
    expect(sw).toMatch(/input:focus-visible \+ \.track\s*\{[^}]*outline:\s*2px solid var\(--accent-light\)/);
  });
  it("the old settings dialog is gone and the app opens the page", async () => {
    const app = (await import("../App.svelte?raw")).default;
    expect(app).not.toContain("SettingsDialog");
    expect(app).toContain("<SettingsPage");
    expect(app).toMatch(/e\.ctrlKey && e\.key === ","/);
  });
  it("the file-exists prompt and the shutdown notice use the shared Modal, with the plain choices", async () => {
    const exists = (await import("../components/ExistsDialog.svelte?raw")).default;
    expect(exists).toContain("<Modal");
    expect(exists).toContain("already exists");
    for (const choice of ["Replace", "Keep both", "Cancel download"]) expect(exists).toContain(`>${choice}<`);
    const shutdown = (await import("../components/ShutdownDialog.svelte?raw")).default;
    expect(shutdown).toContain("<Modal");
    expect(shutdown).toContain("ODM will shut down the PC in ${left} s.");
    expect(shutdown).toContain(">Cancel shutdown<");
    const store = (await import("./downloads.svelte.ts?raw")).default;
    expect(store).toMatch(/shutdown_failed[\s\S]*this\.error = ev\.message/);
    const app = (await import("../App.svelte?raw")).default;
    expect(app).toContain("<ExistsDialog />");
    expect(app).toContain("<ShutdownDialog />");
  });
});
