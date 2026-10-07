/** ODM's black and orange palette, shared by the in-page buttons and the toolbar badge. popup.css repeats these values. */
export const THEME = {
  bg: "#000000",
  surface: "#1b1b1b",
  surface2: "#2f2f2f",
  border: "#3a3a3a",
  text: "#ffffff",
  muted: "#a0a0a0",
  accent: "#ffa31a",
  accentLight: "#ffb84d",
  onAccent: "#000000",
  error: "#ff6b6b",
  onError: "#000000",
} as const;

/** White badge text on orange is 2.1:1, so both badges set dark text. */
export const BADGE = { bg: THEME.accent, text: THEME.onAccent } as const;
export const BADGE_ERROR = { bg: THEME.error, text: THEME.onError } as const;
