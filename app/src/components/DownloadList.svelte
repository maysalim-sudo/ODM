<script lang="ts">
  import { tick } from "svelte";
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import {
    groupRows,
    lineLabel,
    moveInQueue,
    queueOrder,
    remaining,
    remainingLabel,
    sortQueueBySize,
    type GroupKey,
    type RowAction,
  } from "../lib/home";
  import { integration } from "../lib/integration.svelte";
  import { browserConnected, type Row } from "../lib/model";
  import DownloadCard from "./DownloadCard.svelte";
  import Icon from "./Icon.svelte";
  import ProblemCard from "./ProblemCard.svelte";

  let {
    onaction,
    onremove,
    onbrowser,
  }: {
    onaction: (row: Row, a: RowAction) => void;
    onremove: () => void;
    onbrowser: () => void;
  } = $props();

  const NAMES: Record<GroupKey, string> = { needs: "Needs you", active: "Downloading", next: "Up next", done: "Finished" };

  const status = $derived(downloads.filter.status);
  const flat = $derived(status === "active" || status === "completed" || status === "failed");
  const waiting = $derived(downloads.visible.filter((r) => downloads.waitsForChoice(r.id)).map((r) => r.id));
  const groups = $derived(flat ? [] : groupRows(downloads.visible, waiting));
  const shown = $derived(flat ? downloads.visible : groups.flatMap((g) => g.rows));
  const order = $derived(shown.map((r) => r.id));

  /** The whole queue, whatever the type filter hides: the order is saved for all of it. */
  const queue = $derived(queueOrder([...downloads.rows.values()]));
  const queueIds = $derived(queue.map((r) => r.id));
  /** Up next as shown, which a type filter can make shorter than the queue. */
  const nextRows = $derived(groups.find((g) => g.key === "next")?.rows ?? []);

  let sizeDir = $state<"asc" | "desc" | null>(null);
  let dragId = $state<string | null>(null);
  let dropAt = $state<number | null>(null);
  let listEl = $state<HTMLDivElement>();

  function movesFor(i: number): RowAction[] {
    const out: RowAction[] = [];
    if (i > 0) out.push("move-top", "move-up");
    if (i < nextRows.length - 1) out.push("move-down");
    return out;
  }

  /** The queue index for showing a row at `i` of the rows on screen. */
  function queueIndex(i: number): number {
    const target = nextRows[Math.min(i, nextRows.length - 1)];
    return target ? queueIds.indexOf(target.id) : queueIds.length - 1;
  }

  async function move(id: string, to: "top" | number, refocus = false) {
    sizeDir = null;
    await downloads.reorder(moveInQueue(queueIds, id, to));
    if (refocus) {
      await tick();
      listEl?.querySelector<HTMLElement>(`[data-row="${CSS.escape(id)}"]`)?.focus();
    }
  }

  function act(row: Row, a: RowAction, i: number) {
    if (a === "move-top") void move(row.id, "top");
    else if (a === "move-up") void move(row.id, queueIndex(i - 1), true);
    else if (a === "move-down") void move(row.id, queueIndex(i + 1), true);
    else onaction(row, a);
  }

  function sortBySize() {
    const dir = sizeDir === "asc" ? "desc" : "asc";
    void downloads.reorder(sortQueueBySize(queue, dir)).then(() => (sizeDir = dir));
  }

  /** The gap the pointer is nearest to: 0 is above the first row, length is below the last. */
  function gapAt(y: number): number {
    const cards = nextRows.map((r) => listEl?.querySelector(`[data-row="${CSS.escape(r.id)}"]`));
    const i = cards.findIndex((c) => {
      const b = c?.getBoundingClientRect();
      return b !== undefined && y < b.top + b.height / 2;
    });
    return i === -1 ? nextRows.length : i;
  }

  function startDrag(e: PointerEvent, id: string) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    dragId = id;
    dropAt = gapAt(e.clientY);
    const moved = (ev: PointerEvent) => (dropAt = gapAt(ev.clientY));
    const finish = (commit: boolean) => {
      window.removeEventListener("pointermove", moved);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("keydown", key);
      const gap = dropAt;
      dragId = null;
      dropAt = null;
      if (!commit || gap === null) return;
      const from = nextRows.findIndex((r) => r.id === id);
      if (from === -1 || gap === from || gap === from + 1) return;
      void move(id, queueIndex(gap > from ? gap - 1 : gap));
    };
    const up = () => finish(true);
    const key = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") {
        ev.stopPropagation();
        finish(false);
      }
    };
    window.addEventListener("pointermove", moved);
    window.addEventListener("pointerup", up);
    window.addEventListener("keydown", key, true);
  }

  async function pickTorrent() {
    const path = await api.pickTorrentFile().catch(() => null);
    if (path) await downloads.run(() => api.addTorrent({ kind: "file", path }));
  }

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

{#snippet queued(row: Row, i: number)}
  <DownloadCard
    {row}
    line={lineLabel(queueIds.indexOf(row.id))}
    moves={movesFor(i)}
    grip={(e) => startDrag(e, row.id)}
    dragging={dragId === row.id}
    dropEdge={dropAt === i ? "top" : dropAt === i + 1 && i === nextRows.length - 1 ? "bottom" : null}
    onaction={(a) => act(row, a, i)}
    onselect={(mode) => downloads.select(row.id, mode, order)}
  />
{/snippet}

<div class="list" bind:this={listEl}>
  {#if downloads.loaded && downloads.rows.size === 0}
    <div class="first">
      <h1>Paste a link to start</h1>
      <p>ODM reads the link, shows you what it found and saves it to your download folder.</p>
      <div class="ways">
        <section>
          <span class="tile" aria-hidden="true"><Icon name="download" /></span>
          <h3>Any file</h3>
          <p>Paste a link above, or press Ctrl V anywhere in ODM.</p>
        </section>
        <section>
          <span class="tile" aria-hidden="true"><Icon name="globe" /></span>
          <h3>From your browser</h3>
          <p>Downloads and videos come here on their own.</p>
          {#if !browserConnected(integration.status)}<button class="next" onclick={onbrowser}>Connect browser</button>{/if}
        </section>
        <section>
          <span class="tile" aria-hidden="true"><Icon name="magnet" /></span>
          <h3>Torrents</h3>
          <p>Paste a magnet link, or drop a .torrent file on this window.</p>
          <button class="next" onclick={pickTorrent}>Choose a .torrent file</button>
        </section>
      </div>
    </div>
  {:else if shown.length === 0}
    <p class="empty">Nothing here right now.</p>
  {:else if flat}
    <ul>
      {#each shown as row (row.id)}
        {#if status === "failed"}<ProblemCard {row} onaction={(a) => onaction(row, a)} onselect={(mode) => downloads.select(row.id, mode, order)} />{:else}{@render card(row)}{/if}
      {/each}
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
          {:else if g.key === "next" && g.rows.length > 1}
            <button class="link" onclick={sortBySize}>
              Sort by size{sizeDir === "asc" ? " ↑" : sizeDir === "desc" ? " ↓" : ""}
            </button>
          {:else if g.key === "done"}
            <button class="link" onclick={() => clearFinished(g.rows)}>Clear</button>
          {/if}
        </h2>
        <ul>
          {#each g.rows as row, i (row.id)}
            {#if g.key === "next"}{@render queued(row, i)}{:else}{@render card(row)}{/if}
          {/each}
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
  .first {
    max-width: 924px;
    margin: 64px auto 0;
    text-align: center;
  }
  h1 {
    margin: 0;
    font-size: 28px;
    font-weight: 700;
  }
  .first > p {
    margin: 8px 0 24px;
    color: var(--muted);
    font-size: 14px;
  }
  .ways {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 12px;
    text-align: left;
  }
  .ways section {
    padding: 18px;
    border-radius: var(--radius-xl);
    background: var(--surface);
  }
  .tile {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius);
    background: var(--surface-2);
    color: var(--accent);
  }
  h3 {
    margin: 10px 0 4px;
    font-size: 15px;
    font-weight: 600;
  }
  .ways p {
    margin: 0;
    color: var(--muted);
  }
  .next {
    margin-top: 8px;
    padding: 0;
    border-color: transparent;
    background: transparent;
    color: var(--accent);
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
