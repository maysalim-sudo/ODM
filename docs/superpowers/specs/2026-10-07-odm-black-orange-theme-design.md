# ODM black and orange theme — design

**Status:** agreed in conversation on 2026-10-07; icon decision pending (see "Open decision").
**Branch:** `claude/theme-black-orange`. The first version stays untouched on `claude/relaxed-shannon-u4f79v`.

## Goal

Re-skin the desktop app and the browser extension from the current indigo-on-blue-black look to a black and orange look: near-black surfaces, one orange accent (#FFA31A), warm grey secondary text. Same layout, same components, same copy. ODM keeps its own name and arrow mark; it does not imitate any other site's logo or wordmark.

## Palette

Source palette supplied by the user (Material 3 dark amber scheme plus custom surface and status colours). ODM uses this subset. Variable names in `app/src/app.css` stay as they are so components keep working; new names are added where the old UI used literals or `color-mix()`.

| ODM variable | Value | Source token | Use |
|---|---|---|---|
| `--bg` | `#0f0f0f` | surface-base | window, inputs |
| `--surface` | `#1a1a1a` | surface-card | toolbar, sidebar, detail panel, dialogs, table header |
| `--surface-2` | `#262626` | surface-raised | buttons, hover, active item, empty tracks |
| `--border` | `#2d2d2d` | border-deep | hairlines (decorative) |
| `--text` | `#e5e2e1` | on-surface | primary text |
| `--muted` | `#a18d7a` | outline | labels, headers, counts, hints |
| `--accent` | `#ffa31a` | primary-container | primary button, progress fill, button hover border |
| `--accent-light` | `#ffc98e` | primary | focus ring, active sidebar label, active chunk, primary button hover |
| `--on-accent` | `#2b1700` | on-primary-fixed | label on accent and accent-light fills |
| `--error` | `#ffb4ab` | error | failed text, failed bars and chunks |
| `--error-tint` | `#352928` | error mixed 16% into bg | error banner background |
| `--success` | `#34d399` | status-shipped | completed, connected |
| `--success-tint` | `#152e25` | success mixed 16% into bg | OK pill |
| `--warn` | `#a78bfa` | status-spark | needs attention, retrying chunks (moved off amber, which is now the brand) |
| `--warn-tint` | `#2a2539` | warn mixed 18% into bg | warn pill |
| `--info` | `#60a5fa` | status-drafting | queued (new) |
| `--idle` | `#6b7280` | status-archived | paused, cancelled bars only — never text (3.96:1) |
| `--selected` | `#312411` | accent mixed 14% into bg | selected table row |
| `--selected-hover` | `#3f2d11` | accent mixed 20% into bg | selected row under the pointer |
| `--backdrop` | `rgba(0, 0, 0, 0.6)` | — | modal scrim |
| `--shadow-dialog` | `0 20px 60px rgba(0, 0, 0, 0.7)` | — | modal shadow |

Checked contrast (WCAG 2): text on bg 14.9, muted on bg 6.03 and on surface 5.48, accent-light on surface-2 10.1, on-accent on accent 8.57 and on accent-light 11.4, error on surface-2 8.9, warn on surface-2 5.56, info on surface 6.85, text on selected 11.7 and on selected-hover 10.2, each status colour on its tint 5.4 or more.

## Interaction polish (from the design-engineering review)

| Before | After | Why |
|---|---|---|
| Buttons change border instantly, no press state | `:active { transform: scale(0.97) }`, `transition: transform 160ms var(--ease-out), border-color 120ms ease, background-color 120ms ease` | Press feedback tells the user the click landed |
| Hover styles apply on touch too | Hover rules inside `@media (hover: hover) and (pointer: fine)` | No sticky hover on touch screens |
| Primary button hover only changes the (invisible) border | Primary hover fills `--accent-light` | The main action visibly responds |
| Dialogs appear with no transition | `dialog[open]` fades and scales from 0.96 over 200ms `var(--ease-out)` via `@starting-style`; scrim fades 200ms | Occasional element; prevents a jarring pop |
| — | No animation on table row selection or keyboard navigation | Used hundreds of times a day |
| Reduced motion stops bar transitions | Also drops the button scale and dialog scale; keeps opacity fades | Fewer, gentler animations rather than none |

Easing token: `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)`.

## Extension

- `popup.css` uses the same values.
- The in-page "Download video" bar and hover button (shadow DOM CSS in `content.ts`) use the same values, read from one shared module.
- Toolbar badge: background `#ffa31a` with text `#2b1700` (white on orange is only 2.1:1); the error badge uses `#ffb4ab` with `#690005` text.

## Design system

A **new** Design System artifact, "ODM Black & Orange", built from the same component set as the first one. The first artifact is left as it is.

## Open decision

App icon: replace the indigo square with an orange (#ffa31a) square and a near-black (#0f0f0f) arrow, and regenerate every size. The extension copies its icons from the app, so it follows. Task 4 runs only if the user says yes.

## Out of scope

Layout, component structure, copy, a light theme, any reference to another site's branding.
