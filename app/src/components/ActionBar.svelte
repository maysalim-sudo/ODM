<script lang="ts">
  import { downloads } from "../lib/downloads.svelte";
  import { queueSchedule } from "../lib/queueSchedule.svelte";
  import { BAR_MENUS, targets, type SelAction } from "../lib/selection";
  import Icon from "./Icon.svelte";
  import PopMenu from "./PopMenu.svelte";

  /** Always above the list: what to do with the selected downloads. Greyed out with nothing selected. */
  let { onaction }: { onaction: (a: SelAction) => void } = $props();

  const rows = $derived(downloads.selectedRows);
  const closed = $derived(queueSchedule.wait !== null);
  const can = (a: SelAction) => targets(rows, a, closed).length > 0;

  let open = $state<{ label: string; x: number; y: number; anchor: HTMLElement } | null>(null);
  const openMenu = $derived(open ? BAR_MENUS.find((m) => m.label === open?.label) : undefined);

  function toggle(e: MouseEvent, label: string) {
    if (open?.label === label) {
      open = null;
      return;
    }
    const anchor = e.currentTarget as HTMLElement;
    const r = anchor.getBoundingClientRect();
    open = { label, x: r.left, y: r.bottom + 4, anchor };
  }
</script>

<div class="bar" role="toolbar" aria-label="Selected downloads">
  <span class="n" aria-live="polite">{rows.length === 0 ? "Nothing selected" : rows.length === 1 ? "1 selected" : `${rows.length} selected`}</span>
  <button disabled={!can("resume") && !can("start-now")} onclick={() => onaction(can("resume") ? "resume" : "start-now")}>
    <Icon name="play" /> Resume
  </button>
  <button disabled={!can("pause")} onclick={() => onaction("pause")}><Icon name="pause" /> Pause</button>
  <button disabled={!can("stop")} onclick={() => onaction("stop")}><Icon name="x" /> Stop</button>
  <span class="gap" aria-hidden="true"></span>
  {#each BAR_MENUS as m (m.label)}
    <button
      class="drop"
      aria-haspopup="menu"
      aria-expanded={open?.label === m.label}
      disabled={!m.items.some(can)}
      onclick={(e) => toggle(e, m.label)}
    >
      {m.label}
      <Icon name="chevron" size={14} />
    </button>
  {/each}
  {#if rows.length > 0}
    <button class="clear" aria-label="Clear the selection" title="Clear the selection" onclick={() => (downloads.selected = new Set())}>
      <Icon name="x" />
    </button>
  {/if}
</div>

{#if open && openMenu}
  {#key open.label}
    <PopMenu items={openMenu.items} enabled={can} x={open.x} y={open.y} label={openMenu.label} anchor={open.anchor} onchoose={onaction} onclose={() => (open = null)} />
  {/key}
{/if}

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 0 20px 8px;
  }
  .n {
    min-width: 96px;
    margin-right: 4px;
    color: var(--muted);
    font-size: 12px;
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 10px;
    font-size: 13px;
  }
  .drop {
    gap: 4px;
  }
  .gap {
    width: 8px;
  }
  .clear {
    width: 30px;
    padding: 0;
    justify-content: center;
    margin-left: auto;
    border-color: transparent;
    background: transparent;
    color: var(--muted);
  }
</style>
