<script lang="ts">
  import { segments } from "../lib/model";
  import type { ChunkView } from "../lib/types";

  let { chunks, total }: { chunks: ChunkView[]; total: number | null } = $props();

  const segs = $derived(segments(chunks, total));
  const done = $derived(chunks.filter((c) => c.state === "done").length);
</script>

{#if segs}
  <div class="map" role="img" aria-label={`${done} of ${chunks.length} parts finished`}>
    {#each segs as s (s.idx)}
      <div class="seg {s.state}" style:width={`${s.widthPct}%`} title={`Part ${s.idx + 1}: ${s.fillPct.toFixed(0)}%`}>
        <div class="fill" style:width={`${s.fillPct}%`}></div>
      </div>
    {/each}
  </div>
  <p class="legend">{done} of {chunks.length} parts finished</p>
{:else}
  <p class="legend">The part map appears once the file size is known.</p>
{/if}

<style>
  .map {
    display: flex;
    gap: 2px;
    height: 18px;
  }
  .seg {
    min-width: 2px;
    background: var(--surface-2);
    border-radius: 2px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 200ms linear;
  }
  .seg.active .fill {
    background: var(--accent-light);
  }
  .seg.done .fill {
    background: var(--success);
  }
  .seg.retrying .fill {
    background: #f59e0b;
  }
  .seg.failed {
    outline: 1px solid var(--error);
  }
  .seg.failed .fill {
    background: var(--error);
  }
  .legend {
    margin: 6px 0 0;
    color: var(--muted);
  }
  @media (prefers-reduced-motion: reduce) {
    .fill {
      transition: none;
    }
  }
</style>
