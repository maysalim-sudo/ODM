<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "../lib/api";
  import { toolsProgressText, toolsSummary } from "../lib/model";
  import type { ToolsStatus } from "../lib/types";

  let {
    compact = false,
    oninstalled,
  }: {
    /** Only the button and the progress: used where the status lines would be noise. */
    compact?: boolean;
    oninstalled?: (status: ToolsStatus) => void;
  } = $props();

  let status = $state<ToolsStatus | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let progress = $state<{ stage: string; done: number; total: number } | null>(null);

  const summary = $derived(status ? toolsSummary(status) : null);
  const pct = $derived(progress && progress.total > 0 ? Math.min(100, (progress.done / progress.total) * 100) : null);

  onMount(() => {
    api
      .toolsStatus()
      .then((s) => (status = s))
      .catch((e) => (error = api.errorText(e)));
    let off: (() => void) | null = null;
    let closed = false;
    api
      .onEvent((ev) => {
        if (ev.type === "tools_progress") progress = { stage: ev.stage, done: ev.done, total: ev.total };
      })
      .then((un) => {
        if (closed) un();
        else off = un;
      });
    return () => {
      closed = true;
      off?.();
    };
  });

  async function install() {
    if (busy) return;
    busy = true;
    error = null;
    progress = null;
    try {
      status = await api.toolsInstall();
      oninstalled?.(status);
    } catch (e) {
      error = api.errorText(e);
    } finally {
      busy = false;
      progress = null;
    }
  }
</script>

<div class="card">
  {#if !compact}
    <p class="lead">FFmpeg joins video streams, yt-dlp and Deno download videos from sites like YouTube. ODM keeps its own copy, so you don't have to install anything.</p>
    {#if summary}
      <ul>
        {#each summary.lines as line (line)}
          <li>{line}</li>
        {/each}
      </ul>
    {/if}
  {/if}
  <div class="controls">
    <button class="primary" onclick={install} disabled={busy}>
      {busy ? "Downloading…" : summary?.own ? "Update video tools" : "Get video tools"}
    </button>
    {#if !compact && !busy}<span class="hint">About 150 MB.</span>{/if}
  </div>
  {#if busy}
    <div
      class="bar"
      role="progressbar"
      aria-label="Video tools download"
      aria-valuemin="0"
      aria-valuemax="100"
      aria-valuenow={pct === null ? undefined : Math.round(pct)}
    >
      <div class="fill" class:indeterminate={pct === null} style:width={pct === null ? "30%" : `${pct}%`}></div>
    </div>
    <p class="hint" role="status">{progress ? toolsProgressText(progress) : "Starting…"}</p>
  {/if}
  {#if error}<p class="form-error" role="alert">{error}</p>{/if}
</div>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  .lead {
    margin: 0;
    color: var(--muted);
  }
  ul {
    margin: 0;
    padding-left: 18px;
  }
  .controls {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .bar {
    height: 6px;
    background: var(--surface-2);
    border-radius: 3px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 200ms linear;
  }
  .fill.indeterminate {
    animation: slide 1.1s var(--ease-out) infinite alternate;
  }
  @keyframes slide {
    from {
      margin-left: 0;
    }
    to {
      margin-left: 70%;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .fill.indeterminate {
      animation: none;
    }
  }
  .form-error {
    margin: 0;
  }
</style>
