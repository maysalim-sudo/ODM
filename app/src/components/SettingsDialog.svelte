<script lang="ts">
  import * as api from "../lib/api";
  import { CATEGORIES, coerceSettings } from "../lib/model";
  import type { Settings } from "../lib/types";
  import Modal from "./Modal.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  let s = $state<Settings | null>(null);
  let loadedSettings: Settings | null = null;
  let skipHosts = $state("");
  let minCaptureMb = $state<number | null>(1);
  let busy = $state(false);
  let error = $state<string | null>(null);

  const MB = 1024 * 1024;

  $effect(() => {
    if (!open) return;
    s = null;
    error = null;
    api
      .getSettings()
      .then((loaded) => {
        loadedSettings = loaded;
        s = loaded;
        skipHosts = loaded.skip_hosts.join("\n");
        minCaptureMb = Math.round((loaded.min_capture_bytes / MB) * 10) / 10;
      })
      .catch((e) => (error = api.errorText(e)));
  });

  async function browse() {
    if (!s) return;
    const picked = await api.pickFolder(s.download_dir || null).catch(() => null);
    if (picked) s.download_dir = picked;
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!s || !loadedSettings || busy) return;
    busy = true;
    error = null;
    try {
      const mb = typeof minCaptureMb === "number" && Number.isFinite(minCaptureMb) ? minCaptureMb : NaN;
      await api.saveSettings(
        coerceSettings(
          {
            ...$state.snapshot(s),
            skip_hosts: skipHosts
              .split(/\r?\n/)
              .map((h) => h.trim())
              .filter(Boolean),
            // NaN (an emptied field) falls back to the loaded value in coerceSettings.
            min_capture_bytes: Math.max(0, Math.round(mb * MB)),
          },
          loadedSettings,
        ),
      );
      onclose();
    } catch (err) {
      error = api.errorText(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal {open} title="Settings" {onclose} width={620}>
  {#if s}
    <form onsubmit={save}>
      <div class="scroll">
        <h3>Folders</h3>
        <div class="field">
          <label for="set-download-dir">Download folder</label>
          <span class="row">
            <input id="set-download-dir" bind:value={s.download_dir} required />
            <button type="button" onclick={browse}>Browse</button>
          </span>
        </div>
        <p class="hint">Each type goes into its own folder inside the download folder. Leave one empty to save that type directly in it.</p>
        <div class="grid">
          {#each CATEGORIES as c (c)}
            <label class="field">
              <span>{c}</span>
              <input bind:value={s.category_folders[c]} />
            </label>
          {/each}
        </div>

        <h3>Downloads</h3>
        <div class="grid">
          <label class="field">
            <span>Downloads at once (1–10)</span>
            <input type="number" min="1" max="10" bind:value={s.max_concurrent} />
          </label>
          <label class="field">
            <span>Connections per download (1–32)</span>
            <input type="number" min="1" max="32" bind:value={s.max_connections} />
          </label>
          <label class="field">
            <span>Speed limit in KB/s (0 = no limit)</span>
            <input type="number" min="0" bind:value={s.speed_limit_kbps} />
          </label>
          <label class="field">
            <span>Tries per part before giving up (1–20)</span>
            <input type="number" min="1" max="20" bind:value={s.max_attempts} />
          </label>
          <label class="field">
            <span>Connect timeout in seconds (5–120)</span>
            <input type="number" min="5" max="120" bind:value={s.connect_timeout_s} />
          </label>
          <label class="field">
            <span>Read timeout in seconds (10–300)</span>
            <input type="number" min="10" max="300" bind:value={s.read_timeout_s} />
          </label>
        </div>
        <label class="field">
          <span>FFmpeg location (used for video streams; leave empty to find it automatically)</span>
          <input bind:value={s.ffmpeg_path} />
        </label>

        <h3>Window</h3>
        <label class="check"><input type="checkbox" bind:checked={s.close_to_tray} /><span>Keep running in the tray when the window is closed</span></label>
        <label class="check"><input type="checkbox" bind:checked={s.start_minimized} /><span>Start hidden in the tray</span></label>
        <label class="check"><input type="checkbox" bind:checked={s.notifications} /><span>Show a notification when a download finishes</span></label>

        <h3>Browser</h3>
        <p class="hint">These apply once the ODM browser extension is installed.</p>
        <label class="check"><input type="checkbox" bind:checked={s.capture_enabled} /><span>Take over downloads started in the browser</span></label>
        <label class="field">
          <span>Leave files smaller than this many MB to the browser</span>
          <input type="number" min="0" step="0.1" bind:value={minCaptureMb} />
        </label>
        <label class="field">
          <span>Sites to leave to the browser, one per line</span>
          <textarea rows="3" spellcheck="false" bind:value={skipHosts}></textarea>
        </label>
      </div>

      {#if error}<p class="form-error" role="alert">{error}</p>{/if}
      <div class="actions">
        <button type="button" onclick={onclose}>Cancel</button>
        <button type="submit" class="primary" disabled={busy}>Save</button>
      </div>
    </form>
  {:else if error}
    <p class="form-error" role="alert">{error}</p>
    <div class="actions"><button onclick={onclose}>Close</button></div>
  {:else}
    <p class="hint">Loading…</p>
  {/if}
</Modal>

<style>
  .scroll {
    max-height: 62vh;
    overflow-y: auto;
    padding-right: 6px;
  }
  h3 {
    margin: 16px 0 8px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
  }
  h3:first-child {
    margin-top: 0;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0 12px;
  }
  .hint {
    margin-bottom: 8px;
  }
</style>
