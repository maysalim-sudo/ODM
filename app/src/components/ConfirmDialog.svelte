<script lang="ts">
  import { untrack } from "svelte";
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import { formatBytes } from "../lib/format";
  import { selectedSize } from "../lib/fileTree";
  import type { ConfirmArgs, TorrentFile } from "../lib/types";
  import Modal from "./Modal.svelte";
  import TorrentFiles from "./TorrentFiles.svelte";

  const row = $derived(downloads.confirmRow);
  // The reuse offer keeps priority when both are waiting.
  const open = $derived(row !== undefined && downloads.offers.length === 0);

  let filename = $state("");
  let saveDir = $state("");
  let dontAsk = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  // Torrents: the file list (empty until peers send it) and the files the user ticked.
  const isTorrent = $derived(row?.kind === "torrent");
  let files = $state<TorrentFile[]>([]);
  let picked = $state<Set<number>>(new Set());
  const hasMeta = $derived(!!row?.has_meta);
  const fileVersion = $derived(row ? (downloads.fileLists[row.id] ?? 0) : 0);

  $effect(() => {
    const id = row?.id;
    void fileVersion;
    if (!id || !isTorrent || !hasMeta) {
      files = [];
      return;
    }
    let stale = false;
    api
      .torrentFiles(id)
      .then((list) => {
        if (stale) return;
        // A new list starts with every file ticked; a reload keeps the user's ticks.
        const fresh = files.length === 0;
        files = list;
        if (fresh) picked = new Set(list.filter((f) => f.selected).map((f) => f.idx));
      })
      .catch((e) => {
        if (!stale) error = api.errorText(e);
      });
    return () => {
      stale = true;
    };
  });

  const host = $derived.by(() => {
    try {
      return row ? new URL(row.url).host : "";
    } catch {
      return "";
    }
  });

  // Each download starts with its own name and folder.
  $effect(() => {
    const id = downloads.confirmRow?.id;
    untrack(() => {
      const r = downloads.confirmRow;
      filename = r?.filename ?? "";
      saveDir = r?.save_dir ?? "";
      error = null;
      busy = false;
      files = [];
      picked = new Set();
    });
    void id;
  });

  async function browse() {
    const picked = await api.pickFolder(saveDir || null).catch(() => null);
    if (picked) saveDir = picked;
  }

  /** `files`: torrents only; null keeps every file. */
  async function choose(action: ConfirmArgs["action"], files: number[] | null = null) {
    if (!row || busy) return;
    busy = true;
    error = null;
    try {
      await downloads.confirm(
        {
          id: row.id,
          filename: isTorrent ? null : filename.trim() || null,
          save_dir: saveDir.trim() || null,
          action,
          ...(files ? { files } : {}),
        },
        isTorrent ? false : dontAsk,
      );
    } catch (e) {
      error = api.errorText(e);
    } finally {
      busy = false;
    }
  }

  // Closing or Escape counts as "Later", so nothing is lost. The dialog is already gone, so a failure
  // goes to the error bar and the id is dropped.
  async function dismiss() {
    if (!row || busy || !open) return;
    const id = row.id;
    busy = true;
    try {
      await downloads.confirm({ id, filename: filename.trim() || null, save_dir: saveDir.trim() || null, action: "later" }, dontAsk);
    } catch (e) {
      downloads.error = api.errorText(e);
      downloads.confirms = downloads.confirms.filter((x) => x !== id);
    } finally {
      busy = false;
    }
  }
</script>

<!-- A fresh Modal per download: after Escape the browser has closed the native dialog, and the next
     queued download needs it opened again. -->
{#key row?.id}
<Modal {open} title={isTorrent ? "New torrent" : "New download"} onclose={dismiss} width={isTorrent ? 600 : 520}>
  {#if row && isTorrent}
    <p class="info name" title={row.filename}>{row.filename}</p>
    {#if !hasMeta}
      <p class="info" role="status">Fetching file list… You can wait, or download every file now.</p>
    {:else}
      <div class="files">
        <TorrentFiles {files} selected={picked} onchange={(next) => (picked = next)} disabled={busy} />
        <p class="info sel">
          <button type="button" class="link" onclick={() => (picked = new Set(files.map((f) => f.idx)))} disabled={busy}>Select all</button>
          <button type="button" class="link" onclick={() => (picked = new Set())} disabled={busy}>Select none</button>
          <span>{picked.size} of {files.length} files · {formatBytes(selectedSize(files, picked))} of {formatBytes(selectedSize(files, new Set(files.map((f) => f.idx))))}</span>
        </p>
      </div>
    {/if}

    <div class="field">
      <label for="confirm-save-dir">Save to</label>
      <span class="row">
        <input id="confirm-save-dir" bind:value={saveDir} />
        <button type="button" onclick={browse} disabled={busy}>Browse…</button>
      </span>
    </div>

    {#if error}
      <p class="form-error" role="alert">{error}</p>
    {/if}
    <div class="actions">
      <button class="cancel" onclick={() => choose("cancel")} disabled={busy}>Cancel</button>
      <button onclick={() => choose("later", hasMeta && picked.size > 0 ? [...picked] : null)} disabled={busy}>Later</button>
      <button onclick={() => choose("start", hasMeta ? files.map((f) => f.idx) : null)} disabled={busy}>Download all</button>
      <button class="primary" onclick={() => choose("start", [...picked])} disabled={busy || !hasMeta || picked.size === 0}>Download</button>
    </div>
  {:else if row}
    <p class="info">{host || row.url}</p>
    <p class="info">Size: {row.size !== null ? formatBytes(row.size) : "unknown"}</p>

    <label class="field">
      <span>File name</span>
      <input bind:value={filename} />
    </label>

    <div class="field">
      <label for="confirm-save-dir">Save to</label>
      <span class="row">
        <input id="confirm-save-dir" bind:value={saveDir} />
        <button type="button" onclick={browse} disabled={busy}>Browse…</button>
      </span>
    </div>

    <label class="check"><input type="checkbox" bind:checked={dontAsk} /><span>Don't ask again (you can turn this back on in Settings)</span></label>

    {#if error}
      <p class="form-error" role="alert">{error}</p>
    {/if}
    <div class="actions">
      <button class="cancel" onclick={() => choose("cancel")} disabled={busy}>Cancel</button>
      <button onclick={() => choose("later")} disabled={busy}>Later</button>
      <button class="primary" onclick={() => choose("start")} disabled={busy}>Start now</button>
    </div>
  {/if}
</Modal>
{/key}

<style>
  .info {
    margin: 0 0 6px;
    color: var(--muted);
    user-select: text;
    overflow-wrap: anywhere;
  }
  .cancel {
    margin-right: auto;
  }
  .name {
    color: var(--text);
    font-weight: 600;
  }
  .files {
    margin-bottom: 10px;
  }
  .sel {
    display: flex;
    gap: 12px;
    align-items: center;
    margin-top: 6px;
  }
  .sel span {
    margin-left: auto;
  }
  .link {
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    cursor: pointer;
  }
</style>
