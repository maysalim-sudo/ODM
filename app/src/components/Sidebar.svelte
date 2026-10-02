<script lang="ts">
  import { downloads } from "../lib/downloads.svelte";
  import { CATEGORIES, type Filter } from "../lib/model";

  const status: { key: Filter; label: string }[] = [
    { key: "all", label: "All downloads" },
    { key: "active", label: "Active" },
    { key: "queued", label: "Queued" },
    { key: "completed", label: "Completed" },
    { key: "failed", label: "Failed" },
  ];

  const categoryLabel: Record<string, string> = {
    video: "Video",
    audio: "Audio",
    document: "Documents",
    compressed: "Compressed",
    image: "Images",
    program: "Programs",
    general: "Other",
  };
</script>

<nav aria-label="Filters">
  {#each status as s (s.key)}
    <button class:active={downloads.filter === s.key} aria-current={downloads.filter === s.key ? "true" : undefined} onclick={() => downloads.setFilter(s.key)}>
      <span>{s.label}</span>
      <span class="count">{downloads.counts[s.key] ?? 0}</span>
    </button>
  {/each}
  <h2>Categories</h2>
  {#each CATEGORIES as c (c)}
    {@const key = `cat:${c}` as Filter}
    <button class:active={downloads.filter === key} aria-current={downloads.filter === key ? "true" : undefined} onclick={() => downloads.setFilter(key)}>
      <span>{categoryLabel[c]}</span>
      <span class="count">{downloads.counts[key] ?? 0}</span>
    </button>
  {/each}
</nav>

<style>
  nav {
    width: 200px;
    flex: none;
    padding: 8px;
    background: var(--surface);
    border-right: 1px solid var(--border);
    overflow-y: auto;
  }
  h2 {
    margin: 14px 8px 6px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
  }
  button {
    display: flex;
    justify-content: space-between;
    width: 100%;
    margin-bottom: 2px;
    border-color: transparent;
    background: transparent;
    text-align: left;
  }
  button.active {
    background: var(--surface-2);
    border-color: var(--border);
    color: var(--accent-light);
  }
  .count {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
</style>
