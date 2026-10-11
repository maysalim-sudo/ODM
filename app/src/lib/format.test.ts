import { describe, expect, it } from "vitest";
import { formatBytes, formatEta, formatSpeed, joinPath, percent, queueWaitLabel, stateLabel, statusLabel } from "./format";
import type { Schedule } from "./types";

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

describe("statusLabel", () => {
  it("says why a download is paused", () => {
    expect(statusLabel("paused", null)).toBe("Paused");
    expect(statusLabel("paused", "schedule")).toBe("Paused by schedule");
    expect(statusLabel("paused", "exists")).toBe("Waiting for your choice");
    expect(statusLabel("downloading", "schedule")).toBe("Downloading");
    expect(statusLabel("paused", "something new")).toBe("Paused");
  });
});

describe("queueWaitLabel", () => {
  // 2026-10-12 is a Monday.
  const at = (day: number, h: number, m = 0) => new Date(2026, 9, 12 + day, h, m);
  const sched = (from: string, to: string, days = [true, true, true, true, true, true, true]): Schedule => ({
    enabled: true,
    days,
    from,
    to,
  });

  it("is null when the queue may run", () => {
    expect(queueWaitLabel({ ...sched("01:00", "06:00"), enabled: false }, at(0, 12))).toBeNull();
    expect(queueWaitLabel(sched("01:00", "06:00"), at(0, 3))).toBeNull();
    expect(queueWaitLabel(sched("22:00", "06:00"), at(1, 2)), "the night window from Monday").toBeNull();
    expect(queueWaitLabel({ ...sched("01:00", "06:00"), days: [false, false, false, false, false, false, false] }, at(0, 12))).toBeNull();
  });

  it("gives today's start time", () => {
    expect(queueWaitLabel(sched("22:00", "06:00"), at(0, 12))).toBe("Waiting for the schedule (starts 22:00)");
    expect(queueWaitLabel(sched("13:30", "14:00"), at(0, 13, 29))).toBe("Waiting for the schedule (starts 13:30)");
  });

  it("names the day when the start is not today", () => {
    expect(queueWaitLabel(sched("01:00", "06:00"), at(0, 12))).toBe("Waiting for the schedule (starts Tue 01:00)");
    const weekend = [false, false, false, false, false, true, true];
    expect(queueWaitLabel(sched("01:00", "06:00", weekend), at(0, 12))).toBe("Waiting for the schedule (starts Sat 01:00)");
    const mondays = [true, false, false, false, false, false, false];
    expect(queueWaitLabel(sched("01:00", "06:00", mondays), at(0, 12))).toBe("Waiting for the schedule (starts Mon 01:00)");
  });

  it("treats the same from and to as the whole day", () => {
    const sundays = [false, false, false, false, false, false, true];
    expect(queueWaitLabel(sched("00:00", "00:00", sundays), at(0, 12))).toBe("Waiting for the schedule (starts Sun 00:00)");
  });

  it("is shown for queued rows only", () => {
    const wait = "Waiting for the schedule (starts 22:00)";
    expect(statusLabel("queued", null, wait)).toBe(wait);
    expect(statusLabel("queued", null, null)).toBe("Queued");
    expect(statusLabel("paused", null, wait)).toBe("Paused");
    expect(statusLabel("downloading", null, wait)).toBe("Downloading");
  });
});
