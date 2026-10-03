<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import { canCancel } from "../lib/model";
  import Modal from "./Modal.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  let busy = $state(false);
  const count = $derived(downloads.selectedRows.filter(canCancel).length);

  $effect(() => {
    if (open) busy = false;
  });

  // Nothing left to cancel (for example the rows finished meanwhile): close instead of asking about 0.
  $effect(() => {
    if (open && count === 0 && !busy) onclose();
  });

  async function confirm() {
    if (busy) return;
    busy = true;
    const ids = downloads.selectedRows.filter(canCancel).map((r) => r.id);
    await downloads.run(() => api.cancel(ids, false));
    onclose();
  }
</script>

<Modal {open} title={count === 1 ? "Cancel this download?" : `Cancel ${count} downloads?`} {onclose} width={420}>
  <p class="hint">The part already downloaded is deleted. You can't resume a cancelled download.</p>
  <div class="actions">
    <button onclick={onclose}>Keep downloading</button>
    <button class="primary" onclick={confirm} disabled={count === 0 || busy}>Cancel download</button>
  </div>
</Modal>
