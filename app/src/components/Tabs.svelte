<script lang="ts">
  import { downloads } from "../lib/downloads.svelte";
  import { CATEGORIES, CATEGORY_LABELS, type StatusFilter } from "../lib/model";
  import type { ListView } from "../lib/types";
  import Icon from "./Icon.svelte";

  let { view, onview }: { view: ListView; onview: (v: ListView) => void } = $props();

  const TABS: { key: StatusFilter; label: string }[] = [
    { key: "all", label: "All" },
    { key: "active", label: "Downloading" },
    { key: "queued", label: "Up next" },
    { key: "completed", label: "Finished" },
    { key: "failed", label: "Problems" },
  ];
</script>

<div class="bar">
  <div class="tabs" role="tablist" aria-label="Downloads">
    {#each TABS as t (t.key)}
      {@const n = downloads.counts[t.key] ?? 0}
      <button role="tab" aria-selected={downloads.filter.status === t.key} onclick={() => downloads.setStatus(t.key)}>
        {t.label}
        <span class="count" class:bad={t.key === "failed" && n > 0}>{n}</span>
      </button>
    {/each}
  </div>
  <select
    aria-label="Type"
    value={downloads.filter.category ?? ""}
    onchange={(e) => downloads.setCategory(e.currentTarget.value || null)}
  >
    <option value="">All types</option>
    {#each CATEGORIES as c (c)}
      <option value={c}>{CATEGORY_LABELS[c]} ({downloads.counts[`cat:${c}`] ?? 0})</option>
    {/each}
  </select>
  <div class="views" role="group" aria-label="View">
    <button aria-label="Cards" title="Cards" aria-pressed={view === "cards"} onclick={() => onview("cards")}><Icon name="cards" /></button>
    <button aria-label="Detailed view" title="Detailed view" aria-pressed={view === "table"} onclick={() => onview("table")}><Icon name="list" /></button>
  </div>
</div>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 20px 8px;
  }
  .tabs {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 4px;
  }
  /* Tabs are clicked constantly: no press motion. */
  button {
    transition: none;
  }
  button:active:not(:disabled) {
    transform: none;
  }
  [role="tab"] {
    display: inline-flex;
    gap: 8px;
    padding: 6px 10px;
    border-color: transparent;
    background: transparent;
    color: var(--muted);
  }
  [role="tab"][aria-selected="true"] {
    background: var(--surface-2);
    color: var(--text);
  }
  .count {
    font-weight: 400;
    color: var(--muted);
  }
  .count.bad {
    color: var(--error);
    font-weight: 600;
  }
  select {
    height: 32px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--muted);
    padding: 0 6px;
  }
  .views {
    display: flex;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .views button {
    display: grid;
    place-items: center;
    width: 30px;
    height: 26px;
    padding: 0;
    border-color: transparent;
    background: transparent;
    color: var(--muted);
  }
  .views button[aria-pressed="true"] {
    background: var(--surface-2);
    color: var(--text);
  }
</style>
