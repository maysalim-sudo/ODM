import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const read = (rel: string) => readFileSync(new URL(rel, import.meta.url), "utf8");
const bg = read("../background.ts");

describe("background wiring", () => {
  test("both webRequest listeners check detection", () => {
    expect(bg.match(/await detection\.isOn\(/g)?.length).toBeGreaterThanOrEqual(2);
  });
  test("background answers odm-detection-state", () => expect(bg).toContain('case "odm-detection-state"'));
  test("background reacts to storage changes", () => expect(bg).toMatch(/storage\.onChanged\.addListener/));
  test("list-streams is empty when off", () => expect(bg).toMatch(/case "odm-list-streams"[\s\S]{0,400}isOn/));
});
