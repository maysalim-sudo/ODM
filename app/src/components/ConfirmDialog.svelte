<script lang="ts">
  import { untrack } from "svelte";
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import { formatBytes } from "../lib/format";
  import { selectedSize } from "../lib/fileTree";
  import { expiryNote, linkExpiry, listDuplicate, namesInFolder, recentFolders, spaceProblem } from "../lib/newDownload";
  import type { ConfirmArgs, TorrentFile } from "../lib/types";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import TorrentFiles from "./TorrentFiles.svelte";

  const row = $derived(downloads.confirmRow);
  // Effects follow the id, never the row object: that object is rebuilt whenever any
  // download changes, and reading it would reset the form (and the file ticks) mid-choice.
  const confirmId = $derived(downloads.confirmRow?.id);
  // The reuse offer keeps priority when both are waiting.
  const open = $derived(row !== undefined && downloads.offers.length === 0);

  let filename = $state("");
  let saveDir = $state("");
  let dontAsk = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  // Before it starts: the same download already in the list, a file name already taken, its size
  // and the space left on the drive.
  let dupId = $state<string | null>(null);
  const dup = $derived(dupId ? downloads.rows.get(dupId) : undefined);
  /** The name ODM replaced with a free one when the dialog opened, to offer it back. */
  let renamedFrom = $state<string | null>(null);
  let nameTaken = $state(false);
  let suggestion = $state("");
  /** The first name check of a download may rename it; later ones only warn. */
  let autoNamed = false;
  let size = $state<number | null>(null);
  let free = $state<number | null>(null);
  const space = $derived(spaceProblem(size, free));
  // Site videos are fetched fresh from their page; a plain file's or stream's link may expire.
  const expiry = $derived(row && row.kind !== "torrent" && row.kind !== "site" ? expiryNote(linkExpiry(row.url)) : null);
  const recent = $derived(row && row.kind !== "torrent" ? recentFolders([...downloads.rows.values()], saveDir) : []);

  // Torrents: the file list (empty until peers send it) and the files the user ticked.
  const isTorrent = $derived(row?.kind === "torrent");
  let files = $state<TorrentFile[]>([]);
  let picked = $state<Set<number>>(new Set());
  const hasMeta = $derived(!!row?.has_meta);
  const fileVersion = $derived(confirmId ? (downloads.fileLists[confirmId] ?? 0) : 0);

  $effect(() => {
    const id = confirmId;
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
    const id = confirmId;
    untrack(() => {
      const r = downloads.confirmRow;
      filename = r?.filename ?? "";
      saveDir = r?.save_dir ?? "";
      error = null;
      busy = false;
      files = [];
      picked = new Set();
      dupId = r ? (listDuplicate(r, [...downloads.rows.values()])?.id ?? null) : null;
      renamedFrom = null;
      nameTaken = false;
      autoNamed = false;
      size = r?.size ?? null;
      free = null;
      // "Later" can leave it waiting a long time: find its size now (plain files only).
      if (r && r.size === null && r.kind === "file") {
        api
          .measureDownload(r.id)
          .then((s) => {
            if (confirmId === r.id && s !== null) size = s;
          })
          .catch(() => {});
      }
    });
    void id;
  });

  // Whether the name is taken in the folder, on disk or by another download. The first check of a
  // download picks a free name by itself; after that a taken name is pointed out.
  $effect(() => {
    const id = confirmId;
    const name = filename.trim();
    const dir = saveDir.trim();
    if (!id || isTorrent || !name || !dir) {
      nameTaken = false;
      return;
    }
    const others = untrack(() => {
      const r = downloads.rows.get(id);
      return r ? namesInFolder({ ...r, save_dir: dir }, [...downloads.rows.values()]) : [];
    });
    let stale = false;
    const timer = setTimeout(() => {
      api
        .nameCheck(dir, name, others)
        .then((c) => {
          if (stale) return;
          suggestion = c.free;
          if (c.taken && !autoNamed) {
            renamedFrom = name;
            filename = c.free;
            nameTaken = false;
          } else nameTaken = c.taken;
          autoNamed = true;
        })
        .catch(() => {});
    }, 200);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  });

  // The free space where it will be saved.
  $effect(() => {
    const dir = saveDir.trim();
    if (!confirmId || isTorrent || !dir) return;
    let stale = false;
    api
      .freeSpace(dir)
      .then((f) => {
        if (!stale) free = f;
      })
      .catch(() => {});
    return () => {
      stale = true;
    };
  });

  /** Drops the new download and shows the one already in the list. */
  async function showExisting() {
    const existing = dup?.id;
    if (!existing) return;
    await choose("cancel");
    downloads.show(existing);
  }

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
<Modal {open} title={isTorrent ? "New torrent" : "New download"} onclose={dismiss} width={isTorrent ? 860 : 620} panel>
  {#if row && isTorrent}
    <div class="preview">
      <span class="tile" aria-hidden="true"><Icon name="magnet" size={20} /></span>
      <div class="facts">
        <div class="name" title={row.filename}>{row.filename}</div>
        <div class="meta" role="status">
          {#if hasMeta}
            Torrent · {picked.size} of {files.length} files · {formatBytes(selectedSize(files, picked))} of {formatBytes(selectedSize(files, new Set(files.map((f) => f.idx))))}
          {:else}
            Fetching file list… You can wait, or download every file now.
          {/if}
        </div>
      </div>
    </div>
    {#if hasMeta}
      <div class="files">
        <TorrentFiles {files} selected={picked} onchange={(next) => (picked = next)} disabled={busy} />
        <p class="info sel">
          <button type="button" class="link" onclick={() => (picked = new Set(files.map((f) => f.idx)))} disabled={busy}>Select all</button>
          <button type="button" class="link" onclick={() => (picked = new Set())} disabled={busy}>Select none</button>
          <span>ODM stops sharing once your files are finished.</span>
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
      <button class="primary" onclick={() => choose("start", [...picked])} disabled={busy || !hasMeta || picked.size === 0}>
        Download {picked.size === 1 ? "1 file" : `${picked.size} files`}
      </button>
    </div>
  {:else if row}
    <div class="preview">
      <span class="tile" aria-hidden="true"><Icon name="download" size={20} /></span>
      <div class="facts">
        <div class="name" title={row.filename}>{row.filename}</div>
        <div class="meta">{host || row.url} · {size !== null ? formatBytes(size) : "Size unknown"}</div>
      </div>
    </div>

    {#if dup}
      <div class="notice" role="status">
        <span>
          This is already in your list{dup.state === "completed" ? ", finished" : ""}. Start now or Later adds it again.
        </span>
        <button type="button" onclick={showExisting} disabled={busy}>Show it</button>
      </div>
    {/if}

    <label class="field">
      <span>File name</span>
      <input bind:value={filename} aria-invalid={nameTaken} />
    </label>
    {#if nameTaken}
      <p class="note warn" role="status">
        A file named “{filename.trim()}” is already in this folder or your list.
        <button type="button" class="link" onclick={() => (filename = suggestion)}>Use “{suggestion}”</button>
      </p>
    {:else if renamedFrom && renamedFrom !== filename.trim()}
      <p class="note" role="status">
        “{renamedFrom}” is already taken here, so it’s saved as “{filename.trim()}”.
        <button type="button" class="link" onclick={() => (filename = renamedFrom ?? filename)}>Use the original name</button>
      </p>
    {/if}

    <div class="field">
      <label for="confirm-save-dir">Save to</label>
      <span class="row">
        <input id="confirm-save-dir" bind:value={saveDir} />
        {#if recent.length > 0}
          <select
            class="recent"
            aria-label="Recent folders"
            value=""
            onchange={(e) => {
              if (e.currentTarget.value) saveDir = e.currentTarget.value;
              e.currentTarget.value = "";
            }}
          >
            <option value="">Recent…</option>
            {#each recent as dir (dir)}
              <option value={dir}>{dir}</option>
            {/each}
          </select>
        {/if}
        <button type="button" onclick={browse} disabled={busy}>Browse…</button>
      </span>
    </div>
    {#if space}
      <p class="note warn" role="alert">{space}</p>
    {/if}
    {#if expiry}
      <p class="note" role="status">{expiry}</p>
    {/if}

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
  .notice {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
    padding: 8px 12px;
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    background: var(--surface-2);
  }
  .note {
    margin: -6px 0 12px;
    color: var(--muted);
    font-size: 12px;
  }
  .note.warn {
    color: var(--warning, var(--accent));
  }
  .recent {
    max-width: 140px;
  }
  .info {
    margin: 0 0 6px;
    color: var(--muted);
    user-select: text;
    overflow-wrap: anywhere;
  }
  .cancel {
    margin-right: auto;
  }
  .preview {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 14px;
  }
  .tile {
    display: grid;
    place-items: center;
    flex: none;
    width: 44px;
    height: 44px;
    border-radius: var(--radius-lg);
    background: var(--surface-2);
    color: var(--accent);
  }
  .facts {
    min-width: 0;
  }
  .name {
    overflow: hidden;
    font-size: 15px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    overflow-wrap: anywhere;
    color: var(--muted);
    font-size: 12px;
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
    color: var(--muted);
    cursor: pointer;
  }
</style>
