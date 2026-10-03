import { describe, expect, it } from "vitest";
import { withTimeout } from "./timeout";

describe("withTimeout", () => {
  it("resolves the value", async () => {
    expect(await withTimeout(Promise.resolve(5), 50)).toBe(5);
  });
  it("gives null on timeout", async () => {
    expect(await withTimeout(new Promise<number>(() => {}), 20)).toBeNull();
  });
  it("gives null on rejection", async () => {
    expect(await withTimeout(Promise.reject(new Error("x")), 50)).toBeNull();
  });
});
