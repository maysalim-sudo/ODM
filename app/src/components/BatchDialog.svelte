<script lang="ts">
  import * as api from "../lib/api";
  import { parseUrls } from "../lib/model";
  import Modal from "./Modal.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  let text = $state("");
  let saveDir = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let rejected = $state<string[]>([]);

  const parsed = $derived(parseUrls(text));

  $effect(() => {
    if (!open) return;
    text = "";
    saveDir = "";
    error = null;
    rejected = [];
  });

  async function browse() {
    const picked = await api.pickFolder(saveDir || null).catch(() => null);
    if (picked) saveDir = picked;
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (parsed.valid.length === 0 || busy) return;
    busy = true;
    error = null;
    try {
      const result = await api.addBatch(text, saveDir.trim() || null);
      if (result.rejected.length === 0) {
        onclose();
      } else {
        // Keep the dialog open so the lines that were not added can be fixed.
        rejected = result.rejected;
        text = result.rejected.join("\n");
      }
    } catch (err) {
      error = api.errorText(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal {open} title="Add several downloads" {onclose} width={600}>
  <form onsubmit={submit}>
    <label class="field">
      <span>Addresses, one per line</span>
      <!-- svelte-ignore a11y_autofocus -->
      <textarea bind:value={text} rows="10" spellcheck="false" autofocus placeholder="https://example.com/one.zip&#10;https://example.com/two.zip"></textarea>
    </label>
    <p class="hint">
      {parsed.valid.length} ready to add{parsed.rejected.length > 0
        ? ` · ${parsed.rejected.length} not a web address`
        : ""}
    </p>

    <div class="field" style="margin-top:12px">
      <label for="batch-save-dir">Save to</label>
      <span class="row">
        <input id="batch-save-dir" bind:value={saveDir} placeholder="Your download folder, sorted by type" />
        <button type="button" onclick={browse}>Browse</button>
      </span>
    </div>

    {#if rejected.length > 0}
      <p class="form-error" role="alert">
        {rejected.length} {rejected.length === 1 ? "line was" : "lines were"} not added. They are still in the box above.
      </p>
    {/if}
    {#if error}<p class="form-error" role="alert">{error}</p>{/if}

    <div class="actions">
      <button type="button" onclick={onclose}>Cancel</button>
      <button type="submit" class="primary" disabled={parsed.valid.length === 0 || busy}>
        Add {parsed.valid.length || ""}
      </button>
    </div>
  </form>
</Modal>
