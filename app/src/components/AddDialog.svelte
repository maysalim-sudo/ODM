<script lang="ts">
  import * as api from "../lib/api";
  import { formatBytes } from "../lib/format";
  import { CATEGORIES, nextFilename, toInt } from "../lib/model";
  import type { Suggestion } from "../lib/types";
  import Modal from "./Modal.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  let url = $state("");
  let filename = $state("");
  let saveDir = $state("");
  let category = $state("");
  let connections = $state<number | null>(8);
  let defaultConnections = 8;
  let lastAuto: string | null = null;
  let suggestion = $state<Suggestion | null>(null);
  let checking = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  const validUrl = $derived(/^https?:\/\/[^/]/i.test(url.trim()));

  // Fresh form every time the dialog opens.
  $effect(() => {
    if (!open) return;
    url = "";
    filename = "";
    lastAuto = null;
    saveDir = "";
    category = "";
    suggestion = null;
    error = null;
    api
      .getSettings()
      .then((s) => {
        defaultConnections = s.max_connections;
        connections = s.max_connections;
      })
      .catch(() => {});
  });

  // Ask the server about the address once typing pauses.
  $effect(() => {
    const wanted = url.trim();
    suggestion = null;
    if (!open || !validUrl) return;
    const timer = setTimeout(() => {
      checking = true;
      api
        .suggest(wanted)
        .then((s) => {
          if (url.trim() !== wanted) return;
          suggestion = s;
          filename = nextFilename(filename, lastAuto, s.filename);
          lastAuto = s.filename;
        })
        .catch(() => {})
        .finally(() => (checking = false));
    }, 500);
    return () => clearTimeout(timer);
  });

  async function browse() {
    const picked = await api.pickFolder(saveDir || null).catch(() => null);
    if (picked) saveDir = picked;
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!validUrl || busy) return;
    busy = true;
    error = null;
    try {
      await api.addDownload({
        url: url.trim(),
        filename: filename.trim() || null,
        save_dir: saveDir.trim() || null,
        category: category || null,
        connections: toInt(connections, defaultConnections),
      });
      onclose();
    } catch (err) {
      error = api.errorText(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal {open} title="Add download" {onclose}>
  <form onsubmit={submit}>
    <label class="field">
      <span>Address</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input type="url" bind:value={url} placeholder="https://example.com/file.zip" autofocus required />
    </label>
    <p class="hint">
      {#if checking}
        Checking the address…
      {:else if suggestion}
        {suggestion.size === null ? "Size unknown" : formatBytes(suggestion.size)} ·
        {suggestion.resumable ? "can be paused and resumed" : "cannot be resumed if interrupted"}
      {:else if url && !validUrl}
        Enter an address that starts with http:// or https://
      {:else}
        &nbsp;
      {/if}
    </p>

    <label class="field">
      <span>File name</span>
      <input bind:value={filename} placeholder={suggestion?.filename ?? "Chosen automatically"} />
    </label>

    <div class="field">
      <label for="add-save-dir">Save to</label>
      <span class="row">
        <input id="add-save-dir" bind:value={saveDir} placeholder="Your download folder, sorted by type" />
        <button type="button" onclick={browse}>Browse</button>
      </span>
    </div>

    <div class="row">
      <label class="field" style="flex:1">
        <span>Type</span>
        <select bind:value={category}>
          <option value="">Automatic{suggestion ? ` (${suggestion.category})` : ""}</option>
          {#each CATEGORIES as c (c)}
            <option value={c}>{c}</option>
          {/each}
        </select>
      </label>
      <label class="field" style="width:140px">
        <span>Connections</span>
        <input type="number" min="1" max="32" bind:value={connections} />
      </label>
    </div>

    {#if error}<p class="form-error" role="alert">{error}</p>{/if}

    <div class="actions">
      <button type="button" onclick={onclose}>Cancel</button>
      <button type="submit" class="primary" disabled={!validUrl || busy}>Start download</button>
    </div>
  </form>
</Modal>
