import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { idFromKey } from "../../scripts/chromium-id.mjs";
import { HOST_NAME } from "./ids";

describe("identity", () => {
  it("chromium-key.txt yields chromium-id.txt", () => {
    const key = readFileSync("chromium-key.txt", "utf8").trim();
    expect(idFromKey(key)).toBe(readFileSync("chromium-id.txt", "utf8").trim());
  });
  it("host name is fixed", () => {
    expect(HOST_NAME).toBe("com.odm.host");
  });
});

describe("store ids", () => {
  it("chromium-ids.json keeps the dev id in sync with chromium-id.txt", () => {
    const ids = JSON.parse(readFileSync("chromium-ids.json", "utf8"));
    expect(ids.dev).toBe(readFileSync("chromium-id.txt", "utf8").trim());
    expect(Object.keys(ids).sort()).toEqual(["chrome_web_store", "dev", "edge_addons"]);
  });
});
