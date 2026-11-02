<script lang="ts">
  import { tick } from "svelte";
  import * as api from "../lib/api";
  import { formatBytes } from "../lib/format";
  import { CATEGORIES, nextFilename, pastedAddress, toInt } from "../lib/model";
  import { queueSchedule } from "../lib/queueSchedule.svelte";
  import { checkSpeed } from "../lib/settingsPage";
  import type { Suggestion, VideoQuality } from "../lib/types";
  import Icon from "./Icon.svelte";

  /** `blocked`: a dialog is open, so Ctrl+V outside a text field must not open the panel. */
  let { blocked }: { blocked: boolean } = $props();

  const QUALITIES: { value: VideoQuality; label: string }[] = [
    { value: "best", label: "Best" },
    { value: "1080p", label: "1080p" },
    { value: "720p", label: "720p" },
    { value: "480p", label: "480p" },
    { value: "audio", label: "Audio only" },
  ];
  const TILES: Record<string, string> = {
    video: "video",
    audio: "music",
    document: "doc",
    compressed: "archive",
    image: "image",
    program: "program",
  };

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
  let quality = $state<VideoQuality>("best");
  let subtitles = $state(false);
  let settingsQuality: VideoQuality = "best";
  let settingsSubtitles = false;
  let busy = $state(false);
  let error = $state<string | null>(null);
  let more = $state(false);
  let focused = $state(false);
  let input = $state<HTMLInputElement>();
  let panel = $state<HTMLFormElement>();
  /** Where focus goes back to when the panel closes. */
  let before: HTMLElement | null = null;

  const open = $derived(url.trim() !== "");
  const validUrl = $derived(/^https?:\/\/[^/]/i.test(url.trim()));
  /** A magnet link: added as a torrent; its files are chosen in the next window. */
  const isMagnet = $derived(/^magnet:\?/i.test(url.trim()));

  /** Ctrl+N. */
  export function focus() {
    input?.focus();
  }

  function reset() {
    url = "";
    filename = "";
    lastAuto = null;
    saveDir = "";
    category = "";
    suggestion = null;
    variantUrl = "";
    error = null;
    limitKbps = null;
    more = false;
    quality = settingsQuality;
    subtitles = settingsSubtitles;
  }

  function loadDefaults() {
    api
      .getSettings()
      .then((s) => {
        defaultConnections = s.max_connections;
        connections = s.max_connections;
        addsPaused = s.start_mode === "paused";
        settingsQuality = quality = s.video_quality;
        settingsSubtitles = subtitles = s.video_subtitles;
      })
      .catch(() => {});
  }

  // Fresh defaults for each new address.
  $effect(() => {
    if (open) loadDefaults();
  });

  function close() {
    reset();
    (before ?? input)?.focus();
    before = null;
  }

  /** Ctrl+V anywhere but a text field. Text that isn't an address is ignored. */
  function onpaste(e: ClipboardEvent) {
    const t = e.target;
    const typing =
      t instanceof HTMLInputElement ||
      t instanceof HTMLTextAreaElement ||
      t instanceof HTMLSelectElement ||
      (t instanceof HTMLElement && t.isContentEditable);
    if (typing || blocked || open) return;
    const address = pastedAddress(e.clipboardData?.getData("text") ?? "");
    if (!address) return;
    before = t instanceof HTMLElement ? t : null;
    url = address;
    void tick().then(() => input?.focus());
  }

  // Ask the server about the address once typing pauses.
  $effect(() => {
    const wanted = url.trim();
    suggestion = null;
    if (!validUrl) return;
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

  const title = $derived(isMagnet ? "Torrent" : suggestion?.is_page ? (suggestion.filename || "Video") : filename || suggestion?.filename || "New download");
  const tile = $derived(isMagnet ? "magnet" : suggestion?.is_page || suggestion?.is_stream ? "video" : (TILES[suggestion?.category ?? ""] ?? "doc"));
  const primary = $derived(
    isMagnet ? "Add torrent" : addsPaused ? "Add paused" : waits ? "Add to queue" : suggestion?.is_page ? "Download video" : "Download",
  );

  /** What differs from Settings; nothing is stored when the choice matches them. */
  function videoOverride() {
    const o: { quality?: VideoQuality; subtitles?: boolean } = {};
    if (quality !== settingsQuality) o.quality = quality;
    if (subtitles !== settingsSubtitles) o.subtitles = subtitles;
    return Object.keys(o).length > 0 ? o : null;
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    await add(false);
  }

  async function addTorrent(input: Parameters<typeof api.addTorrent>[0]) {
    busy = true;
    error = null;
    try {
      await api.addTorrent(input);
      close();
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
        ...(suggestion?.is_page ? { page: true, video_override: videoOverride() } : {}),
        ...(plainFile && limitKbps != null ? { speed_limit_kbps: Math.round(limitKbps) } : {}),
        ...(startNow ? { start_now: true } : {}),
      });
      close();
    } catch (err) {
      error = api.errorText(err);
    } finally {
      busy = false;
    }
  }

  /** Esc closes; Tab stays between the bar and the panel while it is open, as in a dialog. */
  function onkeydown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (e.key === "Tab") {
      const stops = [input, ...(panel?.querySelectorAll<HTMLElement>("button:not(:disabled), input, select") ?? [])].filter(
        (el): el is HTMLElement => !!el,
      );
      const last = stops.length - 1;
      const at = stops.indexOf(document.activeElement as HTMLElement);
      if (e.shiftKey ? at <= 0 : at === last) {
        e.preventDefault();
        stops[e.shiftKey ? last : 0]?.focus();
      }
    }
  }
</script>

<svelte:window {onpaste} />

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="scrim" onclick={close}></div>
{/if}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="add" class:open {onkeydown}>
  <div class="bar">
    <Icon name="download" />
    <input
      bind:this={input}
      bind:value={url}
      aria-label="Link or magnet to download"
      placeholder="Paste a link or magnet, or drop a file here"
      spellcheck="false"
      autocomplete="off"
      onfocus={() => (focused = true)}
      onblur={() => (focused = false)}
    />
    {#if !open}<kbd>Ctrl V</kbd>{/if}
  </div>

  {#if focused && !open}
    <div class="under">
      <button type="button" class="link" onmousedown={(e) => e.preventDefault()} onclick={pickTorrent}>Or choose a .torrent file…</button>
    </div>
  {/if}

  {#if open}
    <form class="panel" bind:this={panel} onsubmit={submit} aria-label="New download">
      <div class="preview">
        <span class="tile" aria-hidden="true"><Icon name={tile} size={20} /></span>
        <div class="facts">
          <div class="name" title={title}>{title}</div>
          <div class="meta" role="status">
            {#if isMagnet}
              Magnet link · you choose which files to download next
            {:else if checking}
              Checking the address…
            {:else if suggestion?.is_page}
              Video from a web page · downloaded with yt-dlp
            {:else if suggestion?.is_stream}
              Video stream · downloaded in parts and merged into one file with FFmpeg
            {:else if suggestion}
              {suggestion.size === null ? "Size unknown" : formatBytes(suggestion.size)} ·
              {suggestion.resumable ? "Can pause and resume" : "Can't resume if interrupted"}
            {:else if !validUrl}
              Enter an address that starts with http:// or https://, or a magnet link
            {:else}
              &nbsp;
            {/if}
          </div>
        </div>
      </div>

      {#if !isMagnet}
        {#if suggestion?.is_page}
          <div class="group" role="group" aria-label="Quality">
            <span class="label">Quality</span>
            <div class="chips">
              {#each QUALITIES as q (q.value)}
                <button type="button" aria-pressed={quality === q.value} onclick={() => (quality = q.value)}>{q.label}</button>
              {/each}
            </div>
          </div>
          {#if quality !== "audio"}
            <label class="check"><input type="checkbox" bind:checked={subtitles} /> Include subtitles when available</label>
          {/if}
        {:else if suggestion?.is_stream && suggestion.variants.length > 0}
          <div class="group" role="group" aria-label="Quality">
            <span class="label">Quality</span>
            <div class="chips">
              {#each suggestion.variants as v (v.url)}
                <button type="button" aria-pressed={variantUrl === v.url} onclick={() => (variantUrl = v.url)}>{v.label}</button>
              {/each}
            </div>
          </div>
        {/if}

        {#if more}
          <div class="fields">
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
          </div>
          <div class="fields three">
            <label class="field">
              <span>Type</span>
              <select bind:value={category}>
                <option value="">Automatic{suggestion ? ` (${suggestion.category})` : ""}</option>
                {#each CATEGORIES.filter((c) => c !== "torrent") as c (c)}
                  <option value={c}>{c}</option>
                {/each}
              </select>
            </label>
            {#if !suggestion?.is_page}
              <label class="field">
                <span>Connections</span>
                <input type="number" min="1" max="32" bind:value={connections} />
              </label>
            {/if}
            {#if plainFile}
              <label class="field">
                <span>Speed limit (KB/s)</span>
                <input type="number" min="1" step="1" placeholder="No limit" aria-invalid={limitError !== null} bind:value={limitKbps} />
              </label>
            {/if}
          </div>
          {#if limitError}<p class="form-error" role="alert">{limitError}</p>{/if}
        {:else}
          <div class="dest">
            <span class="label">Save to</span>
            <span class="where" title={saveDir}>{saveDir || "Your download folder, sorted by type"}</span>
            <button type="button" class="link" onclick={browse}>Change</button>
          </div>
        {/if}
      {/if}

      {#if error}<p class="form-error" role="alert">{error}</p>{/if}

      <div class="foot">
        {#if !isMagnet}
          <button type="button" class="link" aria-expanded={more} onclick={() => (more = !more)}>
            {more ? "Fewer options" : "More options"}
            <Icon name={more ? "chevron-right" : "chevron"} size={14} />
          </button>
        {/if}
        <span class="spacer"></span>
        <button type="button" class="plain" onclick={close}>Cancel</button>
        {#if waits && !isMagnet}
          <button type="button" disabled={!validUrl || busy || limitError !== null} onclick={() => add(true)}>Start now</button>
        {/if}
        <button type="submit" class="primary" disabled={!(validUrl || isMagnet) || busy || (!isMagnet && limitError !== null)}>
          {primary}
        </button>
      </div>
    </form>
  {/if}
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 15;
    background: var(--backdrop);
  }
  .add {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  .add.open {
    z-index: 16;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 44px;
    padding: 0 8px 0 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
    color: var(--muted);
  }
  .bar:focus-within {
    border-color: var(--accent);
    color: var(--accent);
  }
  .bar input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text);
    user-select: text;
  }
  .bar input::placeholder {
    color: var(--muted);
  }
  kbd {
    flex: none;
    padding: 3px 7px;
    border-radius: var(--radius);
    background: var(--surface-2);
    color: var(--muted);
    font: inherit;
    font-size: 12px;
  }
  .under {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 16;
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  .panel {
    position: absolute;
    top: calc(100% + 8px);
    left: 0;
    right: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px 18px;
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    background: var(--surface);
    box-shadow: var(--shadow-dialog);
  }
  .preview {
    display: flex;
    align-items: center;
    gap: 14px;
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
  .meta,
  .label {
    color: var(--muted);
    font-size: 12px;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .chips button {
    flex: 1;
    min-width: 72px;
    height: 40px;
    background: var(--bg);
  }
  .chips button[aria-pressed="true"] {
    border-color: var(--accent);
    background: var(--selected);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .fields {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .fields.three {
    grid-template-columns: repeat(auto-fit, minmax(0, 1fr));
    grid-auto-flow: column;
  }
  .fields .field {
    min-width: 0;
    margin: 0;
  }
  .dest {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .where {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .spacer {
    flex: 1;
  }
  .link {
    display: inline-flex;
    align-items: center;
    flex: none;
    gap: 4px;
    padding: 2px 4px;
    border-color: transparent;
    background: transparent;
    color: var(--muted);
  }
  .plain {
    border-color: transparent;
    background: transparent;
    color: var(--muted);
  }
</style>
