<script lang="ts">
  import { checkSpeed } from "../lib/settingsPage";
  import Modal from "./Modal.svelte";

  /** "Speed: custom…" for the selected downloads. */
  let { open, count, onapply, onclose }: { open: boolean; count: number; onapply: (kbps: number) => void; onclose: () => void } = $props();

  let kbps = $state<number | null>(512);
  let error = $state<string | null>(null);

  $effect(() => {
    if (open) {
      kbps = 512;
      error = null;
    }
  });

  function apply() {
    error = checkSpeed(kbps, "kb");
    if (error || kbps === null) return;
    onapply(Math.round(kbps));
    onclose();
  }
</script>

<Modal {open} title={count === 1 ? "Speed limit for this download" : `Speed limit for ${count} downloads`} {onclose} width={380}>
  <label class="field">
    <span>KB/s</span>
    <input type="number" min="1" step="1" bind:value={kbps} aria-invalid={error !== null} onkeydown={(e) => e.key === "Enter" && apply()} />
  </label>
  {#if error}<p class="form-error" role="alert">{error}</p>{/if}
  <div class="actions">
    <button onclick={onclose}>Cancel</button>
    <button class="primary" onclick={apply}>Set limit</button>
  </div>
</Modal>
