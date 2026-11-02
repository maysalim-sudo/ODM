import { describe, expect, it } from "vitest";
import { BAR_DEFAULT, clampBar, nudgeBar, readBar } from "./barPosition";

const bar = { width: 200, height: 40 };
const view = { width: 800, height: 600 };

describe("clampBar", () => {
  it("brings a position saved on a larger screen back into view", () => {
    expect(clampBar({ right: 5000, bottom: 5000 }, bar, view)).toEqual({ right: 592, bottom: 552 });
  });
  it("keeps the bar off the top-right and bottom-right edges", () => {
    expect(clampBar({ right: -40, bottom: 2 }, bar, view)).toEqual({ right: 8, bottom: 8 });
  });
  it("leaves the default place alone", () => {
    expect(clampBar(BAR_DEFAULT, bar, view)).toEqual({ right: 16, bottom: 16 });
  });
  it("falls back to the margin when the window is smaller than the bar", () => {
    expect(clampBar({ right: 50, bottom: 50 }, bar, { width: 100, height: 30 })).toEqual({ right: 8, bottom: 8 });
  });
});

describe("nudgeBar", () => {
  const p = { right: 100, bottom: 100 };
  it("moves 16 px for each arrow key", () => {
    expect(nudgeBar(p, "ArrowLeft")).toEqual({ right: 116, bottom: 100 });
    expect(nudgeBar(p, "ArrowRight")).toEqual({ right: 84, bottom: 100 });
    expect(nudgeBar(p, "ArrowUp")).toEqual({ right: 100, bottom: 116 });
    expect(nudgeBar(p, "ArrowDown")).toEqual({ right: 100, bottom: 84 });
  });
  it("ignores other keys", () => {
    expect(nudgeBar(p, "Enter")).toBeNull();
  });
});

describe("readBar", () => {
  it("keeps a saved position", () => {
    expect(readBar({ right: 40, bottom: 60 })).toEqual({ right: 40, bottom: 60 });
  });
  it("uses the default for anything else", () => {
    for (const v of [undefined, null, "x", { right: 1 }, { right: "1", bottom: 2 }, { right: Number.NaN, bottom: 2 }]) {
      expect(readBar(v)).toEqual(BAR_DEFAULT);
    }
  });
});
