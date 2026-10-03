<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import Modal from "./Modal.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  let deleteFile = $state(false);
  let busy = $state(false);
  const count = $derived(downloads.selected.size);

  $effect(() => {
    if (open) {
      deleteFile = false;
      busy = false;
    }
  });

  // Nothing left to remove (for example the rows finished vanishing): close instead of asking about 0.
  $effect(() => {
    if (open && count === 0 && !busy) onclose();
  });

  async function confirm() {
    if (busy) return;
    busy = true;
    const ids = [...downloads.selected];
    if (await downloads.run(() => api.remove(ids, deleteFile))) downloads.selected = new Set();
    onclose();
  }
</script>

<Modal {open} title={count === 1 ? "Remove this download?" : `Remove ${count} downloads?`} {onclose} width={420}>
  <p class="hint">
    They leave the list. Downloads still running are stopped first, and unfinished downloads lose their partial
    data. The box below only deletes finished files.
  </p>
  <label class="check" style="margin-top:12px">
    <input type="checkbox" bind:checked={deleteFile} />
    <span>Also delete finished files from disk</span>
  </label>
  <div class="actions">
    <button onclick={onclose}>Keep</button>
    <button class="primary" onclick={confirm} disabled={count === 0 || busy}>Remove</button>
  </div>
</Modal>
