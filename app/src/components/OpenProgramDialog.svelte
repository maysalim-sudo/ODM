<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import Modal from "./Modal.svelte";

  let { open, id, name, onclose }: { open: boolean; id: string | null; name: string; onclose: () => void } = $props();

  let busy = $state(false);

  $effect(() => {
    if (open) busy = false;
  });

  async function confirm() {
    if (!id || busy) return;
    busy = true;
    const target = id;
    await downloads.run(() => api.openFile(target));
    onclose();
  }
</script>

<Modal {open} title="Open this program?" {onclose} width={440}>
  <p class="hint">
    {name} is a program. Programs from the internet can harm your computer. Only open it if you trust where it came
    from.
  </p>
  <div class="actions">
    <button onclick={onclose}>Cancel</button>
    <button class="primary" onclick={confirm} disabled={!id || busy}>Open</button>
  </div>
</Modal>
