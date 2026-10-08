import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";
import { BADGE, BADGE_ERROR, THEME } from "./theme";

const OLD = [
  "#6366f1", "#818cf8", "#0f0f14", "#14141c", "#1a1a24", "#1e1e28", "#e0e0e6",
  "#6b6b80", "#ef4444", "#22c55e", "#f59e0b", "#7578f3", "#1f1f2b", "#2a2a3a",
];

const read = (rel: string) => readFileSync(new URL(rel, import.meta.url), "utf8").toLowerCase();

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const v = parseInt(hex.slice(i, i + 2), 16) / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

describe("extension theme", () => {
  test("palette", () => {
    expect(THEME).toMatchObject({
      bg: "#000000", surface: "#1b1b1b", accent: "#ffa31a", accentLight: "#ffb84d", onAccent: "#000000",
      text: "#ffffff", muted: "#a0a0a0", error: "#ff6b6b", onError: "#000000",
    });
  });

  test("badge text readable", () => {
    expect(contrast(BADGE.text, BADGE.bg)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(BADGE_ERROR.text, BADGE_ERROR.bg)).toBeGreaterThanOrEqual(4.5);
  });

  test("in-page bar text readable", () => {
    expect(contrast(THEME.text, THEME.surface)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(THEME.muted, THEME.bg)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(THEME.onAccent, THEME.accent)).toBeGreaterThanOrEqual(4.5);
  });

  test("no old palette values in extension sources", () => {
    for (const f of ["../content.ts", "../background.ts", "../popup.css"]) {
      for (const old of OLD) expect(read(f), f).not.toContain(old);
    }
  });

  test("popup.css uses the theme values", () => {
    const css = read("../popup.css");
    for (const v of [THEME.bg, THEME.surface, THEME.text, THEME.muted, THEME.accent]) expect(css).toContain(v);
  });

  test("badge text colour is set", () => {
    expect(read("../background.ts")).toContain("setbadgetextcolor");
  });

  test("switch colours come from the theme", () => {
    const css = read("../popup.css");
    expect(css).toMatch(/\.switch input:checked \+ \.track\s*\{[^}]*background:\s*var\(--accent\)/);
    expect(css).toMatch(/\.switch \.track\s*\{[^}]*background:\s*var\(--surface-2\)/);
    expect(css).toMatch(/\.switch input:focus-visible \+ \.track\s*\{[^}]*outline:\s*2px solid var\(--accent-light\)/);
  });

  test("switch pairs are readable", () => {
    expect(contrast(THEME.onAccent, THEME.accent)).toBeGreaterThanOrEqual(3);
    expect(contrast(THEME.muted, THEME.surface2)).toBeGreaterThanOrEqual(3);
  });

  test("popup has the detection card", () => {
    const html = read("../popup.html");
    expect(html).toContain('id="detection"');
    expect(html).toContain("video detection");
  });
});
