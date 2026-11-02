import { describe, expect, it } from "vitest";
import appSvelte from "../App.svelte?raw";
import problemCard from "../components/ProblemCard.svelte?raw";
import downloadList from "../components/DownloadList.svelte?raw";

describe("the details drawer covers the paste bar", () => {
  it("Ctrl+V does not open the add panel behind it", () => {
    expect(appSvelte).toContain("blocked={dialogOpen || detailId !== null}");
  });

  it("Ctrl+N closes it first, so focus can reach the bar", () => {
    expect(appSvelte).toMatch(/key\.toLowerCase\(\) === "n"\) \{\s*e\.preventDefault\(\);\s*detailId = null;\s*addPanel\?\.focus\(\);/);
  });
});

describe("the Problems tab selects like the other cards", () => {
  it("handles Ctrl+click, Shift+click and Space", () => {
    expect(problemCard).toContain('onselect("range")');
    expect(problemCard).toContain('onselect("toggle")');
    expect(problemCard).toMatch(/e\.key === " "/);
  });

  it("marks selected rows and gets its selection from the list", () => {
    expect(problemCard).toContain("class:selected={downloads.selected.has(row.id)}");
    expect(downloadList).toMatch(/<ProblemCard[^>]*onselect=/);
  });
});
