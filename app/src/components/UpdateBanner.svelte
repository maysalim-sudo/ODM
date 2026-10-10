<script lang="ts">
  import { PAGE_MS, springEase } from "../lib/settingsPage";
  import { bannerShown, bannerText, primaryLabel } from "../lib/updates";
  import { updates } from "../lib/updates.svelte";

  /** A slim bar above the download list when a newer ODM is out. Never a modal. */
  let { onnotes }: { onnotes: () => void } = $props();

  const v = $derived(updates.view);
  const text = $derived(bannerText(v, updates.mode));
  const primary = $derived(primaryLabel(v, updates.mode));
  const showBar = $derived(v.phase === "downloading" || v.phase === "installing");
  const fill = $derived(v.phase === "installing" ? 1 : (v.percent ?? 0) / 100);

  /** Slides down its own height with a critically damped spring and leaves the same way. Reduced motion: a 150 ms fade. */
  function drop(_node: Element) {
    const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
    return reduce
      ? { duration: 150, css: (t: number) => `opacity: ${t}` }
      : { duration: PAGE_MS, easing: springEase, css: (t: number) => `transform: translateY(${(t - 1) * 100}%)` };
  }
</script>

{#if bannerShown(v)}
  <div class="clip">
    <div class="banner" role="status" transition:drop>
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
  </div>
{/if}

<style>
  .clip {
    overflow: hidden;
    flex: none;
  }
  .banner {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 10px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
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
    padding: 3px 10px;
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
