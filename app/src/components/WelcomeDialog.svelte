<script lang="ts">
  import { tick } from "svelte";
  import * as api from "../lib/api";
  import { integration } from "../lib/integration.svelte";
  import { WELCOME_BROWSERS, browserConnected, extensionsPageFor, storeLinkFor } from "../lib/model";
  import { IS_LINUX } from "../lib/platform";
  import type { StoreLinks, ToolsStatus } from "../lib/types";
  import Icon from "./Icon.svelte";
  import ToolsCard from "./ToolsCard.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  const NO_LINKS: StoreLinks = { chrome_web_store_id: null, edge_addons_id: null, amo_url: null };

  let browser = $state<string>("Chrome");
  let links = $state<StoreLinks>(NO_LINKS);
  let error = $state<string | null>(null);
  let toolsDone = $state(false);
  let step = $state(0);
  let stepEl = $state<HTMLElement>();

  const isFirefox = $derived(browser === "Firefox" || browser === "Floorp");
  const storeLink = $derived(storeLinkFor(browser, links));
  const page = $derived(extensionsPageFor(browser));
  const connected = $derived(browserConnected(integration.status));

  $effect(() => {
    if (!open) return;
    error = null;
    step = 0;
    toolsDone = false;
    api
      .storeLinks()
      .then((l) => (links = l))
      .catch(() => (links = NO_LINKS));
    void integration.refresh();
    const timer = setInterval(() => void integration.refresh(), 2000);
    return () => clearInterval(timer);
  });

  // The connect step moves on by itself once the browser talks to ODM.
  $effect(() => {
    if (open && step === 1 && connected) step = 2;
  });

  // Each new screen starts with focus on its first control.
  $effect(() => {
    void step;
    if (open) void tick().then(() => stepEl?.querySelector<HTMLElement>("button")?.focus());
  });

  function toConnect() {
    step = 1;
    if (!storeLink) void openFolder();
  }

  function onkey(e: KeyboardEvent) {
    if (open && e.key === "Escape") void finish();
  }

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

  /** Leaving the flow any way counts as finishing, so it doesn't come back on every start. */
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

<svelte:window onkeydown={onkey} />

{#if open}
  <div class="welcome" role="dialog" aria-modal="true" aria-label="Welcome to ODM">
    <header>
      <span class="mark" aria-hidden="true"><Icon name="download" size={18} /></span>
      <strong>ODM</strong>
      <button class="link skip" onclick={finish}>Skip setup</button>
    </header>

    <div class="step" bind:this={stepEl}>
      <div class="pips" aria-hidden="true">
        {#each [0, 1, 2] as i (i)}<span class:on={i <= step}></span>{/each}
      </div>

      {#if step === 0}
        <h1>Which browser do you use?</h1>
        <p class="lead">ODM connects to it, so downloads and videos go straight to ODM.</p>
        <div class="chips" role="radiogroup" aria-label="Your browser">
          {#each WELCOME_BROWSERS as b (b)}
            <button class="chip" class:on={browser === b} role="radio" aria-checked={browser === b} onclick={() => (browser = b)}>{b}</button>
          {/each}
        </div>
        <div class="foot">
          <button class="primary" onclick={toConnect}>Continue</button>
        </div>
      {:else if step === 1}
        <h1>Add ODM to {browser}</h1>
        {#if storeLink}
          <p class="lead">This page moves on by itself once {browser} connects.</p>
          <button class="primary" onclick={addFromStore}>Add to {browser}</button>
        {:else}
          <p class="lead">Three quick steps. This page moves on by itself once {browser} connects.</p>
          <ol class="steps">
            {#if isFirefox}
              <li>
                <span class="n" aria-hidden="true">1</span>
                <div><b>Open <code>{page}</code></b><small>Then choose Load Temporary Add-on</small></div>
                <button onclick={openPage}>Open page</button>
              </li>
              <li>
                <span class="n" aria-hidden="true">2</span>
                <div><b>Pick <code>manifest.json</code></b><small>It is in the folder that just opened</small></div>
                <button onclick={openFolder}>Open folder</button>
              </li>
              <li>
                <span class="n" aria-hidden="true">3</span>
                <div><b>Load it again after a restart</b><small>Firefox forgets temporary add-ons when it closes</small></div>
              </li>
            {:else}
              <li>
                <span class="n" aria-hidden="true">1</span>
                <div><b>Open <code>{page}</code></b><small>Then turn on Developer mode</small></div>
                <button onclick={openPage}>Open page</button>
              </li>
              <li>
                <span class="n" aria-hidden="true">2</span>
                <div><b>Choose Load unpacked</b><small>It appears once Developer mode is on</small></div>
              </li>
              <li>
                <span class="n" aria-hidden="true">3</span>
                <div><b>Pick the folder that just opened</b><small>ODM has it ready</small></div>
                <button onclick={openFolder}>Open folder</button>
              </li>
            {/if}
          </ol>
        {/if}
        {#if error}<p class="form-error" role="alert">{error}</p>{/if}
        <p class="live" role="status"><span class="dot"></span>Waiting for {browser}…</p>
        <div class="foot">
          <button class="link" onclick={() => (step = 0)}>Back</button>
          <button class="link" onclick={finish}>Do this later</button>
        </div>
      {:else}
        <h1>Save videos from sites</h1>
        {#if connected}<p class="live ok" role="status"><span class="dot"></span>{browser} is connected</p>{/if}
        {#if toolsDone}
          <p class="live ok"><span class="dot"></span>Video tools are ready</p>
          <div class="foot">
            <button class="primary" onclick={finish}>Done</button>
          </div>
        {:else}
          <p class="lead">FFmpeg and yt-dlp let ODM save YouTube and other site videos.{IS_LINUX ? " About 35 MB, plus FFmpeg from your package manager." : " About 150 MB."}</p>
          <ToolsCard compact oninstalled={toolsInstalled} />
          <div class="foot">
            <button class="link" onclick={() => (step = 1)}>Back</button>
            <button class="link" onclick={finish}>Do this later</button>
          </div>
        {/if}
      {/if}
    </div>
  </div>
{/if}

<style>
  .welcome {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    overflow-y: auto;
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 20px 24px;
  }
  .mark {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius);
    background: var(--accent);
    color: var(--on-accent);
  }
  .skip {
    margin-left: auto;
  }
  .step {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    width: min(620px, 100%);
    margin: auto;
    padding: 0 20px 48px;
    text-align: center;
  }
  .pips {
    display: flex;
    gap: 6px;
  }
  .pips span {
    width: 40px;
    height: 4px;
    border-radius: 2px;
    background: var(--surface-2);
  }
  .pips .on {
    background: var(--accent);
  }
  h1 {
    margin: 8px 0 0;
    font-size: 28px;
    font-weight: 700;
  }
  .lead {
    margin: 0;
    color: var(--muted);
    font-size: 14px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
  }
  .chip {
    padding: 10px 16px;
  }
  .chip.on {
    background: var(--selected);
    border-color: var(--accent);
  }
  .steps {
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: 100%;
    margin: 0;
    padding: 16px;
    border-radius: var(--radius-lg);
    background: var(--surface);
    list-style: none;
    text-align: left;
  }
  .steps li {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .steps div {
    flex: 1;
    min-width: 0;
  }
  .steps button {
    flex: none;
  }
  .n {
    display: grid;
    place-items: center;
    flex: none;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: var(--surface-2);
    font-weight: 600;
  }
  b {
    display: block;
    font-weight: 600;
  }
  small {
    color: var(--muted);
    font-size: 12px;
  }
  code {
    font: inherit;
    user-select: text;
  }
  .live {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    margin: 0;
    padding: 10px 18px;
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--muted);
  }
  .live.ok {
    color: var(--success);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
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
  .foot {
    display: flex;
    gap: 16px;
  }
  .link {
    background: none;
    border-color: transparent;
    color: var(--muted);
  }
</style>
