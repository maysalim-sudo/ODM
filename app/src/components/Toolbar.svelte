<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import SpeedChip from "./SpeedChip.svelte";
  import { canCancel, canPause, canResume } from "../lib/model";

  let {
    onadd,
    onbatch,
    onsettings,
    oncancel,
    onremove,
  }: {
    onadd: () => void;
    onbatch: () => void;
    onsettings: () => void;
    oncancel: () => void;
    onremove: () => void;
  } = $props();

  const rows = $derived(downloads.selectedRows);
  const ids = (ok: (r: (typeof rows)[number]) => boolean) => rows.filter(ok).map((r) => r.id);
</script>

<div class="toolbar" role="toolbar" aria-label="Download actions">
  <button class="primary" onclick={onadd}>Add download</button>
  <button onclick={onbatch}>Add several</button>
  <span class="gap"></span>
  <button disabled={!rows.some(canResume)} onclick={() => downloads.run(() => api.resume(ids(canResume)))}>
    Resume
  </button>
  <button disabled={!rows.some(canPause)} onclick={() => downloads.run(() => api.pause(ids(canPause)))}>
    Pause
  </button>
  <button disabled={!rows.some(canCancel)} onclick={oncancel}>Cancel</button>
  <button disabled={rows.length === 0} onclick={onremove}>Remove</button>
  <span class="spacer"></span>
  <SpeedChip />
  <button onclick={onsettings}>Settings</button>
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .gap {
    width: 10px;
  }
  .spacer {
    flex: 1;
  }
</style>
