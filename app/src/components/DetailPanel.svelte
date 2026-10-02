<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import { formatBytes, formatEta, formatSpeed, joinPath, percent, stateLabel } from "../lib/format";
  import type { Detail } from "../lib/types";
  import ChunkMap from "./ChunkMap.svelte";

  let { id, onclose }: { id: string | null; onclose: () => void } = $props();

  let detail = $state<Detail | null>(null);
  let copied = $state(false);

  const row = $derived(id ? downloads.rows.get(id) : undefined);
  // Live chunks while it runs; the stored ones otherwise.
  const chunks = $derived(row?.chunks ?? detail?.chunks ?? []);
  const pct = $derived(row ? percent(row.bytes, row.size) : null);
  // A primitive, so progress ticks (which replace the row object) do not re-run the fetch.
  const rowState = $derived(row?.state);

  // Show nothing stale when switching to another download.
  $effect(() => {
    void id;
    detail = null;
  });

  $effect(() => {
    const wanted = id;
    // Re-read when the download changes state, so stored chunks stay current.
    void rowState;
    if (!wanted) return;
    let cancelled = false;
    api
      .getDetail(wanted)
      .then((d) => {
        if (!cancelled) detail = d;
      })
      .catch((e) => {
        if (!cancelled) downloads.error = api.errorText(e);
      });
    return () => {
      cancelled = true;
    };
  });

  // The download was removed while its panel was open (not merely not loaded yet).
  $effect(() => {
    if (id && downloads.loaded && !row) onclose();
  });

  async function copyUrl() {
    if (!row) return;
    try {
      await navigator.clipboard.writeText(row.url);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch (e) {
      downloads.error = api.errorText(e);
    }
  }
</script>

{#if id && row}
  <aside aria-label="Download details">
    <header>
      <h2 title={row.filename}>{row.filename}</h2>
      <button onclick={onclose} aria-label="Close details">Close</button>
    </header>

    <section>
      <ChunkMap {chunks} total={row.size} />
    </section>

    <dl>
      <dt>Status</dt>
      <dd class:bad={row.state === "failed"}>{stateLabel(row.state)}</dd>
      {#if row.error_message}
        <dt>Problem</dt>
        <dd class="bad wrap">{row.error_message}</dd>
      {/if}
      <dt>Downloaded</dt>
      <dd>
        {formatBytes(row.bytes)}{row.size === null ? "" : ` of ${formatBytes(row.size)}`}{pct === null ? "" : ` (${pct.toFixed(1)}%)`}
      </dd>
      <dt>Speed</dt>
      <dd>{row.state === "downloading" ? formatSpeed(row.speed_bps) : "—"}</dd>
      <dt>Time left</dt>
      <dd>{row.state === "downloading" ? formatEta(row.eta_s) : "—"}</dd>
      <dt>Saved to</dt>
      <dd class="wrap">{joinPath(row.save_dir, row.filename)}</dd>
      <dt>Address</dt>
      <dd class="wrap">{row.url}</dd>
      {#if detail?.final_url && detail.final_url !== row.url}
        <dt>Redirects to</dt>
        <dd class="wrap">{detail.final_url}</dd>
      {/if}
      <dt>Connections</dt>
      <dd>Up to {row.max_connections}</dd>
      {#if detail && detail.header_names.length > 0}
        <dt>Sent headers</dt>
        <dd class="wrap">{detail.header_names.join(", ")}</dd>
      {/if}
    </dl>

    <footer>
      <button disabled={row.state !== "completed"} onclick={() => downloads.run(() => api.openFile(row.id))}>
        Open file
      </button>
      <button onclick={() => downloads.run(() => api.openFolder(row.id))}>Open folder</button>
      <button onclick={copyUrl}>{copied ? "Copied" : "Copy address"}</button>
    </footer>
  </aside>
{/if}

<style>
  aside {
    width: 360px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 12px;
    background: var(--surface);
    border-left: 1px solid var(--border);
    overflow-y: auto;
    user-select: text;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  h2 {
    margin: 0;
    font-size: 14px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  dl {
    display: grid;
    grid-template-columns: 96px 1fr;
    gap: 6px 10px;
    margin: 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    min-width: 0;
  }
  .wrap {
    overflow-wrap: anywhere;
  }
  .bad {
    color: var(--error);
  }
  footer {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: auto;
  }
</style>
