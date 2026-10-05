# Video finder: plain video files and a button on the video

**Goal:** while browsing, ODM finds the videos the page plays and offers to download them, like IDM's "Download this video" button. Plan 4 only detected HLS playlists, so pages that play plain `.mp4`/`.webm` files showed nothing.

**Constraints:**
- These still hold: nothing is sent without a click, and privacy and identity are as in Plan 3's Global Constraints.
- YouTube (`*.googlevideo.com`) is out of scope. It serves range-sliced chunks, not one file, so it must not be listed.
- Gates: extension `pnpm check`, `pnpm test` and `pnpm build`. No Rust or app changes are expected.

## Task 1: detect plain video files (pure logic, `extension/src/lib/streams.ts`)

**Interfaces** (extending the existing module; existing exports keep their behaviour):
```ts
export type StreamKind = "hls" | "dash" | "file";
export type Stream = { url: string; kind: StreamKind; label: string; supported: boolean; size?: number };
export const MIN_VIDEO_BYTES = 2 * 1024 * 1024;
/** Total size from Content-Range "bytes a-b/TOTAL" when present, else Content-Length; undefined when neither is a number. */
export function responseSize(contentLength: string | undefined, contentRange: string | undefined): number | undefined;
/** classify gains a third argument. A "file" is: content type video/mp4, video/webm, video/quicktime, video/x-matroska,
 *  video/ogg, or (when the content type is missing or application/octet-stream) a path ending .mp4/.webm/.mov/.mkv/.m4v.
 *  It must have size >= MIN_VIDEO_BYTES. Hosts ending in googlevideo.com are never classified at all.
 *  HLS and DASH rules are unchanged and do not depend on size. */
export function classify(url: string, contentType: string | undefined, size?: number): StreamKind | null;
// StreamStore.add gains an optional size: add(tabId, url, kind, size?). For "file" the label is
//   "<file name> · <formatted size> · <host>" (formatted with format.ts's existing byte formatter, or a new formatBytes
//   there if none exists: "12.3 MB" style, 1024 base, one decimal), and supported = true. A second add of the same URL
//   updates a missing size but still returns false.
// count() counts every supported item (hls + file).
/** Payload for a plain video file: ADD_DOWNLOAD with source "menu", referrer = page URL, filename = last path segment. */
export function toVideoFilePayload(s: Stream, pageUrl: string, cookies: Cookie[], userAgent: string): AddDownloadPayload;
```

**Tests** (in `streams.test.ts`; write them first and watch them fail):
- `responseSize`:
  - `("100", undefined)` → 100;
  - `(undefined, "bytes 0-1023/5000000")` → 5000000;
  - `("1024", "bytes 0-1023/5000000")` → 5000000;
  - `(undefined, "bytes 0-1/*")` → undefined;
  - `("abc", undefined)` → undefined.
- `classify` files:
  - `video/mp4` at 3 MB → `"file"`; at 1 MB → null; with an unknown size → null;
  - `https://x/a.webm` with `application/octet-stream` at 5 MB → `"file"`;
  - `https://rr1---sn.googlevideo.com/videoplayback?...` with `video/mp4` at 50 MB → null;
  - an `.m3u8` is still `"hls"` with no size.
- `StreamStore`:
  - a file's label contains the name, `MB` and the host;
  - a second add of the same URL with a size fills the size and returns false;
  - `count` includes files.
- `toVideoFilePayload`: `source "menu"`, `referrer` = the page URL, `filename "clip.mp4"` for `https://cdn.x/v/clip.mp4?sig=1`, `headers["User-Agent"]` set, cookies joined.

## Task 2: wiring

**`background.ts`:**
- In `onHeadersReceived`, read the `content-length` and `content-range` response headers, compute `responseSize`, and call `classify(url, type, size)` and `store.add(tabId, url, kind, size)`. Badge and messages are as before.
- `addStream` (the handler for `odm-add-stream`): for a `file` stream, send `ADD_DOWNLOAD` with `toVideoFilePayload`, a 15 000 ms timeout and the cookies for the file URL. HLS is unchanged.
- New message `{type: "odm-add-video", src?: string}` from the content script (the button on a video element):
  1. if `src` is http(s), send it as a file download, the same as a file stream (it doesn't need to be listed: the user clicked on that video);
  2. otherwise (a `blob:` MSE source, or no source), pick the tab's best listed item: the first supported HLS stream, else the largest file stream;
  3. if there is nothing, reply `{ok: false, message: "ODM hasn't seen this video's source yet. Play it for a few seconds and try again."}`.
  
  Reply `{ok, message}` as `odm-add-stream` does.

**`content.ts`:** a hover button on `<video>` elements (top frame and iframes alike, because embedded players often live in iframes):
- Track `<video>` elements with a `MutationObserver` plus an initial scan. Ignore videos smaller than 200×120 CSS px.
- On `mouseenter` / `mousemove` over a video, show one shared fixed-position button (closed shadow root, the same dark tokens) at the video's top-right corner (8 px inset, following `getBoundingClientRect`), labelled "Download with ODM".
- Hide it 1.5 s after the pointer leaves both the video and the button. Hide it on scroll, and on fullscreen change (in fullscreen, the bottom-right page button and the popup are the way in).
- On click, send `odm-add-video` with `src: video.currentSrc`. While waiting, show "Sending…"; afterwards show the reply message under the button for 4 s.
- The existing bottom-right "ODM · N videos" button and its list now also show file items (their label includes the size).

**`popup.ts`:** file items appear in "Videos on this page" with a Download button (`odm-add-stream`), the same as HLS.

**Verification:**
- Run the gates.
- Then `pnpm build` and check that `dist/chromium/content.js` contains "Download with ODM".
- Commit with the message `feat(extension): find plain video files and add a download button on videos`.
