<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import { isRiskyFile } from "../lib/model";
  import Modal from "./Modal.svelte";

  const row = $derived(downloads.finishedRow);
  const open = $derived(row !== undefined);
  // A program or script never opens from here, as with "Open file" in Settings.
  const risky = $derived(!!row && isRiskyFile(row.filename));

  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    void row?.id;
    error = null;
    busy = false;
  });

  async function choose(action: "file" | "folder") {
    if (!row || busy) return;
    const id = row.id;
    busy = true;
    error = null;
    try {
      await (action === "file" ? api.openFile(id) : api.openFolder(id));
      downloads.dropFinished(id);
    } catch (e) {
      error = api.errorText(e);
    } finally {
      busy = false;
    }
  }

  function dismiss() {
    if (row) downloads.dropFinished(row.id);
  }
</script>

<!-- A fresh Modal per download, so the next queued one opens the native dialog again. -->
{#key row?.id}
<Modal {open} title="Download finished" onclose={dismiss} width={460}>
  {#if row}
    <p class="name" title={row.filename}>{row.filename}</p>
    {#if risky}
      <p class="hint">This is a program, so ODM won't open it for you. Open its folder to run it yourself.</p>
    {/if}
    {#if error}
      <p class="form-error" role="alert">{error}</p>
    {/if}
    <div class="actions">
      <button class="cancel" onclick={dismiss} disabled={busy}>Cancel</button>
      <button class={risky ? "primary" : ""} onclick={() => choose("folder")} disabled={busy}>Open folder</button>
      <button class={risky ? "" : "primary"} onclick={() => choose("file")} disabled={busy || risky}>Open file</button>
    </div>
  {/if}
</Modal>
{/key}

<style>
  .name {
    margin: 0 0 8px;
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cancel {
    margin-right: auto;
  }
</style>
