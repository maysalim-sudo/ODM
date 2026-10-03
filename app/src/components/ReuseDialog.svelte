<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import Modal from "./Modal.svelte";

  const offer = $derived(downloads.offers[0]);
  const open = $derived(offer !== undefined);

  let busy = $state(false);
  let error = $state<string | null>(null);

  // A different offer starts with a clean slate.
  $effect(() => {
    void offer?.offer_id;
    error = null;
    busy = false;
  });

  async function choose(reuse: boolean) {
    if (!offer || busy) return;
    busy = true;
    error = null;
    try {
      await downloads.resolveOffer(offer, reuse);
    } catch (e) {
      error = api.errorText(e);
    } finally {
      busy = false;
    }
  }

  // Closing or Escape counts as "Download again". The dialog is already gone, so a failure
  // goes to the error bar and the offer is dropped.
  async function dismiss() {
    if (!offer || busy) return;
    const o = offer;
    busy = true;
    try {
      await downloads.resolveOffer(o, false);
    } catch (e) {
      downloads.error = api.errorText(e);
      downloads.offers = downloads.offers.filter((x) => x.offer_id !== o.offer_id);
    } finally {
      busy = false;
    }
  }
</script>

<Modal {open} title="Continue the earlier download?" onclose={dismiss} width={440}>
  {#if offer}
    <p class="hint">
      {offer.filename} was partly downloaded before. ODM can continue it with the new link, or download it again
      from the start.
    </p>
    {#if error}
      <p class="form-error" role="alert">{error}</p>
    {/if}
    <div class="actions">
      <button onclick={() => choose(false)} disabled={busy}>Download again</button>
      <button class="primary" onclick={() => choose(true)} disabled={busy}>Continue</button>
    </div>
  {/if}
</Modal>
