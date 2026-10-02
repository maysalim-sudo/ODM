<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import Modal from "./Modal.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  let deleteFile = $state(false);
  const count = $derived(downloads.selected.size);

  $effect(() => {
    if (open) deleteFile = false;
  });

  async function confirm() {
    const ids = [...downloads.selected];
    if (await downloads.run(() => api.remove(ids, deleteFile))) downloads.selected = new Set();
    onclose();
  }
</script>

<Modal {open} title={count === 1 ? "Remove this download?" : `Remove ${count} downloads?`} {onclose} width={420}>
  <p class="hint">They leave the list. Downloads still running are stopped first.</p>
  <label class="check" style="margin-top:12px">
    <input type="checkbox" bind:checked={deleteFile} />
    <span>Also delete the files from disk</span>
  </label>
  <div class="actions">
    <button onclick={onclose}>Keep</button>
    <button class="primary" onclick={confirm} disabled={count === 0}>Remove</button>
  </div>
</Modal>
