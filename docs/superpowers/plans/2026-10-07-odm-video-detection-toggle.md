# Video Detection On/Off Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user turn the extension's video detection off for the current website or everywhere, from the popup, without affecting download takeover.

**Architecture:** One pure module (`extension/src/lib/detection.ts`) owns the settings shape, site matching and the popup's view model, plus two small classes that hide the browser's async edges (settings cache, tab address cache) behind injected loaders so they can be unit-tested. `background.ts` gates both `webRequest` listeners and answers the content scripts; `content.ts` asks before showing anything and reacts to on/off messages; the popup reads and writes `storage.local` directly.

**Tech Stack:** TypeScript, MV3 extension APIs (`storage`, `tabs`, `webRequest`, `runtime` messaging), Vitest, Playwright (screenshot only).

**Spec:** `docs/superpowers/specs/2026-10-07-odm-video-detection-toggle-design.md`

## Global Constraints

- Branch `claude/theme-black-orange`. Only `extension/` and docs change; no app or Rust changes.
- Storage: `storage.local`, key `videoDetection`, shape `{ enabled: boolean, offSites: string[] }`; missing or malformed → `{ enabled: true, offSites: [] }`.
- "The site" is the tab's top-level address: host lowercased, trailing dot and a leading `www.` removed; only http/https have a site. A listed site covers its subdomains; `notyoutube.com` does not match `youtube.com`.
- Detection off stops: stream recording, request-header remembering, the in-page bar (incl. "Download this video"), the `<video>` hover button, the popup's video list. It never stops download takeover, the context menu, the Alt bypass, Pause all / Resume all, Open ODM.
- No answer from the background (extension reloading) counts as detection on.
- Popup copy, exact: card title `Video detection`; rows `On <site>` and `On all sites`; state words `On` / `Off`; note `Off everywhere`; disclosure `Turned off on 1 site` / `Turned off on N sites`; button `Turn on`; replacement text `Video detection is off on <site>.` (or `Video detection is off everywhere.` when the global switch is off).
- Popup colours from `popup.css` variables only: on track `--accent`, knob `--on-accent`; off track `--surface-2`, knob `--muted`; focus ring 2px `--accent-light`.
- Checks: `cd extension && pnpm test && pnpm check && pnpm build`.

## Review Focus

1. **Service worker cold start:** Chrome restarts the worker often; a media response can arrive before settings are loaded. Expect an off site to stay off. Task 1 pins `DetectionState.isOn` waiting for the first load.
2. **Tab address unknown after a worker restart:** the in-memory tab map is empty. Expect the right answer via `tabs.get`. Task 1 pins `TabSites.url` falling back to its getter and caching the result.
3. **A subdomain of a listed site:** `youtube.com` listed, popup open on `m.youtube.com`. Expect the switch to show Off, and turning it On to actually turn detection on there. Task 1 pins `setSiteOff(…, false)` removing every entry that covers the site.
4. **Storage edited by hand or by a future version:** strings with spaces, upper case, `https://` prefixes, non-strings, duplicates. Expect clean data, never a crash. Task 1 pins `normalizeSettings`.
5. **Switch flipped while a tab with videos is open:** expect the badge and on-page bar to clear at once, and to come back on the next detected video when turned on. Task 1 pins `changedTabs`; Task 2 wires it; the user's manual check confirms it in a browser.

---

### Task 1: Detection rules module

**Files:**
- Create: `extension/src/lib/detection.ts`
- Create: `extension/src/lib/detection.test.ts`

**Interfaces:**
- Produces:
  - `export const STORAGE_KEY = "videoDetection"`
  - `export type DetectionSettings = { enabled: boolean; offSites: string[] }`
  - `export const DEFAULT_SETTINGS: DetectionSettings`
  - `export function siteOf(url: string | undefined): string | null`
  - `export function normalizeSettings(raw: unknown): DetectionSettings` (also accepts bare hosts or URLs inside `offSites`, via `siteOf` for URLs and the same host clean-up for bare names)
  - `export function detectionOn(s: DetectionSettings, pageUrl: string | undefined): boolean`
  - `export function setSiteOff(s: DetectionSettings, site: string, off: boolean): DetectionSettings`
  - `export function changedTabs(before: DetectionSettings, after: DetectionSettings, tabs: { id: number; url?: string }[]): { off: number[]; on: number[] }`
  - `export type DetectionView = { site: string | null; siteOn: boolean; globalOn: boolean; offSites: string[]; pageOff: boolean; offText: string }`
  - `export function detectionView(s: DetectionSettings, pageUrl: string | undefined): DetectionView`
  - `export class DetectionState { constructor(load: () => Promise<unknown>); isOn(pageUrl: string | undefined): Promise<boolean>; current(): Promise<DetectionSettings>; apply(raw: unknown): { before: DetectionSettings; after: DetectionSettings } }`
  - `export class TabSites { constructor(get: (tabId: number) => Promise<string | undefined>); set(tabId: number, url: string): void; forget(tabId: number): void; url(tabId: number): Promise<string | undefined> }`

- [ ] **Step 1: Write the failing tests** in `detection.test.ts`:

```ts
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
```

- [ ] **Step 2: Run them to see them fail**
Run: `cd extension && pnpm test -- detection`
Expected: FAIL, `Cannot find module './detection'`.

- [ ] **Step 3: Implement `detection.ts`** with the signatures above. Host validity for list entries follows the app's `normalize_host` in `crates/odm-appcore/src/capture.rs`: after clean-up only `[a-z0-9.-]`, at least one dot. `DetectionState` loads lazily on first use, keeps one in-flight promise, and `apply` swaps in `normalizeSettings(raw)` synchronously. `TabSites` does not cache an `undefined` from the getter.

- [ ] **Step 4: Run them to see them pass**
Run: `cd extension && pnpm test -- detection && pnpm check`
Expected: PASS, no type errors.

- [ ] **Step 5: Commit** — `git add extension/src/lib/detection.ts extension/src/lib/detection.test.ts && git commit -m "feat(extension): video detection rules"`

### Task 2: Background worker

**Files:**
- Modify: `extension/src/background.ts` (imports; the two `webRequest` listeners at ~159-186; `tabs.onUpdated` / `onRemoved` at ~189-192; the `onMessage` switch at ~128-152; new `storage.onChanged` listener)
- Create: `extension/src/lib/wiring.test.ts`

**Interfaces:**
- Consumes: `STORAGE_KEY`, `DetectionState`, `TabSites`, `changedTabs` from Task 1.
- Produces: runtime message `{ type: "odm-detection-state" }` → `{ on: boolean }` for the sender's tab; tab message `{ type: "odm-detection", on: boolean }`. Task 3 relies on both.

- [ ] **Step 1: Write the failing wiring test** `wiring.test.ts`, reading sources as text (same pattern as `theme.test.ts`):

```ts
test("both webRequest listeners check detection", () => {
  expect(bg.match(/await detection\.isOn\(/g)?.length).toBeGreaterThanOrEqual(2);
});
test("background answers odm-detection-state", () => expect(bg).toContain('case "odm-detection-state"'));
test("background reacts to storage changes", () => expect(bg).toMatch(/storage\.onChanged\.addListener/));
test("list-streams is empty when off", () => expect(bg).toMatch(/case "odm-list-streams"[\s\S]{0,400}isOn/));
```

- [ ] **Step 2: Run to see it fail** — `cd extension && pnpm test -- wiring`, expected FAIL on all four.

- [ ] **Step 3: Implement.**
  - Module-level `const detection = new DetectionState(async () => (await ext.storage.local.get(STORAGE_KEY))[STORAGE_KEY])` and `const tabSites = new TabSites(async (id) => (await ext.tabs.get(id).catch(() => undefined))?.url)`.
  - `tabs.onUpdated`: when `changeInfo.url`, `tabSites.set(tabId, changeInfo.url)` (keep the existing `clearStreams` rule). `tabs.onRemoved`: also `tabSites.forget(tabId)`.
  - Both `webRequest` listeners become `async`; after the existing `tabId < 0` return, `if (!(await detection.isOn(await tabSites.url(details.tabId)))) return;`. For `onHeadersReceived`, run `classify` first and only then check, so ordinary requests cost nothing.
  - `odm-list-streams`: answer `[]` when detection is off for that tab (now async: `return true`).
  - `odm-detection-state`: `sendResponse({ on })` for `sender.tab?.url` (`return true`).
  - `ext.storage.onChanged.addListener((changes, area) => …)`: only `area === "local"` and `STORAGE_KEY in changes`; `const { before, after } = detection.apply(changes[STORAGE_KEY].newValue)`; query all tabs; for `changedTabs(before, after, tabs)`: each `off` tab → `clearStreams(id)` and send `{ type: "odm-detection", on: false }`; each `on` tab → send `{ type: "odm-detection", on: true }`. Sends are best effort (`.catch(() => {})`).

- [ ] **Step 4: Run checks** — `cd extension && pnpm test && pnpm check && pnpm build`. Expected: all PASS, `dist/chromium` and `dist/firefox` built.

- [ ] **Step 5: Commit** — `git add extension/src && git commit -m "feat(extension): background skips video detection on turned-off sites"`

### Task 3: On-page buttons

**Files:**
- Modify: `extension/src/content.ts` (the top-frame bar block ~56-145; the video hover block ~147-295)
- Modify: `extension/src/lib/wiring.test.ts`

**Interfaces:**
- Consumes: the two messages from Task 2.
- Produces: nothing new.

- [ ] **Step 1: Add failing assertions** to `wiring.test.ts`:

```ts
test("content asks for the detection state", () => expect(content).toContain('type: "odm-detection-state"'));
test("content handles on/off messages", () => expect(content).toMatch(/m\?\.type === "odm-detection"/));
test("the bar does not render while off", () => expect(content).toMatch(/const render = \(\) => \{[\s\S]{0,200}!detecting/));
test("the hover button does not show while off", () => expect(content).toMatch(/const show = \(video: HTMLVideoElement\) => \{[\s\S]{0,120}!detecting/));
```

- [ ] **Step 2: Run to see them fail** — `cd extension && pnpm test -- wiring`, expected 4 new failures.

- [ ] **Step 3: Implement.** One module-level `let detecting = true` shared by both blocks, and one listener for `odm-detection` that sets it, then: `false` → bar `streams = []`, `render()` and hover `hide()`; `true` → `render()`. At start-up send `{ type: "odm-detection-state" }`; set `detecting = r?.on !== false` (no answer or an error keeps it true), then `render()` in the top frame. `render` returns early (hiding the host) when `!detecting`; `show` returns early when `!detecting`. Leave the Alt-key listeners untouched.

- [ ] **Step 4: Run checks** — `cd extension && pnpm test && pnpm check && pnpm build`. Expected: all PASS.

- [ ] **Step 5: Commit** — `git add extension/src && git commit -m "feat(extension): on-page buttons follow video detection"`

### Task 4: Popup switches

**Files:**
- Modify: `extension/src/popup.html` (new `<section id="detection">` between `#summary` and `#permission`; new `<p id="detection-off" class="muted" hidden>` before `#videos`)
- Modify: `extension/src/popup.ts` (new `showDetection()`, `showVideos()` skips when `pageOff`)
- Modify: `extension/src/popup.css` (card, `.switch`, disclosure list)
- Modify: `extension/src/lib/theme.test.ts`
- Modify: `README.md` (one sentence under "Browser integration")

**Interfaces:**
- Consumes: `STORAGE_KEY`, `normalizeSettings`, `setSiteOff`, `detectionView` from Task 1.

- [ ] **Step 1: Add failing assertions** to `theme.test.ts`:

```ts
test("switch colours come from the theme", () => {
  const css = read("../popup.css");
  expect(css).toMatch(/\.switch input:checked \+ \.track\s*\{[^}]*background:\s*var\(--accent\)/);
  expect(css).toMatch(/\.switch \.track\s*\{[^}]*background:\s*var\(--surface-2\)/);
  expect(css).toMatch(/\.switch input:focus-visible \+ \.track\s*\{[^}]*outline:\s*2px solid var\(--accent-light\)/);
});
test("switch pairs are readable", () => {
  expect(contrast(THEME.onAccent, THEME.accent)).toBeGreaterThanOrEqual(3);
  expect(contrast(THEME.muted, THEME.surface2)).toBeGreaterThanOrEqual(3);
});
test("popup has the detection card", () => {
  const html = read("../popup.html");
  expect(html).toContain('id="detection"');
  expect(html).toContain("Video detection");
});
```

- [ ] **Step 2: Run to see them fail** — `cd extension && pnpm test -- theme`, expected 2 failures (the contrast test passes already; it guards the values).

- [ ] **Step 3: Implement.**
  - Each switch is `<label class="switch"><span>On all sites</span><input type="checkbox" role="switch" id="detect-all"><span class="track" aria-hidden="true"></span><span class="state">On</span></label>`; the site row is the same with `id="detect-site"` and its label text filled in.
  - `showDetection()`: active tab via `tabs.query`, settings via `normalizeSettings((await ext.storage.local.get(STORAGE_KEY))[STORAGE_KEY])`, then render from `detectionView`. Changes write `ext.storage.local.set({ [STORAGE_KEY]: next })` and re-render; the popup does not message ODM.
  - The disclosure is a `<details>` whose `<summary>` reads the exact copy from Global Constraints, one `<li>` per site with a `Turn on` button calling `setSiteOff(s, site, false)`.
  - When `pageOff`: hide `#videos`, show `#detection-off` with `offText`. When the site switch is turned back on, the popup re-runs `showVideos()`.
  - Track 36×20px, knob 16px, `--radius`-independent pill (`border-radius: 999px` is allowed here: it is a switch). Knob moves with `transform: translateX(16px)`, `transition: transform 160ms var(--ease-out)`, none under reduced motion.
  - README: "The extension popup can turn video detection off for the current site or everywhere; downloads are still taken over."

- [ ] **Step 4: Run checks** — `cd extension && pnpm test && pnpm check && pnpm build`. Expected: all PASS.

- [ ] **Step 5: Screenshot.** Serve `extension/dist/chromium` with `python3 -m http.server`, open `popup.html` in Playwright with a stubbed `chrome` (`runtime.sendMessage` → PING ok, `tabs.query` → one tab on `https://www.youtube.com/watch?v=1`, `storage.local` in memory, `permissions.contains` → true), and capture three states: default, after clicking `On youtube.com`, after clicking `On all sites`. Expected: copy and colours as in Global Constraints; no page errors.

- [ ] **Step 6: Commit** — `git add extension/src README.md && git commit -m "feat(extension): popup switches for video detection"`

### Task 5: Push and hand over

- [ ] **Step 1:** `git push origin claude/theme-black-orange`.
- [ ] **Step 2:** Update the "ODM Black & Orange" design system's `Popup` README and preview with the Video detection card (https://claude.ai/artifact/DfGd8w5R8yrz5kZGeHeaWi).
- [ ] **Step 3:** Give the user the manual check list: YouTube page bar hides when turned off; an embedded player on a news site follows the news site; the badge clears at once; turning on brings detection back on the next video; downloads are still taken over on a turned-off site.
