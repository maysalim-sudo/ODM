<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import { afterQueueText } from "../lib/model";
  import Modal from "./Modal.svelte";

  // "When the queue is done": sleep, shut down or quit after 60 s, unless cancelled here.
  const pending = $derived(downloads.afterQueue);
  const open = $derived(pending !== null);
  let now = $state(Date.now());
  let busy = $state(false);

  $effect(() => {
    if (!open) return;
    busy = false;
    now = Date.now();
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  const left = $derived(pending === null ? 0 : Math.max(0, Math.ceil((pending.at - now) / 1000)));

  // Closing the dialog cancels too: the safe choice.
  async function cancel() {
    if (busy || !open) return;
    busy = true;
    const ok = await downloads.run(() => api.cancelAfterQueue());
    if (ok) downloads.afterQueue = null;
    busy = false;
  }
</script>

<Modal {open} title="The queue is done" onclose={cancel} width={420}>
  <p class="hint" aria-live="polite">
    {pending ? afterQueueText(pending.action, left) : ""}
  </p>
  <div class="actions">
    <button class="primary" onclick={cancel} disabled={busy}>Cancel</button>
  </div>
</Modal>
