<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import type { ExistsChoice } from "../lib/types";
  import Modal from "./Modal.svelte";

  const prompt = $derived(downloads.prompts[0]);
  const open = $derived(prompt !== undefined);

  let busy = $state(false);
  let error = $state<string | null>(null);

  // A different prompt starts with a clean slate.
  $effect(() => {
    void prompt?.id;
    error = null;
    busy = false;
  });

  async function choose(choice: ExistsChoice) {
    if (!prompt || busy) return;
    busy = true;
    error = null;
    try {
      await downloads.answerExists(prompt, choice);
    } catch (e) {
      error = api.errorText(e);
    } finally {
      busy = false;
    }
  }

  // Closing without a choice leaves the download waiting; Resume asks again.
  function dismiss() {
    if (prompt) downloads.dismissPrompt(prompt.id);
  }
</script>

<!-- A fresh Modal per prompt, so the next queued one opens the native dialog again. -->
{#key prompt?.id}
<Modal {open} title={prompt ? `${prompt.name} already exists` : ""} onclose={dismiss} width={460}>
  {#if prompt}
    <p class="hint">
      A file with this name is already in the folder. Replace overwrites it. Keep both saves the new one with a number
      added to its name.
    </p>
    {#if error}
      <p class="form-error" role="alert">{error}</p>
    {/if}
    <div class="actions">
      <button onclick={() => choose("cancel")} disabled={busy}>Cancel download</button>
      <button onclick={() => choose("replace")} disabled={busy}>Replace</button>
      <button class="primary" onclick={() => choose("keep_both")} disabled={busy}>Keep both</button>
    </div>
  {/if}
</Modal>
{/key}
