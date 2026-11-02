/** Where the on-page bar sits, measured from the window's right and bottom edges. */
export type BarPosition = { right: number; bottom: number };
type Size = { width: number; height: number };

export const BAR_KEY = "odmBarPosition";
export const BAR_DEFAULT: BarPosition = { right: 16, bottom: 16 };
const MARGIN = 8;
const NUDGE = 16;

const within = (value: number, room: number): number => Math.max(MARGIN, Math.min(value, Math.max(MARGIN, room - MARGIN)));

/** Keeps the whole bar in the window, so a place saved on a larger screen still shows. */
export function clampBar(p: BarPosition, bar: Size, view: Size): BarPosition {
  return { right: within(p.right, view.width - bar.width), bottom: within(p.bottom, view.height - bar.height) };
}

/** The place after an arrow key; null for any other key. */
export function nudgeBar(p: BarPosition, key: string): BarPosition | null {
  switch (key) {
    case "ArrowLeft":
      return { ...p, right: p.right + NUDGE };
    case "ArrowRight":
      return { ...p, right: p.right - NUDGE };
    case "ArrowUp":
      return { ...p, bottom: p.bottom + NUDGE };
    case "ArrowDown":
      return { ...p, bottom: p.bottom - NUDGE };
    default:
      return null;
  }
}

/** A stored value, or the default when it is missing or not a position. */
export function readBar(value: unknown): BarPosition {
  const p = value as Partial<BarPosition> | null | undefined;
  return Number.isFinite(p?.right) && Number.isFinite(p?.bottom) ? { right: p!.right!, bottom: p!.bottom! } : BAR_DEFAULT;
}
