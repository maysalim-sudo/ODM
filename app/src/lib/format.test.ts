import { describe, expect, it } from "vitest";
import { formatBytes, formatEta, formatSpeed, joinPath, percent, stateLabel } from "./format";

describe("formatBytes", () => {
  it("picks a readable unit", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1536 * 1024)).toBe("1.5 MB");
    expect(formatBytes(3 * 1024 ** 3)).toBe("3.00 GB");
  });
});

describe("formatSpeed", () => {
  it("shows a dash when idle", () => {
    expect(formatSpeed(0)).toBe("—");
    expect(formatSpeed(2048)).toBe("2.0 KB/s");
  });
});

describe("formatEta", () => {
  it("uses the two largest units", () => {
    expect(formatEta(null)).toBe("—");
    expect(formatEta(0)).toBe("0 s");
    expect(formatEta(59)).toBe("59 s");
    expect(formatEta(61)).toBe("1 min 1 s");
    expect(formatEta(3700)).toBe("1 h 1 min");
    expect(formatEta(90000)).toBe("1 d 1 h");
  });
});

describe("percent", () => {
  it("is a clamped whole-number-friendly value", () => {
    expect(percent(50, 200)).toBe(25);
    expect(percent(300, 200)).toBe(100);
  });
  it("percent_is_null_without_total", () => {
    expect(percent(123, null)).toBeNull();
    expect(percent(0, 0)).toBeNull();
  });
});

describe("stateLabel", () => {
  it("is sentence case", () => {
    expect(stateLabel("downloading")).toBe("Downloading");
    expect(stateLabel("probing")).toBe("Starting");
  });
});

describe("joinPath", () => {
  it("uses the separator the directory uses", () => {
    expect(joinPath("C:\\Users\\me\\Downloads", "a.zip")).toBe("C:\\Users\\me\\Downloads\\a.zip");
    expect(joinPath("/home/me", "a.zip")).toBe("/home/me/a.zip");
  });
  it("does not double a trailing separator", () => {
    expect(joinPath("C:\\dl\\", "a.zip")).toBe("C:\\dl\\a.zip");
    expect(joinPath("/dl/", "a.zip")).toBe("/dl/a.zip");
  });
});
