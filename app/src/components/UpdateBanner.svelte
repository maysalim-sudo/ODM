<script lang="ts">
  import { bannerShown, bannerText, primaryLabel } from "../lib/updates";
  import { updates } from "../lib/updates.svelte";

  /** The status bar's notice when a newer ODM is out. Never a modal. */
  let { onnotes }: { onnotes: () => void } = $props();

  const v = $derived(updates.view);
  const text = $derived(bannerText(v, updates.mode));
  const primary = $derived(primaryLabel(v, updates.mode));
  const showBar = $derived(v.phase === "downloading" || v.phase === "installing");
  const fill = $derived(v.phase === "installing" ? 1 : (v.percent ?? 0) / 100);
</script>

{#if bannerShown(v)}
  <div class="banner" role="status">
    <span class="text">{text}</span>
    <div class="actions">
      <button onclick={onnotes}>What's new</button>
      {#if primary}<button class="primary" onclick={() => void updates.install()}>{primary}</button>{/if}
      {#if v.phase !== "installing"}<button onclick={() => updates.later()}>Later</button>{/if}
    </div>
    {#if showBar}
      <div class="bar" aria-hidden="true" style:transform={`scaleX(${fill})`}></div>
    {/if}
  </div>
{/if}

<style>
  .banner {
    position: relative;
    display: flex;
    align-items: center;
    flex: 0 1 auto;
    min-width: 0;
    gap: 10px;
    color: var(--text);
  }
  .text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    gap: 6px;
    flex: none;
  }
  .actions button {
    padding: 2px 8px;
  }
  .bar {
    position: absolute;
    left: 0;
    right: 0;
    bottom: -1px;
    height: 2px;
    background: var(--accent);
    transform-origin: left center;
    transition: transform 250ms var(--ease-out);
  }
  @media (prefers-reduced-motion: reduce) {
    .bar {
      transition: none;
    }
  }
</style>
