<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import { canCancel, canPause, canResume, type Row } from "../lib/model";
  import { queueSchedule } from "../lib/queueSchedule.svelte";
  import Icon from "./Icon.svelte";

  let { oncancel, onremove }: { oncancel: () => void; onremove: () => void } = $props();

  const rows = $derived(downloads.selectedRows);
  const ids = (ok: (r: Row) => boolean) => rows.filter(ok).map((r) => r.id);
  // While the queue schedule is closed, Resume starts the selected queued rows now.
  const resumable = (r: Row) => canResume(r, queueSchedule.wait !== null);
</script>

{#if rows.length > 0}
  <div class="bar" role="toolbar" aria-label="Selected downloads">
    <span class="n">{rows.length} selected</span>
    {#if rows.some(resumable)}
      <button onclick={() => downloads.run(() => api.resume(ids(resumable)))}><Icon name="play" /> Resume</button>
    {/if}
    {#if rows.some(canPause)}
      <button onclick={() => downloads.run(() => api.pause(ids(canPause)))}><Icon name="pause" /> Pause</button>
    {/if}
    {#if rows.some(canCancel)}
      <button onclick={oncancel}><Icon name="x" /> Cancel</button>
    {/if}
    <button onclick={onremove}><Icon name="trash" /> Remove</button>
    <button class="close" aria-label="Clear the selection" title="Clear the selection" onclick={() => (downloads.selected = new Set())}>
      <Icon name="x" />
    </button>
  </div>
{/if}

<style>
  .bar {
    position: absolute;
    bottom: 12px;
    left: 50%;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: 6px;
    translate: -50% 0;
    padding: 8px 10px 8px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    background: var(--surface-2);
    box-shadow: var(--shadow-dialog);
    white-space: nowrap;
  }
  .n {
    margin-right: 10px;
    font-weight: 600;
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--surface);
  }
  .close {
    padding: 6px;
    border-color: transparent;
    background: transparent;
    color: var(--muted);
  }
</style>
