import { describe, expect, it } from "vitest";
import { AltTracker } from "./alt";

describe("AltTracker", () => {
  it("is inactive initially", () => {
    expect(new AltTracker().active(0)).toBe(false);
  });
  it("is active while held", () => {
    const t = new AltTracker();
    t.down(0);
    expect(t.active(10)).toBe(true);
  });
  it("stays active for the grace period after release", () => {
    const t = new AltTracker();
    t.down(0);
    t.up(100);
    expect(t.active(1599)).toBe(true);
    expect(t.active(1600)).toBe(false);
  });
});
