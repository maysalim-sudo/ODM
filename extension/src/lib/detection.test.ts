import { describe, expect, it } from "vitest";
import {
  DEFAULT_SETTINGS,
  DetectionState,
  TabSites,
  changedTabs,
  detectionOn,
  detectionView,
  normalizeSettings,
  setSiteOff,
  siteOf,
} from "./detection";

describe("siteOf", () => {
  it("normalises the host", () => {
    expect(siteOf("https://WWW.YouTube.com./watch?v=1")).toBe("youtube.com");
    expect(siteOf("http://m.youtube.com/")).toBe("m.youtube.com");
  });
  it("has no site for non-web pages", () => {
    for (const u of ["chrome://newtab/", "about:blank", "file:///C:/a.mp4", "moz-extension://x/popup.html", "", undefined])
      expect(siteOf(u)).toBeNull();
  });
});
describe("normalizeSettings", () => {
  it("defaults when missing or malformed", () => {
    for (const raw of [undefined, null, 5, "x", { enabled: "yes" }]) expect(normalizeSettings(raw)).toEqual(DEFAULT_SETTINGS);
  });
  it("cleans the list", () => {
    expect(normalizeSettings({ enabled: false, offSites: [" YouTube.com ", "https://www.vimeo.com/x", 7, "youtube.com", "not a host"] }))
      .toEqual({ enabled: false, offSites: ["vimeo.com", "youtube.com"] });
  });
});
describe("detectionOn", () => {
  const s = { enabled: true, offSites: ["youtube.com"] };
  it("is off on a listed site and its subdomains", () => {
    expect(detectionOn(s, "https://www.youtube.com/watch")).toBe(false);
    expect(detectionOn(s, "https://m.youtube.com/")).toBe(false);
  });
  it("does not match a lookalike", () => expect(detectionOn(s, "https://notyoutube.com/")).toBe(true));
  it("is off everywhere when the global switch is off", () =>
    expect(detectionOn({ enabled: false, offSites: [] }, "https://vimeo.com/")).toBe(false));
  it("is on for pages with no site", () => expect(detectionOn(s, "about:blank")).toBe(true));
});
describe("setSiteOff", () => {
  it("adds once, sorted", () => {
    const a = setSiteOff(setSiteOff(DEFAULT_SETTINGS, "vimeo.com", true), "abc.com", true);
    expect(setSiteOff(a, "vimeo.com", true).offSites).toEqual(["abc.com", "vimeo.com"]);
  });
  it("turning a subdomain on removes the parent entry that covers it", () =>
    expect(setSiteOff({ enabled: true, offSites: ["other.com", "youtube.com"] }, "m.youtube.com", false).offSites).toEqual(["other.com"]));
  it("keeps the global switch", () => expect(setSiteOff({ enabled: false, offSites: [] }, "a.com", true).enabled).toBe(false));
});
describe("changedTabs", () => {
  it("lists tabs that switched", () => {
    const tabs = [{ id: 1, url: "https://youtube.com/" }, { id: 2, url: "https://vimeo.com/" }, { id: 3 }];
    expect(changedTabs(DEFAULT_SETTINGS, { enabled: true, offSites: ["youtube.com"] }, tabs)).toEqual({ off: [1], on: [] });
    expect(changedTabs({ enabled: false, offSites: [] }, DEFAULT_SETTINGS, tabs)).toEqual({ off: [], on: [1, 2, 3] });
  });
});
describe("detectionView", () => {
  it("describes a page on a listed site", () =>
    expect(detectionView({ enabled: true, offSites: ["youtube.com"] }, "https://www.youtube.com/watch")).toEqual({
      site: "youtube.com", siteOn: false, globalOn: true, offSites: ["youtube.com"], pageOff: true,
      offText: "Video detection is off on youtube.com.",
    }));
  it("says everywhere when the global switch is off", () =>
    expect(detectionView({ enabled: false, offSites: [] }, "https://vimeo.com/").offText).toBe("Video detection is off everywhere."));
  it("has no site switch on a non-web page", () => expect(detectionView(DEFAULT_SETTINGS, "chrome://newtab/").site).toBeNull());
});
describe("DetectionState", () => {
  it("waits for the first load before answering", async () => {
    let release!: (v: unknown) => void;
    const state = new DetectionState(() => new Promise((r) => (release = r)));
    const answer = state.isOn("https://youtube.com/");
    release({ enabled: true, offSites: ["youtube.com"] });
    expect(await answer).toBe(false);
  });
  it("treats a failed load as defaults", async () => {
    const state = new DetectionState(() => Promise.reject(new Error("no storage")));
    expect(await state.isOn("https://youtube.com/")).toBe(true);
  });
  it("apply returns before and after", async () => {
    const state = new DetectionState(async () => undefined);
    await state.current();
    expect(state.apply({ enabled: false, offSites: [] })).toEqual({ before: DEFAULT_SETTINGS, after: { enabled: false, offSites: [] } });
  });
});
describe("TabSites", () => {
  it("falls back to the getter once, then caches", async () => {
    let calls = 0;
    const tabs = new TabSites(async () => (calls++, "https://youtube.com/"));
    expect(await tabs.url(4)).toBe("https://youtube.com/");
    expect(await tabs.url(4)).toBe("https://youtube.com/");
    expect(calls).toBe(1);
  });
  it("prefers a set address and forgets closed tabs", async () => {
    const tabs = new TabSites(async () => undefined);
    tabs.set(4, "https://vimeo.com/");
    expect(await tabs.url(4)).toBe("https://vimeo.com/");
    tabs.forget(4);
    expect(await tabs.url(4)).toBeUndefined();
  });
});
