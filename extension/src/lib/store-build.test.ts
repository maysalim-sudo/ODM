import { describe, expect, it } from "vitest";
import { storeManifest } from "../../scripts/store-manifest.mjs";

describe("store manifest", () => {
  it("drops the dev key, which the stores reject", () => {
    const m = storeManifest({ name: "ODM", version: "1", key: "abc" });
    expect(m).toEqual({ name: "ODM", version: "1" });
  });
  it("does not mutate its input", () => {
    const src = { name: "ODM", key: "abc" };
    storeManifest(src);
    expect(src.key).toBe("abc");
  });
});
