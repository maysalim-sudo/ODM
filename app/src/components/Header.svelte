<script lang="ts">
  import Icon from "./Icon.svelte";

  let {
    onadd,
    onbatch,
    onsettings,
  }: {
    onadd: () => void;
    onbatch: () => void;
    onsettings: () => void;
  } = $props();
</script>

<header>
  <span class="mark" aria-hidden="true"><Icon name="download" size={22} /></span>
  <button class="paste" onclick={onadd}>
    <Icon name="download" />
    <span class="hint">Paste a link or magnet, or drop a file here</span>
    <kbd>Ctrl V</kbd>
  </button>
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
  .paste {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 44px;
    padding: 0 8px 0 14px;
    border-radius: var(--radius-lg);
    background: var(--surface);
    color: var(--muted);
    font-weight: 400;
    text-align: left;
  }
  .hint {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  kbd {
    flex: none;
    padding: 3px 7px;
    border-radius: var(--radius);
    background: var(--surface-2);
    font: inherit;
    font-size: 12px;
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
