<script lang="ts">
  import * as api from "../lib/api";
  import Modal from "./Modal.svelte";

  let { open, id, onclose }: { open: boolean; id: string | null; onclose: () => void } = $props();

  let url = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  const valid = $derived(/^https?:\/\//i.test(url.trim()));

  $effect(() => {
    if (open) {
      url = "";
      busy = false;
      error = null;
    }
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!id || !valid || busy) return;
    busy = true;
    error = null;
    try {
      await api.refreshUrl(id, url.trim());
      onclose();
    } catch (err) {
      error = api.errorText(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal {open} title="Refresh the link" {onclose} width={480}>
  <form onsubmit={submit}>
    <label class="field">
      <span>New link</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input type="url" bind:value={url} placeholder="https://example.com/file.zip" autofocus required />
    </label>
    <p class="hint">Paste a fresh link for the same file from the browser. ODM continues from where it stopped.</p>
    {#if error}
      <p class="form-error" role="alert">{error}</p>
    {/if}
    <div class="actions">
      <button type="button" onclick={onclose}>Cancel</button>
      <button class="primary" type="submit" disabled={!valid || busy}>Continue download</button>
    </div>
  </form>
</Modal>
