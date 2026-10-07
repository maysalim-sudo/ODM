import { describe, expect, test } from "vitest";
import appCss from "../app.css?raw";
import appSvelte from "../App.svelte?raw";

// Guards the black and orange palette (docs/superpowers/specs/2026-10-07-odm-black-orange-theme-design.md).

const components = import.meta.glob<string>("../components/*.svelte", { query: "?raw", import: "default", eager: true });

const OLD = [
  "#6366f1", "#818cf8", "#0f0f14", "#14141c", "#1a1a24", "#1e1e28", "#e0e0e6",
  "#6b6b80", "#ef4444", "#22c55e", "#f59e0b", "#7578f3", "#1f1f2b", "#2a2a3a",
];

function rootVars(css: string): Record<string, string> {
  const block = /:root\s*\{([^}]*)\}/.exec(css)?.[1] ?? "";
  const out: Record<string, string> = {};
  for (const m of block.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) out[m[1]] = m[2].trim();
  return out;
}

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

const styleOf = (src: string) => /<style>([\s\S]*?)<\/style>/.exec(src)?.[1] ?? "";

const vars = rootVars(appCss);
const componentStyles: [string, string][] = [
  ...Object.entries(components).map(([file, src]): [string, string] => [file, styleOf(src)]),
  ["App.svelte", styleOf(appSvelte)],
];
const allAppSource = [appCss, appSvelte, ...Object.values(components)].join("\n");
const sidebarSrc = styleOf(components["../components/Sidebar.svelte"]);
const chunkMapSrc = styleOf(components["../components/ChunkMap.svelte"]);
const modalSrc = styleOf(components["../components/Modal.svelte"]);

describe("palette", () => {
  test("root variables match the spec palette", () => {
    expect(vars).toMatchObject({
      "--bg": "#000000", "--surface": "#1b1b1b", "--surface-2": "#2f2f2f", "--border": "#3a3a3a",
      "--text": "#ffffff", "--muted": "#a0a0a0", "--accent": "#ffa31a", "--accent-light": "#ffb84d",
      "--on-accent": "#000000", "--error": "#ff6b6b", "--error-tint": "#291111", "--success": "#34d399",
      "--success-tint": "#082218", "--warn": "#a78bfa", "--warn-tint": "#211c32", "--info": "#60a5fa",
      "--idle": "#6b7280", "--selected": "#291a04", "--selected-hover": "#382406", "--radius": "4px",
    });
  });

  test.each([
    ["--text", "--bg", 4.5], ["--text", "--selected", 10], ["--text", "--selected-hover", 10], ["--muted", "--selected", 4.5],
    ["--muted", "--bg", 4.5], ["--muted", "--surface", 4.5], ["--muted", "--surface-2", 4.5],
    ["--accent", "--surface-2", 4.5], ["--on-accent", "--accent", 4.5], ["--on-accent", "--accent-light", 4.5],
    ["--error", "--surface-2", 4.5], ["--error", "--error-tint", 4.5], ["--success", "--success-tint", 4.5],
    ["--warn", "--warn-tint", 4.5], ["--warn", "--surface-2", 4.5], ["--info", "--surface", 4.5],
    ["--accent-light", "--surface", 3],
  ] as const)("%s on %s ≥ %d:1", (fg, bg, min) => {
    expect(contrast(vars[fg], vars[bg])).toBeGreaterThanOrEqual(min);
  });

  test("selected-hover differs from surface-2", () => {
    expect(vars["--selected-hover"]).not.toBe(vars["--surface-2"]);
  });
});

describe("components", () => {
  test("no colour literals in component styles", () => {
    for (const [file, src] of componentStyles) expect(src, file).not.toMatch(/#[0-9a-f]{3,8}\b|rgba?\(/i);
  });

  test("no old palette values left", () => {
    for (const old of OLD) expect(allAppSource.toLowerCase()).not.toContain(old);
  });

  test("active sidebar item is an orange pill", () => {
    expect(sidebarSrc).toMatch(/button\.active\s*\{[^}]*background:\s*var\(--accent\)/);
  });

  test("failed chunk keeps its outline", () => {
    expect(chunkMapSrc).toMatch(/\.seg\.failed\s*\{[^}]*outline:/);
  });
});

describe("interaction", () => {
  test("ease-out token", () => expect(vars["--ease-out"]).toBe("cubic-bezier(0.23, 1, 0.32, 1)"));
  test("no transition: all", () => expect(allAppSource).not.toMatch(/transition:\s*all\b/));
  test("press feedback", () => expect(appCss).toMatch(/button:active:not\(:disabled\)\s*\{[^}]*scale\(0\.97\)/));
  test("hover gated to fine pointers", () => expect(appCss).toMatch(/@media \(hover: hover\) and \(pointer: fine\)/));
  test("forced colours keep button edges", () => expect(appCss).toMatch(/@media \(forced-colors: active\)[\s\S]*ButtonText/));
  test("reduced motion drops scale", () => expect(appCss).toMatch(/prefers-reduced-motion: reduce[\s\S]*transform:\s*none/));
  test("dialog enters with @starting-style", () => expect(modalSrc).toMatch(/@starting-style/));
});

describe("progress states", () => {
  const bar = components["../components/ProgressBar.svelte"];
  test("paused and cancelled bars use --idle", () => {
    expect(bar).toMatch(/class:idle=\{state === "paused" \|\| state === "cancelled"\}/);
    expect(styleOf(bar)).toMatch(/\.idle \.fill\s*\{[^}]*background:\s*var\(--idle\)/);
  });
  test("queued bars use --info", () => {
    expect(bar).toMatch(/class:queued=\{state === "queued"\}/);
    expect(styleOf(bar)).toMatch(/\.queued \.fill\s*\{[^}]*background:\s*var\(--info\)/);
  });
});
