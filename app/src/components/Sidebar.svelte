<script lang="ts">
  import { downloads } from "../lib/downloads.svelte";
  import { integration } from "../lib/integration.svelte";
  import { CATEGORIES, CATEGORY_LABELS, integrationProblem, relativeTime, type Filter } from "../lib/model";

  let { onbrowser }: { onbrowser: () => void } = $props();

  const st = $derived(integration.status);
  const attention = $derived(st !== null && integrationProblem(st) !== null);
  const label = $derived(
    st?.last_contact_ms != null
      ? `Browser: connected, ${relativeTime(st.last_contact_ms, integration.now)}`
      : "Browser: not connected yet",
  );

  const status: { key: Filter; label: string }[] = [
    { key: "all", label: "All downloads" },
    { key: "active", label: "Active" },
    { key: "queued", label: "Queued" },
    { key: "completed", label: "Completed" },
    { key: "failed", label: "Failed" },
  ];

</script>

<div class="side">
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
      <span>{CATEGORY_LABELS[c]}</span>
      <span class="count">{downloads.counts[key] ?? 0}</span>
    </button>
  {/each}
</nav>
<button class="chip" onclick={onbrowser} title="Browser connection">
  <span class="dot" class:warn={attention} aria-hidden="true"></span>
  <span>{label}</span>
  {#if attention}<span class="sr">Needs attention</span>{/if}
</button>
</div>

<style>
  .side {
    width: 200px;
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-right: 1px solid var(--border);
  }
  nav {
    flex: 1;
    min-height: 0;
    padding: 8px;
    overflow-y: auto;
  }
  .chip {
    justify-content: flex-start;
    align-items: center;
    gap: 8px;
    width: auto;
    margin: 0 8px 8px;
    padding: 6px 8px;
    border-color: var(--border);
    background: var(--surface-2);
    color: var(--muted);
    font-size: 12px;
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--success);
  }
  .dot.warn {
    background: var(--warn);
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
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
    background: var(--accent);
    border-color: var(--accent);
    color: var(--on-accent);
  }
  button.active .count {
    color: var(--on-accent);
  }
  /* Filters are clicked constantly: no press motion. */
  nav button {
    transition: none;
  }
  nav button:active:not(:disabled) {
    transform: none;
  }
  .count {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
</style>
