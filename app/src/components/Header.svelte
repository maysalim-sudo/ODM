<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";

  let {
    children,
    onbatch,
    onsettings,
  }: {
    /** The paste bar. */
    children: Snippet;
    onbatch: () => void;
    onsettings: () => void;
  } = $props();
</script>

<header>
  <span class="mark" aria-hidden="true"><Icon name="download" size={22} /></span>
  {@render children()}
  <button class="side" onclick={onbatch}><Icon name="add-many" /> Add several</button>
  <button class="side" onclick={onsettings}><Icon name="sliders" /> Settings</button>
</header>

<style>
  header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 20px;
  }
  .mark {
    display: grid;
    place-items: center;
    flex: none;
    width: 32px;
    height: 32px;
    border-radius: var(--radius);
    background: var(--accent);
    color: var(--on-accent);
  }
  .side {
    display: inline-flex;
    align-items: center;
    flex: none;
    gap: 6px;
    height: 44px;
    padding: 0 14px;
    border-radius: var(--radius-lg);
  }
</style>
