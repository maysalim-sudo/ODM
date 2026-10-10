<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import Modal from "./Modal.svelte";

  const open = $derived(downloads.shutdownAt !== null);
  let now = $state(Date.now());
  let busy = $state(false);

  $effect(() => {
    if (!open) return;
    busy = false;
    now = Date.now();
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  const left = $derived(downloads.shutdownAt === null ? 0 : Math.max(0, Math.ceil((downloads.shutdownAt - now) / 1000)));

  // Closing the dialog cancels too: the safe choice.
  async function cancel() {
    if (busy || !open) return;
    busy = true;
    const ok = await downloads.run(() => api.cancelShutdown());
    if (ok) downloads.shutdownAt = null;
    busy = false;
  }
</script>

<Modal {open} title="The queue is done" onclose={cancel} width={420}>
  <p class="hint" aria-live="polite">
    {left > 0 ? `ODM will shut down the PC in ${left} s.` : "ODM is shutting down the PC."}
  </p>
  <div class="actions">
    <button class="primary" onclick={cancel} disabled={busy}>Cancel shutdown</button>
  </div>
</Modal>
