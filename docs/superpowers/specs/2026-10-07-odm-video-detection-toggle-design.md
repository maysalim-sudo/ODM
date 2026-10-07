# Video detection on/off in the extension — design

**Status:** agreed in conversation on 2026-10-07.
**Branch:** `claude/theme-black-orange`.

## Goal

Let the user stop ODM's browser extension from watching a website for videos: per site from the popup ("on this site"), and everywhere with one switch. Download takeover is not part of this; it keeps working on every site.

## What "video detection" covers

Turning detection off for a site stops, for every tab whose address is on that site:

- recording video and stream responses (`webRequest.onHeadersReceived` in `background.ts`), so no badge count and an empty "Videos on this page" list;
- remembering request headers for those responses (`webRequest.onSendHeaders`);
- the in-page "ODM · N videos" bar, including its "Download this video" form on supported video sites;
- the button that appears over `<video>` elements.

It does **not** stop: taking over downloads started in the browser, the "Download with ODM" context menu, the Alt-key bypass, Pause all / Resume all, or Open ODM. The app's existing "Sites to leave to the browser" list and capture switch are unchanged and unrelated.

## Settings

Stored in `chrome.storage.local` (`browser.storage.local` in Firefox) under the key `videoDetection`:

```json
{ "enabled": true, "offSites": ["youtube.com", "example-news.com"] }
```

- `enabled` is the global switch. `offSites` is the per-site list, kept as-is while `enabled` is false, so turning the global switch back on restores the exceptions.
- Missing, unreadable or malformed data means `{ enabled: true, offSites: [] }`: today's behaviour. Entries that do not normalise to a host are dropped; duplicates are dropped.
- Local to this browser profile. No sync, no app involvement.

## Rules (`extension/src/lib/detection.ts`)

Pure functions, no browser APIs:

- `siteOf(url: string): string | null` — the URL's host, lowercased, trailing dot and a leading `www.` removed; `null` for anything that is not http or https or has no host.
- `normalizeSettings(raw: unknown): DetectionSettings` — the defaults and clean-up above.
- `detectionOn(s: DetectionSettings, pageUrl: string | undefined): boolean` — false when `enabled` is false, or when the page's site equals an entry or is a subdomain of one; true otherwise, including pages with no site.
- `setSiteOff(s: DetectionSettings, site: string, off: boolean): DetectionSettings` — adds the site, or on turn-on removes every entry that covers it (the site and any listed parent, so `m.youtube.com` really turns on when `youtube.com` was listed); no duplicates, list kept sorted.

"The site" is always the **tab's** address (the top-level page), never the address of the video or of an embedding frame. A player from another domain embedded in `example-news.com` follows `example-news.com`.

## Background worker (`background.ts`)

- Holds the settings in memory: loaded at start-up (the service worker restarts often; listeners wait for the first load), refreshed on `storage.onChanged`.
- Tracks each tab's top-level address from `tabs.onUpdated` (`changeInfo.url`), forgets it on `tabs.onRemoved`, and falls back to `tabs.get` when unknown.
- `onSendHeaders` and `onHeadersReceived` skip any tab where `detectionOn` is false.
- `odm-list-streams` answers `[]` for such a tab.
- New message `odm-detection-state` → `{ on: boolean }` for the sender's tab, used by content scripts.
- When settings change, every open tab whose state went from on to off loses its stream list and badge, and receives `{ type: "odm-detection", on: false }`; tabs that went from off to on receive `{ type: "odm-detection", on: true }`.

## On-page buttons (`content.ts`)

- On start, both the bar (top frame) and the video hover button (every frame) ask `odm-detection-state` and stay inert until the answer is `on: true`. No answer (extension reloading) counts as on, matching today.
- On `{ type: "odm-detection", on: false }` they remove what they show and stop reacting to `odm-streams` and video hovers; on `on: true` they resume.

## Popup (`popup.html`, `popup.ts`, `popup.css`)

A **Video detection** card under the connection status, above "Videos on this page":

- **"On <site>"** switch for the active tab's site (`siteOf`), hidden for pages with no site. Disabled, with the note "Off everywhere", while the global switch is off.
- **"On all sites"** switch.
- **"Turned off on N sites"** disclosure listing each site with a **Turn on** button; hidden when the list is empty.
- When detection is off for the active tab, the "Videos on this page" section is replaced by "Video detection is off on <site>."
- Switches are `<input type="checkbox" role="switch">` with a visible "On"/"Off" word. On: `--accent` track, `--on-accent` knob. Off: `--surface-2` track with a `--muted` knob. Keyboard focus shows the 2px `--accent-light` ring. Text and marks meet 4.5:1 and 3:1.
- The popup writes storage directly; it never waits on ODM, so it works while ODM is closed.

## Testing

- Unit tests for every function in `lib/detection.ts`: `www.` and trailing-dot handling, non-http URLs, subdomain matching (and that `notyoutube.com` does not match `youtube.com`), global switch, add/remove without duplicates, malformed storage.
- Theme test: popup switch colours meet the contrast floors.
- A Playwright screenshot of `popup.html` with stubbed extension APIs in three states: on, off on this site, off everywhere.
- `pnpm test`, `pnpm check`, `pnpm build` in `extension/`.
- Manual, by the user in a real browser: YouTube page bar, an embedded player on another site, flipping both switches with a tab open.

## Out of scope

Syncing the list with the ODM app or across browsers, a per-site switch for download takeover, a context-menu entry for the toggle, keyboard shortcuts.
