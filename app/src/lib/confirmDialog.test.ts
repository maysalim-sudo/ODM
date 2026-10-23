import { describe, expect, it } from "vitest";

// The confirm dialog's effects must follow the download's id, not the row object: the row
// object is rebuilt whenever any download changes, and an effect that reads it re-runs then,
// resetting the user's file ticks, name and folder mid-choice.
describe("confirm dialog reactivity", () => {
  it("keys its effects on the id alone", async () => {
    const src = (await import("../components/ConfirmDialog.svelte?raw")).default as string;
    expect(src).toMatch(/const confirmId = \$derived\(downloads\.confirmRow\?\.id\)/);
    const effects = src.split("$effect(").slice(1).map((e) => e.slice(0, e.indexOf("});")));
    expect(effects.length).toBeGreaterThanOrEqual(2);
    for (const e of effects) {
      expect(e, e).not.toMatch(/row\?\.id|confirmRow\?\.id/);
      expect(e, e).toContain("confirmId");
    }
  });
});
