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
  test("magnet clicks go to ODM as torrents", () => expect(bg).toMatch(/case "odm-magnet"[\s\S]{0,200}ADD_TORRENT|case "odm-magnet"[\s\S]{0,200}addMagnet/));
  test(".torrent downloads go to ODM as torrents", () => expect(bg).toMatch(/isTorrentDownload\(view\)[\s\S]{0,400}"ADD_TORRENT"/));
  test("the context menu takes magnet links", () => expect(bg).toMatch(/onClicked[\s\S]{0,400}isMagnet\(url\)/));
});

describe("content wiring", () => {
  test("content catches magnet link clicks", () => expect(content).toMatch(/closest\??\.?\("a\[href\]"\)[\s\S]{0,200}isMagnet/));
  test("magnet clicks fall back to the browser", () => expect(content).toMatch(/location\.href = href[\s\S]{0,300}type: "odm-magnet"[\s\S]{0,200}catch\(fallBack\)/));
  test("content asks for the detection state", () => expect(content).toContain('type: "odm-detection-state"'));
  test("content handles on/off messages", () => expect(content).toMatch(/m\?\.type === "odm-detection"/));
  test("the bar does not render while off", () => expect(content).toMatch(/const render = \(\) => \{[\s\S]{0,200}!detecting/));
  test("the hover button does not show while off", () =>
    expect(content).toMatch(/const show = \(video: HTMLVideoElement\) => \{[\s\S]{0,120}!detecting/));
});
