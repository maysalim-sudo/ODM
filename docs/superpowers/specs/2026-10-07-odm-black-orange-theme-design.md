# ODM black and orange theme — design

**Status:** agreed in conversation on 2026-10-07, including the orange app icon.
**Branch:** `claude/theme-black-orange`. The first version stays untouched on `claude/relaxed-shannon-u4f79v`.

## Goal

Re-skin the desktop app and the browser extension to a high-contrast black, white and orange look: true black window, flat dark-grey panels, pure white text, neutral greys, one loud orange (#FFA31A) with black text on it, bold compact type and small corners. Same layout, same components, same copy. ODM keeps its own name and arrow mark; it does not imitate any other site's logo or wordmark (no word split across an orange box).

## Palette

Variable names in `app/src/app.css` stay as they are so components keep working; new names are added where the old UI used literals or `color-mix()`.

| ODM variable | Value | Use |
|---|---|---|
| `--bg` | `#000000` | window, inputs |
| `--surface` | `#1b1b1b` | toolbar, sidebar, detail panel, dialogs, table header |
| `--surface-2` | `#2f2f2f` | buttons, hover, empty tracks |
| `--border` | `#3a3a3a` | hairlines |
| `--text` | `#ffffff` | primary text |
| `--muted` | `#a0a0a0` | labels, headers, counts, hints |
| `--accent` | `#ffa31a` | primary button, active sidebar pill, progress fill, button hover border |
| `--accent-light` | `#ffb84d` | primary button and active pill hover, focus ring, active chunk |
| `--on-accent` | `#000000` | text on accent and accent-light fills |
| `--error` | `#ff6b6b` | failed text, failed bars and chunks |
| `--error-tint` | `#291111` | error banner background (error 16% into bg) |
| `--success` | `#34d399` | completed, connected |
| `--success-tint` | `#082218` | OK pill (success 16% into bg) |
| `--warn` | `#a78bfa` | needs attention, retrying chunks (off amber, which is now the brand) |
| `--warn-tint` | `#211c32` | warn pill (warn 20% into bg) |
| `--info` | `#60a5fa` | queued |
| `--idle` | `#6b7280` | paused and cancelled bars only, never text |
| `--selected` | `#291a04` | selected table row (accent 16% into bg) |
| `--selected-hover` | `#382406` | selected row under the pointer (accent 22% into bg) |
| `--backdrop` | `rgba(0, 0, 0, 0.7)` | modal scrim |
| `--shadow-dialog` | `0 20px 60px rgba(0, 0, 0, 0.8)` | modal shadow |
| `--radius` | `4px` | was 6px; dialogs and pills keep 8px (`--radius-lg`) |

Checked contrast (WCAG 2): text on bg 21, on surface-2 13.4, on selected 16.9, on selected-hover 14.8; muted on bg 8.0, on surface 6.6, on surface-2 5.1, on selected 6.5; on-accent on accent 10.5 and on accent-light 12.2; accent on surface-2 6.7; error on surface-2 4.8; warn on surface 6.3; info on surface-2 5.3; each status colour on its tint 6.0 or more; border on bg 1.85 (decorative).

## Character

| Element | Look |
|---|---|
| Primary button | solid `--accent`, weight 700, `--on-accent` text; hover `--accent-light` |
| Other buttons | `--surface-2`, white text, weight 600; hover border `--accent` |
| Active sidebar filter | solid `--accent` pill with `--on-accent` text and count (was a grey box with an indigo label) |
| Titles, table headers | weight 700 |
| Progress bar | `--accent` fill on `--surface-2`, like a player's scrub bar |
| Corners | 4px controls, 8px dialogs and pills |
| Font | `"Segoe UI", Arial, system-ui, sans-serif` |

## Interaction polish (design-engineering review)

| Before | After | Why |
|---|---|---|
| Buttons change border instantly, no press state | `:active { transform: scale(0.97) }`, `transition: transform 160ms var(--ease-out), border-color 120ms ease, background-color 120ms ease` | Press feedback tells the user the click landed |
| Hover styles apply on touch too | Hover rules inside `@media (hover: hover) and (pointer: fine)` | No sticky hover on touch screens |
| Dialogs appear with no transition | `dialog[open]` fades and scales from 0.96 over 200ms `var(--ease-out)` via `@starting-style`; scrim fades 200ms | Occasional element; prevents a jarring pop |
| — | No animation on row selection or keyboard navigation | Used hundreds of times a day |
| Reduced motion stops bar transitions | Also drops the button and dialog scale; keeps opacity fades | Fewer, gentler animations rather than none |

Easing token: `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)`.

## Extension

- `popup.css` uses the same values and character rules.
- The in-page "Download video" bar and hover button (shadow DOM CSS in `content.ts`) use the same values from one shared module: dark body, orange border, bold, orange "Download" fill with black text.
- Toolbar badge: `#ffa31a` with `#000000` text (white on orange is 2.1:1); error badge `#ff6b6b` with `#000000` text (7.6:1).

## App icon

Same geometry as `app/app-icon.svg`; square fill `#ffa31a`, arrow and baseline `#000000`. Every size in `app/src-tauri/icons/` is regenerated from it; the extension copies its 32 and 128 px icons from there at build time.

## Design system

A **new** Design System artifact, "ODM Black & Orange". The first artifact is left as it is.

## Out of scope

Layout, component structure, copy, a light theme, any reference to another site's branding.
