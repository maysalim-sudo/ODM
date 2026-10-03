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
  it("ignores a release without a press (window blur)", () => {
    const t = new AltTracker();
    t.up(100);
    expect(t.active(101)).toBe(false);
    t.down(200);
    t.up(300);
    t.up(1000); // a later blur must not extend the grace period
    expect(t.active(1799)).toBe(true);
    expect(t.active(1800)).toBe(false);
  });
});
