<script lang="ts">
  import { untrack } from "svelte";
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import { formatBytes } from "../lib/format";
  import type { ConfirmArgs } from "../lib/types";
  import Modal from "./Modal.svelte";

  const row = $derived(downloads.confirmRow);
  // The reuse offer keeps priority when both are waiting.
  const open = $derived(row !== undefined && downloads.offers.length === 0);

  let filename = $state("");
  let saveDir = $state("");
  let dontAsk = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  const host = $derived.by(() => {
    try {
      return row ? new URL(row.url).host : "";
    } catch {
      return "";
    }
  });

  // Each download starts with its own name and folder.
  $effect(() => {
    const id = downloads.confirmRow?.id;
    untrack(() => {
      const r = downloads.confirmRow;
      filename = r?.filename ?? "";
      saveDir = r?.save_dir ?? "";
      error = null;
      busy = false;
    });
    void id;
  });

  async function browse() {
    const picked = await api.pickFolder(saveDir || null).catch(() => null);
    if (picked) saveDir = picked;
  }

  async function choose(action: ConfirmArgs["action"]) {
    if (!row || busy) return;
    busy = true;
    error = null;
    try {
      await downloads.confirm(
        { id: row.id, filename: filename.trim() || null, save_dir: saveDir.trim() || null, action },
        dontAsk,
      );
    } catch (e) {
      error = api.errorText(e);
    } finally {
      busy = false;
    }
  }

  // Closing or Escape counts as "Later", so nothing is lost. The dialog is already gone, so a failure
  // goes to the error bar and the id is dropped.
  async function dismiss() {
    if (!row || busy || !open) return;
    const id = row.id;
    busy = true;
    try {
      await downloads.confirm({ id, filename: filename.trim() || null, save_dir: saveDir.trim() || null, action: "later" }, dontAsk);
    } catch (e) {
      downloads.error = api.errorText(e);
      downloads.confirms = downloads.confirms.filter((x) => x !== id);
    } finally {
      busy = false;
    }
  }
</script>

<!-- A fresh Modal per download: after Escape the browser has closed the native dialog, and the next
     queued download needs it opened again. -->
{#key row?.id}
<Modal {open} title="New download" onclose={dismiss} width={520}>
  {#if row}
    <p class="info">{host || row.url}</p>
    <p class="info">Size: {row.size !== null ? formatBytes(row.size) : "unknown"}</p>

    <label class="field">
      <span>File name</span>
      <input bind:value={filename} />
    </label>

    <div class="field">
      <label for="confirm-save-dir">Save to</label>
      <span class="row">
        <input id="confirm-save-dir" bind:value={saveDir} />
        <button type="button" onclick={browse} disabled={busy}>Browse…</button>
      </span>
    </div>

    <label class="check"><input type="checkbox" bind:checked={dontAsk} /><span>Don't ask again (you can turn this back on in Settings)</span></label>

    {#if error}
      <p class="form-error" role="alert">{error}</p>
    {/if}
    <div class="actions">
      <button class="cancel" onclick={() => choose("cancel")} disabled={busy}>Cancel</button>
      <button onclick={() => choose("later")} disabled={busy}>Later</button>
      <button class="primary" onclick={() => choose("start")} disabled={busy}>Start now</button>
    </div>
  {/if}
</Modal>
{/key}

<style>
  .info {
    margin: 0 0 6px;
    color: var(--muted);
    user-select: text;
    overflow-wrap: anywhere;
  }
  .cancel {
    margin-right: auto;
  }
</style>
