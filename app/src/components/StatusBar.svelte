<script lang="ts">
  import { downloads } from "../lib/downloads.svelte";
  import { remaining, remainingLabel } from "../lib/home";
  import { integration } from "../lib/integration.svelte";
  import { integrationProblem, relativeTime } from "../lib/model";
  import SpeedChip from "./SpeedChip.svelte";
  import UpdateBanner from "./UpdateBanner.svelte";

  let { onbrowser, onnotes }: { onbrowser: () => void; onnotes: () => void } = $props();

  const st = $derived(integration.status);
  const attention = $derived(st !== null && integrationProblem(st) !== null);
  const label = $derived(
    st?.last_contact_ms != null
      ? `Browser: connected, ${relativeTime(st.last_contact_ms, integration.now)}`
      : "Browser: not connected yet",
  );
  const left = $derived(remainingLabel(remaining([...downloads.rows.values()])));
</script>

<footer>
  <button class="browser" onclick={onbrowser} title="Browser connection">
    <span class="dot" class:warn={attention} aria-hidden="true"></span>
    <span>{label}</span>
    {#if attention}<span class="sr">Needs attention</span>{/if}
  </button>
  <UpdateBanner {onnotes} />
  <span class="spacer"></span>
  {#if left}<span class="left">{left}</span>{/if}
  <SpeedChip />
</footer>

<style>
  footer {
    display: flex;
    align-items: center;
    gap: 12px;
    flex: none;
    min-height: 36px;
    padding: 2px 16px;
    border-top: 1px solid var(--border);
    background: var(--surface);
    color: var(--muted);
    font-size: 12px;
  }
  .browser {
    display: inline-flex;
    align-items: center;
    flex: none;
    gap: 8px;
    padding: 4px 6px;
    border-color: transparent;
    background: transparent;
    font-weight: 400;
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--success);
  }
  .dot.warn {
    background: var(--warn);
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
  .spacer {
    flex: 1;
  }
  .left {
    flex: none;
    white-space: nowrap;
  }
</style>
