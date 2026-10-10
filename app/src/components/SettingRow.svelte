<script lang="ts">
  import type { Snippet } from "svelte";
  import { rowDef } from "../lib/settingsPage";

  /** One settings row: label and a muted line on the left, the control on the right, extras underneath. */
  let { id, children, below }: { id: string; children?: Snippet; below?: Snippet } = $props();

  const r = $derived(rowDef(id));
</script>

<div class="srow">
  <div class="text">
    <span class="label" id={`set-${id}-label`}>{r.label}</span>
    <span class="desc" id={`set-${id}-desc`}>{r.description}</span>
  </div>
  {#if children}<div class="control">{@render children()}</div>{/if}
  {#if below}<div class="below">{@render below()}</div>{/if}
</div>

<style>
  .srow {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    column-gap: 16px;
    padding: 12px 14px;
  }
  .text {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .label {
    font-size: 14px;
    font-weight: 600;
  }
  .desc {
    font-size: 12.5px;
    color: var(--muted);
  }
  .control {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    max-width: 360px;
  }
  .below {
    grid-column: 1 / -1;
    margin-top: 10px;
  }
</style>
