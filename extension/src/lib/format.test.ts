import { describe, expect, it } from "vitest";
import { formatBytes, formatSpeed, statusLine } from "./format";

describe("format", () => {
  it("formats speeds", () => {
    expect(formatSpeed(0)).toBe("0 B/s");
    expect(formatSpeed(512)).toBe("512 B/s");
    expect(formatSpeed(1536)).toBe("1.5 KB/s");
    expect(formatSpeed(3565158)).toBe("3.4 MB/s");
  });
  it("formats byte sizes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(3 * 1024 * 1024)).toBe("3.0 MB");
    expect(formatBytes(5 * 1024 ** 3)).toBe("5.0 GB");
  });
  it("builds the status line", () => {
    expect(statusLine({ active: 2, queued: 1, total_speed_bps: 3565158 })).toBe("2 active · 1 queued · 3.4 MB/s");
  });
});
