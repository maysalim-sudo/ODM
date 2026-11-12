import { describe, expect, it } from "vitest";
import appSvelte from "../App.svelte?raw";
import problemCard from "../components/ProblemCard.svelte?raw";
import bottomPanel from "../components/BottomPanel.svelte?raw";
import downloadCard from "../components/DownloadCard.svelte?raw";
import downloadList from "../components/DownloadList.svelte?raw";
import actionBar from "../components/ActionBar.svelte?raw";

describe("the bottom panel replaces the details drawer", () => {
  it("is always in the window, under the list", () => {
    expect(appSvelte).toContain("<BottomPanel");
    expect(appSvelte).not.toContain("DetailPanel");
    expect(appSvelte).toContain("blocked={dialogOpen}");
  });

  it("one click on a card selects it instead of opening a drawer", () => {
    for (const src of [downloadCard, problemCard]) {
      expect(src).toContain('else onselect("one");');
      expect(src).not.toContain('onaction("details")');
    }
  });

  it("Ctrl+A selects every row shown, outside text fields", () => {
    expect(appSvelte).toMatch(/if \(typing\) return;[\s\S]*e\.key\.toLowerCase\(\) === "a"\) \{\s*e\.preventDefault\(\);\s*downloads\.selectAll\(\);/);
  });

  it("the action bar above the list replaces the floating bar, always there", () => {
    expect(appSvelte).toContain("<ActionBar onaction={selAction} />");
    expect(appSvelte).not.toContain("SelectionBar");
    expect(actionBar).not.toMatch(/\{#if rows\.length > 0\}\s*<div class="bar"/);
  });

  it("a right-click on a download opens the menu for the selection", () => {
    expect(appSvelte).toContain('closest<HTMLElement>("[data-row]")');
    expect(appSvelte).toMatch(/if \(!downloads\.selected\.has\(id\)\) downloads\.select\(id, "one"\);/);
    expect(appSvelte).toContain("items={contextMenu(downloads.selectedRows");
  });

  it("double-click opens only a finished file", () => {
    expect(appSvelte).toContain('if (row?.state === "completed") rowAction(row, "open");');
    expect(downloadCard).toContain('if (row.state === "completed") onaction("open");');
  });
});

describe("row buttons stay centred", () => {
  it("cancels the shared .actions margin on cards and in the panel", () => {
    for (const src of [downloadCard, bottomPanel]) {
      expect(src).toMatch(/\.actions \{[^}]*margin-top: 0;/);
    }
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
    expect(downloadList).toContain("onaction={(a) => onaction(row, a)} onselect={(mode) => downloads.select(row.id, mode, order)} />{:else}");
  });
});
