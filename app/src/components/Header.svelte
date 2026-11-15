<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";
  import Logo from "./Logo.svelte";

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
  <Logo size={32} />
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
