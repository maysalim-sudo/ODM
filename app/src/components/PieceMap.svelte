<script lang="ts">
  import { pieceCells } from "../lib/fileTree";
  import type { PieceMap } from "../lib/types";

  /** Which pieces of a torrent are on disk, grouped into cells like the part map. */
  let { pieces }: { pieces: PieceMap } = $props();

  const CELLS = 120;
  const cells = $derived(pieceCells(pieces.have_hex, pieces.count, CELLS));
  const have = $derived.by(() => {
    let n = 0;
    for (let i = 0; i < pieces.have_hex.length; i += 2) {
      let b = parseInt(pieces.have_hex.slice(i, i + 2), 16);
      while (b) {
        n += b & 1;
        b >>= 1;
      }
    }
    return Math.min(n, pieces.count);
  });
  const legend = $derived(`${have} of ${pieces.count} pieces`);
</script>

<div class="map" role="img" aria-label={legend}>
  {#each cells as c, i (i)}
    <div class="cell" class:done={c >= 1} style:--fill={`${Math.round(c * 100)}%`}></div>
  {/each}
</div>
<p class="legend">{legend}</p>

<style>
  .map {
    display: flex;
    gap: 1px;
    height: 18px;
  }
  .cell {
    flex: 1;
    min-width: 1px;
    border-radius: 1px;
    background: linear-gradient(to top, var(--accent) var(--fill), var(--surface-2) var(--fill));
  }
  .cell.done {
    background: var(--success);
  }
  .legend {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: 0.85em;
  }
</style>
