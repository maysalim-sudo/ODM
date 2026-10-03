import { describe, expect, it } from "vitest";
import { formatSpeed, statusLine } from "./format";

describe("format", () => {
  it("formats speeds", () => {
    expect(formatSpeed(0)).toBe("0 B/s");
    expect(formatSpeed(512)).toBe("512 B/s");
    expect(formatSpeed(1536)).toBe("1.5 KB/s");
    expect(formatSpeed(3565158)).toBe("3.4 MB/s");
  });
  it("builds the status line", () => {
    expect(statusLine({ active: 2, queued: 1, total_speed_bps: 3565158 })).toBe("2 active · 1 queued · 3.4 MB/s");
  });
});
