<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import { queueSchedule } from "../lib/queueSchedule.svelte";
  import { formatBytes, formatEta, formatSpeed, joinPath } from "../lib/format";
  import { PANEL_MIN, chartPoints, clampHeight, combined, panelSubject, sumSeries } from "../lib/details";
  import { ACTION_LABELS, FIX_LABELS, rowActions, type RowAction } from "../lib/home";
  import { canRefresh, canResume, isActive, needsVideoTools, problemInfo, progressPct, rowStatus } from "../lib/model";
  import { checkSpeed } from "../lib/settingsPage";
  import { SPEED_SAMPLES, speedHistory } from "../lib/speedHistory.svelte";
  import type { Detail } from "../lib/types";
  import ChunkMap from "./ChunkMap.svelte";
  import Icon from "./Icon.svelte";
  import PieceMap from "./PieceMap.svelte";
  import ProgressBar from "./ProgressBar.svelte";
  import TorrentFiles from "./TorrentFiles.svelte";
  import ToolsCard from "./ToolsCard.svelte";

  let { onaction }: { onaction: (a: RowAction) => void } = $props();

  type Tab = "info" | "speed" | "parts" | "files";
  const HEIGHT_KEY = "odm.panel.height";
  const TAB_KEY = "odm.panel.tab";

  function stored(key: string): string | null {
    try {
      return localStorage.getItem(key);
    } catch {
      return null;
    }
  }
  function store(key: string, value: string) {
    try {
      localStorage.setItem(key, value);
    } catch {
      // Not remembered this time; the panel still works.
    }
  }

  let tab = $state<Tab>((["info", "speed", "parts", "files"] as Tab[]).find((t) => t === stored(TAB_KEY)) ?? "info");
  let height = $state(Number(stored(HEIGHT_KEY)) || 260);

  const subject = $derived(panelSubject(downloads.selected, downloads.rows));
  const row = $derived(subject.kind === "one" ? subject.row : undefined);
  const id = $derived(row?.id ?? null);
  const isTorrent = $derived(row?.kind === "torrent");
  // Files belong to torrents; anything else falls back to Info.
  const shown = $derived<Tab>(tab === "files" && !isTorrent ? "info" : tab);

  function pick(t: Tab) {
    tab = t;
    store(TAB_KEY, t);
  }

  // "Details" from a row menu or a problem's fix brings the Info tab forward.
  export function showInfo() {
    pick("info");
  }

  // ---- Resizing: drag the top edge; the list keeps a usable height.
  let panel = $state<HTMLElement>();
  function startResize(e: PointerEvent) {
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const startY = e.clientY;
    const startH = height;
    const available = panel?.parentElement?.clientHeight ?? window.innerHeight;
    const move = (ev: PointerEvent) => (height = clampHeight(startH + (startY - ev.clientY), available));
    const up = () => {
      handle.removeEventListener("pointermove", move);
      store(HEIGHT_KEY, String(Math.round(height)));
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up, { once: true });
  }
  function resizeKey(e: KeyboardEvent) {
    if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
    e.preventDefault();
    const available = panel?.parentElement?.clientHeight ?? window.innerHeight;
    height = clampHeight(height + (e.key === "ArrowUp" ? 20 : -20), available);
    store(HEIGHT_KEY, String(Math.round(height)));
  }

  // ---- One download: the stored detail (chunks, headers, torrent files), re-read as it changes.
  let detail = $state<Detail | null>(null);
  let copied = $state(false);
  let toolsReady = $state(false);
  const rowState = $derived(row?.state);
  const fileVersion = $derived(id ? (downloads.fileLists[id] ?? 0) : 0);
  let tick = $state(0);
  $effect(() => {
    if (!isTorrent || !rowState || !isActive(rowState)) return;
    const timer = setInterval(() => (tick += 1), 2000);
    return () => clearInterval(timer);
  });
  $effect(() => {
    void id;
    detail = null;
    toolsReady = false;
  });
  $effect(() => {
    const wanted = id;
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

  const waiting = $derived(id ? downloads.waitsForChoice(id) : false);
  const problem = $derived(row?.state === "failed" ? problemInfo(row) : null);
  const primary = $derived(row ? rowActions(row, queueSchedule.wait !== null, waiting).primary : null);
  const primaryLabel = $derived(primary === "fix" && problem ? FIX_LABELS[problem.action] : primary ? ACTION_LABELS[primary] : "");
  const chunks = $derived(row?.chunks ?? detail?.chunks ?? []);
  const pct = $derived(row && (row.segments || row.size) ? progressPct(row) : null);
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
    if (await downloads.run(() => api.setDownloadLimit(target, kbps))) void downloads.reload();
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

  // ---- Several: totals, and their speeds added up.
  const totals = $derived(subject.kind === "many" ? combined(subject.rows) : null);
  const series = $derived(
    subject.kind === "one"
      ? (speedHistory.byId.get(subject.row.id) ?? [])
      : subject.kind === "many"
        ? sumSeries(subject.rows.map((r) => speedHistory.byId.get(r.id) ?? []))
        : [],
  );
  const nowSpeed = $derived(subject.kind === "one" ? (isActive(subject.row.state) ? subject.row.speed_bps : 0) : (totals?.speed_bps ?? 0));
  const peak = $derived(series.length ? Math.max(...series) : 0);
  const average = $derived(series.length ? series.reduce((a, b) => a + b, 0) / series.length : 0);

  const CHART_W = 600;
  const CHART_H = 100;
  const line = $derived(chartPoints(series, CHART_W, CHART_H, SPEED_SAMPLES));
  const area = $derived(line ? `${line.split(" ")[0].split(",")[0]},${CHART_H} ${line} ${CHART_W},${CHART_H}` : "");
</script>

<section class="panel" bind:this={panel} style:height="{height}px" style:min-height="{PANEL_MIN}px" aria-label="Download details">
  <!-- A focusable separator is a window splitter: drag it, or use the arrow keys. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="handle"
    role="separator"
    aria-orientation="horizontal"
    aria-label="Resize details"
    aria-valuenow={Math.round(height)}
    tabindex="0"
    onpointerdown={startResize}
    onkeydown={resizeKey}
  ></div>
  <header>
    <div class="tabs" role="tablist" aria-label="Details">
      <button role="tab" aria-selected={shown === "info"} onclick={() => pick("info")}>Info</button>
      <button role="tab" aria-selected={shown === "speed"} onclick={() => pick("speed")}>Speed</button>
      <button role="tab" aria-selected={shown === "parts"} onclick={() => pick("parts")}>Parts</button>
      {#if isTorrent}
        <button role="tab" aria-selected={shown === "files"} onclick={() => pick("files")}>Files and peers</button>
      {/if}
    </div>
    <span class="title" title={row?.filename}>
      {#if row}{row.filename}{:else if totals}{totals.count} downloads{/if}
    </span>
  </header>

  <div class="content" role="tabpanel">
    {#if subject.kind === "none"}
      <p class="empty">Select a download to see its details.</p>
    {:else if shown === "speed"}
      <div class="speed">
        <div class="stats">
          <div><span>Now</span><strong>{formatSpeed(nowSpeed)}</strong></div>
          <div><span>Average</span><strong>{series.length ? formatSpeed(average) : "—"}</strong></div>
          <div><span>Peak</span><strong>{series.length ? formatSpeed(peak) : "—"}</strong></div>
        </div>
        {#if series.length === 0}
          <p class="empty">{subject.kind === "one" ? "The graph starts when this download runs." : "The graph starts when one of these runs."}</p>
        {:else}
          <svg class="chart" viewBox="0 0 {CHART_W} {CHART_H}" preserveAspectRatio="none" role="img" aria-label="Download speed over the last two minutes, peak {formatSpeed(peak)}">
            <polygon points={area} class="fill" />
            <polyline points={line} class="stroke" vector-effect="non-scaling-stroke" />
          </svg>
          <p class="axis"><span>2 min ago</span><span>Now</span></p>
        {/if}
      </div>
    {:else if subject.kind === "many" && totals}
      {#if shown === "info"}
        <div class="many">
          <div class="amount">
            <span class="pct">{totals.size > 0 && totals.unknown === 0 ? `${Math.floor((totals.bytes / totals.size) * 100)}%` : ""}</span>
            <span class="of">
              {formatBytes(totals.bytes)}{totals.size > 0 ? ` of ${formatBytes(totals.size)}` : ""}{totals.unknown > 0
                ? ` · ${totals.unknown} of unknown size`
                : ""}
            </span>
          </div>
          <div class="stats">
            <div><span>Selected</span><strong>{totals.count}</strong></div>
            <div><span>Running</span><strong>{totals.active}</strong></div>
            <div><span>Speed</span><strong>{totals.active ? formatSpeed(totals.speed_bps) : "—"}</strong></div>
          </div>
          <p class="hint">Use the bar below the list to pause, resume or remove them together.</p>
        </div>
      {:else}
        <p class="empty">Select one download to see its {shown === "parts" ? "parts" : "files"}.</p>
      {/if}
    {:else if row}
      {#if shown === "info"}
        <div class="info">
          <div class="col">
            <div class="amount">
              {#if pct !== null}<span class="pct">{Math.floor(pct)}%</span>{/if}
              <span class="of">{formatBytes(row.bytes)}{row.size === null ? "" : ` of ${formatBytes(row.size)}`}</span>
            </div>
            <ProgressBar value={pct} state={row.state} />
            <div class="stats">
              <div><span>Speed</span><strong>{row.state === "downloading" ? formatSpeed(row.speed_bps) : "—"}</strong></div>
              <div><span>Time left</span><strong>{row.state === "downloading" ? formatEta(row.eta_s) : "—"}</strong></div>
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
                <button class="primary" onclick={() => onaction(primary)}>{primaryLabel}</button>
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
            {#if needsVideoTools(row.error_message)}
              <div class="tools">
                {#if toolsReady}
                  <p class="hint">The video tools are installed.</p>
                  {#if canResume(row)}
                    <button class="primary" onclick={() => downloads.run(() => api.resume([row.id]))}>Retry</button>
                  {/if}
                {:else}
                  <ToolsCard compact oninstalled={() => (toolsReady = true)} />
                {/if}
              </div>
            {/if}
          </div>
          <dl>
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
            {:else if isTorrent}
              <dt>Info hash</dt>
              <dd class="wrap">{row.info_hash}</dd>
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
            {#if row.error_message}
              <dt>Error</dt>
              <dd class="bad wrap">{row.error_message}</dd>
            {/if}
          </dl>
        </div>
      {:else if shown === "parts"}
        <div class="parts">
          {#if isTorrent}
            {#if detail?.torrent?.pieces}
              <PieceMap pieces={detail.torrent.pieces} />
            {:else}
              <p class="empty">The piece map shows while the torrent is running.</p>
            {/if}
          {:else if row.kind === "hls"}
            <p class="big">{row.segments ? `${row.segments.done} of ${row.segments.total} parts` : "Parts show while the stream downloads."}</p>
            {#if row.segments}<ProgressBar value={progressPct(row)} state={row.state} />{/if}
          {:else if row.kind === "site"}
            <p class="empty">yt-dlp downloads this one, so ODM can't show its parts.</p>
          {:else}
            <ChunkMap {chunks} total={row.size} />
          {/if}
        </div>
      {:else if shown === "files" && isTorrent}
        <div class="files">
          <dl class="peers">
            <dt>Peers</dt>
            <dd>{row.torrent ? `${row.torrent.peers_connected} connected (${row.torrent.peers_seen} found)` : "—"}</dd>
            <dt>Uploading</dt>
            <dd>{row.torrent && row.state === "downloading" ? formatSpeed(row.torrent.up_bps) : "—"}</dd>
            <dt>Trackers</dt>
            <dd>{detail?.torrent ? detail.torrent.trackers : "—"}</dd>
          </dl>
          {#if detail?.torrent && detail.torrent.files.length > 0}
            <TorrentFiles files={detail.torrent.files} selected={selectedFiles} onchange={changeFiles} progress disabled={row.state === "cancelled"} />
            {#if filesError}<p class="bad" role="alert">{filesError}</p>{/if}
          {:else if !row.has_meta}
            <p class="empty">Fetching file list…</p>
          {/if}
        </div>
      {/if}
    {/if}
  </div>
</section>

<style>
  .panel {
    position: relative;
    flex: none;
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--border);
    background: var(--surface);
    user-select: text;
  }
  .handle {
    position: absolute;
    top: -4px;
    left: 0;
    right: 0;
    height: 8px;
    cursor: ns-resize;
    z-index: 2;
  }
  .handle:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -3px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 20px 0;
    border-bottom: 1px solid var(--border);
  }
  .tabs {
    display: flex;
    gap: 2px;
    flex: none;
  }
  [role="tab"] {
    padding: 6px 12px;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    background: transparent;
    color: var(--muted);
    transition: none;
  }
  [role="tab"]:active:not(:disabled) {
    transform: none;
  }
  [role="tab"][aria-selected="true"] {
    border-bottom-color: var(--accent);
    color: var(--text);
  }
  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-size: 12px;
    text-align: right;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 14px 20px;
  }
  .empty,
  .hint {
    margin: 0;
    color: var(--muted);
  }
  .info {
    display: grid;
    grid-template-columns: minmax(260px, 1fr) minmax(0, 1.4fr);
    gap: 24px;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .many {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 560px;
  }
  .amount {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  .pct {
    font-size: 24px;
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
    grid-template-columns: repeat(auto-fit, minmax(110px, 1fr));
    gap: 8px;
  }
  .stats div {
    display: flex;
    flex-direction: column;
    padding: 6px 12px;
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
    justify-content: flex-start;
    gap: 8px;
    margin-top: 0;
  }
  .actions button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-height: 32px;
  }
  .square {
    width: 32px;
    padding: 0;
  }
  dl {
    display: grid;
    grid-template-columns: 110px minmax(0, 1fr);
    align-content: start;
    gap: 6px 10px;
    min-width: 0;
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
  .speed {
    display: flex;
    flex-direction: column;
    gap: 10px;
    height: 100%;
  }
  .speed .stats {
    max-width: 480px;
  }
  .chart {
    flex: 1;
    width: 100%;
    min-height: 60px;
    border-bottom: 1px solid var(--border);
  }
  .fill {
    fill: var(--accent);
    opacity: 0.18;
  }
  .stroke {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
    stroke-linejoin: round;
  }
  .axis {
    display: flex;
    justify-content: space-between;
    margin: 0;
    color: var(--muted);
    font-size: 11px;
  }
  .parts,
  .files {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .big {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .peers {
    max-width: 420px;
  }
  @media (max-width: 760px) {
    .info {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
