<script lang="ts">
  import { formatBytes } from "../lib/format";
  import { buildTree, nodeState, toggle, type FileNode } from "../lib/fileTree";
  import type { TorrentFile } from "../lib/types";

  /**
   * A torrent's files as a tree with tick boxes. Folders tick all their files. With
   * `progress`, each file shows how much of it is on disk.
   */
  let {
    files,
    selected,
    onchange,
    progress = false,
    disabled = false,
  }: {
    files: TorrentFile[];
    selected: Set<number>;
    onchange: (next: Set<number>) => void;
    progress?: boolean;
    disabled?: boolean;
  } = $props();

  const tree = $derived(buildTree(files));
  const byIdx = $derived(new Map(files.map((f) => [f.idx, f])));
  let collapsed = $state<Set<string>>(new Set());

  function flip(path: string) {
    const next = new Set(collapsed);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    collapsed = next;
  }

  function pct(n: FileNode): number | null {
    if (n.idx === undefined) return null;
    const f = byIdx.get(n.idx);
    return f && f.size > 0 ? Math.min(100, (f.done / f.size) * 100) : null;
  }
</script>

{#snippet item(n: FileNode, depth: number)}
  {@const state = nodeState(selected, n)}
  {@const folder = n.idx === undefined}
  <li>
    <div class="line" style:padding-left={`${depth * 18}px`}>
      {#if folder}
        <button
          type="button"
          class="twisty"
          aria-label={collapsed.has(n.path) ? `Open ${n.name}` : `Close ${n.name}`}
          aria-expanded={!collapsed.has(n.path)}
          onclick={() => flip(n.path)}>{collapsed.has(n.path) ? "▸" : "▾"}</button
        >
      {:else}
        <span class="twisty"></span>
      {/if}
      <label class="name" title={n.path}>
        <input
          type="checkbox"
          checked={state === "on"}
          indeterminate={state === "mixed"}
          {disabled}
          onchange={(e) => onchange(toggle(selected, n, (e.currentTarget as HTMLInputElement).checked))}
        />
        <span class:folder>{n.name}</span>
      </label>
      {#if progress && !folder}
        {@const p = pct(n)}
        <span class="bar" aria-hidden="true"><span class="fill" style:width={`${p ?? 0}%`}></span></span>
        <span class="pct">{p === null ? "" : `${Math.floor(p)}%`}</span>
      {/if}
      <span class="size">{formatBytes(n.size)}</span>
    </div>
    {#if folder && !collapsed.has(n.path)}
      <ul>
        {#each n.children as c (c.path)}
          {@render item(c, depth + 1)}
        {/each}
      </ul>
    {/if}
  </li>
{/snippet}

<ul class="tree" aria-label="Files">
  {#each tree as n (n.path)}
    {@render item(n, 0)}
  {/each}
</ul>

<style>
  .tree,
  .tree ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .tree {
    max-height: 260px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px 0;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-right: 8px;
    min-height: 26px;
  }
  .twisty {
    width: 18px;
    flex: none;
    padding: 0;
    border: none;
    background: none;
    color: var(--muted);
  }
  .name {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }
  .name span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .folder {
    font-weight: 600;
  }
  .size,
  .pct {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    flex: none;
  }
  .size {
    min-width: 72px;
    text-align: right;
  }
  .pct {
    min-width: 36px;
    text-align: right;
  }
  .bar {
    width: 70px;
    height: 6px;
    flex: none;
    border-radius: 3px;
    background: var(--surface-2);
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--accent);
  }
</style>
