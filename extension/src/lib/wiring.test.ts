import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const read = (rel: string) => readFileSync(new URL(rel, import.meta.url), "utf8");
const bg = read("../background.ts");
const content = read("../content.ts");

describe("background wiring", () => {
  test("both webRequest listeners check detection", () => {
    expect(bg.match(/await detection\.isOn\(/g)?.length).toBeGreaterThanOrEqual(2);
  });
  test("background answers odm-detection-state", () => expect(bg).toContain('case "odm-detection-state"'));
  test("background reacts to storage changes", () => expect(bg).toMatch(/storage\.onChanged\.addListener/));
  test("list-streams is empty when off", () => expect(bg).toMatch(/case "odm-list-streams"[\s\S]{0,400}isOn/));
});

describe("content wiring", () => {
  test("content asks for the detection state", () => expect(content).toContain('type: "odm-detection-state"'));
  test("content handles on/off messages", () => expect(content).toMatch(/m\?\.type === "odm-detection"/));
  test("the bar does not render while off", () => expect(content).toMatch(/const render = \(\) => \{[\s\S]{0,200}!detecting/));
  test("the hover button does not show while off", () =>
    expect(content).toMatch(/const show = \(video: HTMLVideoElement\) => \{[\s\S]{0,120}!detecting/));
});
