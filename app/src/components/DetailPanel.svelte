<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import { queueSchedule } from "../lib/queueSchedule.svelte";
  import { formatBytes, formatEta, formatSpeed, joinPath, statusLabel } from "../lib/format";
  import { canRefresh, canResume, isRiskyFile, needsVideoTools, progressPct } from "../lib/model";
  import { checkSpeed } from "../lib/settingsPage";
  import type { Detail } from "../lib/types";
  import ChunkMap from "./ChunkMap.svelte";
  import ToolsCard from "./ToolsCard.svelte";

  let {
    id,
    onclose,
    onrefresh,
    onrisky,
  }: { id: string | null; onclose: () => void; onrefresh: () => void; onrisky: () => void } = $props();

  let detail = $state<Detail | null>(null);
  let copied = $state(false);
  /** The video tools were just installed, so the failed download can be retried. */
  let toolsReady = $state(false);

  const row = $derived(id ? downloads.rows.get(id) : undefined);
  // Live chunks while it runs; the stored ones otherwise.
  const chunks = $derived(row?.chunks ?? detail?.chunks ?? []);
  const pct = $derived(row && (row.segments || row.size) ? progressPct(row) : null);
  // A primitive, so progress ticks (which replace the row object) do not re-run the fetch.
  const rowState = $derived(row?.state);

  // Show nothing stale when switching to another download.
  $effect(() => {
    void id;
    detail = null;
    toolsReady = false;
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

  // The own speed limit: a draft while "Custom" is being typed, applied on Enter or blur.
  let limitCustom = $state(false);
  let limitDraft = $state<number | null>(null);
  let limitError = $state<string | null>(null);
  const rowLimit = $derived(row?.speed_limit_kbps ?? null);

  $effect(() => {
    void id;
    limitCustom = rowLimit !== null;
    limitDraft = rowLimit;
    limitError = null;
  });

  async function setLimit(kbps: number | null) {
    if (!row) return;
    const target = row.id;
    if (await downloads.run(() => api.setDownloadLimit(target, kbps))) {
      // The list row is refreshed by the next upsert; reload it now so the panel shows it.
      void downloads.reload();
    }
  }

  function chooseLimit(e: Event) {
    const custom = (e.currentTarget as HTMLSelectElement).value === "custom";
    limitCustom = custom;
    limitError = null;
    if (!custom) void setLimit(null);
    else if (limitDraft === null) limitDraft = 512;
  }

  function applyLimit() {
    limitError = checkSpeed(limitDraft, "kb");
    if (limitError || limitDraft === null) return;
    const kbps = Math.round(limitDraft);
    if (kbps !== rowLimit) void setLimit(kbps);
  }

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

    {#if row.kind !== "hls" && row.kind !== "site"}
      <section>
        <ChunkMap {chunks} total={row.size} />
      </section>
    {/if}

    <dl>
      <dt>Status</dt>
      <dd class:bad={row.state === "failed"}>{statusLabel(row.state, row.pause_reason, queueSchedule.wait)}</dd>
      {#if row.error_message}
        <dt>Problem</dt>
        <dd class="bad wrap">{row.error_message}</dd>
      {/if}
      <dt>Downloaded</dt>
      <dd>
        {formatBytes(row.bytes)}{row.size === null ? "" : ` of ${formatBytes(row.size)}`}{pct === null ? "" : ` (${pct.toFixed(1)}%)`}
      </dd>
      {#if row.kind === "hls" && row.segments}
        <dt>Segments</dt>
        <dd>{row.segments.done} of {row.segments.total}</dd>
      {/if}
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
      {#if row.kind === "site"}
        <dt>Method</dt>
        <dd>Downloaded with yt-dlp</dd>
      {:else}
        <dt>Connections</dt>
        <dd>Up to {row.max_connections}</dd>
      {/if}
      {#if row.kind === "file" && row.state !== "completed" && row.state !== "cancelled"}
        <dt><label for="detail-limit">Speed limit</label></dt>
        <dd class="limit">
          <select id="detail-limit" value={limitCustom ? "custom" : "none"} onchange={chooseLimit}>
            <option value="none">Unlimited</option>
            <option value="custom">Custom</option>
          </select>
          {#if limitCustom}
            <span class="limit-value">
              <input
                type="number"
                min="1"
                step="1"
                aria-label="Speed limit in KB/s"
                aria-invalid={limitError !== null}
                bind:value={limitDraft}
                onblur={applyLimit}
                onkeydown={(e) => e.key === "Enter" && applyLimit()}
              />
              KB/s
            </span>
          {/if}
          {#if limitError}<span class="bad" role="alert">{limitError}</span>{/if}
        </dd>
      {/if}
      {#if detail && detail.header_names.length > 0}
        <dt>Sent headers</dt>
        <dd class="wrap">{detail.header_names.join(", ")}</dd>
      {/if}
    </dl>

    {#if needsVideoTools(row.error_message)}
      <section class="tools">
        {#if toolsReady}
          <p class="hint">The video tools are installed.</p>
          {#if canResume(row)}
            <button class="primary" onclick={() => downloads.run(() => api.resume([row.id]))}>Retry</button>
          {/if}
        {:else}
          <ToolsCard compact oninstalled={() => (toolsReady = true)} />
        {/if}
      </section>
    {/if}

    <footer>
      <button
        disabled={row.state !== "completed"}
        onclick={() => (isRiskyFile(row.filename) ? onrisky() : downloads.run(() => api.openFile(row.id)))}
      >
        Open file
      </button>
      {#if canRefresh(row)}
        <button onclick={onrefresh}>Refresh URL</button>
      {/if}
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
    font-weight: 700;
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
  .limit {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .limit-value {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .limit-value input {
    width: 88px;
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
