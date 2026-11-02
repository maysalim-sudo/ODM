<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import { groupRows, remaining, remainingLabel, type GroupKey, type RowAction } from "../lib/home";
  import type { Row } from "../lib/model";
  import DownloadCard from "./DownloadCard.svelte";

  let {
    onaction,
    onremove,
  }: {
    onaction: (row: Row, a: RowAction) => void;
    onremove: () => void;
  } = $props();

  const NAMES: Record<GroupKey, string> = { needs: "Needs you", active: "Downloading", next: "Up next", done: "Finished" };

  const flat = $derived(downloads.filter.status !== "all");
  const waiting = $derived(downloads.visible.filter((r) => downloads.waitsForChoice(r.id)).map((r) => r.id));
  const groups = $derived(flat ? [] : groupRows(downloads.visible, waiting));
  const shown = $derived(flat ? downloads.visible : groups.flatMap((g) => g.rows));
  const order = $derived(shown.map((r) => r.id));

  function clearFinished(rows: Row[]) {
    downloads.selected = new Set(rows.map((r) => r.id));
    onremove();
  }
</script>

{#snippet card(row: Row)}
  <DownloadCard
    {row}
    onaction={(a) => onaction(row, a)}
    onselect={(mode) => downloads.select(row.id, mode, order)}
  />
{/snippet}

<div class="list">
  {#if shown.length === 0}
    <p class="empty">
      {downloads.rows.size === 0 ? "No downloads yet. Paste a link to start one." : "Nothing here right now."}
    </p>
  {:else if flat}
    <ul>
      {#each shown as row (row.id)}{@render card(row)}{/each}
    </ul>
  {:else}
    {#each groups as g (g.key)}
      {@const left = g.key === "active" || g.key === "next" ? remainingLabel(remaining(g.rows)) : null}
      <section aria-label={NAMES[g.key]}>
        <h2>
          <span>{NAMES[g.key]}</span>
          <span class="n">{g.rows.length}</span>
          {#if left}<span class="n">· {left}</span>{/if}
          {#if g.key === "active"}
            <button class="link" onclick={() => downloads.run(api.pauseAll)}>Pause all</button>
          {:else if g.key === "done"}
            <button class="link" onclick={() => clearFinished(g.rows)}>Clear</button>
          {/if}
        </h2>
        <ul>
          {#each g.rows as row (row.id)}{@render card(row)}{/each}
        </ul>
      </section>
    {/each}
  {/if}
</div>

<style>
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 2px 20px 12px;
  }
  .empty {
    margin: 80px auto;
    color: var(--muted);
    text-align: center;
  }
  ul {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 12px 2px 8px;
    color: var(--muted);
    font-size: 12px;
    font-weight: 600;
  }
  section:first-child h2 {
    margin-top: 4px;
  }
  .n {
    font-weight: 400;
  }
  .link {
    margin-left: auto;
    padding: 0 2px;
    border-color: transparent;
    background: transparent;
    color: var(--muted);
    font-size: 12px;
  }
</style>
