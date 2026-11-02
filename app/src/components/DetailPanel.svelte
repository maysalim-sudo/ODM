<script lang="ts">
  import { tick as nextTick } from "svelte";
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import { queueSchedule } from "../lib/queueSchedule.svelte";
  import { formatBytes, formatEta, formatSpeed, joinPath } from "../lib/format";
  import { ACTION_LABELS, FIX_LABELS, rowActions, type RowAction } from "../lib/home";
  import { canRefresh, canResume, isActive, needsVideoTools, problemInfo, progressPct, rowStatus } from "../lib/model";
  import { checkSpeed } from "../lib/settingsPage";
  import type { Detail } from "../lib/types";
  import ChunkMap from "./ChunkMap.svelte";
  import Icon from "./Icon.svelte";
  import PieceMap from "./PieceMap.svelte";
  import ProgressBar from "./ProgressBar.svelte";
  import TorrentFiles from "./TorrentFiles.svelte";
  import ToolsCard from "./ToolsCard.svelte";

  let { id, onclose, onaction }: { id: string | null; onclose: () => void; onaction: (a: RowAction) => void } = $props();

  let detail = $state<Detail | null>(null);
  let copied = $state(false);
  /** The video tools were just installed, so the failed download can be retried. */
  let toolsReady = $state(false);

  const row = $derived(id ? downloads.rows.get(id) : undefined);
  const waiting = $derived(id ? downloads.waitsForChoice(id) : false);
  const problem = $derived(row?.state === "failed" ? problemInfo(row) : null);
  const primary = $derived(row ? rowActions(row, queueSchedule.wait !== null, waiting).primary : null);
  // Remove has its own button here, so a cancelled row has no main action.
  const primaryLabel = $derived(primary === "fix" && problem ? FIX_LABELS[problem.action] : primary ? ACTION_LABELS[primary] : "");
  // Live chunks while it runs; the stored ones otherwise.
  const chunks = $derived(row?.chunks ?? detail?.chunks ?? []);
  const pct = $derived(row && (row.segments || row.size) ? progressPct(row) : null);
  // A primitive, so progress ticks (which replace the row object) do not re-run the fetch.
  const rowState = $derived(row?.state);
  const isTorrent = $derived(row?.kind === "torrent");
  const fileVersion = $derived(id ? (downloads.fileLists[id] ?? 0) : 0);
  /** A running torrent's piece map and file progress change all the time: re-read them. */
  let tick = $state(0);
  $effect(() => {
    if (!isTorrent || !rowState || !isActive(rowState)) return;
    const timer = setInterval(() => (tick += 1), 2000);
    return () => clearInterval(timer);
  });
  let filesError = $state<string | null>(null);
  const selectedFiles = $derived(new Set((detail?.torrent?.files ?? []).filter((f) => f.selected).map((f) => f.idx)));

  async function changeFiles(next: Set<number>) {
    if (!row) return;
    filesError = null;
    try {
      await api.setTorrentFiles(row.id, [...next]);
    } catch (e) {
      filesError = api.errorText(e);
    }
  }

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
    void fileVersion;
    void tick;
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

  // Focus moves in while the drawer is open and goes back to where it came from.
  let drawer = $state<HTMLElement>();
  let closeButton = $state<HTMLButtonElement>();
  $effect(() => {
    if (!drawer) return;
    const before = document.activeElement as HTMLElement | null;
    void nextTick().then(() => closeButton?.focus());
    return () => before?.focus();
  });

  function onkey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      onclose();
    } else if (e.key === "Tab" && drawer) {
      const stops = [...drawer.querySelectorAll<HTMLElement>("button:not(:disabled), input, select, summary")];
      const last = stops.length - 1;
      const at = stops.indexOf(document.activeElement as HTMLElement);
      if (e.shiftKey ? at <= 0 : at === last) {
        e.preventDefault();
        stops[e.shiftKey ? last : 0]?.focus();
      }
    }
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
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="scrim" onclick={onclose}></div>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <aside bind:this={drawer} aria-label="Download details" onkeydown={onkey}>
    <header>
      <h2 title={row.filename}>{row.filename}</h2>
      <button bind:this={closeButton} class="plain" onclick={onclose} aria-label="Close details" title="Close"><Icon name="x" /></button>
    </header>

    <div class="amount">
      {#if pct !== null}<span class="pct">{Math.floor(pct)}%</span>{/if}
      <span class="of">{formatBytes(row.bytes)}{row.size === null ? "" : ` of ${formatBytes(row.size)}`}</span>
    </div>
    <ProgressBar value={pct} state={row.state} />

    <div class="stats">
      <div>
        <span>Speed</span>
        <strong>{row.state === "downloading" ? formatSpeed(row.speed_bps) : "—"}</strong>
      </div>
      <div>
        <span>Time left</span>
        <strong>{row.state === "downloading" ? formatEta(row.eta_s) : "—"}</strong>
      </div>
    </div>

    {#if problem}
      <div class="problem" role="status">
        <strong>{problem.title}</strong>
        <p>{problem.body}</p>
      </div>
    {:else}
      <p class="status">{rowStatus(row, waiting, queueSchedule.wait)}</p>
    {/if}

    <div class="actions">
      {#if primary}
        <button class="primary main" onclick={() => onaction(primary)}>{primaryLabel}</button>
      {/if}
      {#if canRefresh(row) && problem?.action !== "new-link"}
        <button onclick={() => onaction("new-link")}>Paste new link</button>
      {/if}
      <button onclick={() => onaction("open-folder")}><Icon name="folder" /> Open folder</button>
      <button class="square" onclick={copyUrl} aria-label={copied ? "Link copied" : "Copy link"} title={copied ? "Copied" : "Copy link"}>
        <Icon name={copied ? "check" : "copy"} />
      </button>
      <button class="square" onclick={() => onaction("remove")} aria-label="Remove from list" title="Remove from list"><Icon name="trash" /></button>
    </div>

    <dl>
      <dt>Saved to</dt>
      <dd class="wrap">{joinPath(row.save_dir, row.filename)}</dd>
      {#if isTorrent}
        <dt>Peers</dt>
        <dd>{row.torrent ? `${row.torrent.peers_connected} connected (${row.torrent.peers_seen} found)` : "—"}</dd>
        <dt>Uploading</dt>
        <dd>{row.torrent && row.state === "downloading" ? formatSpeed(row.torrent.up_bps) : "—"}</dd>
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
    </dl>

    {#if isTorrent && detail?.torrent && detail.torrent.files.length > 0}
      <section>
        <h3>Files</h3>
        <TorrentFiles files={detail.torrent.files} selected={selectedFiles} onchange={changeFiles} progress disabled={row.state === "cancelled"} />
        {#if filesError}<p class="bad" role="alert">{filesError}</p>{/if}
      </section>
    {:else if isTorrent && !row.has_meta}
      <p class="hint">Fetching file list…</p>
    {/if}

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

    <details>
      <summary>Technical details <Icon name="chevron" /></summary>
      {#if isTorrent}
        {#if detail?.torrent?.pieces}
          <PieceMap pieces={detail.torrent.pieces} />
        {/if}
      {:else if row.kind !== "hls" && row.kind !== "site"}
        <ChunkMap {chunks} total={row.size} />
      {/if}
      <dl>
        {#if row.kind === "hls" && row.segments}
          <dt>Segments</dt>
          <dd>{row.segments.done} of {row.segments.total}</dd>
        {/if}
        {#if row.kind === "site"}
          <dt>Method</dt>
          <dd>Downloaded with yt-dlp</dd>
        {:else if isTorrent}
          <dt>Info hash</dt>
          <dd class="wrap">{row.info_hash}</dd>
        {:else}
          <dt>Connections</dt>
          <dd>Up to {row.max_connections}</dd>
        {/if}
        <dt>Address</dt>
        <dd class="wrap">{row.url}</dd>
        {#if detail?.final_url && detail.final_url !== row.url}
          <dt>Redirects to</dt>
          <dd class="wrap">{detail.final_url}</dd>
        {/if}
        {#if detail && detail.header_names.length > 0}
          <dt>Sent headers</dt>
          <dd class="wrap">{detail.header_names.join(", ")}</dd>
        {/if}
        {#if row.error_message}
          <dt>Error</dt>
          <dd class="bad wrap">{row.error_message}</dd>
        {/if}
      </dl>
    </details>
  </aside>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 20;
    background: var(--backdrop);
  }
  aside {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 21;
    width: 400px;
    max-width: 100vw;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px 20px;
    background: var(--surface);
    border-left: 1px solid var(--border);
    box-shadow: var(--shadow-dialog);
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
    font-size: 15px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 13px;
    font-weight: 600;
  }
  .plain {
    display: grid;
    place-items: center;
    flex: none;
    width: 28px;
    height: 28px;
    padding: 0;
    border-color: transparent;
    background: transparent;
    color: var(--muted);
  }
  .amount {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  .pct {
    font-size: 28px;
    font-weight: 700;
    line-height: 1.1;
  }
  .of,
  .status {
    color: var(--muted);
  }
  .status {
    margin: 0;
  }
  .stats {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .stats div {
    display: flex;
    flex-direction: column;
    padding: 8px 12px;
    border-radius: var(--radius);
    background: var(--bg);
  }
  .stats span {
    color: var(--muted);
    font-size: 12px;
  }
  .stats strong {
    font-size: 15px;
    font-weight: 600;
  }
  .problem {
    padding: 10px 12px;
    border: 1px solid var(--error);
    border-radius: var(--radius-lg);
    background: var(--error-tint);
  }
  .problem p {
    margin: 4px 0 0;
    color: var(--muted);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 0;
  }
  .actions button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-height: 34px;
  }
  .main {
    flex: 1;
  }
  .square {
    width: 34px;
    padding: 0;
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
  details {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }
  summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
    font-weight: 600;
    cursor: pointer;
    list-style: none;
  }
  summary::-webkit-details-marker {
    display: none;
  }
</style>
