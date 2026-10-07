# ODM Black and Orange Theme Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Re-skin the ODM desktop app and browser extension to the black and orange palette, with press and dialog polish, and publish a second design system for it.

**Architecture:** Colours stay CSS custom properties on `:root` in `app/src/app.css`; components keep reading the same variable names, and the few literals and `color-mix()` calls move to new variables. The extension gets one TypeScript module of colour constants that `content.ts` (shadow-DOM CSS) and `background.ts` (badge) read, and `popup.css` repeats the values. Vitest guard tests read the stylesheets as text and check values, contrast and the absence of old colours.

**Tech Stack:** Svelte 5 + Vite (app), Tauri 2 (icons), TypeScript + Vitest (both packages), pnpm.

**Spec:** `docs/superpowers/specs/2026-10-07-odm-black-orange-theme-design.md`

## Global Constraints

- All work on branch `claude/theme-black-orange`; never commit to `claude/relaxed-shannon-u4f79v` or `master`.
- Colour values are exactly the spec's palette table (lowercase hex in code).
- Keep existing variable names (`--bg`, `--surface`, `--surface-2`, `--border`, `--text`, `--muted`, `--accent`, `--accent-light`, `--error`, `--success`, `--warn`, `--radius`).
- No hex or `rgb()` literal in any `app/src/components/*.svelte` or `app/src/App.svelte` style block after Task 1.
- Old palette values must not appear anywhere in `app/src` or `extension/src`: `#6366f1 #818cf8 #0f0f14 #14141c #1a1a24 #1e1e28 #e0e0e6 #6b6b80 #ef4444 #22c55e #f59e0b #7578f3 #1f1f2b #2a2a3a`.
- Text pairs ≥ 4.5:1 (spec palette is pre-checked); borders, focus rings and state marks ≥ 3:1. `--idle` is never used for text.
- No layout, copy or component-structure changes. No imitation of any other site's logo or wordmark.
- UI animation ≤ 200ms, easing `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)`; nothing animates on keyboard navigation or row selection.
- App checks: `pnpm check`, `pnpm test`, `pnpm build` in `app/`. Extension checks: `pnpm check`, `pnpm test`, `pnpm build` in `extension/`.

## Review Focus

1. **Badge on an orange background:** Chrome and Firefox draw white badge text by default (2.1:1 on #ffa31a). Expect dark, readable text. Task 3 sets the text colour and tests the pair.
2. **The in-page bar on bright and dark websites:** it floats over arbitrary pages. Expect it to stay readable on white and on black pages. Task 3 keeps an opaque `#1b1b1b` body and a 1px `#ffa31a` border, and tests text on that body.
3. **Selected row vs hover vs progress fill:** all now warm. Expect the selected row to read as selected, hovered or not. Task 1 tests that text holds ≥ 10:1 on `--selected` and `--selected-hover`, and that `--selected-hover` differs from `--surface-2`.
4. **Chunk states told apart by colour alone:** orange, green and violet have near-equal lightness (≤ 1.4:1 apart). Expect the state to be readable without colour. The chunk-map legend words and the failed segment's outline stay; Task 1's test asserts the `.seg.failed` outline rule still exists.
5. **Windows High Contrast (forced colours):** expect buttons and inputs to keep visible edges. Task 2 adds a `@media (forced-colors: active)` rule giving buttons `border-color: ButtonText` and checks it in the guard test.

---

### Task 1: App palette

**Files:**
- Modify: `app/src/app.css:1-15` (the `:root` block)
- Modify: `app/src/components/Modal.svelte:52,55` (shadow, backdrop → variables)
- Modify: `app/src/components/ChunkMap.svelte:48` (`#f59e0b` → `var(--warn)`)
- Modify: `app/src/App.svelte` `.error` (→ `var(--error-tint)`)
- Modify: `app/src/components/BrowserDialog.svelte` `.pill`, `.pill.ok` (→ `var(--warn-tint)`, `var(--success-tint)`)
- Modify: `app/src/components/DownloadTable.svelte` selected rows (→ `var(--selected)`, `var(--selected-hover)`)
- Modify: `app/src/app.css` `button.primary` colour `#fff` → `var(--on-accent)`
- Modify: `app/src/components/Sidebar.svelte` (`button.active` → orange pill), `DetailPanel.svelte` and `DownloadTable.svelte` (heading and `th` weight 700)
- Create: `app/src/lib/theme.test.ts`

**Interfaces:**
- Produces: CSS variables `--on-accent --error-tint --success-tint --warn-tint --info --idle --selected --selected-hover --backdrop --shadow-dialog` on `:root`, plus the existing ones with new values. Tasks 2 and 5 rely on these names.

- [ ] **Step 1: Write the failing guard test** `app/src/lib/theme.test.ts`

Reads `../app.css`, every `../components/*.svelte` and `../App.svelte` as text (`readFileSync(new URL(..., import.meta.url))`). Helpers inside the test file: `rootVars(css): Record<string,string>` (parse `--name: value;` inside the first `:root {}`), `contrast(a: string, b: string): number` (WCAG 2 relative luminance on `#rrggbb`).

```ts
test("root variables match the spec palette", () => {
  expect(vars).toMatchObject({
    "--bg": "#000000", "--surface": "#1b1b1b", "--surface-2": "#2f2f2f", "--border": "#3a3a3a",
    "--text": "#ffffff", "--muted": "#a0a0a0", "--accent": "#ffa31a", "--accent-light": "#ffb84d",
    "--on-accent": "#000000", "--error": "#ff6b6b", "--error-tint": "#291111", "--success": "#34d399",
    "--success-tint": "#082218", "--warn": "#a78bfa", "--warn-tint": "#211c32", "--info": "#60a5fa",
    "--idle": "#6b7280", "--selected": "#291a04", "--selected-hover": "#382406", "--radius": "4px",
  });
});
test.each([
  ["--text", "--bg", 4.5], ["--text", "--selected", 10], ["--text", "--selected-hover", 10], ["--muted", "--selected", 4.5],
  ["--muted", "--bg", 4.5], ["--muted", "--surface", 4.5], ["--muted", "--surface-2", 4.5],
  ["--accent", "--surface-2", 4.5], ["--on-accent", "--accent", 4.5], ["--on-accent", "--accent-light", 4.5],
  ["--error", "--surface-2", 4.5], ["--error", "--error-tint", 4.5], ["--success", "--success-tint", 4.5],
  ["--warn", "--warn-tint", 4.5], ["--warn", "--surface-2", 4.5], ["--info", "--surface", 4.5],
  ["--accent-light", "--surface", 3],
])("%s on %s ≥ %d:1", (fg, bg, min) => expect(contrast(vars[fg], vars[bg])).toBeGreaterThanOrEqual(min));
test("selected-hover differs from surface-2", () => expect(vars["--selected-hover"]).not.toBe(vars["--surface-2"]));
test("no colour literals in component styles", () => {
  for (const [file, src] of componentStyles) expect(src, file).not.toMatch(/#[0-9a-f]{3,8}\b|rgba?\(/i);
});
test("no old palette values left", () => { for (const old of OLD) expect(allAppSource).not.toContain(old); });
test("active sidebar item is an orange pill", () => expect(sidebarSrc).toMatch(/button\.active\s*\{[^}]*background:\s*var\(--accent\)/));
test("failed chunk keeps its outline", () => expect(chunkMapSrc).toMatch(/\.seg\.failed\s*\{[^}]*outline:/));
```

`OLD` is the Global Constraints list. `sidebarSrc`, `chunkMapSrc`, `modalSrc` are those components' `<style>` blocks. `componentStyles` is the `<style>` block of each `.svelte` file.

- [ ] **Step 2: Run it to see it fail**

Run: `cd app && pnpm test -- theme`
Expected: FAIL on the palette values and the literals in Modal/ChunkMap.

- [ ] **Step 3: Update `:root` in `app/src/app.css`** with every spec palette row (set `--radius: 4px`, add `--radius-lg: 8px`, keep `color-scheme: dark`, font stack `"Segoe UI", Arial, system-ui, sans-serif`), and apply the spec's Character table: `button { font-weight: 600 }`, `button.primary { color: var(--on-accent); font-weight: 700 }`, modal and pill radius → `var(--radius-lg)`, Modal/DetailPanel `h2` and table `th` weight 700, Sidebar `button.active` → `background: var(--accent); border-color: var(--accent); color: var(--on-accent)` with its `.count` also `var(--on-accent)`.

- [ ] **Step 4: Replace the literals and `color-mix()` calls** in the five component files listed above with the matching variables.

- [ ] **Step 5: Run tests and the type check**

Run: `cd app && pnpm test && pnpm check`
Expected: all PASS, 0 svelte-check errors.

- [ ] **Step 6: Commit**

```bash
git add app/src/app.css app/src/App.svelte app/src/components app/src/lib/theme.test.ts
git commit -m "feat(ui): black and orange palette"
```

### Task 2: Press, hover and dialog polish

**Files:**
- Modify: `app/src/app.css` (button rules, `--ease-out`, forced-colors, reduced motion)
- Modify: `app/src/components/Modal.svelte` (dialog enter transition)
- Modify: `app/src/lib/theme.test.ts` (add assertions)

**Interfaces:**
- Consumes: variables from Task 1.
- Produces: `--ease-out` on `:root`.

- [ ] **Step 1: Add failing assertions** to `theme.test.ts`:

```ts
test("ease-out token", () => expect(vars["--ease-out"]).toBe("cubic-bezier(0.23, 1, 0.32, 1)"));
test("no transition: all", () => expect(allAppSource).not.toMatch(/transition:\s*all\b/));
test("press feedback", () => expect(appCss).toMatch(/button:active:not\(:disabled\)\s*\{[^}]*scale\(0\.97\)/));
test("hover gated to fine pointers", () => expect(appCss).toMatch(/@media \(hover: hover\) and \(pointer: fine\)/));
test("forced colours keep button edges", () => expect(appCss).toMatch(/@media \(forced-colors: active\)[\s\S]*ButtonText/));
test("reduced motion drops scale", () => expect(appCss).toMatch(/prefers-reduced-motion: reduce[\s\S]*transform:\s*none/));
test("dialog enters with @starting-style", () => expect(modalSrc).toMatch(/@starting-style/));
```

- [ ] **Step 2: Run to see them fail** — `cd app && pnpm test -- theme`, expected FAIL on the seven new tests.

- [ ] **Step 3: Implement in `app/src/app.css`:**
  - `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)` in `:root`.
  - `button { transition: transform 160ms var(--ease-out), border-color 120ms ease, background-color 120ms ease; }`
  - `button:active:not(:disabled) { transform: scale(0.97); }`
  - Move `button:hover:not(:disabled)` inside `@media (hover: hover) and (pointer: fine)`, and add `button.primary:hover:not(:disabled) { background: var(--accent-light); border-color: var(--accent-light); }` there.
  - `@media (forced-colors: active) { button, input, select, textarea { border-color: ButtonText; } }`
  - `@media (prefers-reduced-motion: reduce) { button { transition: border-color 120ms ease, background-color 120ms ease; } button:active:not(:disabled) { transform: none; } }`
  - Table rows and sidebar items get no transition (Global Constraints).

- [ ] **Step 4: Implement the dialog entrance in `Modal.svelte`:** `dialog[open]` transitions `opacity` and `transform` over 200ms `var(--ease-out)`, with `@starting-style { opacity: 0; transform: scale(0.96); }`; `dialog[open]::backdrop` fades from `opacity: 0` over 200ms. Under reduced motion keep the opacity fade and drop the scale. `transform-origin` stays centred (modal).

- [ ] **Step 5: Run checks** — `cd app && pnpm test && pnpm check && pnpm build`, expected PASS and a built `dist/`.

- [ ] **Step 6: Commit** — `git commit -am "feat(ui): press feedback, fine-pointer hover, dialog entrance"`

### Task 3: Extension theme

**Files:**
- Create: `extension/src/lib/theme.ts`
- Create: `extension/src/lib/theme.test.ts`
- Modify: `extension/src/content.ts:33-51,148-160` (both shadow CSS strings)
- Modify: `extension/src/background.ts:23,31,47` (badge colours)
- Modify: `extension/src/popup.css:1-7` and the `.primary` / button rules

**Interfaces:**
- Produces: `export const THEME: { bg, surface, surface2, border, text, muted, accent, accentLight, onAccent, error, onError }` (all `#rrggbb` strings, values from the spec) and `export const BADGE: { bg: string; text: string }`, `export const BADGE_ERROR: { bg: string; text: string }`.

- [ ] **Step 1: Write `theme.test.ts`** (same `contrast` helper as Task 1):

```ts
test("palette", () => expect(THEME).toMatchObject({ bg: "#000000", surface: "#1b1b1b", accent: "#ffa31a", accentLight: "#ffb84d", onAccent: "#000000", text: "#ffffff", muted: "#a0a0a0", error: "#ff6b6b", onError: "#000000" }));
test("badge text readable", () => {
  expect(contrast(BADGE.text, BADGE.bg)).toBeGreaterThanOrEqual(4.5);
  expect(contrast(BADGE_ERROR.text, BADGE_ERROR.bg)).toBeGreaterThanOrEqual(4.5);
});
test("in-page bar text readable", () => {
  expect(contrast(THEME.text, THEME.surface)).toBeGreaterThanOrEqual(4.5);
  expect(contrast(THEME.muted, THEME.bg)).toBeGreaterThanOrEqual(4.5);
  expect(contrast(THEME.onAccent, THEME.accent)).toBeGreaterThanOrEqual(4.5);
});
test("no old palette values in extension sources", () => {
  for (const f of ["../content.ts", "../background.ts", "../popup.css"]) for (const old of OLD) expect(read(f), f).not.toContain(old);
});
test("popup.css uses the theme values", () => {
  for (const v of [THEME.bg, THEME.surface, THEME.text, THEME.muted, THEME.accent]) expect(read("../popup.css")).toContain(v);
});
```

- [ ] **Step 2: Run to see it fail** — `cd extension && pnpm test -- theme`, expected FAIL (module missing).

- [ ] **Step 3: Create `theme.ts`** with the constants. `BADGE = { bg: THEME.accent, text: THEME.onAccent }`, `BADGE_ERROR = { bg: THEME.error, text: THEME.onError }`.

- [ ] **Step 4: Use it.** In `content.ts` build `SHADOW_CSS` and `VIDEO_CSS` with template interpolation of `THEME.*`: bar and hover button body `surface`, border `accent`, hover `surface2`, list and message `bg` with `border` hairline, close and message text `muted`, `.dl` fill `accent` with `onAccent` text and `accentLight` hover. In `background.ts` replace `BADGE_COLOR` with `BADGE`/`BADGE_ERROR`, and call `ext.action.setBadgeTextColor?.({ color })` next to each `setBadgeBackgroundColor` (optional call: older browsers lack it). In `popup.css` set the same five `:root` values, `button.primary { color: #000000; font-weight: 700 }`, radius 4px, and add the Task 2 press and fine-pointer hover rules.

- [ ] **Step 5: Run checks** — `cd extension && pnpm test && pnpm check && pnpm build`, expected PASS and `dist/chromium`, `dist/firefox` built.

- [ ] **Step 6: Commit** — `git add extension/src && git commit -m "feat(extension): black and orange theme, readable badge"`

### Task 4: App icon (approved; done first, see commit "feat: orange app icon")

**Files:**
- Modify: `app/app-icon.svg` (rect fill `#ffa31a`, both strokes `#000000`; geometry unchanged)
- Regenerate: everything under `app/src-tauri/icons/`

- [ ] **Step 1: Edit the SVG** fills as above.
- [ ] **Step 2: Regenerate** — `cd app && pnpm install && pnpm tauri icon app-icon.svg`. Expected: every file in `app/src-tauri/icons/` rewritten, no new files outside it.
- [ ] **Step 3: Check** — `git status --short app/src-tauri/icons | wc -l` equals the previous file count (52); open `icons/128x128.png` and confirm an orange square with a dark arrow. `cd extension && pnpm build` copies the new 32 and 128 px icons.
- [ ] **Step 4: Commit** — `git add app/app-icon.svg app/src-tauri/icons && git commit -m "feat: orange app icon"`

### Task 5: Visual check and design system

**Files:** none in the repo (screenshots and the artifact live in the scratchpad).

- [ ] **Step 1: Screenshot the UI.** `cd app && pnpm dev`, open `http://localhost:1420` (the Vite port in `vite.config.ts`) in Playwright Chromium at 1280×800, screenshot the main window and the Add download dialog. Tauri calls fail in a plain browser, so expect the error banner; it is part of what to check. If the window renders blank, use the design system previews from Step 3 instead.
- [ ] **Step 2: Review the screenshots** against the Review Focus list (selected vs hover row, primary vs default button, banner legibility).
- [ ] **Step 3: Publish a new Design System artifact** "ODM Black & Orange" from the Design System type: copy the first system's files, replace `tokens.json` colours with the spec table, update README colour and motion sections, `bundle.css` press and dialog rules, and redraw the cover in black and orange. The first artifact (https://claude.ai/artifact/55NEnWWkZF3na9xQiQuTJz) is not touched.

### Task 6: Push

- [ ] **Step 1:** `git log --oneline master..claude/theme-black-orange` shows only this plan's commits.
- [ ] **Step 2:** `git push -u origin claude/theme-black-orange`. Expected: new remote branch; `origin/claude/relaxed-shannon-u4f79v` unchanged.
