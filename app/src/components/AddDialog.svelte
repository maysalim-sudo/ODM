<script lang="ts">
  import * as api from "../lib/api";
  import { formatBytes } from "../lib/format";
  import { CATEGORIES, nextFilename, toInt } from "../lib/model";
  import { queueSchedule } from "../lib/queueSchedule.svelte";
  import { checkSpeed } from "../lib/settingsPage";
  import type { Suggestion } from "../lib/types";
  import Modal from "./Modal.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  let url = $state("");
  let filename = $state("");
  let saveDir = $state("");
  let category = $state("");
  let connections = $state<number | null>(8);
  /** KB/s; empty = no limit of its own. */
  let limitKbps = $state<number | null>(null);
  /** "Start new downloads: Paused" adds this one paused too. */
  let addsPaused = $state(false);
  let defaultConnections = 8;
  let lastAuto: string | null = null;
  let suggestion = $state<Suggestion | null>(null);
  let checking = $state(false);
  let variantUrl = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  const validUrl = $derived(/^https?:\/\/[^/]/i.test(url.trim()));
  /** A magnet link: added as a torrent; its files are chosen in the next window. */
  const isMagnet = $derived(/^magnet:\?/i.test(url.trim()));

  // Fresh form every time the dialog opens.
  $effect(() => {
    if (!open) return;
    url = "";
    filename = "";
    lastAuto = null;
    saveDir = "";
    category = "";
    suggestion = null;
    variantUrl = "";
    error = null;
    limitKbps = null;
    api
      .getSettings()
      .then((s) => {
        defaultConnections = s.max_connections;
        connections = s.max_connections;
        addsPaused = s.start_mode === "paused";
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
          variantUrl = s.variants[0]?.url ?? "";
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

  const plainFile = $derived(!suggestion?.is_page && !suggestion?.is_stream);
  const limitError = $derived(limitKbps == null || !plainFile ? null : checkSpeed(limitKbps, "kb"));

  /** Without "Start now", the download waits: paused, or for the queue schedule. */
  const waits = $derived(addsPaused || queueSchedule.wait !== null);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    await add(false);
  }

  async function addTorrent(input: Parameters<typeof api.addTorrent>[0]) {
    busy = true;
    error = null;
    try {
      await api.addTorrent(input);
      onclose();
    } catch (err) {
      error = api.errorText(err);
    } finally {
      busy = false;
    }
  }

  async function pickTorrent() {
    const path = await api.pickTorrentFile().catch(() => null);
    if (path) await addTorrent({ kind: "file", path });
  }

  async function add(startNow: boolean) {
    if (isMagnet && !busy) return addTorrent({ kind: "magnet", text: url.trim() });
    if (!validUrl || busy || limitError) return;
    busy = true;
    error = null;
    try {
      await api.addDownload({
        url: url.trim(),
        filename: filename.trim() || null,
        save_dir: saveDir.trim() || null,
        category: category || null,
        connections: toInt(connections, defaultConnections),
        ...(suggestion?.is_stream ? { stream: true, variant_url: variantUrl || null } : {}),
        ...(suggestion?.is_page ? { page: true } : {}),
        ...(plainFile && limitKbps != null ? { speed_limit_kbps: Math.round(limitKbps) } : {}),
        ...(startNow ? { start_now: true } : {}),
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
      <input type="url" bind:value={url} placeholder="https://example.com/file.zip or a magnet link" autofocus required />
    </label>
    <p class="hint">
      {#if isMagnet}
        Magnet link · you choose which files to download next
      {:else if checking}
        Checking the address…
      {:else if suggestion?.is_page}
        This page's video will be downloaded with yt-dlp.
      {:else if suggestion?.is_stream}
        Video stream · downloaded in parts and merged into one file with FFmpeg
      {:else if suggestion}
        {suggestion.size === null ? "Size unknown" : formatBytes(suggestion.size)} ·
        {suggestion.resumable ? "can be paused and resumed" : "cannot be resumed if interrupted"}
      {:else if url && !validUrl}
        Enter an address that starts with http:// or https://
      {:else}
        &nbsp;
      {/if}
    </p>

    {#if !isMagnet}
    {#if suggestion?.is_stream && suggestion.variants.length > 0}
      <label class="field">
        <span>Quality</span>
        <select bind:value={variantUrl}>
          {#each suggestion.variants as v (v.url)}
            <option value={v.url}>{v.label}</option>
          {/each}
        </select>
      </label>
    {/if}

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
          {#each CATEGORIES.filter((c) => c !== "torrent") as c (c)}
            <option value={c}>{c}</option>
          {/each}
        </select>
      </label>
      {#if !suggestion?.is_page}
        <label class="field" style="width:140px">
          <span>Connections</span>
          <input type="number" min="1" max="32" bind:value={connections} />
        </label>
      {/if}
      {#if plainFile}
        <label class="field" style="width:140px">
          <span>Limit speed (KB/s)</span>
          <input type="number" min="1" step="1" placeholder="No limit" aria-invalid={limitError !== null} bind:value={limitKbps} />
        </label>
      {/if}
    </div>
    {#if limitError}<p class="form-error" role="alert">{limitError}</p>{/if}
    {/if}

    {#if error}<p class="form-error" role="alert">{error}</p>{/if}

    <div class="actions">
      <button type="button" class="torrent-file" onclick={pickTorrent} disabled={busy}>Torrent file…</button>
      <button type="button" onclick={onclose}>Cancel</button>
      {#if waits && !isMagnet}
        <button type="button" disabled={!validUrl || busy || limitError !== null} onclick={() => add(true)}>Start now</button>
      {/if}
      <button type="submit" class="primary" disabled={!(validUrl || isMagnet) || busy || (!isMagnet && limitError !== null)}>
        {isMagnet ? "Add torrent" : addsPaused ? "Add paused" : waits ? "Add to queue" : "Start download"}
      </button>
    </div>
  </form>
</Modal>

<style>
  .torrent-file {
    margin-right: auto;
  }
</style>
