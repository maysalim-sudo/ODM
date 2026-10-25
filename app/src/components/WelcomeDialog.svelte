<script lang="ts">
  import * as api from "../lib/api";
  import { integration } from "../lib/integration.svelte";
  import { WELCOME_BROWSERS, browserConnected, extensionsPageFor, storeLinkFor } from "../lib/model";
  import { IS_LINUX } from "../lib/platform";
  import type { StoreLinks, ToolsStatus } from "../lib/types";
  import Modal from "./Modal.svelte";
  import ToolsCard from "./ToolsCard.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  const NO_LINKS: StoreLinks = { chrome_web_store_id: null, edge_addons_id: null, amo_url: null };

  let browser = $state<string>("Chrome");
  let links = $state<StoreLinks>(NO_LINKS);
  let error = $state<string | null>(null);
  let toolsSkipped = $state(false);
  let toolsDone = $state(false);

  const isFirefox = $derived(browser === "Firefox" || browser === "Floorp");
  const storeLink = $derived(storeLinkFor(browser, links));
  const page = $derived(extensionsPageFor(browser));
  const connected = $derived(browserConnected(integration.status));

  $effect(() => {
    if (!open) return;
    error = null;
    toolsSkipped = false;
    toolsDone = false;
    api
      .storeLinks()
      .then((l) => (links = l))
      .catch(() => (links = NO_LINKS));
    void integration.refresh();
    const timer = setInterval(() => void integration.refresh(), 2000);
    return () => clearInterval(timer);
  });

  async function run(action: () => Promise<void>) {
    error = null;
    try {
      await action();
    } catch (e) {
      error = api.errorText(e);
    }
  }

  const addFromStore = () => run(() => api.openStorePage(browser, storeLink ?? ""));
  const openFolder = () => run(() => api.openExtensionFolder(isFirefox ? "firefox" : "chromium"));
  const openPage = () => run(() => api.openBrowserExtensionsPage(browser));

  /** Closing the dialog any way counts as finishing, so it doesn't come back on every start. */
  async function finish() {
    try {
      const s = await api.getSettings();
      if (!s.onboarding_done) await api.saveSettings({ ...s, onboarding_done: true });
    } catch {
      // Not being able to save only means the guide shows again next time.
    }
    onclose();
  }

  function toolsInstalled(s: ToolsStatus) {
    toolsDone = !!(s.ffmpeg && s.ytdlp);
  }
</script>

<Modal {open} title="Welcome to ODM" onclose={finish} width={600}>
  <p class="lead">Three quick steps connect your browser, so downloads and videos go straight to ODM.</p>

  <section>
    <h3><span class="num">1</span> Your browser</h3>
    <div class="chips" role="radiogroup" aria-label="Your browser">
      {#each WELCOME_BROWSERS as b (b)}
        <button
          class="chip"
          class:on={browser === b}
          role="radio"
          aria-checked={browser === b}
          onclick={() => (browser = b)}>{b}</button
        >
      {/each}
    </div>
  </section>

  <section>
    <h3><span class="num">2</span> Add the extension</h3>
    {#if storeLink}
      <div class="buttons">
        <button class="primary" onclick={addFromStore}>Add to {browser}</button>
      </div>
    {:else}
      <div class="buttons">
        <button onclick={openFolder}>Open extension folder</button>
        <button onclick={openPage}>Open extensions page</button>
      </div>
      {#if isFirefox}
        <ol>
          <li>Open <code>{page}</code> and choose <b>Load Temporary Add-on</b>.</li>
          <li>Pick <code>manifest.json</code> in the folder that just opened.</li>
          <li>Firefox forgets temporary add-ons when it closes, so load it again after a restart.</li>
        </ol>
      {:else}
        <ol>
          <li>On <code>{page}</code>, turn on <b>Developer mode</b>.</li>
          <li>Choose <b>Load unpacked</b>.</li>
          <li>Pick the folder that just opened.</li>
        </ol>
      {/if}
    {/if}
    {#if error}<p class="form-error" role="alert">{error}</p>{/if}
    <p class="live" class:ok={connected} role="status">
      {#if connected}<span class="dot"></span>Connected ✓{:else}<span class="dot"></span>Waiting for your browser…{/if}
    </p>
  </section>

  <section>
    <h3><span class="num">3</span> Video tools</h3>
    {#if toolsDone}
      <p class="live ok"><span class="dot"></span>Video tools are ready ✓</p>
    {:else if toolsSkipped}
      <p class="hint">Skipped. You can get them any time in Settings.</p>
      <button onclick={() => (toolsSkipped = false)}>Get video tools</button>
    {:else}
      <p class="hint">FFmpeg and yt-dlp let ODM save YouTube and other site videos.{IS_LINUX ? "" : " About 150 MB."}</p>
      <div class="buttons">
        <ToolsCard compact oninstalled={toolsInstalled} />
        <button class="link" onclick={() => (toolsSkipped = true)}>Skip for now</button>
      </div>
    {/if}
  </section>

  <div class="actions"><button class="primary" onclick={finish}>Done</button></div>
</Modal>

<style>
  .lead {
    margin: 0 0 4px;
    color: var(--muted);
  }
  section {
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 8px;
    font-size: 13px;
  }
  .num {
    display: inline-grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 12px;
    font-weight: 700;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip.on {
    background: var(--selected);
    border-color: var(--accent);
    color: var(--accent-light);
  }
  .buttons {
    display: flex;
    align-items: flex-start;
    flex-wrap: wrap;
    gap: 8px;
  }
  ol {
    margin: 8px 0 0;
    padding-left: 20px;
    color: var(--muted);
  }
  code {
    color: var(--text);
    user-select: text;
  }
  b {
    color: var(--text);
  }
  .live {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 10px 0 0;
    color: var(--muted);
  }
  .live.ok {
    color: var(--success);
    font-weight: 600;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--idle);
  }
  .live.ok .dot {
    background: var(--success);
  }
  .live:not(.ok) .dot {
    animation: pulse 1.4s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .live:not(.ok) .dot {
      animation: none;
    }
  }
  .link {
    background: none;
    border-color: transparent;
    color: var(--muted);
    text-decoration: underline;
  }
  .hint {
    margin: 0 0 8px;
    color: var(--muted);
  }
  .form-error {
    margin: 8px 0 0;
  }
</style>
